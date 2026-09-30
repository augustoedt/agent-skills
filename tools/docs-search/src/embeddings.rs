//! Phase 4.4 — `local-embeddings-v1`.
//!
//! Local, CPU-only embedding retrieval with the single frozen E5 model. The engine
//! never reaches the network: every tensor, tokenizer rule and vector comes from
//! artifacts that were frozen before implementation. The vector index is a single
//! private binary file that can be deleted and rebuilt from the Markdown corpus.
//!
//! The public frozen engine configuration lives in [`frozen_configuration`] and is
//! expected to match `#/$defs/embeddingConfig` in
//! `evaluation/engine-bakeoff-protocol.schema.json` byte for byte.

use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use serde::Deserialize;
use serde_json::{Value, json};
use tokenizers::{Tokenizer, TruncationDirection, TruncationParams, TruncationStrategy};

use crate::corpus::{self, Chunk, Corpus};
use crate::evaluate::corpus_fingerprint;
use crate::search::{
    SearchTimings, elapsed_ms, excerpt, meaningful_query_tokens, round_score, tokens,
    validate_request,
};
use crate::sha256;
use crate::sqlite_cache::{IncrementalStep, IndexState, IndexingMetrics, cache_path};
use crate::types::{
    CorpusSummary, SCHEMA_VERSION, SearchRequest, SearchResponse, SearchResult, SearchSelection,
};

pub(crate) const EMBEDDINGS_ENGINE: &str = "local-embeddings-v1";
pub(crate) const EMBEDDINGS_CONFIG_SHA256: &str =
    "de41db0af0e15a1dac47b2504617c0f6dbba8f10a2b0ea822a1e9ab2bb208e3d";

const INDEX_FILE: &str = "embeddings-v1.bin";
const INDEX_SCHEMA_VERSION: &str = "embeddings-1";
const INDEX_FORMAT_VERSION: u32 = 1;
const PARSER_VERSION: &str = "markdown-heading-fence-aware-v1";
const INDEX_MAGIC: &[u8; 8] = b"DSE1BIN\x00";
pub(crate) const CANDIDATE_DEPTH: usize = 50;
const ABSTENTION_THRESHOLD: f32 = 0.80;
const L2_EPSILON: f32 = 1e-12;
const EXPECTED_DIMENSION: usize = 384;
const EXPECTED_MAX_TOKENS: usize = 512;
// Batch size one makes passage vectors bit-stable across add/remove/rename updates while still
// satisfying dynamic-longest-per-batch padding from the frozen protocol.
const EMBED_BATCH_SIZE: usize = 1;

const FROZEN_MODEL_ID: &str = "intfloat/multilingual-e5-small";
const FROZEN_MODEL_REVISION: &str = "614241f622f53c4eeff9890bdc4f31cfecc418b3";
const FROZEN_MODEL_LICENSE: &str = "MIT";
const FROZEN_QUERY_PREFIX: &str = "query: ";
const FROZEN_PASSAGE_PREFIX: &str = "passage: ";
const FROZEN_POOLING: &str = "attention-mask-mean";
const FROZEN_NORMALIZATION: &str = "l2-f32-epsilon-1e-12";
const FROZEN_SIMILARITY: &str = "f32-dot-product-on-l2-normalized-vectors";
const FROZEN_DTYPE: &str = "f32";
const FROZEN_RUNTIME: &str = "candle-cpu-0.9.1";
const FROZEN_TOKENIZERS: &str = "0.21.1";
const FROZEN_ARTIFACTS_SHA256_FILE: &str = "SHA256SUMS";

const EXPECTED_ARTIFACTS: [(&str, u64, &str); 3] = [
    (
        "config.json",
        655,
        "69137736cab8b8903a07fe8afaafdda25aac55415a12a55d1bffa9f581abf959",
    ),
    (
        "tokenizer.json",
        17_082_730,
        "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39",
    ),
    (
        "model.safetensors",
        470_641_600,
        "1a55775f53449dac10a2bcbc312469fac40b96d53198c407081a831f81c98477",
    ),
];

/// Frozen engine configuration for `local-embeddings-v1`.
///
/// The returned value must equal `#/$defs/embeddingConfig` from the frozen protocol
/// schema. `model_ref` is resolved against `protocol.model`.
pub(crate) fn frozen_configuration() -> Value {
    json!({
        "model_ref": "protocol.model",
        "tokenization": {
            "add_special_tokens": true,
            "truncation": "right",
            "max_tokens": EXPECTED_MAX_TOKENS,
            "stride": 0,
            "padding": "dynamic-longest-per-batch",
            "query_prefix": FROZEN_QUERY_PREFIX,
            "passage_prefix": FROZEN_PASSAGE_PREFIX
        },
        "vector_storage": "little-endian-f32-384-dim-l2-normalized-keyed-by-chunk-hash",
        "candidate_generation": "exact-linear-scan-of-all-chunk-vectors-no-ann",
        "candidate_depth": CANDIDATE_DEPTH,
        "ranking": "f32-dot-product-descending-then-path-asc-line-start-asc",
        "abstention": "return-empty-when-top-dot-product-is-less-than-0.80"
    })
}

// ---------------------------------------------------------------------------
// Model specification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelArtifact {
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelSpec {
    pub id: String,
    pub revision: String,
    pub license: String,
    pub dimension: usize,
    pub max_tokens: usize,
    pub query_prefix: String,
    pub passage_prefix: String,
    pub pooling: String,
    pub normalization: String,
    pub similarity: String,
    pub dtype: String,
    pub runtime: String,
    pub tokenizers: String,
    pub artifacts: Vec<ModelArtifact>,
    pub artifacts_sha256_file: String,
}

impl ModelSpec {
    /// Stable fingerprint of the frozen model identity and artifact hashes.
    pub(crate) fn fingerprint(&self) -> String {
        let mut material = String::new();
        material.push_str(&self.id);
        material.push('\0');
        material.push_str(&self.revision);
        material.push('\0');
        material.push_str(&self.dimension.to_string());
        material.push('\0');
        for artifact in &self.artifacts {
            material.push_str(artifact_basename(&artifact.file).unwrap_or(&artifact.file));
            material.push('\0');
            material.push_str(&artifact.sha256);
            material.push('\0');
        }
        sha256::digest_hex(material.as_bytes())
    }
}

/// Parse and validate model metadata supplied by the immutable private protocol.
///
/// Public code and schema pin the model identity, inference contract, artifact byte
/// counts and SHA-256 digests. Runtime validation then checks the local files.
pub(crate) fn parse_model_spec(value: &Value) -> Result<ModelSpec> {
    let spec: ModelSpec = serde_json::from_value(value.clone())
        .context("model specification does not match the frozen schema")?;
    if spec.id != FROZEN_MODEL_ID
        || spec.revision != FROZEN_MODEL_REVISION
        || spec.license != FROZEN_MODEL_LICENSE
        || spec.dimension != EXPECTED_DIMENSION
        || spec.max_tokens != EXPECTED_MAX_TOKENS
        || spec.query_prefix != FROZEN_QUERY_PREFIX
        || spec.passage_prefix != FROZEN_PASSAGE_PREFIX
        || spec.pooling != FROZEN_POOLING
        || spec.normalization != FROZEN_NORMALIZATION
        || spec.similarity != FROZEN_SIMILARITY
        || spec.dtype != FROZEN_DTYPE
        || spec.runtime != FROZEN_RUNTIME
        || spec.tokenizers != FROZEN_TOKENIZERS
    {
        bail!("model specification differs from the frozen local-embeddings-v1 contract");
    }
    if spec.artifacts.len() != EXPECTED_ARTIFACTS.len() {
        bail!("model specification must declare exactly three artifacts");
    }
    for (artifact, (name, bytes, sha256)) in spec.artifacts.iter().zip(EXPECTED_ARTIFACTS) {
        if artifact_basename(&artifact.file) != Some(name)
            || artifact.bytes != bytes
            || artifact.sha256 != sha256
            || !is_lower_hex(&artifact.sha256, 64)
        {
            bail!("model artifact {name} differs from the frozen protocol metadata");
        }
    }
    if artifact_basename(&spec.artifacts_sha256_file) != Some(FROZEN_ARTIFACTS_SHA256_FILE) {
        bail!("artifacts_sha256_file must reference {FROZEN_ARTIFACTS_SHA256_FILE}");
    }
    Ok(spec)
}

fn artifact_basename(value: &str) -> Option<&str> {
    Path::new(value).file_name().and_then(|name| name.to_str())
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn resolve_model_path(directory: &Path, configured: &str, name: &str) -> Result<PathBuf> {
    let configured = Path::new(configured);
    if configured.file_name().and_then(|value| value.to_str()) != Some(name) {
        bail!("model artifact must end with {name}");
    }
    if configured.is_absolute() {
        if configured.parent() != Some(directory) {
            bail!("model artifact {name} must share the frozen model directory");
        }
        Ok(configured.to_path_buf())
    } else if configured == Path::new(name) {
        Ok(directory.join(configured))
    } else {
        bail!("relative model artifact {name} must not escape or nest below its directory")
    }
}

fn artifact_for<'a>(spec: &'a ModelSpec, name: &str) -> Result<&'a ModelArtifact> {
    spec.artifacts
        .iter()
        .find(|artifact| artifact_basename(&artifact.file) == Some(name))
        .ok_or_else(|| anyhow!("model artifact {name} is missing from the frozen specification"))
}

/// Validate every artifact on disk: existence, regular file type, non-symlink,
/// exact byte count and SHA-256 digest, plus the `SHA256SUMS` manifest.
///
/// No network access is performed and no artifact is written or repaired.
pub(crate) fn validate_model_artifacts(spec: &ModelSpec, directory: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(directory)
        .with_context(|| format!("model directory {} is missing", directory.display()))?;
    if !metadata.is_dir() {
        bail!("model directory {} is not a directory", directory.display());
    }
    for artifact in &spec.artifacts {
        let name = artifact_basename(&artifact.file)
            .ok_or_else(|| anyhow!("model artifact filename is not UTF-8"))?;
        let path = resolve_model_path(directory, &artifact.file, name)?;
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("model artifact {} is missing", path.display()))?;
        if metadata.file_type().is_symlink() {
            bail!("model artifact {} must not be a symlink", path.display());
        }
        if !metadata.is_file() {
            bail!("model artifact {} is not a regular file", path.display());
        }
        if metadata.len() != artifact.bytes {
            bail!(
                "model artifact {} has {} bytes, expected {}",
                path.display(),
                metadata.len(),
                artifact.bytes
            );
        }
        let digest = file_sha256(&path)?;
        if digest != artifact.sha256 {
            bail!(
                "model artifact {} has an unexpected SHA-256",
                path.display()
            );
        }
    }
    validate_sha256sums(spec, directory)
}

fn validate_sha256sums(spec: &ModelSpec, directory: &Path) -> Result<()> {
    let path = resolve_model_path(
        directory,
        &spec.artifacts_sha256_file,
        FROZEN_ARTIFACTS_SHA256_FILE,
    )?;
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("model manifest {} is missing", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!(
            "model manifest {} must be a regular non-symlink file",
            path.display()
        );
    }
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read model manifest {}", path.display()))?;
    let mut listed: BTreeMap<String, String> = BTreeMap::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut fields = trimmed.split_whitespace();
        let (Some(digest), Some(name)) = (fields.next(), fields.next()) else {
            bail!("model manifest {} has an invalid line", path.display());
        };
        let name = name.trim_start_matches('*');
        listed.insert(name.to_owned(), digest.to_ascii_lowercase());
    }
    for artifact in &spec.artifacts {
        let name = artifact_basename(&artifact.file)
            .ok_or_else(|| anyhow!("model artifact filename is not UTF-8"))?;
        match listed.get(name) {
            Some(digest) if digest == &artifact.sha256 => {}
            _ => bail!(
                "model manifest {} does not pin the frozen digest for {name}",
                path.display()
            ),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Embedder
// ---------------------------------------------------------------------------

struct Embedder {
    tokenizer: Tokenizer,
    model: BertModel,
    device: Device,
    pad_token_id: u32,
    dimension: usize,
    query_prefix: String,
    passage_prefix: String,
}

impl Embedder {
    fn load(spec: &ModelSpec, directory: &Path) -> Result<Self> {
        let config_artifact = artifact_for(spec, "config.json")?;
        let config: BertConfig = serde_json::from_slice(&fs::read(resolve_model_path(
            directory,
            &config_artifact.file,
            "config.json",
        )?)?)
        .context("failed to parse the frozen model config.json")?;
        if config.hidden_size != spec.dimension {
            bail!(
                "model config hidden_size {} differs from the frozen dimension {}",
                config.hidden_size,
                spec.dimension
            );
        }

        if config.model_type.as_deref() != Some("bert") {
            bail!("frozen model config must declare model_type bert");
        }
        let tokenizer_artifact = artifact_for(spec, "tokenizer.json")?;
        let mut tokenizer = Tokenizer::from_file(resolve_model_path(
            directory,
            &tokenizer_artifact.file,
            "tokenizer.json",
        )?)
        .map_err(|error| anyhow!("failed to load the frozen tokenizer.json: {error}"))?;
        let pad_token_id = tokenizer
            .token_to_id("<pad>")
            .ok_or_else(|| anyhow!("frozen tokenizer does not define <pad>"))?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: spec.max_tokens,
                stride: 0,
                strategy: TruncationStrategy::LongestFirst,
                direction: TruncationDirection::Right,
            }))
            .map_err(|error| anyhow!("failed to configure tokenizer truncation: {error}"))?;

        let weights_artifact = artifact_for(spec, "model.safetensors")?;
        let weights_path =
            resolve_model_path(directory, &weights_artifact.file, "model.safetensors")?;
        let device = Device::Cpu;
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights_path], DType::F32, &device)
                .context("failed to map the frozen model.safetensors")?
        };
        let model =
            BertModel::load(vb, &config).context("failed to construct the frozen BERT encoder")?;

        Ok(Self {
            tokenizer,
            model,
            device,
            pad_token_id,
            dimension: spec.dimension,
            query_prefix: spec.query_prefix.clone(),
            passage_prefix: spec.passage_prefix.clone(),
        })
    }

    fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        self.embed_one(query, &self.query_prefix)
    }

    fn embed_passages(&self, passages: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embed(passages, &self.passage_prefix)
    }

    fn embed_one(&self, text: &str, prefix: &str) -> Result<Vec<f32>> {
        let mut vectors = self.embed(std::slice::from_ref(&text.to_owned()), prefix)?;
        vectors
            .pop()
            .ok_or_else(|| anyhow!("embedding pipeline returned no vector"))
    }

    fn embed(&self, texts: &[String], prefix: &str) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut vectors = Vec::with_capacity(texts.len());
        for batch in texts.chunks(EMBED_BATCH_SIZE) {
            let encodings = batch
                .iter()
                .map(|text| {
                    let prepared = format!("{prefix}{text}");
                    self.tokenizer
                        .encode(prepared, true)
                        .map_err(|error| anyhow!("tokenization failed: {error}"))
                })
                .collect::<Result<Vec<_>>>()?;
            let max_len = encodings
                .iter()
                .map(|encoding| encoding.get_ids().len())
                .max()
                .unwrap_or(0)
                .max(1);
            let batch_size = encodings.len();
            let mut ids = vec![self.pad_token_id; batch_size * max_len];
            let mut masks = vec![0u32; batch_size * max_len];
            let mut types = vec![0u32; batch_size * max_len];
            for (index, encoding) in encodings.iter().enumerate() {
                let length = encoding.get_ids().len();
                if length > max_len {
                    bail!("tokenized sequence exceeds the dynamic batch width");
                }
                let start = index * max_len;
                ids[start..start + length].copy_from_slice(encoding.get_ids());
                masks[start..start + length].copy_from_slice(encoding.get_attention_mask());
                types[start..start + length].copy_from_slice(encoding.get_type_ids());
            }
            let input_ids = Tensor::from_vec(ids, (batch_size, max_len), &self.device)?;
            let attention_mask =
                Tensor::from_vec(masks.clone(), (batch_size, max_len), &self.device)?;
            let token_type_ids = Tensor::from_vec(types, (batch_size, max_len), &self.device)?;
            let hidden = self
                .model
                .forward(&input_ids, &token_type_ids, Some(&attention_mask))?;
            let hidden = hidden
                .to_vec3::<f32>()
                .context("failed to materialize model hidden states")?;
            for (index, row) in hidden.iter().enumerate() {
                let start = index * max_len;
                let row_mask = &masks[start..start + max_len];
                let mut vector = mean_pool(row, row_mask)?;
                if vector.len() != self.dimension {
                    bail!(
                        "model returned {} dimensions, expected {}",
                        vector.len(),
                        self.dimension
                    );
                }
                l2_normalize(&mut vector, L2_EPSILON);
                vectors.push(vector);
            }
        }
        Ok(vectors)
    }
}

/// Attention-mask mean pooling over `hidden` rows where `mask == 1`.
fn mean_pool(hidden: &[Vec<f32>], mask: &[u32]) -> Result<Vec<f32>> {
    if hidden.is_empty() || hidden.len() != mask.len() {
        bail!("attention-mask mean pooling received inconsistent inputs");
    }
    let dimension = hidden[0].len();
    let mut pooled = vec![0f32; dimension];
    let mut count = 0f32;
    for (row, flag) in hidden.iter().zip(mask) {
        if row.len() != dimension {
            bail!("attention-mask mean pooling received ragged rows");
        }
        if *flag == 0 {
            continue;
        }
        count += 1.0;
        for (accumulator, value) in pooled.iter_mut().zip(row) {
            *accumulator += *value;
        }
    }
    if count > 0.0 {
        for value in &mut pooled {
            *value /= count;
        }
    }
    Ok(pooled)
}

/// L2 normalization with the frozen denominator floor of `1e-12`.
fn l2_normalize(vector: &mut [f32], epsilon: f32) {
    let sum_of_squares: f32 = vector.iter().map(|value| value * value).sum();
    let denominator = sum_of_squares.sqrt().max(epsilon);
    for value in vector.iter_mut() {
        *value /= denominator;
    }
}

// ---------------------------------------------------------------------------
// Binary index
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct IndexRecord {
    storage_key: String,
    chunk_hash: String,
    file_hash: String,
    path: String,
    heading: Option<String>,
    line_start: u64,
    line_end: u64,
    text: String,
    vector: Vec<f32>,
}

impl IndexRecord {
    fn from_chunk(chunk: Chunk, vector: Vec<f32>) -> Self {
        Self {
            storage_key: storage_key(&chunk),
            chunk_hash: chunk.chunk_hash,
            file_hash: chunk.file_hash,
            path: chunk.path,
            heading: chunk.heading,
            line_start: chunk.line_start as u64,
            line_end: chunk.line_end as u64,
            text: chunk.text,
            vector,
        }
    }

    fn to_chunk(&self) -> Chunk {
        Chunk {
            path: self.path.clone(),
            heading: self.heading.clone(),
            line_start: self.line_start as usize,
            line_end: self.line_end as usize,
            text: self.text.clone(),
            file_hash: self.file_hash.clone(),
            chunk_hash: self.chunk_hash.clone(),
        }
    }

    fn evidence(&self) -> IndexEvidence {
        IndexEvidence {
            storage_key: self.storage_key.clone(),
            chunk_hash: self.chunk_hash.clone(),
            file_hash: self.file_hash.clone(),
            path: self.path.clone(),
            heading: self.heading.clone(),
            line_start: self.line_start,
            line_end: self.line_end,
            text: self.text.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexEvidence {
    storage_key: String,
    chunk_hash: String,
    file_hash: String,
    path: String,
    heading: Option<String>,
    line_start: u64,
    line_end: u64,
    text: String,
}

#[derive(Debug, Clone)]
struct IndexMetadata {
    schema_version: String,
    parser_version: String,
    engine_config_sha256: String,
    model_fingerprint: String,
    corpus_fingerprint: String,
    root: String,
    dimension: u32,
}

fn storage_key(chunk: &Chunk) -> String {
    let identity = format!(
        "{}\0{}\0{}\0{}",
        chunk.path,
        chunk.heading.as_deref().unwrap_or_default(),
        chunk.line_start,
        chunk.line_end
    );
    format!(
        "{}:{}",
        chunk.chunk_hash,
        sha256::digest_hex(identity.as_bytes())
    )
}

fn expected_evidence(corpus: &Corpus) -> Vec<IndexEvidence> {
    corpus
        .chunks
        .iter()
        .map(|chunk| IndexEvidence {
            storage_key: storage_key(chunk),
            chunk_hash: chunk.chunk_hash.clone(),
            file_hash: chunk.file_hash.clone(),
            path: chunk.path.clone(),
            heading: chunk.heading.clone(),
            line_start: chunk.line_start as u64,
            line_end: chunk.line_end as u64,
            text: chunk.text.clone(),
        })
        .collect()
}

fn encode_index(metadata: &IndexMetadata, records: &[IndexRecord]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    out.extend_from_slice(INDEX_MAGIC);
    push_u32(&mut out, INDEX_FORMAT_VERSION);
    push_u32(&mut out, metadata.dimension);
    push_u64(&mut out, records.len() as u64);
    push_str(&mut out, &metadata.schema_version)?;
    push_str(&mut out, &metadata.parser_version)?;
    push_str(&mut out, &metadata.engine_config_sha256)?;
    push_str(&mut out, &metadata.model_fingerprint)?;
    push_str(&mut out, &metadata.corpus_fingerprint)?;
    push_str(&mut out, &metadata.root)?;
    for record in records {
        push_str(&mut out, &record.storage_key)?;
        push_str(&mut out, &record.chunk_hash)?;
        push_str(&mut out, &record.file_hash)?;
        push_str(&mut out, &record.path)?;
        match &record.heading {
            Some(heading) => {
                out.push(1);
                push_str(&mut out, heading)?;
            }
            None => out.push(0),
        }
        push_u64(&mut out, record.line_start);
        push_u64(&mut out, record.line_end);
        push_str(&mut out, &record.text)?;
        if record.vector.len() != metadata.dimension as usize {
            bail!("index record vector has an unexpected dimension");
        }
        for value in &record.vector {
            if !value.is_finite() {
                bail!("index record vector contains a non-finite value");
            }
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    let digest = sha256::digest_hex(&out);
    out.extend_from_slice(digest.as_bytes());
    Ok(out)
}

fn decode_index(bytes: &[u8]) -> Result<(IndexMetadata, Vec<IndexRecord>)> {
    let minimum = INDEX_MAGIC.len() + 4 + 4 + 8 + 64;
    if bytes.len() < minimum {
        bail!("embeddings index is truncated");
    }
    let (body, digest_bytes) = bytes.split_at(bytes.len() - 64);
    let expected =
        std::str::from_utf8(digest_bytes).context("embeddings index digest is not UTF-8")?;
    if sha256::digest_hex(body) != expected {
        bail!("embeddings index checksum mismatch");
    }
    let mut reader = Reader::new(body);
    if reader.take(INDEX_MAGIC.len())? != INDEX_MAGIC {
        bail!("embeddings index magic header mismatch");
    }
    let format_version = reader.u32()?;
    if format_version != INDEX_FORMAT_VERSION {
        bail!("unsupported embeddings index format version {format_version}");
    }
    let dimension = reader.u32()?;
    let record_count = reader.u64()?;
    let metadata = IndexMetadata {
        schema_version: reader.string()?,
        parser_version: reader.string()?,
        engine_config_sha256: reader.string()?,
        model_fingerprint: reader.string()?,
        corpus_fingerprint: reader.string()?,
        root: reader.string()?,
        dimension,
    };
    if dimension == 0 || dimension as usize != EXPECTED_DIMENSION {
        bail!("embeddings index declares an unsupported dimension {dimension}");
    }
    let record_count =
        usize::try_from(record_count).context("embeddings index record count overflow")?;
    let minimum_record_bytes = (dimension as usize)
        .checked_mul(std::mem::size_of::<f32>())
        .ok_or_else(|| anyhow!("embeddings index dimension overflows record size"))?;
    if record_count > reader.remaining() / minimum_record_bytes {
        bail!("embeddings index record count exceeds the available bytes");
    }
    let mut records = Vec::with_capacity(record_count);
    for _ in 0..record_count {
        let storage_key = reader.string()?;
        let chunk_hash = reader.string()?;
        let file_hash = reader.string()?;
        let path = reader.string()?;
        let heading = match reader.u8()? {
            0 => None,
            1 => Some(reader.string()?),
            other => bail!("invalid embeddings index heading marker {other}"),
        };
        let line_start = reader.u64()?;
        let line_end = reader.u64()?;
        let text = reader.string()?;
        let vector = reader.f32_vec(dimension as usize)?;
        records.push(IndexRecord {
            storage_key,
            chunk_hash,
            file_hash,
            path,
            heading,
            line_start,
            line_end,
            text,
            vector,
        });
    }
    if !reader.is_empty() {
        bail!("embeddings index has trailing bytes");
    }
    Ok((metadata, records))
}

fn validate_index(
    index_path: &Path,
    corpus: &Corpus,
    engine_config_sha256: &str,
    model_fingerprint: &str,
) -> Result<()> {
    let bytes = fs::read(index_path)
        .with_context(|| format!("failed to read embeddings index {}", index_path.display()))?;
    let (metadata, records) = decode_index(&bytes)?;
    let expected_root = corpus.root.to_string_lossy().into_owned();
    for (label, actual, expected) in [
        (
            "schema_version",
            metadata.schema_version.as_str(),
            INDEX_SCHEMA_VERSION,
        ),
        (
            "parser_version",
            metadata.parser_version.as_str(),
            PARSER_VERSION,
        ),
        (
            "engine_config_sha256",
            metadata.engine_config_sha256.as_str(),
            engine_config_sha256,
        ),
        (
            "model_fingerprint",
            metadata.model_fingerprint.as_str(),
            model_fingerprint,
        ),
        (
            "corpus_fingerprint",
            metadata.corpus_fingerprint.as_str(),
            corpus_fingerprint(&corpus.file_hashes).as_str(),
        ),
        ("root", metadata.root.as_str(), expected_root.as_str()),
    ] {
        if actual != expected {
            bail!("embeddings index metadata {label} does not match the current contract");
        }
    }
    if records.len() != corpus.chunks.len() {
        bail!("embeddings index record count does not match the current corpus");
    }
    let mut actual: Vec<IndexEvidence> = Vec::with_capacity(records.len());
    for record in &records {
        validate_record(record, metadata.dimension as usize)?;
        actual.push(record.evidence());
    }
    let mut expected = expected_evidence(corpus);
    actual.sort_by(compare_evidence);
    expected.sort_by(compare_evidence);
    if actual != expected {
        bail!("embeddings index evidence differs from the current corpus");
    }
    Ok(())
}

fn validate_record(record: &IndexRecord, dimension: usize) -> Result<()> {
    if record.vector.len() != dimension {
        bail!("embeddings index record vector has an unexpected dimension");
    }
    if blake3::hash(record.text.as_bytes()).to_hex().to_string() != record.chunk_hash {
        bail!("embeddings index record chunk hash does not match its text");
    }
    let chunk = record.to_chunk();
    if storage_key(&chunk) != record.storage_key {
        bail!("embeddings index record storage key is not canonical");
    }
    let mut sum = 0f64;
    for value in &record.vector {
        if !value.is_finite() {
            bail!("embeddings index record vector contains a non-finite value");
        }
        sum += (*value as f64) * (*value as f64);
    }
    if (sum.sqrt() - 1.0).abs() > 1e-3 {
        bail!("embeddings index record vector is not L2 normalized");
    }
    Ok(())
}

fn compare_evidence(left: &IndexEvidence, right: &IndexEvidence) -> std::cmp::Ordering {
    left.path
        .cmp(&right.path)
        .then_with(|| left.line_start.cmp(&right.line_start))
        .then_with(|| left.storage_key.cmp(&right.storage_key))
}

fn validate_update_source(
    index_path: &Path,
    corpus: &Corpus,
    engine_config_sha256: &str,
    model_fingerprint: &str,
) -> Result<()> {
    let bytes = fs::read(index_path)
        .with_context(|| format!("failed to read embeddings index {}", index_path.display()))?;
    let (metadata, _) = decode_index(&bytes)?;
    for (label, actual, expected) in [
        (
            "schema_version",
            metadata.schema_version.as_str(),
            INDEX_SCHEMA_VERSION,
        ),
        (
            "parser_version",
            metadata.parser_version.as_str(),
            PARSER_VERSION,
        ),
        (
            "engine_config_sha256",
            metadata.engine_config_sha256.as_str(),
            engine_config_sha256,
        ),
        (
            "model_fingerprint",
            metadata.model_fingerprint.as_str(),
            model_fingerprint,
        ),
        (
            "root",
            metadata.root.as_str(),
            corpus.root.to_string_lossy().as_ref(),
        ),
    ] {
        if actual != expected {
            bail!("embeddings index metadata {label} is incompatible with the update source");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Index lifecycle
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct IndexCacheState {
    files: usize,
    chunks: usize,
    corpus_fingerprint: String,
}

fn index_state(corpus: &Corpus) -> IndexCacheState {
    IndexCacheState {
        files: corpus.files,
        chunks: corpus.chunks.len(),
        corpus_fingerprint: corpus_fingerprint(&corpus.file_hashes),
    }
}

pub(crate) struct EmbeddingsIndex {
    root: PathBuf,
    index_path: PathBuf,
    engine_config_sha256: String,
    model_fingerprint: String,
    embedder: Arc<Embedder>,
    state: Mutex<IndexCacheState>,
}

impl EmbeddingsIndex {
    pub(crate) fn open_existing(
        root: &Path,
        corpus: &Corpus,
        engine_config_sha256: &str,
        model_spec: &ModelSpec,
        model_directory: &Path,
    ) -> Result<Self> {
        let model_fingerprint = model_spec.fingerprint();
        validate_model_artifacts(model_spec, model_directory)?;
        let embedder = Arc::new(Embedder::load(model_spec, model_directory)?);
        let index_path = index_path_for_root(root)?;
        validate_index(
            &index_path,
            corpus,
            engine_config_sha256,
            &model_fingerprint,
        )?;
        Ok(Self::at_path(
            corpus,
            index_path,
            engine_config_sha256,
            model_fingerprint,
            embedder,
        ))
    }

    pub(crate) fn prepare_fresh(
        root: &Path,
        corpus: &Corpus,
        engine_config_sha256: &str,
        model_spec: &ModelSpec,
        model_directory: &Path,
    ) -> Result<(Self, IndexingMetrics)> {
        let model_fingerprint = model_spec.fingerprint();
        validate_model_artifacts(model_spec, model_directory)?;
        let embedder = Arc::new(Embedder::load(model_spec, model_directory)?);
        let index_path = index_path_for_root(root)?;
        if let Some(parent) = index_path.parent() {
            create_private_directory(parent)?;
        }
        remove_fresh_index_files(&index_path)?;
        let started = Instant::now();
        build_atomic(
            &index_path,
            root,
            corpus,
            engine_config_sha256,
            &model_fingerprint,
            &embedder,
        )?;
        let full_build_ms = elapsed_ms(started);
        validate_index(
            &index_path,
            corpus,
            engine_config_sha256,
            &model_fingerprint,
        )?;
        let runtime_checks = BTreeMap::from([
            ("model_artifacts_sha256_verified".to_owned(), true),
            ("candle_cpu_f32".to_owned(), true),
            ("tokenizer_truncation_right_512".to_owned(), true),
            ("attention_mask_mean_pooling".to_owned(), true),
        ]);
        Ok((
            Self::at_path(
                corpus,
                index_path,
                engine_config_sha256,
                model_fingerprint,
                embedder,
            ),
            IndexingMetrics {
                full_build_ms: Some(full_build_ms),
                incremental_steps: Vec::new(),
                rebuild_succeeded: true,
                corruption_detected: false,
                runtime_checks,
            },
        ))
    }

    pub(crate) fn search(&self, request: SearchRequest) -> Result<(SearchResponse, SearchTimings)> {
        self.search_with_recovery(request, false)
    }

    /// Search the exact vector scan, rebuilding atomically on corruption.
    ///
    /// A forced rebuild failure fails closed: no lexical fallback and no partial
    /// results are returned.
    pub(crate) fn search_with_recovery(
        &self,
        mut request: SearchRequest,
        force_rebuild_failure: bool,
    ) -> Result<(SearchResponse, SearchTimings)> {
        validate_request(&request)?;
        if meaningful_query_tokens(&request.query).is_empty() {
            bail!("query must contain at least one letter or number");
        }
        let request_root = request.root.canonicalize().with_context(|| {
            format!(
                "failed to resolve embeddings request root {}",
                request.root.display()
            )
        })?;
        if request_root != self.root {
            bail!("embeddings request root differs from the bound index root");
        }
        request.root = self.root.clone();
        match self.search_index(request.clone()) {
            Ok(result) => Ok(result),
            Err(index_error) => {
                let corpus = corpus::load(&self.root)?;
                let rebuild = if force_rebuild_failure {
                    Err(anyhow!("forced atomic rebuild failure"))
                } else {
                    rebuild_at(
                        &self.index_path,
                        &self.root,
                        &corpus,
                        &self.engine_config_sha256,
                        &self.model_fingerprint,
                        &self.embedder,
                    )
                };
                if let Err(rebuild_error) = rebuild {
                    bail!(
                        "embeddings index failed ({index_error:#}); atomic rebuild failed ({rebuild_error:#}); fail-closed"
                    );
                }
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| anyhow!("embeddings index state mutex is poisoned"))?;
                *state = index_state(&corpus);
                drop(state);
                self.search_index(request)
            }
        }
    }

    fn search_index(&self, request: SearchRequest) -> Result<(SearchResponse, SearchTimings)> {
        let total_started = Instant::now();
        let lookup_started = Instant::now();
        let (_, current_hashes) = corpus::file_hashes(&self.root)?;
        let current_fingerprint = corpus_fingerprint(&current_hashes);
        let (files, chunks) = {
            let state = self
                .state
                .lock()
                .map_err(|_| anyhow!("embeddings index state mutex is poisoned"))?;
            if state.corpus_fingerprint != current_fingerprint
                || state.files != current_hashes.len()
            {
                bail!("embeddings index corpus fingerprint is stale");
            }
            (state.files, state.chunks)
        };
        let bytes = fs::read(&self.index_path).with_context(|| {
            format!(
                "failed to read embeddings index {}",
                self.index_path.display()
            )
        })?;
        let (metadata, records) = decode_index(&bytes)?;
        if metadata.corpus_fingerprint != current_fingerprint
            || metadata.engine_config_sha256 != self.engine_config_sha256
            || metadata.model_fingerprint != self.model_fingerprint
            || metadata.schema_version != INDEX_SCHEMA_VERSION
            || metadata.parser_version != PARSER_VERSION
            || metadata.root != self.root.to_string_lossy()
            || metadata.dimension as usize != EXPECTED_DIMENSION
            || records.len() != chunks
        {
            bail!("embeddings index metadata is stale");
        }
        for record in &records {
            validate_record(record, EXPECTED_DIMENSION)?;
        }
        let lookup_ms = elapsed_ms(lookup_started);

        let ranking_started = Instant::now();
        let query_vector = self.embedder.embed_query(&request.query)?;
        let query_tokens = meaningful_query_tokens(&request.query);
        let candidates_examined = records.len();
        let selected = rank_for_request(
            &records,
            &query_vector,
            request.limit,
            request.max_results_per_path,
        )?;
        let ranking_ms = elapsed_ms(ranking_started);

        let excerpt_started = Instant::now();
        let results = selected
            .into_iter()
            .enumerate()
            .map(|(position, (raw_rank, (index, score)))| {
                let record = &records[index];
                let chunk = record.to_chunk();
                let (text, line_start, line_end) =
                    excerpt(&chunk, &query_tokens, request.max_excerpt_chars);
                SearchResult {
                    rank: position + 1,
                    raw_rank,
                    path: record.path.clone(),
                    heading: record.heading.clone(),
                    line_start,
                    line_end,
                    excerpt: text,
                    file_hash: record.file_hash.clone(),
                    chunk_hash: record.chunk_hash.clone(),
                    score: round_score(score as f64),
                    matched_terms: matched_terms(&query_tokens, record),
                }
            })
            .collect();
        let excerpt_ms = elapsed_ms(excerpt_started);

        Ok((
            SearchResponse {
                schema_version: SCHEMA_VERSION,
                engine: EMBEDDINGS_ENGINE,
                query: request.query,
                root: self.root.to_string_lossy().into_owned(),
                corpus: CorpusSummary {
                    files,
                    chunks: records.len(),
                },
                selection: SearchSelection {
                    max_results_per_path: request.max_results_per_path,
                },
                results,
            },
            SearchTimings {
                lookup_ms,
                ranking_ms,
                excerpt_ms,
                total_ms: elapsed_ms(total_started),
                candidates_examined,
            },
        ))
    }

    pub(crate) fn index_path(&self) -> &Path {
        &self.index_path
    }

    pub(crate) fn verify_current(&self, corpus: &Corpus) -> Result<()> {
        validate_index(
            &self.index_path,
            corpus,
            &self.engine_config_sha256,
            &self.model_fingerprint,
        )
    }

    fn at_path(
        corpus: &Corpus,
        index_path: PathBuf,
        engine_config_sha256: &str,
        model_fingerprint: String,
        embedder: Arc<Embedder>,
    ) -> Self {
        Self {
            root: corpus.root.clone(),
            index_path,
            engine_config_sha256: engine_config_sha256.to_owned(),
            model_fingerprint,
            embedder,
            state: Mutex::new(index_state(corpus)),
        }
    }
}

pub(crate) fn index_path_for_root(root: &Path) -> Result<PathBuf> {
    Ok(cache_path(root)?.with_file_name(INDEX_FILE))
}

// ---------------------------------------------------------------------------
// Ranking
// ---------------------------------------------------------------------------

fn dot_product(left: &[f32], right: &[f32]) -> Result<f32> {
    if left.len() != right.len() {
        bail!("vector dimension mismatch during exact scan");
    }
    Ok(left
        .iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum())
}

/// Exact scan over all vectors, ordered by dot product descending with path/line
/// tie-breakers, then truncated to the candidate depth. Returns empty when the
/// best score is below the frozen abstention threshold.
fn rank_records(
    records: &[IndexRecord],
    query: &[f32],
    candidate_depth: usize,
    threshold: f32,
) -> Result<Vec<(usize, f32)>> {
    let mut scored = Vec::with_capacity(records.len());
    for (index, record) in records.iter().enumerate() {
        scored.push((index, dot_product(query, &record.vector)?));
    }
    scored.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| records[left.0].path.cmp(&records[right.0].path))
            .then_with(|| records[left.0].line_start.cmp(&records[right.0].line_start))
    });
    let best = scored.first().map(|(_, score)| *score).unwrap_or(f32::MIN);
    if best < threshold {
        return Ok(Vec::new());
    }
    scored.truncate(candidate_depth);
    Ok(scored)
}

fn rank_for_request(
    records: &[IndexRecord],
    query: &[f32],
    limit: usize,
    max_results_per_path: Option<usize>,
) -> Result<Vec<(usize, (usize, f32))>> {
    let scored = rank_records(records, query, CANDIDATE_DEPTH, ABSTENTION_THRESHOLD)?;
    let mut selected = Vec::with_capacity(limit);
    let mut path_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (position, (index, score)) in scored.into_iter().enumerate() {
        let path = records[index].path.as_str();
        if let Some(maximum) = max_results_per_path {
            let count = path_counts.entry(path).or_insert(0usize);
            if *count >= maximum {
                continue;
            }
            *count += 1;
        }
        selected.push((position + 1, (index, score)));
        if selected.len() == limit {
            break;
        }
    }
    Ok(selected)
}

fn matched_terms(query_tokens: &[String], record: &IndexRecord) -> Vec<String> {
    let mut indexed: HashSet<String> = tokens(&record.text).into_iter().collect();
    indexed.extend(tokens(record.heading.as_deref().unwrap_or_default()));
    indexed.extend(tokens(&record.path));
    query_tokens
        .iter()
        .filter(|term| indexed.contains(*term))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Atomic persistence
// ---------------------------------------------------------------------------

fn build_atomic(
    index_path: &Path,
    root: &Path,
    corpus: &Corpus,
    engine_config_sha256: &str,
    model_fingerprint: &str,
    embedder: &Embedder,
) -> Result<()> {
    let parent = index_path
        .parent()
        .ok_or_else(|| anyhow!("embeddings index path has no parent"))?;
    create_private_directory(parent)?;
    let temp_path = temporary_index_path(index_path)?;
    if temp_path.exists() {
        bail!(
            "temporary embeddings index already exists: {}",
            temp_path.display()
        );
    }
    let result = (|| -> Result<()> {
        let texts: Vec<String> = corpus
            .chunks
            .iter()
            .map(|chunk| chunk.text.clone())
            .collect();
        let vectors = embedder.embed_passages(&texts)?;
        if vectors.len() != corpus.chunks.len() {
            bail!("embedding pipeline did not return one vector per chunk");
        }
        let records: Vec<IndexRecord> = corpus
            .chunks
            .iter()
            .cloned()
            .zip(vectors)
            .map(|(chunk, vector)| IndexRecord::from_chunk(chunk, vector))
            .collect();
        let metadata = IndexMetadata {
            schema_version: INDEX_SCHEMA_VERSION.to_owned(),
            parser_version: PARSER_VERSION.to_owned(),
            engine_config_sha256: engine_config_sha256.to_owned(),
            model_fingerprint: model_fingerprint.to_owned(),
            corpus_fingerprint: corpus_fingerprint(&corpus.file_hashes),
            root: corpus.root.to_string_lossy().into_owned(),
            dimension: EXPECTED_DIMENSION as u32,
        };
        let bytes = encode_index(&metadata, &records)?;
        write_private_file(&temp_path, &bytes)?;
        fs::rename(&temp_path, index_path).with_context(|| {
            format!(
                "failed to atomically install embeddings index {}",
                index_path.display()
            )
        })?;
        fs::set_permissions(index_path, fs::Permissions::from_mode(0o600))?;
        sync_directory(parent)?;
        let _ = root;
        Ok(())
    })();
    if result.is_err() && temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn update_atomic(
    index_path: &Path,
    root: &Path,
    corpus: &Corpus,
    engine_config_sha256: &str,
    model_fingerprint: &str,
    embedder: &Embedder,
) -> Result<()> {
    validate_update_source(index_path, corpus, engine_config_sha256, model_fingerprint)?;
    let parent = index_path
        .parent()
        .ok_or_else(|| anyhow!("embeddings index path has no parent"))?;
    let temp_path = temporary_index_path(index_path)?;
    if temp_path.exists() {
        bail!(
            "temporary embeddings index already exists: {}",
            temp_path.display()
        );
    }
    let result = (|| -> Result<()> {
        let bytes = fs::read(index_path)?;
        let (_, existing) = decode_index(&bytes)?;
        let mut existing_by_key: BTreeMap<String, IndexRecord> = existing
            .into_iter()
            .map(|record| (record.storage_key.clone(), record))
            .collect();
        let mut reused: Vec<Option<IndexRecord>> = Vec::with_capacity(corpus.chunks.len());
        let mut changed_texts = Vec::new();
        for chunk in &corpus.chunks {
            let key = storage_key(chunk);
            match existing_by_key.remove(&key) {
                Some(record)
                    if record.file_hash == chunk.file_hash
                        && record.chunk_hash == chunk.chunk_hash
                        && record.text == chunk.text =>
                {
                    reused.push(Some(record));
                }
                _ => {
                    changed_texts.push(chunk.text.clone());
                    reused.push(None);
                }
            }
        }
        let vectors = embedder.embed_passages(&changed_texts)?;
        if vectors.len() != changed_texts.len() {
            bail!("embedding pipeline did not return one vector per changed chunk");
        }
        let mut vector_iter = vectors.into_iter();
        let mut records = Vec::with_capacity(corpus.chunks.len());
        for (chunk, reused_record) in corpus.chunks.iter().zip(reused) {
            match reused_record {
                Some(record) => records.push(record),
                None => {
                    let vector = vector_iter
                        .next()
                        .ok_or_else(|| anyhow!("missing embedding for a changed chunk"))?;
                    records.push(IndexRecord::from_chunk(chunk.clone(), vector));
                }
            }
        }
        let metadata = IndexMetadata {
            schema_version: INDEX_SCHEMA_VERSION.to_owned(),
            parser_version: PARSER_VERSION.to_owned(),
            engine_config_sha256: engine_config_sha256.to_owned(),
            model_fingerprint: model_fingerprint.to_owned(),
            corpus_fingerprint: corpus_fingerprint(&corpus.file_hashes),
            root: corpus.root.to_string_lossy().into_owned(),
            dimension: EXPECTED_DIMENSION as u32,
        };
        let bytes = encode_index(&metadata, &records)?;
        write_private_file(&temp_path, &bytes)?;
        fs::rename(&temp_path, index_path)?;
        fs::set_permissions(index_path, fs::Permissions::from_mode(0o600))?;
        sync_directory(parent)?;
        let _ = root;
        Ok(())
    })();
    if result.is_err() && temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn rebuild_at(
    index_path: &Path,
    root: &Path,
    corpus: &Corpus,
    engine_config_sha256: &str,
    model_fingerprint: &str,
    embedder: &Embedder,
) -> Result<IndexState> {
    let corruption_detected = index_path.exists()
        && validate_index(index_path, corpus, engine_config_sha256, model_fingerprint).is_err();
    build_atomic(
        index_path,
        root,
        corpus,
        engine_config_sha256,
        model_fingerprint,
        embedder,
    )?;
    validate_index(index_path, corpus, engine_config_sha256, model_fingerprint)?;
    Ok(IndexState {
        corruption_detected,
        rebuild_succeeded: true,
    })
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("failed to create embeddings index {}", path.display()))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn remove_fresh_index_files(index_path: &Path) -> Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(index_path) {
        if metadata.is_dir() {
            bail!("embeddings index path is unexpectedly a directory");
        }
        fs::remove_file(index_path)?;
    }
    let parent = index_path
        .parent()
        .ok_or_else(|| anyhow!("embeddings index path has no parent"))?;
    let name = index_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("embeddings index filename is not UTF-8"))?;
    let prefix = format!(".{name}.tmp-");
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.is_dir() {
                bail!("temporary embeddings index path is unexpectedly a directory");
            }
            fs::remove_file(entry.path())?;
        }
    }
    sync_directory(parent)
}

fn temporary_index_path(index_path: &Path) -> Result<PathBuf> {
    let name = index_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("embeddings index filename is not UTF-8"))?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(index_path.with_file_name(format!(".{name}.tmp-{}-{nonce}", std::process::id())))
}

struct TemporaryDirectoryCleanup(PathBuf);

impl Drop for TemporaryDirectoryCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn unique_temp_directory(label: &str) -> Result<PathBuf> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = env::temp_dir().join(format!(
        "docs-search-embeddings-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::DirBuilder::new().mode(0o700).create(&path)?;
    Ok(path)
}

fn sync_directory(path: &Path) -> Result<()> {
    OpenOptions::new().read(true).open(path)?.sync_all()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Incremental workload
// ---------------------------------------------------------------------------

pub(crate) fn run_incremental_workload(
    index: &EmbeddingsIndex,
    source_root: &Path,
    queries: &[String],
) -> Result<Vec<IncrementalStep>> {
    let engine_config_sha256 = index.engine_config_sha256.as_str();
    let model_fingerprint = index.model_fingerprint.as_str();
    let embedder = &index.embedder;
    let source = corpus::load(source_root)?;
    let selected: Vec<_> = source.file_hashes.keys().take(4).cloned().collect();
    if selected.len() != 4 {
        bail!("embeddings incremental workload requires at least four selected Markdown files");
    }
    let directory = unique_temp_directory("incremental")?;
    let _cleanup = TemporaryDirectoryCleanup(directory.clone());
    let root = directory.join("corpus");
    fs::create_dir(&root)?;
    for relative in &selected {
        copy_selected_file(source_root, &root, relative, relative)?;
    }
    let update_index = directory.join(INDEX_FILE);
    let full_index = directory.join("full-embeddings.bin");
    let initial = corpus::load(&root)?;
    build_atomic(
        &update_index,
        &root,
        &initial,
        engine_config_sha256,
        model_fingerprint,
        embedder,
    )?;

    let mut steps = Vec::new();
    for operation in ["add", "modify", "rename", "remove"] {
        apply_incremental_operation(operation, &root, &selected)?;
        let current = corpus::load(&root)?;
        let started = Instant::now();
        update_atomic(
            &update_index,
            &root,
            &current,
            engine_config_sha256,
            model_fingerprint,
            embedder,
        )?;
        let elapsed_ms = elapsed_ms(started);
        validate_index(
            &update_index,
            &current,
            engine_config_sha256,
            model_fingerprint,
        )?;
        if full_index.exists() {
            fs::remove_file(&full_index)?;
        }
        build_atomic(
            &full_index,
            &root,
            &current,
            engine_config_sha256,
            model_fingerprint,
            embedder,
        )?;
        let stale_results = compare_indexes(
            &update_index,
            &full_index,
            &current,
            engine_config_sha256,
            model_fingerprint,
            embedder,
            queries,
        )?;
        steps.push(IncrementalStep {
            operation: operation.to_owned(),
            elapsed_ms,
            equivalent_to_full_rebuild: stale_results == 0,
            stale_results,
        });
    }
    fs::remove_dir_all(&directory)?;
    Ok(steps)
}

#[allow(clippy::too_many_arguments)]
fn compare_indexes(
    update_path: &Path,
    full_path: &Path,
    corpus: &Corpus,
    engine_config_sha256: &str,
    model_fingerprint: &str,
    embedder: &Embedder,
    queries: &[String],
) -> Result<usize> {
    validate_index(update_path, corpus, engine_config_sha256, model_fingerprint)?;
    validate_index(full_path, corpus, engine_config_sha256, model_fingerprint)?;
    let (_, update_records) = decode_index(&fs::read(update_path)?)?;
    let (_, full_records) = decode_index(&fs::read(full_path)?)?;
    let mut stale_results = 0usize;

    let mut update_evidence: Vec<IndexEvidence> =
        update_records.iter().map(IndexRecord::evidence).collect();
    let mut full_evidence: Vec<IndexEvidence> =
        full_records.iter().map(IndexRecord::evidence).collect();
    update_evidence.sort_by(compare_evidence);
    full_evidence.sort_by(compare_evidence);
    if update_evidence != full_evidence {
        stale_results += 1;
    }

    let full_by_key: BTreeMap<&str, &IndexRecord> = full_records
        .iter()
        .map(|record| (record.storage_key.as_str(), record))
        .collect();
    for record in &update_records {
        match full_by_key.get(record.storage_key.as_str()) {
            Some(other) if vectors_close(&record.vector, &other.vector, 1e-5) => {}
            _ => {
                stale_results += 1;
                break;
            }
        }
    }

    for query in queries {
        let query_vector = embedder.embed_query(query)?;
        let update_ranking = ranking_projection(&update_records, &query_vector)?;
        let full_ranking = ranking_projection(&full_records, &query_vector)?;
        if update_ranking != full_ranking {
            stale_results += 1;
        }
    }
    Ok(stale_results)
}

fn ranking_projection(records: &[IndexRecord], query: &[f32]) -> Result<Vec<(String, f64)>> {
    let scored = rank_records(records, query, CANDIDATE_DEPTH, ABSTENTION_THRESHOLD)?;
    Ok(scored
        .into_iter()
        .map(|(index, score)| {
            (
                records[index].storage_key.clone(),
                round_score(score as f64),
            )
        })
        .collect())
}

fn vectors_close(left: &[f32], right: &[f32], tolerance: f32) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| (left - right).abs() <= tolerance)
}

fn apply_incremental_operation(operation: &str, root: &Path, selected: &[String]) -> Result<()> {
    match operation {
        "add" => {
            let destination = root.join("docs/__docs-search-bakeoff-added.md");
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)?;
            file.write_all(
                b"# docs-search incremental addition\n\nDeterministic embeddings fixture.\n",
            )?;
            file.sync_all()?;
            Ok(())
        }
        "modify" => {
            let path = root.join(&selected[1]);
            let mut file = OpenOptions::new().append(true).open(path)?;
            file.write_all(
                b"\n\n## docs-search incremental modification\n\nDeterministic embeddings fixture.\n",
            )?;
            file.sync_all()?;
            Ok(())
        }
        "rename" => {
            let destination = root.join("docs/__docs-search-bakeoff-renamed.md");
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(root.join(&selected[2]), destination)?;
            Ok(())
        }
        "remove" => {
            fs::remove_file(root.join(&selected[3]))?;
            Ok(())
        }
        _ => bail!("unsupported embeddings incremental operation {operation}"),
    }
}

fn copy_selected_file(
    source_root: &Path,
    destination_root: &Path,
    source_relative: &str,
    destination_relative: &str,
) -> Result<()> {
    let destination = destination_root.join(destination_relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source_root.join(source_relative), &destination)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Fault injection
// ---------------------------------------------------------------------------

pub(crate) fn fault_injection_passed(
    index: &EmbeddingsIndex,
    model_spec: &ModelSpec,
    model_directory: &Path,
) -> bool {
    fault_injection(index, model_spec, model_directory).is_ok()
}

fn fault_injection(
    source_index: &EmbeddingsIndex,
    model_spec: &ModelSpec,
    model_directory: &Path,
) -> Result<()> {
    let directory = unique_temp_directory("fault")?;
    let result = (|| -> Result<()> {
        // Missing model artifacts must fail closed before any inference.
        let missing = directory.join("missing-model");
        fs::create_dir(&missing)?;
        let mut missing_spec = model_spec.clone();
        for artifact in &mut missing_spec.artifacts {
            artifact.file = artifact_basename(&artifact.file)
                .ok_or_else(|| anyhow!("model artifact filename is not UTF-8"))?
                .to_owned();
        }
        missing_spec.artifacts_sha256_file = FROZEN_ARTIFACTS_SHA256_FILE.to_owned();
        if validate_model_artifacts(&missing_spec, &missing).is_ok() {
            bail!("missing model artifacts did not fail closed");
        }
        validate_model_artifacts(model_spec, model_directory)?;

        // Corruption must trigger an atomic rebuild, and a forced rebuild failure
        // must fail closed without lexical fallback or partial results.
        let root = source_index.root.as_path();
        let engine_config_sha256 = source_index.engine_config_sha256.as_str();
        let model_fingerprint = source_index.model_fingerprint.as_str();
        let embedder = Arc::clone(&source_index.embedder);
        let corpus = corpus::load(root)?;
        let index_path = directory.join(INDEX_FILE);
        build_atomic(
            &index_path,
            root,
            &corpus,
            engine_config_sha256,
            model_fingerprint,
            &embedder,
        )?;
        let index = EmbeddingsIndex::at_path(
            &corpus,
            index_path.clone(),
            engine_config_sha256,
            model_fingerprint.to_owned(),
            embedder,
        );

        corrupt_index(&index_path)?;
        index.search_with_recovery(request(root), false)?;
        let current = corpus::load(root)?;
        validate_index(
            &index_path,
            &current,
            engine_config_sha256,
            model_fingerprint,
        )
        .context("automatic embeddings corruption recovery did not install a valid index")?;

        corrupt_index(&index_path)?;
        let error = index
            .search_with_recovery(request(root), true)
            .expect_err("forced embeddings rebuild failure must fail closed");
        if !error.to_string().contains("fail-closed") {
            bail!("forced embeddings rebuild failure did not report fail-closed behavior");
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&directory);
    result
}

fn request(root: &Path) -> SearchRequest {
    SearchRequest {
        root: root.to_path_buf(),
        query: "fault injection".to_owned(),
        limit: 5,
        max_excerpt_chars: 1_200,
        max_results_per_path: None,
    }
}

fn corrupt_index(index_path: &Path) -> Result<()> {
    let mut bytes = fs::read(index_path)?;
    let last = bytes
        .len()
        .checked_sub(1)
        .ok_or_else(|| anyhow!("embeddings index is empty"))?;
    bytes[last] ^= 0xff;
    fs::write(index_path, bytes)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Binary helpers
// ---------------------------------------------------------------------------

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_str(out: &mut Vec<u8>, value: &str) -> Result<()> {
    let length =
        u32::try_from(value.len()).context("string is too large for the embeddings index")?;
    push_u32(out, length);
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| anyhow!("embeddings index offset overflow"))?;
        if end > self.bytes.len() {
            bail!("embeddings index is truncated");
        }
        let slice = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn u64(&mut self) -> Result<u64> {
        let bytes = self.take(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn string(&mut self) -> Result<String> {
        let length = self.u32()? as usize;
        let bytes = self.take(length)?;
        Ok(std::str::from_utf8(bytes)
            .context("embeddings index string is not UTF-8")?
            .to_owned())
    }

    fn f32_vec(&mut self, length: usize) -> Result<Vec<f32>> {
        let byte_length = length
            .checked_mul(4)
            .ok_or_else(|| anyhow!("embeddings vector length overflow"))?;
        let bytes = self.take(byte_length)?;
        let mut vector = Vec::with_capacity(length);
        for chunk in bytes.chunks_exact(4) {
            vector.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        Ok(vector)
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    fn is_empty(&self) -> bool {
        self.remaining() == 0
    }
}

// ---------------------------------------------------------------------------
// Streaming SHA-256 for large frozen artifacts
// ---------------------------------------------------------------------------

fn file_sha256(path: &Path) -> Result<String> {
    let file = fs::File::open(path)
        .with_context(|| format!("failed to open model artifact {}", path.display()))?;
    sha256::digest_reader(file)
        .with_context(|| format!("failed to hash model artifact {}", path.display()))
}

// ---------------------------------------------------------------------------
// Model-free unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::os::unix::fs::symlink;

    use tempfile::tempdir;

    use super::*;

    fn metadata() -> IndexMetadata {
        IndexMetadata {
            schema_version: INDEX_SCHEMA_VERSION.to_owned(),
            parser_version: PARSER_VERSION.to_owned(),
            engine_config_sha256: "c".repeat(64),
            model_fingerprint: "f".repeat(64),
            corpus_fingerprint: "d".repeat(64),
            root: "/tmp/example".to_owned(),
            dimension: EXPECTED_DIMENSION as u32,
        }
    }

    fn record(path: &str, line_start: u64, chunk_hash: &str, vector: Vec<f32>) -> IndexRecord {
        IndexRecord {
            storage_key: format!("{chunk_hash}:{}", "0".repeat(64)),
            chunk_hash: chunk_hash.to_owned(),
            file_hash: "a".repeat(64),
            path: path.to_owned(),
            heading: None,
            line_start,
            line_end: line_start + 1,
            text: format!("evidence for {path} at {line_start}"),
            vector,
        }
    }

    fn normalized(values: [f32; 4]) -> Vec<f32> {
        let mut vector = vec![0f32; EXPECTED_DIMENSION];
        vector[..4].copy_from_slice(&values);
        l2_normalize(&mut vector, L2_EPSILON);
        vector
    }

    fn frozen_model_value() -> Value {
        json!({
            "id": FROZEN_MODEL_ID,
            "revision": FROZEN_MODEL_REVISION,
            "license": FROZEN_MODEL_LICENSE,
            "dimension": EXPECTED_DIMENSION,
            "max_tokens": EXPECTED_MAX_TOKENS,
            "query_prefix": FROZEN_QUERY_PREFIX,
            "passage_prefix": FROZEN_PASSAGE_PREFIX,
            "pooling": FROZEN_POOLING,
            "normalization": FROZEN_NORMALIZATION,
            "similarity": FROZEN_SIMILARITY,
            "dtype": FROZEN_DTYPE,
            "runtime": FROZEN_RUNTIME,
            "tokenizers": FROZEN_TOKENIZERS,
            "artifacts": [
                {"file": "config.json", "bytes": EXPECTED_ARTIFACTS[0].1, "sha256": EXPECTED_ARTIFACTS[0].2},
                {"file": "tokenizer.json", "bytes": EXPECTED_ARTIFACTS[1].1, "sha256": EXPECTED_ARTIFACTS[1].2},
                {"file": "model.safetensors", "bytes": EXPECTED_ARTIFACTS[2].1, "sha256": EXPECTED_ARTIFACTS[2].2},
            ],
            "artifacts_sha256_file": FROZEN_ARTIFACTS_SHA256_FILE,
        })
    }

    #[test]
    fn frozen_configuration_matches_the_published_schema() {
        let schema: Value = serde_json::from_str(include_str!(
            "../evaluation/engine-bakeoff-protocol.schema.json"
        ))
        .expect("published bake-off protocol schema should be valid JSON");
        let expected = &schema["$defs"]["embeddingConfig"]["properties"];
        let tokenization = &schema["$defs"]["tokenization"]["properties"];
        let ours = frozen_configuration();

        assert_eq!(ours["model_ref"], expected["model_ref"]["const"]);
        assert_eq!(ours["vector_storage"], expected["vector_storage"]["const"]);
        assert_eq!(
            ours["candidate_generation"],
            expected["candidate_generation"]["const"]
        );
        assert_eq!(
            ours["candidate_depth"],
            expected["candidate_depth"]["const"]
        );
        assert_eq!(ours["ranking"], expected["ranking"]["const"]);
        assert_eq!(ours["abstention"], expected["abstention"]["const"]);
        for field in [
            "add_special_tokens",
            "truncation",
            "max_tokens",
            "stride",
            "padding",
            "query_prefix",
            "passage_prefix",
        ] {
            assert_eq!(
                ours["tokenization"][field], tokenization[field]["const"],
                "tokenization.{field} must match the frozen schema"
            );
        }
    }

    #[test]
    fn parse_model_spec_accepts_only_the_frozen_contract() {
        let spec = parse_model_spec(&frozen_model_value()).expect("frozen model must parse");
        assert_eq!(spec.fingerprint().len(), 64);
        assert_eq!(spec.dimension, EXPECTED_DIMENSION);

        let mut wrong_dimension = frozen_model_value();
        wrong_dimension["dimension"] = json!(768);
        assert!(parse_model_spec(&wrong_dimension).is_err());

        let mut wrong_artifact = frozen_model_value();
        wrong_artifact["artifacts"][2]["sha256"] = json!("g".repeat(64));
        assert!(parse_model_spec(&wrong_artifact).is_err());

        let mut unknown_field = frozen_model_value();
        unknown_field["extra"] = json!(true);
        assert!(parse_model_spec(&unknown_field).is_err());

        let mut missing_weights = frozen_model_value();
        missing_weights["artifacts"] = json!([
            {"file": "config.json", "bytes": EXPECTED_ARTIFACTS[0].1, "sha256": EXPECTED_ARTIFACTS[0].2},
            {"file": "tokenizer.json", "bytes": EXPECTED_ARTIFACTS[1].1, "sha256": EXPECTED_ARTIFACTS[1].2},
        ]);
        assert!(parse_model_spec(&missing_weights).is_err());
    }

    #[test]
    fn attention_mask_mean_pooling_ignores_padding() {
        let hidden = vec![
            vec![1.0, 2.0, 3.0, 4.0],
            vec![5.0, 6.0, 7.0, 8.0],
            vec![9.0, 9.0, 9.0, 9.0],
        ];
        let pooled = mean_pool(&hidden, &[1, 1, 0]).unwrap();
        assert_eq!(pooled, vec![3.0, 4.0, 5.0, 6.0]);
        let empty = mean_pool(&hidden, &[0, 0, 0]).unwrap();
        assert_eq!(empty, vec![0.0, 0.0, 0.0, 0.0]);
        assert!(mean_pool(&hidden, &[1, 0]).is_err());
    }

    #[test]
    fn l2_normalization_is_stable_for_zero_and_epsilon() {
        let mut vector = vec![3.0f32, 4.0];
        l2_normalize(&mut vector, L2_EPSILON);
        let norm: f32 = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
        assert!((vector[0] - 0.6).abs() < 1e-6);
        assert!((vector[1] - 0.8).abs() < 1e-6);

        let mut zero = vec![0.0f32; 4];
        l2_normalize(&mut zero, L2_EPSILON);
        assert!(zero.iter().all(|value| *value == 0.0));
    }

    #[test]
    fn exact_scan_orders_by_dot_product_then_path_and_line() {
        let records = vec![
            record("b.md", 5, &"b".repeat(64), normalized([1.0, 0.0, 0.0, 0.0])),
            record("a.md", 9, &"c".repeat(64), normalized([1.0, 0.0, 0.0, 0.0])),
            record("a.md", 2, &"d".repeat(64), normalized([1.0, 0.0, 0.0, 0.0])),
            record("c.md", 1, &"e".repeat(64), normalized([0.0, 1.0, 0.0, 0.0])),
        ];
        let query = normalized([1.0, 0.0, 0.0, 0.0]);
        let ranked = rank_records(&records, &query, CANDIDATE_DEPTH, ABSTENTION_THRESHOLD).unwrap();
        let order: Vec<(String, u64)> = ranked
            .iter()
            .map(|(index, _)| (records[*index].path.clone(), records[*index].line_start))
            .collect();
        assert_eq!(
            order,
            vec![
                ("a.md".to_owned(), 2),
                ("a.md".to_owned(), 9),
                ("b.md".to_owned(), 5),
                ("c.md".to_owned(), 1),
            ]
        );
    }

    #[test]
    fn abstention_returns_nothing_below_the_frozen_threshold() {
        let low = vec![record(
            "a.md",
            1,
            &"1".repeat(64),
            normalized([0.5, 0.5, 0.5, 0.5]),
        )];
        let query = normalized([1.0, 0.0, 0.0, 0.0]);
        assert!(
            rank_records(&low, &query, CANDIDATE_DEPTH, ABSTENTION_THRESHOLD)
                .unwrap()
                .is_empty()
        );

        let high = vec![record(
            "a.md",
            1,
            &"2".repeat(64),
            normalized([0.99, 0.1, 0.0, 0.0]),
        )];
        assert!(
            !rank_records(&high, &query, CANDIDATE_DEPTH, ABSTENTION_THRESHOLD)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn candidate_depth_truncates_before_selection() {
        let records: Vec<IndexRecord> = (0..100)
            .map(|index| {
                record(
                    &format!("{index:03}.md"),
                    index,
                    &format!("{:064x}", index),
                    normalized([1.0, 0.0, 0.0, 0.0]),
                )
            })
            .collect();
        let query = normalized([1.0, 0.0, 0.0, 0.0]);
        let ranked = rank_records(&records, &query, CANDIDATE_DEPTH, ABSTENTION_THRESHOLD).unwrap();
        assert_eq!(ranked.len(), CANDIDATE_DEPTH);
        let selected = rank_for_request(&records, &query, 5, None).unwrap();
        assert_eq!(selected.len(), 5);
    }

    #[test]
    fn index_roundtrip_preserves_evidence_and_vectors() {
        let records = vec![
            record("a.md", 1, &"3".repeat(64), normalized([1.0, 0.0, 0.0, 0.0])),
            IndexRecord {
                heading: Some("Section".to_owned()),
                ..record("b.md", 2, &"4".repeat(64), normalized([0.0, 1.0, 0.0, 0.0]))
            },
        ];
        let encoded = encode_index(&metadata(), &records).unwrap();
        let (decoded_metadata, decoded_records) = decode_index(&encoded).unwrap();
        assert_eq!(decoded_metadata.schema_version, INDEX_SCHEMA_VERSION);
        assert_eq!(decoded_metadata.root, "/tmp/example");
        assert_eq!(decoded_records, records);
    }

    #[test]
    fn corruption_and_truncation_are_detected() {
        let records = vec![record(
            "a.md",
            1,
            &"5".repeat(64),
            normalized([1.0, 0.0, 0.0, 0.0]),
        )];
        let mut encoded = encode_index(&metadata(), &records).unwrap();

        let last = encoded.len() - 1;
        encoded[last] ^= 0xff;
        assert!(decode_index(&encoded).is_err());

        let mut encoded = encode_index(&metadata(), &records).unwrap();
        encoded[20] ^= 0xff;
        assert!(decode_index(&encoded).is_err());

        assert!(decode_index(&encoded[..16]).is_err());
    }

    #[test]
    fn duplicate_chunk_hashes_keep_distinct_storage_keys_and_evidence() {
        let duplicate_hash = "6".repeat(64);
        let mut first = record("a.md", 1, &duplicate_hash, normalized([1.0, 0.0, 0.0, 0.0]));
        let mut second = record("b.md", 7, &duplicate_hash, normalized([0.0, 1.0, 0.0, 0.0]));
        // Rebuild canonical storage keys and text so validation accepts both records.
        first.text = "duplicate text".to_owned();
        first.chunk_hash = blake3::hash(first.text.as_bytes()).to_hex().to_string();
        first.storage_key = storage_key(&first.to_chunk());
        second.text = "duplicate text".to_owned();
        second.chunk_hash = blake3::hash(second.text.as_bytes()).to_hex().to_string();
        second.storage_key = storage_key(&second.to_chunk());
        assert_ne!(first.storage_key, second.storage_key);

        let encoded = encode_index(&metadata(), &[first.clone(), second.clone()]).unwrap();
        let (_, decoded) = decode_index(&encoded).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].storage_key, first.storage_key);
        assert_eq!(decoded[1].storage_key, second.storage_key);
        assert_eq!(decoded[0].path, "a.md");
        assert_eq!(decoded[1].path, "b.md");
    }

    fn artifact_content(file: &str) -> Vec<u8> {
        format!("frozen {file}").into_bytes()
    }

    fn artifact(file: &str, _bytes: &[u8]) -> ModelArtifact {
        let content = artifact_content(file);
        ModelArtifact {
            file: file.to_owned(),
            bytes: content.len() as u64,
            sha256: sha256::digest_hex(&content),
        }
    }

    fn artifact_spec(artifacts: Vec<ModelArtifact>) -> ModelSpec {
        ModelSpec {
            id: FROZEN_MODEL_ID.to_owned(),
            revision: FROZEN_MODEL_REVISION.to_owned(),
            license: FROZEN_MODEL_LICENSE.to_owned(),
            dimension: EXPECTED_DIMENSION,
            max_tokens: EXPECTED_MAX_TOKENS,
            query_prefix: FROZEN_QUERY_PREFIX.to_owned(),
            passage_prefix: FROZEN_PASSAGE_PREFIX.to_owned(),
            pooling: FROZEN_POOLING.to_owned(),
            normalization: FROZEN_NORMALIZATION.to_owned(),
            similarity: FROZEN_SIMILARITY.to_owned(),
            dtype: FROZEN_DTYPE.to_owned(),
            runtime: FROZEN_RUNTIME.to_owned(),
            tokenizers: FROZEN_TOKENIZERS.to_owned(),
            artifacts,
            artifacts_sha256_file: FROZEN_ARTIFACTS_SHA256_FILE.to_owned(),
        }
    }

    fn write_artifacts(directory: &Path, spec: &ModelSpec) {
        let mut sums = String::new();
        for artifact in &spec.artifacts {
            fs::write(
                directory.join(&artifact.file),
                artifact_content(&artifact.file),
            )
            .unwrap();
            sums.push_str(&format!("{}  {}\n", artifact.sha256, artifact.file));
        }
        fs::write(directory.join(&spec.artifacts_sha256_file), sums.as_bytes()).unwrap();
    }

    #[test]
    fn missing_artifacts_fail_closed() {
        let artifacts = vec![
            artifact("config.json", b"config"),
            artifact("tokenizer.json", b"tokenizer"),
            artifact("model.safetensors", b"weights"),
        ];
        let spec = artifact_spec(artifacts);

        let empty = tempdir().unwrap();
        assert!(validate_model_artifacts(&spec, empty.path()).is_err());

        let missing = empty.path().join("does-not-exist");
        assert!(validate_model_artifacts(&spec, &missing).is_err());
    }

    #[test]
    fn artifact_size_hash_symlink_and_manifest_violations_fail_closed() {
        let artifacts = vec![
            artifact("config.json", b"config"),
            artifact("tokenizer.json", b"tokenizer"),
            artifact("model.safetensors", b"weights"),
        ];
        let spec = artifact_spec(artifacts);

        let directory = tempdir().unwrap();
        write_artifacts(directory.path(), &spec);
        validate_model_artifacts(&spec, directory.path()).unwrap();

        // Wrong size.
        let mut wrong_size = spec.clone();
        wrong_size.artifacts[0].bytes += 1;
        assert!(validate_model_artifacts(&wrong_size, directory.path()).is_err());

        // Wrong digest.
        let mut wrong_hash = spec.clone();
        wrong_hash.artifacts[1].sha256 = "0".repeat(64);
        assert!(validate_model_artifacts(&wrong_hash, directory.path()).is_err());

        // Symlinked artifact.
        let symlinked = tempdir().unwrap();
        write_artifacts(symlinked.path(), &spec);
        let target = symlinked.path().join("model.safetensors");
        fs::remove_file(&target).unwrap();
        symlink(symlinked.path().join("config.json"), &target).unwrap();
        assert!(validate_model_artifacts(&spec, symlinked.path()).is_err());

        // Missing manifest.
        let unmanifested = tempdir().unwrap();
        write_artifacts(unmanifested.path(), &spec);
        fs::remove_file(unmanifested.path().join(FROZEN_ARTIFACTS_SHA256_FILE)).unwrap();
        assert!(validate_model_artifacts(&spec, unmanifested.path()).is_err());

        // Manifest without the frozen digest.
        let stale = tempdir().unwrap();
        write_artifacts(stale.path(), &spec);
        fs::write(
            stale.path().join(FROZEN_ARTIFACTS_SHA256_FILE),
            b"0000000000000000000000000000000000000000000000000000000000000000  config.json\n",
        )
        .unwrap();
        assert!(validate_model_artifacts(&spec, stale.path()).is_err());
    }

    #[test]
    fn streaming_sha256_matches_the_one_shot_digest() {
        for input in [b"".as_slice(), b"abc".as_slice(), &[0u8; 1000]] {
            assert_eq!(
                sha256::digest_reader(Cursor::new(input)).unwrap(),
                sha256::digest_hex(input)
            );
        }
    }

    #[test]
    fn index_path_is_a_distinct_private_binary_file() {
        let path = index_path_for_root(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert_eq!(path.file_name().unwrap(), INDEX_FILE);
        assert!(path.to_string_lossy().ends_with("/embeddings-v1.bin"));
        assert_ne!(path.file_name().unwrap(), "index.sqlite3");
    }
}
