use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::corpus::{self, Chunk};
use crate::embeddings::{
    EMBEDDINGS_ENGINE, EmbeddingsIndex, ModelSpec,
    fault_injection_passed as embeddings_fault_injection_passed, parse_model_spec,
    run_incremental_workload as run_embeddings_incremental_workload,
};
use crate::evaluate::{
    ContextStats, EvaluationSummary, LatencyStats, QueryStatus, RECALL_CUTOFF,
    base_query_evaluation, corpus_fingerprint, latency_stats, round_six,
    successful_query_evaluation, summarize,
};
use crate::evaluation::{EvaluationCategory, EvaluationQuery, parse_evaluation_set};
use crate::fts5::{
    CANDIDATE_DEPTH as FTS5_CANDIDATE_DEPTH, FTS5_ENGINE, Fts5Index,
    fault_injection_passed as fts5_fault_injection_passed,
    run_incremental_workload as run_fts5_incremental_workload,
};
use crate::search::{SearchTimings, search, search_with_timings};
use crate::sha256;
use crate::sqlite_cache::{
    IndexingMetrics, SQLITE_ENGINE, SqliteCache, SqliteRuntime, fault_injection_passed,
    run_incremental_workload,
};
use crate::types::{ENGINE, SearchRequest, SearchResult};

pub const BAKEOFF_OBSERVATION_SCHEMA_VERSION: u32 = 1;
pub const BAKEOFF_REPORT_SCHEMA_VERSION: u32 = 1;
pub const BAKEOFF_PROTOCOL_TAG: &str = "engine-bakeoff-v1";
const LIMIT: usize = 5;
const MAX_EXCERPT_CHARS: usize = 1_200;
const LEXICAL_CONFIG_SHA256: &str =
    "4b352dbc5fb1ed6818f6cca023664858ae563bd9bffdcc7c4b7db2e7fdec4b8d";
const SQLITE_CONFIG_SHA256: &str =
    "858fa10a7ea31eb46d9933817b221c2a722f936c4d8565f1bb830f66dbc64ab2";
const FTS5_CONFIG_SHA256: &str = "3aedaee7026d230eea9345a962f6913c9d7890dd0fe175704c29aafb10384034";
const EMBEDDINGS_CONFIG_SHA256: &str =
    "de41db0af0e15a1dac47b2504617c0f6dbba8f10a2b0ea822a1e9ab2bb208e3d";

#[derive(Debug, Clone)]
pub struct ObserveRequest {
    pub protocol_path: PathBuf,
    pub input_id: String,
    pub engine: String,
    pub run: u8,
    pub provenance_path: PathBuf,
    pub ready_file: PathBuf,
    pub output: PathBuf,
}

#[derive(Debug, Clone)]
pub struct FinalizeRequest {
    pub protocol_path: PathBuf,
    pub observation_path: PathBuf,
    pub other_observation_path: PathBuf,
    pub measurements_path: PathBuf,
    pub report_tag: String,
    pub output: PathBuf,
}

trait BakeoffEngine {
    fn id(&self) -> &'static str;

    fn prepare(
        &mut self,
        _root: &Path,
        _corpus: &corpus::Corpus,
        _queries: &[EvaluationQuery],
    ) -> Result<(IndexingMetrics, Option<SqliteRuntime>)> {
        Ok((IndexingMetrics::direct(), None))
    }

    fn open_existing(&mut self, _root: &Path, _corpus: &corpus::Corpus) -> Result<()> {
        Ok(())
    }

    fn search(
        &self,
        request: SearchRequest,
    ) -> Result<(crate::types::SearchResponse, SearchTimings)>;

    fn fault_injection_passed(&self, root: &Path) -> bool;

    fn index_path(&self) -> Option<&Path> {
        None
    }

    fn fallback_count(&self) -> usize {
        0
    }
}

struct LexicalBm25;

impl BakeoffEngine for LexicalBm25 {
    fn id(&self) -> &'static str {
        ENGINE
    }

    fn search(
        &self,
        request: SearchRequest,
    ) -> Result<(crate::types::SearchResponse, SearchTimings)> {
        search_with_timings(request)
    }

    fn fault_injection_passed(&self, root: &Path) -> bool {
        let missing = root.join(".docs-search-bakeoff-missing-root");
        !missing.exists()
            && search(SearchRequest {
                root: missing,
                query: "fault injection".to_owned(),
                limit: LIMIT,
                max_excerpt_chars: MAX_EXCERPT_CHARS,
                max_results_per_path: None,
            })
            .is_err()
    }
}

struct SqliteCachedBm25 {
    engine_config_sha256: String,
    cache: Option<SqliteCache>,
    fallback_count: Cell<usize>,
}

struct Fts5Search {
    engine_config_sha256: String,
    index: Option<Fts5Index>,
}

struct EmbeddingsSearch {
    engine_config_sha256: String,
    model_spec: ModelSpec,
    model_directory: PathBuf,
    index: Option<EmbeddingsIndex>,
}

impl BakeoffEngine for EmbeddingsSearch {
    fn id(&self) -> &'static str {
        EMBEDDINGS_ENGINE
    }

    fn prepare(
        &mut self,
        root: &Path,
        corpus: &corpus::Corpus,
        queries: &[EvaluationQuery],
    ) -> Result<(IndexingMetrics, Option<SqliteRuntime>)> {
        let (index, mut indexing) = EmbeddingsIndex::prepare_fresh(
            root,
            corpus,
            &self.engine_config_sha256,
            &self.model_spec,
            &self.model_directory,
        )?;
        let query_texts: Vec<_> = queries.iter().map(|query| query.query.clone()).collect();
        indexing.incremental_steps =
            run_embeddings_incremental_workload(&index, root, &query_texts)?;
        index.verify_current(corpus)?;
        self.index = Some(index);
        Ok((indexing, None))
    }

    fn open_existing(&mut self, root: &Path, corpus: &corpus::Corpus) -> Result<()> {
        self.index = Some(EmbeddingsIndex::open_existing(
            root,
            corpus,
            &self.engine_config_sha256,
            &self.model_spec,
            &self.model_directory,
        )?);
        Ok(())
    }

    fn search(
        &self,
        request: SearchRequest,
    ) -> Result<(crate::types::SearchResponse, SearchTimings)> {
        self.index
            .as_ref()
            .ok_or_else(|| anyhow!("embeddings engine was not prepared"))?
            .search(request)
    }

    fn fault_injection_passed(&self, _root: &Path) -> bool {
        self.index.as_ref().is_some_and(|index| {
            embeddings_fault_injection_passed(index, &self.model_spec, &self.model_directory)
        })
    }

    fn index_path(&self) -> Option<&Path> {
        self.index.as_ref().map(EmbeddingsIndex::index_path)
    }
}

impl BakeoffEngine for Fts5Search {
    fn id(&self) -> &'static str {
        FTS5_ENGINE
    }

    fn prepare(
        &mut self,
        root: &Path,
        corpus: &corpus::Corpus,
        queries: &[EvaluationQuery],
    ) -> Result<(IndexingMetrics, Option<SqliteRuntime>)> {
        let (index, mut indexing, runtime) =
            Fts5Index::prepare_fresh(root, corpus, &self.engine_config_sha256)?;
        let query_texts: Vec<_> = queries.iter().map(|query| query.query.clone()).collect();
        indexing.incremental_steps =
            run_fts5_incremental_workload(root, &query_texts, &self.engine_config_sha256)?;
        index.verify_current(corpus)?;
        self.index = Some(index);
        Ok((indexing, Some(runtime)))
    }

    fn open_existing(&mut self, root: &Path, corpus: &corpus::Corpus) -> Result<()> {
        self.index = Some(Fts5Index::open_existing(
            root,
            corpus,
            &self.engine_config_sha256,
        )?);
        Ok(())
    }

    fn search(
        &self,
        request: SearchRequest,
    ) -> Result<(crate::types::SearchResponse, SearchTimings)> {
        self.index
            .as_ref()
            .ok_or_else(|| anyhow!("FTS5 engine was not prepared"))?
            .search(request)
    }

    fn fault_injection_passed(&self, root: &Path) -> bool {
        fts5_fault_injection_passed(root, &self.engine_config_sha256)
    }

    fn index_path(&self) -> Option<&Path> {
        self.index.as_ref().map(Fts5Index::index_path)
    }
}

impl BakeoffEngine for SqliteCachedBm25 {
    fn id(&self) -> &'static str {
        SQLITE_ENGINE
    }

    fn prepare(
        &mut self,
        root: &Path,
        corpus: &corpus::Corpus,
        queries: &[EvaluationQuery],
    ) -> Result<(IndexingMetrics, Option<SqliteRuntime>)> {
        let (cache, mut indexing, runtime) =
            SqliteCache::prepare_fresh(root, corpus, &self.engine_config_sha256)?;
        let query_texts: Vec<_> = queries.iter().map(|query| query.query.clone()).collect();
        indexing.incremental_steps =
            run_incremental_workload(root, &query_texts, &self.engine_config_sha256)?;
        cache.verify_current(corpus)?;
        self.cache = Some(cache);
        Ok((indexing, Some(runtime)))
    }

    fn open_existing(&mut self, root: &Path, corpus: &corpus::Corpus) -> Result<()> {
        self.cache = Some(SqliteCache::open_existing(
            root,
            corpus,
            &self.engine_config_sha256,
        )?);
        Ok(())
    }

    fn search(
        &self,
        request: SearchRequest,
    ) -> Result<(crate::types::SearchResponse, SearchTimings)> {
        let (response, timings, fallback_used) = self
            .cache
            .as_ref()
            .ok_or_else(|| anyhow!("SQLite cache engine was not prepared"))?
            .search_with_recovery(request, false)?;
        if fallback_used {
            self.fallback_count
                .set(self.fallback_count.get().saturating_add(1));
        }
        Ok((response, timings))
    }

    fn fault_injection_passed(&self, root: &Path) -> bool {
        fault_injection_passed(root, &self.engine_config_sha256)
    }

    fn index_path(&self) -> Option<&Path> {
        self.cache.as_ref().map(SqliteCache::index_path)
    }

    fn fallback_count(&self) -> usize {
        self.fallback_count.get()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    schema_version: u32,
    protocol_tag: String,
    protocol_sha256: String,
    engine: String,
    engine_config_sha256: String,
    engine_configuration: Value,
    run: u8,
    provenance: ObservationProvenance,
    input: ReportInput,
    retrieval: ObservationRetrieval,
    quality: Value,
    per_category: BTreeMap<String, Value>,
    queries: Vec<ObservedQuery>,
    timing_ms: Value,
    indexing: IndexingMetrics,
    sqlite_runtime: Option<ObservedSqliteRuntime>,
    index_path: Option<String>,
    normal_fallback_count: usize,
    baseline_comparison: Value,
    category_deltas: BTreeMap<String, Value>,
    ranking_projection_sha256: String,
    evidence_projection_sha256: String,
    evidence_valid: bool,
    fault_injection_passed: bool,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationRetrieval {
    limit: usize,
    recall_cutoff: usize,
    max_excerpt_chars: usize,
    max_results_per_path: Option<usize>,
    candidates_examined: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportInput {
    id: String,
    role: String,
    root: String,
    files: usize,
    chunks: usize,
    corpus_fingerprint: String,
    queries_schema_version: u32,
    dataset_blake3: String,
    queries_sha256: String,
    query_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedQuery {
    id: String,
    category: EvaluationCategory,
    status: QueryStatus,
    results: Vec<ObservedResult>,
    first_relevant_rank: Option<usize>,
    first_relevant_heading_rank: Option<usize>,
    relevant_paths_found: Option<usize>,
    hit_at_1: Option<bool>,
    recall_at_5: Option<f64>,
    reciprocal_rank_at_5: Option<f64>,
    no_answer_false_positive: Option<bool>,
    context_chars: usize,
    timing_ms: QueryTimings,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedResult {
    rank: usize,
    path: String,
    heading: Option<String>,
    line_start: usize,
    line_end: usize,
    score: f64,
    file_hash: String,
    chunk_hash: String,
    excerpt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryTimings {
    lookup: f64,
    ranking: f64,
    excerpt: f64,
    total: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationProvenance {
    tool: ToolMeasurement,
    cargo_version: String,
    process_environment: BTreeMap<String, String>,
    host_sha256: String,
    host_verified: bool,
    isolation: IsolationAttestation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IsolationAttestation {
    denylist_sha256: String,
    verified_before_path_resolution: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedSqliteRuntime {
    version: String,
    compile_options_sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalMeasurements {
    schema_version: u32,
    protocol_sha256: String,
    input_id: String,
    engine: String,
    run: u8,
    observation_sha256: String,
    tool: ToolMeasurement,
    environment: EnvironmentMeasurement,
    timing_ms: OuterTiming,
    resources_bytes: ResourceMeasurement,
    started_at: String,
    finished_at: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ToolMeasurement {
    version: String,
    git_revision: String,
    binary_sha256: String,
    rust_version: String,
    cargo_lock_sha256: String,
    profile: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentMeasurement {
    os: String,
    architecture: String,
    machine: String,
    cpu: String,
    logical_cpus: usize,
    memory_bytes: u64,
    load_average: [f64; 3],
    sqlite_version: Option<String>,
    sqlite_compile_options_sha256: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct OuterTiming {
    startup: f64,
    end_to_end: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ResourceMeasurement {
    peak_rss: u64,
    index_logical: u64,
    index_allocated: u64,
    shared_model: u64,
}

pub fn observe(request: ObserveRequest) -> Result<()> {
    if request.run != 1 && request.run != 2 {
        bail!("bake-off run must be 1 or 2");
    }
    ensure_engine_available(&request.engine)?;
    let protocol_bytes = fs::read(&request.protocol_path).with_context(|| {
        format!(
            "failed to read bake-off protocol {}",
            request.protocol_path.display()
        )
    })?;
    let protocol: Value =
        serde_json::from_slice(&protocol_bytes).context("bake-off protocol must be valid JSON")?;
    validate_protocol(&protocol)?;
    let protocol_sha256 = sha256::digest_hex(&protocol_bytes);
    let provenance: ObservationProvenance =
        read_json(&request.provenance_path, "observation provenance")?;
    validate_provenance(&provenance, &protocol)?;
    let engine_configuration = engine_configuration(&protocol, &request.engine)?.clone();
    validate_frozen_engine_configuration(&request.engine, &engine_configuration)?;
    let engine_config_sha256 = canonical_json_sha256(&engine_configuration)?;
    let mut engine = engine(
        &request.engine,
        &engine_config_sha256,
        protocol.get("model"),
    )?;
    let input = protocol_input(&protocol, &request.input_id)?;
    let root = PathBuf::from(required_string(input, "root")?);
    let queries_path = PathBuf::from(required_string(input, "queries")?);
    let baseline_path = PathBuf::from(required_string(input, "baseline_report")?);

    verify_file_sha256(
        &queries_path,
        required_string(input, "queries_sha256")?,
        "query dataset",
    )?;
    verify_file_sha256(
        &baseline_path,
        required_string(input, "baseline_report_sha256")?,
        "baseline report",
    )?;
    let snapshot_manifest = PathBuf::from(required_string(input, "snapshot_manifest")?);
    verify_file_sha256(
        &snapshot_manifest,
        required_string(input, "snapshot_manifest_sha256")?,
        "snapshot manifest",
    )?;

    let dataset_bytes = fs::read(&queries_path)
        .with_context(|| format!("failed to read query dataset {}", queries_path.display()))?;
    let dataset_text = std::str::from_utf8(&dataset_bytes).context("query dataset is not UTF-8")?;
    let set = parse_evaluation_set(dataset_text).context("invalid bake-off query dataset")?;
    if set
        .queries
        .iter()
        .any(|query| query.disabled_reason.is_some())
    {
        bail!("bake-off datasets cannot contain disabled queries");
    }
    if blake3::hash(&dataset_bytes).to_hex().as_str() != required_string(input, "dataset_blake3")? {
        bail!("query dataset BLAKE3 does not match the frozen protocol");
    }

    let loaded = corpus::load(&root)?;
    let fingerprint = corpus_fingerprint(&loaded.file_hashes);
    verify_input_corpus(input, &loaded, &fingerprint, set.queries.len())?;
    let (mut indexing, sqlite_runtime) = engine.prepare(&root, &loaded, &set.queries)?;
    let index_path = engine
        .index_path()
        .map(|path| path.to_string_lossy().into_owned());
    write_ready_file(&request.ready_file)?;

    let mut evaluations = Vec::with_capacity(set.queries.len());
    let mut observed_queries = Vec::with_capacity(set.queries.len());
    let mut candidates_examined = 0usize;
    let mut errors = Vec::new();

    for query in set.queries {
        let started = Instant::now();
        let response = engine.search(SearchRequest {
            root: root.clone(),
            query: query.query.clone(),
            limit: LIMIT,
            max_excerpt_chars: MAX_EXCERPT_CHARS,
            max_results_per_path: None,
        });
        match response {
            Ok((response, timings)) => {
                validate_results(&loaded.chunks, &response.results)?;
                candidates_examined = candidates_examined
                    .checked_add(timings.candidates_examined)
                    .ok_or_else(|| anyhow!("candidate counter overflow"))?;
                let evaluation = successful_query_evaluation(
                    query.clone(),
                    response.results.clone(),
                    timings.total_ms,
                );
                observed_queries.push(observed_query(
                    &evaluation,
                    &response.results,
                    QueryTimings {
                        lookup: timings.lookup_ms,
                        ranking: timings.ranking_ms,
                        excerpt: timings.excerpt_ms,
                        total: timings.total_ms,
                    },
                ));
                evaluations.push(evaluation);
            }
            Err(error) => {
                let elapsed = elapsed_ms(started);
                errors.push(format!("{}: {error}", query.id));
                let mut evaluation = base_query_evaluation(query, QueryStatus::Error);
                evaluation.latency_ms = Some(elapsed);
                evaluation.error = Some(error.to_string());
                observed_queries.push(observed_query(
                    &evaluation,
                    &[],
                    QueryTimings {
                        lookup: 0.0,
                        ranking: 0.0,
                        excerpt: 0.0,
                        total: elapsed,
                    },
                ));
                evaluations.push(evaluation);
            }
        }
    }

    let summary = summarize(evaluations.iter());
    let mut per_category = BTreeMap::new();
    for (name, category) in categories() {
        per_category.insert(
            name.to_owned(),
            summary_value(&summarize(
                evaluations
                    .iter()
                    .filter(|evaluation| evaluation.category == category),
            )),
        );
    }
    let quality = summary_value(&summary);
    let report_input = ReportInput {
        id: request.input_id,
        role: "development".to_owned(),
        root: root.to_string_lossy().into_owned(),
        files: loaded.files,
        chunks: loaded.chunks.len(),
        corpus_fingerprint: fingerprint,
        queries_schema_version: set.schema_version,
        dataset_blake3: blake3::hash(&dataset_bytes).to_hex().to_string(),
        queries_sha256: sha256::digest_hex(&dataset_bytes),
        query_count: evaluations.len(),
    };
    let (baseline, category_deltas) = baseline_comparison(
        &baseline_path,
        required_string(input, "baseline_report_sha256")?,
        &report_input,
        engine.id(),
        &quality,
        &per_category,
        &observed_queries,
    )?;
    let ranking_projection_sha256 = ranking_projection(&observed_queries)?;
    let evidence_projection_sha256 = evidence_projection(&observed_queries)?;
    let timing_ms = timing_value(&observed_queries);
    let fault_injection_passed = engine.fault_injection_passed(&root);
    if matches!(engine.id(), SQLITE_ENGINE | FTS5_ENGINE | EMBEDDINGS_ENGINE) {
        indexing.corruption_detected |= fault_injection_passed;
        indexing.rebuild_succeeded &= fault_injection_passed;
    }
    let sqlite_runtime = sqlite_runtime.map(|runtime| ObservedSqliteRuntime {
        version: runtime.version,
        compile_options_sha256: runtime.compile_options_sha256,
    });

    let observation = Observation {
        schema_version: BAKEOFF_OBSERVATION_SCHEMA_VERSION,
        protocol_tag: BAKEOFF_PROTOCOL_TAG.to_owned(),
        protocol_sha256,
        engine: engine.id().to_owned(),
        engine_config_sha256,
        engine_configuration,
        run: request.run,
        provenance,
        input: report_input,
        retrieval: ObservationRetrieval {
            limit: LIMIT,
            recall_cutoff: RECALL_CUTOFF,
            max_excerpt_chars: MAX_EXCERPT_CHARS,
            max_results_per_path: None,
            candidates_examined,
        },
        quality,
        per_category,
        queries: observed_queries,
        timing_ms,
        indexing,
        sqlite_runtime,
        index_path,
        normal_fallback_count: engine.fallback_count(),
        baseline_comparison: baseline,
        category_deltas,
        ranking_projection_sha256,
        evidence_projection_sha256,
        evidence_valid: true,
        fault_injection_passed,
        errors,
    };
    write_json_new(&request.output, &observation)
}

pub fn finalize(request: FinalizeRequest) -> Result<()> {
    let protocol_bytes = fs::read(&request.protocol_path).with_context(|| {
        format!(
            "failed to read bake-off protocol {}",
            request.protocol_path.display()
        )
    })?;
    let protocol: Value =
        serde_json::from_slice(&protocol_bytes).context("bake-off protocol must be valid JSON")?;
    validate_protocol(&protocol)?;
    let protocol_sha256 = sha256::digest_hex(&protocol_bytes);
    let observation: Observation = read_json(&request.observation_path, "observation")?;
    let other: Observation = read_json(&request.other_observation_path, "other observation")?;
    let measurements: ExternalMeasurements =
        read_json(&request.measurements_path, "external measurements")?;

    validate_observation(&observation, &protocol, &protocol_sha256)?;
    validate_observation(&other, &protocol, &protocol_sha256)?;
    validate_pair(&observation, &other)?;
    validate_measurements(
        &measurements,
        &protocol,
        &observation,
        &sha256::digest_hex(&fs::read(&request.observation_path)?),
    )?;
    validate_report_tag(
        &request.report_tag,
        &observation.input.id,
        &observation.engine,
        observation.run,
    )?;

    let deterministic = observation.ranking_projection_sha256 == other.ranking_projection_sha256
        && observation.evidence_projection_sha256 == other.evidence_projection_sha256;
    let run_variation = run_variation_passed(&observation, &other, &protocol)?;
    let checks = budget_checks(
        &observation,
        &measurements,
        &protocol,
        deterministic,
        run_variation,
    )?;
    let status = if checks.values().all(|passed| *passed) && observation.errors.is_empty() {
        "pass"
    } else {
        "fail"
    };

    let report = json!({
        "schema_version": BAKEOFF_REPORT_SCHEMA_VERSION,
        "protocol_tag": BAKEOFF_PROTOCOL_TAG,
        "protocol_sha256": observation.protocol_sha256,
        "report_tag": request.report_tag,
        "run": observation.run,
        "engine": observation.engine,
        "engine_config_sha256": observation.engine_config_sha256,
        "tool": measurements.tool,
        "input": observation.input,
        "environment": measurements.environment,
        "baseline_comparison": observation.baseline_comparison,
        "retrieval": {
            "limit": observation.retrieval.limit,
            "recall_cutoff": observation.retrieval.recall_cutoff,
            "max_excerpt_chars": observation.retrieval.max_excerpt_chars,
            "max_results_per_path": observation.retrieval.max_results_per_path,
            "candidates_examined": observation.retrieval.candidates_examined,
            "engine_configuration": observation.engine_configuration,
        },
        "quality": observation.quality,
        "per_category": observation.per_category,
        "queries": report_queries(&observation.queries, &observation.engine),
        "timing_ms": merge_timing(&observation.timing_ms, &measurements.timing_ms)?,
        "indexing": observation.indexing,
        "resources_bytes": measurements.resources_bytes,
        "determinism": {
            "ranking_projection_sha256": observation.ranking_projection_sha256,
            "evidence_projection_sha256": observation.evidence_projection_sha256,
            "matches_other_run": deterministic,
        },
        "evidence_validation": {
            "paths": true,
            "headings": true,
            "lines": true,
            "file_hashes": true,
            "chunk_hashes": true,
            "excerpts": true,
        },
        "fallback": {
            "used_during_normal_run": observation.normal_fallback_count > 0,
            "normal_run_count": observation.normal_fallback_count,
            "fault_injection_passed": observation.fault_injection_passed,
            "behavior": fallback_behavior(&observation.engine)?,
        },
        "errors": observation.errors,
        "budget_checks": checks,
        "status": status,
        "started_at": measurements.started_at,
        "finished_at": measurements.finished_at,
    });
    write_json_value_new(&request.output, &report)
}

fn ensure_engine_available(id: &str) -> Result<()> {
    match id {
        ENGINE | SQLITE_ENGINE | FTS5_ENGINE | EMBEDDINGS_ENGINE => Ok(()),
        "hybrid-rrf-v1" => {
            bail!("engine {id} is frozen but not implemented before its planned phase")
        }
        _ => bail!("unknown bake-off engine {id}"),
    }
}

fn model_directory(spec: &ModelSpec) -> Result<PathBuf> {
    let config = spec
        .artifacts
        .iter()
        .find(|artifact| {
            Path::new(&artifact.file)
                .file_name()
                .and_then(|name| name.to_str())
                == Some("config.json")
        })
        .ok_or_else(|| anyhow!("frozen model config artifact is missing"))?;
    Path::new(&config.file)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("frozen model artifact path has no parent"))
}

fn engine(
    id: &str,
    engine_config_sha256: &str,
    model: Option<&Value>,
) -> Result<Box<dyn BakeoffEngine>> {
    ensure_engine_available(id)?;
    match id {
        ENGINE => Ok(Box::new(LexicalBm25)),
        SQLITE_ENGINE => Ok(Box::new(SqliteCachedBm25 {
            engine_config_sha256: engine_config_sha256.to_owned(),
            cache: None,
            fallback_count: Cell::new(0),
        })),
        FTS5_ENGINE => Ok(Box::new(Fts5Search {
            engine_config_sha256: engine_config_sha256.to_owned(),
            index: None,
        })),
        EMBEDDINGS_ENGINE => {
            let model_spec = parse_model_spec(
                model.ok_or_else(|| anyhow!("protocol.model is required for embeddings"))?,
            )?;
            let model_directory = model_directory(&model_spec)?;
            Ok(Box::new(EmbeddingsSearch {
                engine_config_sha256: engine_config_sha256.to_owned(),
                model_spec,
                model_directory,
                index: None,
            }))
        }
        _ => unreachable!("availability check rejected unsupported engine"),
    }
}

fn fallback_behavior(engine: &str) -> Result<&'static str> {
    match engine {
        ENGINE => Ok("not-applicable"),
        SQLITE_ENGINE => Ok("explicit-direct-bm25"),
        FTS5_ENGINE | EMBEDDINGS_ENGINE => Ok("explicit-fail-closed"),
        _ => bail!("fallback behavior is unavailable for engine {engine}"),
    }
}

fn validate_protocol(protocol: &Value) -> Result<()> {
    if protocol["schema_version"] != 1
        || protocol["protocol_tag"] != BAKEOFF_PROTOCOL_TAG
        || protocol["status"] != "frozen-before-implementation"
        || protocol["scope"] != "development-only"
    {
        bail!("unsupported or unfrozen bake-off protocol");
    }
    if protocol["policy"]["holdouts_allowed"] != false
        || protocol["policy"]["default_engine"] != ENGINE
        || protocol["policy"]["runs_per_engine_input"] != 2
    {
        bail!("bake-off protocol policy does not match Phase 4.1");
    }
    Ok(())
}

fn validate_frozen_engine_configuration(id: &str, configuration: &Value) -> Result<()> {
    let expected = match id {
        ENGINE => LEXICAL_CONFIG_SHA256,
        SQLITE_ENGINE => SQLITE_CONFIG_SHA256,
        FTS5_ENGINE => FTS5_CONFIG_SHA256,
        EMBEDDINGS_ENGINE => EMBEDDINGS_CONFIG_SHA256,
        _ => return Ok(()),
    };
    let runtime_configuration = match id {
        FTS5_ENGINE => Some(crate::fts5::frozen_configuration()),
        EMBEDDINGS_ENGINE => Some(crate::embeddings::frozen_configuration()),
        _ => None,
    };
    if runtime_configuration
        .as_ref()
        .is_some_and(|runtime| runtime != configuration)
    {
        bail!("{id} configuration values differ from the runtime implementation");
    }
    let actual = canonical_json_sha256(configuration)?;
    if actual != expected {
        bail!("engine {id} configuration differs from the frozen implementation contract");
    }
    Ok(())
}

fn engine_configuration<'a>(protocol: &'a Value, id: &str) -> Result<&'a Value> {
    protocol["engines"]
        .as_array()
        .ok_or_else(|| anyhow!("protocol engines must be an array"))?
        .iter()
        .find(|engine| engine["id"] == id)
        .and_then(|engine| engine.get("configuration"))
        .ok_or_else(|| anyhow!("protocol does not define engine {id}"))
}

fn protocol_input<'a>(protocol: &'a Value, input_id: &str) -> Result<&'a Value> {
    let input = protocol["inputs"]
        .as_array()
        .ok_or_else(|| anyhow!("protocol inputs must be an array"))?
        .iter()
        .find(|input| input["id"] == input_id)
        .ok_or_else(|| {
            anyhow!("input {input_id} is not part of the frozen development protocol")
        })?;
    if input["role"] != "development" {
        bail!("input {input_id} is not a development input");
    }
    Ok(input)
}

fn verify_file_sha256(path: &Path, expected: &str, label: &str) -> Result<()> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read {label} {}", path.display()))?;
    if sha256::digest_hex(&bytes) != expected {
        bail!("{label} SHA-256 does not match the frozen protocol");
    }
    Ok(())
}

fn verify_input_corpus(
    input: &Value,
    corpus: &corpus::Corpus,
    fingerprint: &str,
    query_count: usize,
) -> Result<()> {
    if required_usize(input, "files")? != corpus.files
        || required_usize(input, "chunks")? != corpus.chunks.len()
        || required_string(input, "corpus_fingerprint")? != fingerprint
        || required_usize(input, "query_count")? != query_count
    {
        bail!("current input does not match the frozen corpus metadata");
    }
    Ok(())
}

fn observed_query(
    evaluation: &crate::evaluate::QueryEvaluation,
    results: &[SearchResult],
    timing_ms: QueryTimings,
) -> ObservedQuery {
    ObservedQuery {
        id: evaluation.id.clone(),
        category: evaluation.category,
        status: evaluation.status,
        results: results
            .iter()
            .map(|result| ObservedResult {
                rank: result.rank,
                path: result.path.clone(),
                heading: result.heading.clone(),
                line_start: result.line_start,
                line_end: result.line_end,
                score: result.score,
                file_hash: result.file_hash.clone(),
                chunk_hash: result.chunk_hash.clone(),
                excerpt: result.excerpt.clone(),
            })
            .collect(),
        first_relevant_rank: evaluation.first_relevant_rank,
        first_relevant_heading_rank: evaluation.first_relevant_heading_rank,
        relevant_paths_found: evaluation.relevant_paths_found,
        hit_at_1: evaluation.hit_at_1,
        recall_at_5: evaluation.recall_at_5,
        reciprocal_rank_at_5: evaluation.reciprocal_rank_at_5,
        no_answer_false_positive: evaluation.no_answer_false_positive,
        context_chars: evaluation.context_chars,
        timing_ms,
        error: evaluation.error.clone(),
    }
}

fn validate_results(chunks: &[Chunk], results: &[SearchResult]) -> Result<()> {
    for result in results {
        let chunk = chunks
            .iter()
            .find(|chunk| {
                chunk.path == result.path
                    && chunk.chunk_hash == result.chunk_hash
                    && chunk.file_hash == result.file_hash
                    && chunk.heading == result.heading
                    && result.line_start >= chunk.line_start
                    && result.line_end <= chunk.line_end
            })
            .ok_or_else(|| anyhow!("result evidence does not identify a frozen chunk"))?;
        if result.line_start < chunk.line_start || result.line_end > chunk.line_end {
            bail!("result line range escapes its chunk");
        }
        let start = result.line_start - chunk.line_start;
        let end = result.line_end - chunk.line_start;
        let lines: Vec<_> = chunk.text.lines().collect();
        if end >= lines.len() {
            bail!("result line range exceeds chunk text");
        }
        let expected: String = lines[start..=end]
            .join("\n")
            .chars()
            .take(MAX_EXCERPT_CHARS)
            .collect();
        if expected != result.excerpt {
            bail!("result excerpt does not match the frozen source lines");
        }
    }
    Ok(())
}

fn summary_value(summary: &EvaluationSummary) -> Value {
    json!({
        "queries_total": summary.queries_total,
        "executed": summary.executed,
        "skipped": summary.skipped,
        "failed": summary.failed,
        "answerable": summary.answerable,
        "no_answer": summary.no_answer,
        "hit_at_1": summary.hit_at_1,
        "recall_at_5_macro": summary.recall_at_5_macro,
        "recall_at_5_micro": summary.recall_at_5_micro,
        "mrr_at_5": summary.mrr_at_5,
        "no_answer_false_positives": summary.no_answer_false_positives,
        "no_answer_false_positive_rate": summary.no_answer_false_positive_rate,
        "context_chars": context_value(&summary.context_chars),
    })
}

fn context_value(context: &ContextStats) -> Value {
    json!({
        "mean": context.mean,
        "p95": context.p95,
        "max": context.max,
        "total": context.total,
    })
}

fn categories() -> [(&'static str, EvaluationCategory); 4] {
    [
        ("exact", EvaluationCategory::Exact),
        ("semantic", EvaluationCategory::Semantic),
        ("ambiguous", EvaluationCategory::Ambiguous),
        ("no_answer", EvaluationCategory::NoAnswer),
    ]
}

fn baseline_comparison(
    baseline_path: &Path,
    expected_sha256: &str,
    input: &ReportInput,
    engine: &str,
    quality: &Value,
    per_category: &BTreeMap<String, Value>,
    queries: &[ObservedQuery],
) -> Result<(Value, BTreeMap<String, Value>)> {
    let bytes = fs::read(baseline_path)
        .with_context(|| format!("failed to read baseline report {}", baseline_path.display()))?;
    let hash = sha256::digest_hex(&bytes);
    if hash != expected_sha256 {
        bail!("baseline report SHA-256 changed after the protocol freeze");
    }
    let baseline: Value =
        serde_json::from_slice(&bytes).context("baseline report is invalid JSON")?;
    if baseline["schema_version"] != 2
        || baseline["engine"] != ENGINE
        || baseline["dataset_hash"] != input.dataset_blake3
        || baseline["corpus"]["fingerprint"] != input.corpus_fingerprint
        || baseline["config"]["limit"] != LIMIT
        || baseline["config"]["max_excerpt_chars"] != MAX_EXCERPT_CHARS
        || !baseline["config"]["max_results_per_path"].is_null()
    {
        bail!("baseline report does not match the frozen input and retrieval contract");
    }
    if matches!(engine, ENGINE | SQLITE_ENGINE) {
        validate_lexical_baseline_equivalence(&baseline, queries)?;
    }

    let current = quality;
    let baseline_summary = &baseline["summary"];
    let metric_delta = |name: &str| -> Result<f64> {
        Ok(round_six(
            required_f64(current, name)? - required_f64(baseline_summary, name)?,
        ))
    };
    let no_answer_delta = required_i64(current, "no_answer_false_positives")?
        - required_i64(baseline_summary, "no_answer_false_positives")?;
    let mut category_deltas = BTreeMap::new();
    for (category, _) in categories() {
        let current = per_category
            .get(category)
            .ok_or_else(|| anyhow!("{category} category summary is missing"))?;
        let baseline_category = &baseline["per_category"][category];
        let mut deltas = serde_json::Map::new();
        for metric in ["hit_at_1", "recall_at_5_macro", "mrr_at_5"] {
            let delta = match (current[metric].as_f64(), baseline_category[metric].as_f64()) {
                (Some(current), Some(baseline)) => Some(round_six(current - baseline)),
                (None, None) => None,
                _ => bail!("baseline category metric availability changed"),
            };
            deltas.insert(metric.to_owned(), serde_json::to_value(delta)?);
        }
        category_deltas.insert(category.to_owned(), Value::Object(deltas));
    }
    let semantic_delta = required_f64(
        category_deltas
            .get("semantic")
            .ok_or_else(|| anyhow!("semantic category delta is missing"))?,
        "recall_at_5_macro",
    )?;
    let current_context = required_f64(&current["context_chars"], "total")?;
    let baseline_context = required_f64(&baseline_summary["context_chars"], "total")?;
    let context_change = if baseline_context == 0.0 {
        if current_context == 0.0 {
            0.0
        } else {
            bail!("cannot calculate context change from a zero baseline")
        }
    } else {
        round_six((current_context - baseline_context) / baseline_context)
    };

    Ok((
        json!({
            "report_sha256": hash,
            "engine": ENGINE,
            "deltas": {
                "hit_at_1": metric_delta("hit_at_1")?,
                "recall_at_5_macro": metric_delta("recall_at_5_macro")?,
                "recall_at_5_micro": metric_delta("recall_at_5_micro")?,
                "mrr_at_5": metric_delta("mrr_at_5")?,
                "no_answer_false_positives": no_answer_delta,
            },
            "semantic_recall_at_5_macro_delta": semantic_delta,
            "context_total_relative_change": context_change,
        }),
        category_deltas,
    ))
}

fn validate_lexical_baseline_equivalence(
    baseline: &Value,
    queries: &[ObservedQuery],
) -> Result<()> {
    let baseline_queries = baseline["queries"]
        .as_array()
        .ok_or_else(|| anyhow!("baseline queries must be an array"))?;
    if baseline_queries.len() != queries.len() {
        bail!("lexical baseline query count changed");
    }
    for (baseline, current) in baseline_queries.iter().zip(queries) {
        if baseline["id"] != current.id
            || baseline["status"] != serde_json::to_value(current.status)?
            || baseline["context_chars"] != current.context_chars
        {
            bail!("lexical baseline query outcome changed for {}", current.id);
        }
        let baseline_results = baseline["results"]
            .as_array()
            .ok_or_else(|| anyhow!("baseline query results must be an array"))?;
        if baseline_results.len() != current.results.len() {
            bail!("lexical baseline result count changed for {}", current.id);
        }
        for (baseline, current_result) in baseline_results.iter().zip(&current.results) {
            if baseline["rank"] != current_result.rank
                || baseline["path"] != current_result.path
                || baseline["heading"] != serde_json::to_value(&current_result.heading)?
                || baseline["line_start"] != current_result.line_start
                || baseline["line_end"] != current_result.line_end
                || baseline["score"].as_f64() != Some(current_result.score)
                || baseline["excerpt_chars"] != current_result.excerpt.chars().count()
            {
                bail!("lexical baseline ranking changed for {}", current.id);
            }
        }
    }
    Ok(())
}

fn timing_value(queries: &[ObservedQuery]) -> Value {
    let lookup: Vec<_> = queries.iter().map(|query| query.timing_ms.lookup).collect();
    let ranking: Vec<_> = queries
        .iter()
        .map(|query| query.timing_ms.ranking)
        .collect();
    let excerpt: Vec<_> = queries
        .iter()
        .map(|query| query.timing_ms.excerpt)
        .collect();
    let total: Vec<_> = queries.iter().map(|query| query.timing_ms.total).collect();
    json!({
        "lookup": latency_value(&latency_stats(&lookup)),
        "ranking": latency_value(&latency_stats(&ranking)),
        "excerpt": latency_value(&latency_stats(&excerpt)),
        "query_total": latency_value(&latency_stats(&total)),
    })
}

fn latency_value(stats: &LatencyStats) -> Value {
    json!({
        "mean": stats.mean,
        "p50": stats.p50,
        "p95": stats.p95,
        "max": stats.max,
        "total": stats.total,
    })
}

fn ranking_projection(queries: &[ObservedQuery]) -> Result<String> {
    let projection: Vec<_> = queries
        .iter()
        .flat_map(|query| {
            query.results.iter().map(|result| {
                json!([
                    query.id,
                    result.rank,
                    result.path,
                    result.heading,
                    result.line_start,
                    result.line_end,
                    result.file_hash,
                    result.chunk_hash,
                ])
            })
        })
        .collect();
    canonical_json_sha256(&json!(projection))
}

fn evidence_projection(queries: &[ObservedQuery]) -> Result<String> {
    let projection: Vec<_> = queries
        .iter()
        .flat_map(|query| {
            query.results.iter().map(|result| {
                json!([
                    query.id,
                    result.rank,
                    result.path,
                    result.heading,
                    result.line_start,
                    result.line_end,
                    result.file_hash,
                    result.chunk_hash,
                    sha256::digest_hex(result.excerpt.as_bytes()),
                ])
            })
        })
        .collect();
    canonical_json_sha256(&json!(projection))
}

fn validate_observation(
    observation: &Observation,
    protocol: &Value,
    protocol_sha256: &str,
) -> Result<()> {
    if observation.schema_version != BAKEOFF_OBSERVATION_SCHEMA_VERSION
        || observation.protocol_tag != BAKEOFF_PROTOCOL_TAG
        || observation.protocol_sha256 != protocol_sha256
        || ensure_engine_available(&observation.engine).is_err()
        || !matches!(observation.run, 1 | 2)
        || observation.input.role != "development"
        || observation.retrieval.limit != LIMIT
        || observation.retrieval.recall_cutoff != RECALL_CUTOFF
        || observation.retrieval.max_excerpt_chars != MAX_EXCERPT_CHARS
        || observation.retrieval.max_results_per_path.is_some()
        || !observation.evidence_valid
    {
        bail!("observation does not match the frozen Phase 4.1 contract");
    }
    validate_provenance(&observation.provenance, protocol)?;
    let input = protocol_input(protocol, &observation.input.id)?;
    if required_string(input, "root")? != observation.input.root
        || required_string(input, "queries_sha256")? != observation.input.queries_sha256
        || required_string(input, "dataset_blake3")? != observation.input.dataset_blake3
        || required_string(input, "corpus_fingerprint")? != observation.input.corpus_fingerprint
        || required_usize(input, "files")? != observation.input.files
        || required_usize(input, "chunks")? != observation.input.chunks
        || required_usize(input, "query_count")? != observation.input.query_count
        || observation.input.queries_schema_version != 2
    {
        bail!("observation input does not match the protocol");
    }
    let config = engine_configuration(protocol, &observation.engine)?;
    validate_frozen_engine_configuration(&observation.engine, config)?;
    if canonical_json_sha256(config)? != observation.engine_config_sha256
        || config != &observation.engine_configuration
    {
        bail!("observation engine configuration does not match the protocol");
    }

    let query_path = PathBuf::from(required_string(input, "queries")?);
    verify_file_sha256(
        &query_path,
        required_string(input, "queries_sha256")?,
        "query dataset",
    )?;
    let dataset = fs::read(&query_path)?;
    if blake3::hash(&dataset).to_hex().as_str() != observation.input.dataset_blake3 {
        bail!("observation dataset BLAKE3 changed");
    }
    let set = parse_evaluation_set(std::str::from_utf8(&dataset)?)?;
    let root = Path::new(&observation.input.root);
    let expected_index = match observation.engine.as_str() {
        SQLITE_ENGINE => Some(crate::sqlite_cache::cache_path(root)?),
        FTS5_ENGINE => Some(crate::fts5::index_path_for_root(root)?),
        EMBEDDINGS_ENGINE => Some(crate::embeddings::index_path_for_root(root)?),
        _ => None,
    };
    if let Some(expected_index) = expected_index
        && observation.index_path.as_deref() != Some(expected_index.to_string_lossy().as_ref())
    {
        bail!("indexed observation path differs from the deterministic cache path");
    }
    let loaded = corpus::load(root)?;
    let mut validation_engine = engine(
        &observation.engine,
        &observation.engine_config_sha256,
        protocol.get("model"),
    )?;
    validation_engine.open_existing(root, &loaded)?;
    if validation_engine.fault_injection_passed(root) != observation.fault_injection_passed {
        bail!("observation fault injection result is not reproducible");
    }
    verify_input_corpus(
        input,
        &loaded,
        &corpus_fingerprint(&loaded.file_hashes),
        set.queries.len(),
    )?;
    validate_engine_observation(observation)?;
    let (evaluations, recomputed_candidates) = recompute_observation(
        &set.queries,
        &observation.queries,
        &loaded.chunks,
        root,
        validation_engine.as_ref(),
    )?;
    let quality = summary_value(&summarize(evaluations.iter()));
    let mut per_category = BTreeMap::new();
    for (name, category) in categories() {
        per_category.insert(
            name.to_owned(),
            summary_value(&summarize(
                evaluations
                    .iter()
                    .filter(|evaluation| evaluation.category == category),
            )),
        );
    }
    let expected_errors: Vec<_> = observation
        .queries
        .iter()
        .filter_map(|query| {
            query
                .error
                .as_ref()
                .map(|error| format!("{}: {error}", query.id))
        })
        .collect();
    if quality != observation.quality
        || per_category != observation.per_category
        || timing_value(&observation.queries) != observation.timing_ms
        || expected_errors != observation.errors
    {
        bail!("observation metrics or timings are not reproducible");
    }
    if observation.retrieval.candidates_examined != recomputed_candidates {
        bail!("observation candidate count differs from the engine rerun");
    }
    if matches!(
        observation.engine.as_str(),
        ENGINE | SQLITE_ENGINE | EMBEDDINGS_ENGINE
    ) {
        let expected_candidates = observation
            .input
            .chunks
            .checked_mul(required_usize(&observation.quality, "executed")?)
            .ok_or_else(|| anyhow!("candidate counter overflow"))?;
        if observation.retrieval.candidates_examined != expected_candidates {
            bail!("BM25 observation candidate count does not equal all chunks per executed query");
        }
    } else if observation.engine == FTS5_ENGINE {
        let maximum = FTS5_CANDIDATE_DEPTH
            .checked_mul(required_usize(&observation.quality, "executed")?)
            .ok_or_else(|| anyhow!("FTS5 candidate counter overflow"))?;
        if observation.retrieval.candidates_examined > maximum {
            bail!("FTS5 observation exceeds the frozen candidate depth");
        }
    }
    let baseline_path = PathBuf::from(required_string(input, "baseline_report")?);
    let (expected_baseline, expected_category_deltas) = baseline_comparison(
        &baseline_path,
        required_string(input, "baseline_report_sha256")?,
        &observation.input,
        &observation.engine,
        &quality,
        &per_category,
        &observation.queries,
    )?;
    if expected_baseline != observation.baseline_comparison
        || expected_category_deltas != observation.category_deltas
    {
        bail!("observation baseline comparison is not reproducible");
    }
    if ranking_projection(&observation.queries)? != observation.ranking_projection_sha256
        || evidence_projection(&observation.queries)? != observation.evidence_projection_sha256
    {
        bail!("observation determinism projection is invalid");
    }
    Ok(())
}

fn validate_engine_observation(observation: &Observation) -> Result<()> {
    if observation.normal_fallback_count != 0 {
        bail!("normal bake-off queries cannot use fallback");
    }
    match observation.engine.as_str() {
        ENGINE => {
            if observation.indexing != IndexingMetrics::direct()
                || observation.sqlite_runtime.is_some()
                || observation.index_path.is_some()
            {
                bail!("direct BM25 observation contains indexed-engine state");
            }
        }
        SQLITE_ENGINE | FTS5_ENGINE | EMBEDDINGS_ENGINE => {
            let full_build = observation
                .indexing
                .full_build_ms
                .ok_or_else(|| anyhow!("indexed observation lacks full build timing"))?;
            let operations: Vec<_> = observation
                .indexing
                .incremental_steps
                .iter()
                .map(|step| step.operation.as_str())
                .collect();
            let expected = ["add", "modify", "rename", "remove"];
            if !full_build.is_finite()
                || full_build < 0.0
                || operations != expected
                || observation.indexing.incremental_steps.iter().any(|step| {
                    !step.elapsed_ms.is_finite()
                        || step.elapsed_ms < 0.0
                        || !step.equivalent_to_full_rebuild
                        || step.stale_results != 0
                })
                || !observation.indexing.rebuild_succeeded
                || !observation.indexing.corruption_detected
                || observation
                    .indexing
                    .runtime_checks
                    .values()
                    .any(|passed| !passed)
                || observation.index_path.as_deref().is_none_or(str::is_empty)
            {
                bail!("indexed observation state is invalid");
            }
            if observation.engine == EMBEDDINGS_ENGINE {
                let expected_checks = BTreeMap::from([
                    ("attention_mask_mean_pooling".to_owned(), true),
                    ("candle_cpu_f32".to_owned(), true),
                    ("model_artifacts_sha256_verified".to_owned(), true),
                    ("tokenizer_truncation_right_512".to_owned(), true),
                ]);
                if observation.sqlite_runtime.is_some()
                    || observation.indexing.runtime_checks != expected_checks
                {
                    bail!("embeddings observation runtime provenance is invalid");
                }
            } else {
                if observation.indexing.runtime_checks.len() != 3 {
                    bail!("SQLite indexed observation runtime checks are invalid");
                }
                let runtime = observation.sqlite_runtime.as_ref().ok_or_else(|| {
                    anyhow!("indexed observation lacks SQLite runtime provenance")
                })?;
                let actual = crate::sqlite_cache::validate_runtime()?;
                if runtime.version != actual.version
                    || runtime.compile_options_sha256 != actual.compile_options_sha256
                    || observation.indexing.runtime_checks != actual.checks
                {
                    bail!("indexed observation SQLite runtime provenance is invalid");
                }
            }
        }
        _ => bail!("engine observation validation is unavailable"),
    }
    Ok(())
}

fn recompute_observation(
    queries: &[EvaluationQuery],
    observed: &[ObservedQuery],
    chunks: &[Chunk],
    root: &Path,
    engine: &dyn BakeoffEngine,
) -> Result<(Vec<crate::evaluate::QueryEvaluation>, usize)> {
    if queries.len() != observed.len() {
        bail!("observation query count changed");
    }
    let recomputed: Vec<(crate::evaluate::QueryEvaluation, usize)> = queries
        .iter()
        .zip(observed)
        .map(|(query, observed)| {
            if query.id != observed.id || query.category != observed.category {
                bail!("observation query identity or category changed");
            }
            let results: Vec<_> = observed
                .results
                .iter()
                .map(|result| SearchResult {
                    rank: result.rank,
                    raw_rank: result.rank,
                    path: result.path.clone(),
                    heading: result.heading.clone(),
                    line_start: result.line_start,
                    line_end: result.line_end,
                    excerpt: result.excerpt.clone(),
                    file_hash: result.file_hash.clone(),
                    chunk_hash: result.chunk_hash.clone(),
                    score: result.score,
                    matched_terms: Vec::new(),
                })
                .collect();
            validate_results(chunks, &results)?;
            let (evaluation, candidates_examined) = match observed.status {
                QueryStatus::Ok => {
                    let (response, timings) = engine.search(SearchRequest {
                        root: root.to_path_buf(),
                        query: query.query.clone(),
                        limit: LIMIT,
                        max_excerpt_chars: MAX_EXCERPT_CHARS,
                        max_results_per_path: None,
                    })?;
                    let rerun_results: Vec<_> = response
                        .results
                        .iter()
                        .map(|result| ObservedResult {
                            rank: result.rank,
                            path: result.path.clone(),
                            heading: result.heading.clone(),
                            line_start: result.line_start,
                            line_end: result.line_end,
                            score: result.score,
                            file_hash: result.file_hash.clone(),
                            chunk_hash: result.chunk_hash.clone(),
                            excerpt: result.excerpt.clone(),
                        })
                        .collect();
                    if rerun_results != observed.results {
                        bail!(
                            "observation ranking differs from the engine for {}",
                            query.id
                        );
                    }
                    (
                        successful_query_evaluation(
                            query.clone(),
                            results.clone(),
                            observed.timing_ms.total,
                        ),
                        timings.candidates_examined,
                    )
                }
                QueryStatus::Error => {
                    if !results.is_empty() || observed.error.is_none() {
                        bail!("failed observation query has results or lacks an error");
                    }
                    let mut evaluation = base_query_evaluation(query.clone(), QueryStatus::Error);
                    evaluation.latency_ms = Some(observed.timing_ms.total);
                    evaluation.error = observed.error.clone();
                    (evaluation, 0)
                }
                QueryStatus::Skipped => bail!("bake-off observations cannot skip queries"),
            };
            if observed_query(&evaluation, &results, observed.timing_ms) != *observed {
                bail!("observation query metrics changed");
            }
            Ok((evaluation, candidates_examined))
        })
        .collect::<Result<_>>()?;
    let candidates_examined = recomputed.iter().try_fold(0usize, |total, (_, count)| {
        total
            .checked_add(*count)
            .ok_or_else(|| anyhow!("candidate counter overflow during observation recomputation"))
    })?;
    Ok((
        recomputed
            .into_iter()
            .map(|(evaluation, _)| evaluation)
            .collect(),
        candidates_examined,
    ))
}

fn validate_pair(current: &Observation, other: &Observation) -> Result<()> {
    if current.run == other.run
        || !matches!((current.run, other.run), (1, 2) | (2, 1))
        || current.protocol_sha256 != other.protocol_sha256
        || current.engine != other.engine
        || current.engine_config_sha256 != other.engine_config_sha256
        || current.provenance != other.provenance
        || current.input.id != other.input.id
        || current.input.corpus_fingerprint != other.input.corpus_fingerprint
        || current.input.queries_sha256 != other.input.queries_sha256
    {
        bail!("observations do not form the two frozen runs of one engine/input pair");
    }
    Ok(())
}

fn validate_measurements(
    measurements: &ExternalMeasurements,
    protocol: &Value,
    observation: &Observation,
    observation_sha256: &str,
) -> Result<()> {
    let host = &protocol["measurement"]["host"];
    if measurements.schema_version != 1
        || measurements.protocol_sha256 != observation.protocol_sha256
        || measurements.input_id != observation.input.id
        || measurements.engine != observation.engine
        || measurements.run != observation.run
        || measurements.observation_sha256 != observation_sha256
        || measurements.tool != observation.provenance.tool
        || validate_tool(&measurements.tool).is_err()
        || measurements.environment.os != required_string(host, "os")?
        || measurements.environment.architecture != required_string(host, "architecture")?
        || measurements.environment.machine != required_string(host, "machine")?
        || measurements.environment.cpu != required_string(host, "cpu")?
        || measurements.environment.logical_cpus != required_usize(host, "logical_cpus")?
        || measurements.environment.memory_bytes != required_u64(host, "memory_bytes")?
        || measurements
            .environment
            .load_average
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        || !measurements.timing_ms.startup.is_finite()
        || !measurements.timing_ms.end_to_end.is_finite()
        || measurements.timing_ms.startup < 0.0
        || measurements.timing_ms.end_to_end < measurements.timing_ms.startup
        || !looks_like_utc_timestamp(&measurements.started_at)
        || !looks_like_utc_timestamp(&measurements.finished_at)
    {
        bail!("external measurements do not match the common bake-off contract");
    }
    match observation.engine.as_str() {
        ENGINE => {
            if measurements.environment.sqlite_version.is_some()
                || measurements
                    .environment
                    .sqlite_compile_options_sha256
                    .is_some()
                || measurements.resources_bytes.index_logical != 0
                || measurements.resources_bytes.index_allocated != 0
            {
                bail!("direct BM25 measurements contain SQLite or index state");
            }
        }
        SQLITE_ENGINE | FTS5_ENGINE => {
            if measurements.resources_bytes.shared_model != 0 {
                bail!("SQLite indexed-engine measurements contain shared model bytes");
            }
            let runtime = observation
                .sqlite_runtime
                .as_ref()
                .ok_or_else(|| anyhow!("indexed-engine SQLite runtime provenance is missing"))?;
            let index_path = Path::new(
                observation
                    .index_path
                    .as_deref()
                    .ok_or_else(|| anyhow!("indexed-engine path is missing"))?,
            );
            let metadata = fs::symlink_metadata(index_path)?;
            let logical = metadata.len();
            let allocated = metadata.blocks().saturating_mul(512);
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || measurements.environment.sqlite_version.as_deref()
                    != Some(runtime.version.as_str())
                || measurements
                    .environment
                    .sqlite_compile_options_sha256
                    .as_deref()
                    != Some(runtime.compile_options_sha256.as_str())
                || measurements.resources_bytes.index_logical != logical
                || measurements.resources_bytes.index_allocated != allocated
                || logical == 0
                || allocated == 0
            {
                bail!("indexed-engine measurements do not match the observed runtime or index");
            }
        }
        EMBEDDINGS_ENGINE => {
            let index_path = Path::new(
                observation
                    .index_path
                    .as_deref()
                    .ok_or_else(|| anyhow!("embeddings index path is missing"))?,
            );
            let metadata = fs::symlink_metadata(index_path)?;
            let logical = metadata.len();
            let allocated = metadata.blocks().saturating_mul(512);
            let expected_model = required_u64(
                &protocol["budgets"]["resources_bytes"],
                "shared_model_exact",
            )?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || measurements.environment.sqlite_version.is_some()
                || measurements
                    .environment
                    .sqlite_compile_options_sha256
                    .is_some()
                || measurements.resources_bytes.index_logical != logical
                || measurements.resources_bytes.index_allocated != allocated
                || measurements.resources_bytes.shared_model != expected_model
                || logical == 0
                || allocated == 0
            {
                bail!("embeddings measurements do not match the observed index or frozen model");
            }
        }
        _ => bail!("measurement validation is unavailable for the engine"),
    }
    if observation.engine != EMBEDDINGS_ENGINE && measurements.resources_bytes.shared_model != 0 {
        bail!("non-embedding measurements contain shared model bytes");
    }
    Ok(())
}

fn validate_tool(tool: &ToolMeasurement) -> Result<()> {
    if tool.version != env!("CARGO_PKG_VERSION")
        || tool.profile != "release"
        || tool.rust_version.trim().is_empty()
        || !is_hex(&tool.git_revision, 40)
        || !is_hex(&tool.binary_sha256, 64)
        || !is_hex(&tool.cargo_lock_sha256, 64)
    {
        bail!("tool provenance is invalid");
    }
    let cargo_lock = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock");
    verify_file_sha256(
        &cargo_lock,
        &tool.cargo_lock_sha256,
        "implementation Cargo.lock",
    )
}

fn validate_provenance(provenance: &ObservationProvenance, protocol: &Value) -> Result<()> {
    validate_tool(&provenance.tool)?;
    if provenance.tool.rust_version != required_string(&protocol["toolchain"], "rust_version")?
        || provenance.cargo_version != required_string(&protocol["toolchain"], "cargo_version")?
    {
        bail!("Rust toolchain differs from the frozen protocol");
    }
    if !provenance.host_verified
        || provenance.host_sha256 != canonical_json_sha256(&protocol["measurement"]["host"])?
    {
        bail!("benchmark host attestation is invalid");
    }
    let expected_environment = protocol["measurement"]["process_environment"]
        .as_object()
        .ok_or_else(|| anyhow!("protocol process environment must be an object"))?;
    if provenance.process_environment.len() != expected_environment.len() {
        bail!("process environment does not match the frozen protocol");
    }
    for (name, expected) in expected_environment {
        let expected = expected
            .as_str()
            .ok_or_else(|| anyhow!("protocol process environment value must be a string"))?;
        if provenance.process_environment.get(name).map(String::as_str) != Some(expected)
            || std::env::var(name).ok().as_deref() != Some(expected)
        {
            bail!("process environment variable {name} does not match the frozen protocol");
        }
    }
    if !provenance.isolation.verified_before_path_resolution
        || provenance.isolation.denylist_sha256
            != required_string(&protocol["consumed_holdout_block"], "denylist_sha256")?
    {
        bail!("consumed-holdout isolation attestation is invalid");
    }
    Ok(())
}

fn validate_report_tag(tag: &str, input_id: &str, engine: &str, run: u8) -> Result<()> {
    let expected = format!("engine-bakeoff-v1-{input_id}-{engine}-run-{run}");
    if tag != expected {
        bail!("report tag must equal {expected}");
    }
    Ok(())
}

fn run_variation_passed(
    current: &Observation,
    other: &Observation,
    protocol: &Value,
) -> Result<bool> {
    let current_p95 = required_f64(&current.timing_ms["query_total"], "p95")?;
    let other_p95 = required_f64(&other.timing_ms["query_total"], "p95")?;
    let absolute = (current_p95 - other_p95).abs();
    let denominator = current_p95.min(other_p95).max(f64::EPSILON);
    let relative = absolute / denominator;
    let budget = &protocol["budgets"]["latency_ms"]["run_variation"];
    Ok(!(relative > required_f64(budget, "relative_max")?
        && absolute > required_f64(budget, "absolute_p95_ms_max")?))
}

fn budget_checks(
    observation: &Observation,
    measurements: &ExternalMeasurements,
    protocol: &Value,
    deterministic: bool,
    run_variation: bool,
) -> Result<BTreeMap<String, bool>> {
    let quality_budget = &protocol["budgets"]["quality"];
    let deltas = &observation.baseline_comparison["deltas"];
    let global_quality = [
        ("hit_at_1", "hit_at_1_max_absolute_drop"),
        ("recall_at_5_macro", "recall_at_5_macro_max_absolute_drop"),
        ("recall_at_5_micro", "recall_at_5_micro_max_absolute_drop"),
        ("mrr_at_5", "mrr_at_5_max_absolute_drop"),
    ]
    .iter()
    .all(|(metric, budget)| {
        required_f64(deltas, metric)
            .and_then(|delta| Ok(delta >= -required_f64(quality_budget, budget)?))
            .unwrap_or(false)
    });
    let category_quality = [
        ("exact", "hit_at_1", "exact_hit_at_1_max_absolute_drop"),
        (
            "exact",
            "recall_at_5_macro",
            "exact_recall_at_5_macro_max_absolute_drop",
        ),
        ("exact", "mrr_at_5", "exact_mrr_at_5_max_absolute_drop"),
        (
            "ambiguous",
            "hit_at_1",
            "ambiguous_hit_at_1_max_absolute_drop",
        ),
        (
            "ambiguous",
            "recall_at_5_macro",
            "ambiguous_recall_at_5_macro_max_absolute_drop",
        ),
        (
            "ambiguous",
            "mrr_at_5",
            "ambiguous_mrr_at_5_max_absolute_drop",
        ),
    ]
    .iter()
    .all(|(category, metric, budget)| {
        required_f64(&observation.category_deltas[*category], metric)
            .and_then(|delta| Ok(delta >= -required_f64(quality_budget, budget)?))
            .unwrap_or(false)
    });
    let quality_passed = global_quality && category_quality;
    let no_answer_passed = required_i64(deltas, "no_answer_false_positives")? <= 0;
    let execution_passed = required_usize(&observation.quality, "failed")? == 0
        && required_usize(&observation.quality, "skipped")? == 0;
    let context_passed = required_f64(
        &observation.baseline_comparison,
        "context_total_relative_change",
    )? <= required_f64(
        &protocol["budgets"]["context"],
        "max_relative_total_increase",
    )?;
    let latency_budget = &protocol["budgets"]["latency_ms"];
    let startup_cap = required_f64(
        &latency_budget["startup_max_by_engine"],
        &observation.engine,
    )?;
    let query_cap = required_f64(
        &latency_budget["query_p95_max_by_engine"],
        &observation.engine,
    )?;
    let query_p95 = required_f64(&observation.timing_ms["query_total"], "p95")?;
    let resource_budget = &protocol["budgets"]["resources_bytes"];
    let memory_cap = required_u64(
        &resource_budget["peak_rss_max_by_engine"],
        &observation.engine,
    )?;
    let disk_cap = required_u64(
        &resource_budget["index_storage_max_by_engine"],
        &observation.engine,
    )?;
    let (index_build, incremental_equivalence) = match observation.engine.as_str() {
        ENGINE => (observation.indexing.full_build_ms.is_none(), true),
        SQLITE_ENGINE | FTS5_ENGINE | EMBEDDINGS_ENGINE => {
            let indexing_budget = &protocol["budgets"]["indexing_ms"];
            let full_build = observation.indexing.full_build_ms.unwrap_or(f64::INFINITY);
            let full_cap = required_f64(
                &indexing_budget["full_build_max_by_engine"],
                &observation.engine,
            )?;
            let relative_cap = required_f64(
                indexing_budget,
                "incremental_step_max_relative_to_full_build",
            )?;
            (
                full_build <= full_cap,
                observation.indexing.incremental_steps.len() == 4
                    && observation.indexing.incremental_steps.iter().all(|step| {
                        step.equivalent_to_full_rebuild
                            && step.stale_results == 0
                            && step.elapsed_ms <= full_build * relative_cap
                    }),
            )
        }
        _ => (false, false),
    };

    Ok(BTreeMap::from([
        ("quality".to_owned(), quality_passed),
        ("no_answer".to_owned(), no_answer_passed),
        ("query_execution".to_owned(), execution_passed),
        ("evidence".to_owned(), observation.evidence_valid),
        ("context".to_owned(), context_passed),
        (
            "startup".to_owned(),
            measurements.timing_ms.startup <= startup_cap,
        ),
        ("query_latency".to_owned(), query_p95 <= query_cap),
        ("index_build".to_owned(), index_build),
        (
            "incremental_equivalence".to_owned(),
            incremental_equivalence,
        ),
        (
            "memory".to_owned(),
            measurements.resources_bytes.peak_rss <= memory_cap,
        ),
        (
            "disk".to_owned(),
            measurements.resources_bytes.index_allocated <= disk_cap
                && measurements.resources_bytes.index_logical <= disk_cap,
        ),
        ("determinism".to_owned(), deterministic),
        ("rebuild".to_owned(), observation.indexing.rebuild_succeeded),
        ("fallback".to_owned(), observation.fault_injection_passed),
        (
            "holdout_isolation".to_owned(),
            observation
                .provenance
                .isolation
                .verified_before_path_resolution,
        ),
        ("run_variation".to_owned(), run_variation),
        (
            "shared_model_size".to_owned(),
            if observation.engine == EMBEDDINGS_ENGINE {
                measurements.resources_bytes.shared_model
                    == required_u64(resource_budget, "shared_model_exact")?
            } else {
                measurements.resources_bytes.shared_model == 0
            },
        ),
    ]))
}

fn report_queries(queries: &[ObservedQuery], engine: &str) -> Value {
    Value::Array(
        queries
            .iter()
            .map(|query| {
                json!({
                    "id": query.id,
                    "category": query.category,
                    "status": query.status,
                    "results": query.results.iter().map(|result| json!({
                        "rank": result.rank,
                        "path": result.path,
                        "heading": result.heading,
                        "line_start": result.line_start,
                        "line_end": result.line_end,
                        "score": result.score,
                        "file_hash": result.file_hash,
                        "chunk_hash": result.chunk_hash,
                        "excerpt_chars": result.excerpt.chars().count(),
                        "source_ranks": {
                            "fts5": (engine == FTS5_ENGINE).then_some(result.rank),
                            "embedding": (engine == EMBEDDINGS_ENGINE).then_some(result.rank)
                        },
                    })).collect::<Vec<_>>(),
                    "first_relevant_rank": query.first_relevant_rank,
                    "first_relevant_heading_rank": query.first_relevant_heading_rank,
                    "relevant_paths_found": query.relevant_paths_found,
                    "hit_at_1": query.hit_at_1,
                    "recall_at_5": query.recall_at_5,
                    "reciprocal_rank_at_5": query.reciprocal_rank_at_5,
                    "no_answer_false_positive": query.no_answer_false_positive,
                    "context_chars": query.context_chars,
                    "timing_ms": query.timing_ms,
                    "error": query.error,
                })
            })
            .collect(),
    )
}

fn merge_timing(inner: &Value, outer: &OuterTiming) -> Result<Value> {
    for field in ["lookup", "ranking", "excerpt", "query_total"] {
        if !inner[field].is_object() {
            bail!("observation timing is missing {field}");
        }
    }
    Ok(json!({
        "startup": outer.startup,
        "lookup": inner["lookup"],
        "ranking": inner["ranking"],
        "excerpt": inner["excerpt"],
        "query_total": inner["query_total"],
        "end_to_end": outer.end_to_end,
    }))
}

fn canonical_json_sha256(value: &Value) -> Result<String> {
    Ok(sha256::digest_hex(canonical_json(value)?.as_bytes()))
}

fn canonical_json(value: &Value) -> Result<String> {
    match value {
        Value::Null => Ok("null".to_owned()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                return Ok(value.to_string());
            }
            if let Some(value) = value.as_u64() {
                return Ok(value.to_string());
            }
            let value = value
                .as_f64()
                .ok_or_else(|| anyhow!("canonical JSON number is not finite"))?;
            if !value.is_finite() {
                bail!("canonical JSON does not support non-finite numbers");
            }
            if value == 0.0 {
                return Ok("0".to_owned());
            }
            if value.fract() == 0.0 && value.abs() < 1e21 {
                return Ok(format!("{value:.0}"));
            }
            Ok(value.to_string())
        }
        Value::String(value) => Ok(serde_json::to_string(value)?),
        Value::Array(values) => Ok(format!(
            "[{}]",
            values
                .iter()
                .map(canonical_json)
                .collect::<Result<Vec<_>>>()?
                .join(",")
        )),
        Value::Object(values) => {
            if values.keys().any(|key| !key.is_ascii()) {
                bail!("Phase 4.1 canonical JSON requires ASCII object keys");
            }
            let mut keys: Vec<_> = values.keys().collect();
            keys.sort_unstable();
            let fields = keys
                .into_iter()
                .map(|key| {
                    Ok(format!(
                        "{}:{}",
                        serde_json::to_string(key)?,
                        canonical_json(&values[key])?
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(format!("{{{}}}", fields.join(",")))
        }
    }
}

fn write_ready_file(path: &Path) -> Result<()> {
    write_bytes_new(path, b"ready\n", "ready marker")
}

fn write_json_new<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_bytes_new(path, &bytes, "bake-off observation")
}

fn write_json_value_new(path: &Path, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_bytes_new(path, &bytes, "bake-off report")
}

fn write_bytes_new(path: &Path, bytes: &[u8], label: &str) -> Result<()> {
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("failed to create new {label} {}", path.display()))?;
    output
        .write_all(bytes)
        .with_context(|| format!("failed to write {label} {}", path.display()))?;
    output
        .sync_all()
        .with_context(|| format!("failed to sync {label} {}", path.display()))?;
    let mut permissions = output.metadata()?.permissions();
    use std::os::unix::fs::PermissionsExt;
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> Result<T> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read {label} {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("invalid {label} JSON"))
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .ok_or_else(|| anyhow!("{field} must be a string"))
}

fn required_f64(value: &Value, field: &str) -> Result<f64> {
    value[field]
        .as_f64()
        .ok_or_else(|| anyhow!("{field} must be a number"))
}

fn required_i64(value: &Value, field: &str) -> Result<i64> {
    value[field]
        .as_i64()
        .ok_or_else(|| anyhow!("{field} must be an integer"))
}

fn required_u64(value: &Value, field: &str) -> Result<u64> {
    value[field]
        .as_u64()
        .ok_or_else(|| anyhow!("{field} must be a non-negative integer"))
}

fn required_usize(value: &Value, field: &str) -> Result<usize> {
    usize::try_from(required_u64(value, field)?).map_err(|_| anyhow!("{field} does not fit usize"))
}

fn elapsed_ms(started: Instant) -> f64 {
    (started.elapsed().as_secs_f64() * 1_000_000.0).round() / 1_000.0
}

fn is_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn looks_like_utc_timestamp(value: &str) -> bool {
    value.len() >= 20 && value.contains('T') && value.ends_with('Z')
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn canonical_json_sorts_keys_and_normalizes_integral_floats() {
        let value = json!({"z": 1.0, "a": [true, 0.75, "x"]});
        assert_eq!(
            canonical_json(&value).unwrap(),
            r#"{"a":[true,0.75,"x"],"z":1}"#
        );
        assert_eq!(
            canonical_json_sha256(&lexical_configuration()).unwrap(),
            LEXICAL_CONFIG_SHA256
        );
        assert_eq!(
            canonical_json_sha256(&sqlite_configuration()).unwrap(),
            SQLITE_CONFIG_SHA256
        );
        assert_eq!(
            canonical_json_sha256(&fts5_configuration()).unwrap(),
            FTS5_CONFIG_SHA256
        );
        assert_eq!(
            canonical_json_sha256(&embedding_configuration()).unwrap(),
            EMBEDDINGS_CONFIG_SHA256
        );
        let mut tampered = fts5_configuration();
        tampered["candidate_depth"] = json!(49);
        assert!(validate_frozen_engine_configuration(FTS5_ENGINE, &tampered).is_err());
    }

    #[test]
    fn report_writer_refuses_overwrite_and_uses_private_mode() {
        let directory = tempdir().unwrap();
        let output = directory.path().join("report.json");
        write_json_value_new(&output, &json!({"ok": true})).unwrap();
        assert_eq!(
            output.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(write_json_value_new(&output, &json!({"ok": false})).is_err());
    }

    #[test]
    fn report_tag_is_bound_to_its_run() {
        validate_report_tag(
            "engine-bakeoff-v1-stable-v1-lexical-bm25-v1-run-1",
            "stable-v1",
            ENGINE,
            1,
        )
        .unwrap();
        assert!(
            validate_report_tag(
                "engine-bakeoff-v1-stable-v1-lexical-bm25-v1-run-2",
                "stable-v1",
                ENGINE,
                1,
            )
            .is_err()
        );
        assert!(validate_report_tag("Engine-bakeoff-v1-run-1", "stable-v1", ENGINE, 1).is_err());
    }

    #[test]
    fn unavailable_engine_is_explicit() {
        let Err(error) = ensure_engine_available("hybrid-rrf-v1") else {
            panic!("hybrid must remain unavailable before Phase 4.5");
        };
        assert!(error.to_string().contains("not implemented"));
        let Err(error) = ensure_engine_available("unknown") else {
            panic!("unknown engine must fail");
        };
        assert!(error.to_string().contains("unknown"));
    }

    #[test]
    fn evidence_validation_disambiguates_duplicate_chunks_by_line_range() {
        let chunk = |line_start| Chunk {
            path: "README.md".to_owned(),
            heading: Some("Setup".to_owned()),
            line_start,
            line_end: line_start,
            text: "repeatable evidence".to_owned(),
            file_hash: "file-hash".to_owned(),
            chunk_hash: "chunk-hash".to_owned(),
        };
        let chunks = vec![chunk(5), chunk(20)];
        let result = SearchResult {
            rank: 1,
            raw_rank: 1,
            path: "README.md".to_owned(),
            heading: Some("Setup".to_owned()),
            line_start: 20,
            line_end: 20,
            excerpt: "repeatable evidence".to_owned(),
            file_hash: "file-hash".to_owned(),
            chunk_hash: "chunk-hash".to_owned(),
            score: 1.0,
            matched_terms: vec!["repeatable".to_owned()],
        };
        validate_results(&chunks, &[result]).unwrap();
    }

    #[test]
    fn paired_observations_finalize_schema_shaped_direct_sqlite_and_fts5_reports() {
        let directory = tempdir().unwrap();
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = manifest_dir.join("evaluation/fixtures/stable-v1");
        let queries = manifest_dir.join("evaluation/queries.json");
        let loaded = corpus::load(&root).unwrap();
        let fingerprint = corpus_fingerprint(&loaded.file_hashes);
        let dataset = fs::read(&queries).unwrap();
        let query_set = parse_evaluation_set(std::str::from_utf8(&dataset).unwrap()).unwrap();

        let baseline = crate::evaluate(crate::EvaluateRequest {
            root: root.clone(),
            queries_path: queries.clone(),
            limit: LIMIT,
            max_excerpt_chars: MAX_EXCERPT_CHARS,
            max_results_per_path: None,
        })
        .unwrap();
        let baseline_path = directory.path().join("baseline.json");
        fs::write(
            &baseline_path,
            format!("{}\n", serde_json::to_string_pretty(&baseline).unwrap()),
        )
        .unwrap();
        let baseline_hash = sha256::digest_hex(&fs::read(&baseline_path).unwrap());
        let snapshot_manifest = directory.path().join("snapshot-manifest.json");
        fs::write(&snapshot_manifest, "{}\n").unwrap();
        let snapshot_hash = sha256::digest_hex(&fs::read(&snapshot_manifest).unwrap());
        let lexical_config = lexical_configuration();
        let protocol = json!({
            "schema_version": 1,
            "protocol_tag": BAKEOFF_PROTOCOL_TAG,
            "status": "frozen-before-implementation",
            "scope": "development-only",
            "policy": {
                "holdouts_allowed": false,
                "default_engine": ENGINE,
                "runs_per_engine_input": 2
            },
            "consumed_holdout_block": {
                "denylist_sha256": "2".repeat(64)
            },
            "toolchain": {
                "rust_version": "test-rust",
                "cargo_version": "test-cargo"
            },
            "measurement": {
                "process_environment": {},
                "host": {
                    "os": "test-os",
                    "architecture": "arm64",
                    "machine": "test-machine",
                    "cpu": "test-cpu",
                    "logical_cpus": 1,
                    "memory_bytes": 1
                }
            },
            "engines": [
                {"id": ENGINE, "configuration": lexical_config},
                {"id": SQLITE_ENGINE, "configuration": sqlite_configuration()},
                {"id": FTS5_ENGINE, "configuration": fts5_configuration()}
            ],
            "inputs": [{
                "id": "stable-v1",
                "role": "development",
                "root": root,
                "queries": queries,
                "queries_sha256": sha256::digest_hex(&dataset),
                "dataset_blake3": blake3::hash(&dataset).to_hex().to_string(),
                "query_count": query_set.queries.len(),
                "files": loaded.files,
                "chunks": loaded.chunks.len(),
                "corpus_fingerprint": fingerprint,
                "baseline_report": baseline_path,
                "baseline_report_sha256": baseline_hash,
                "snapshot_manifest": snapshot_manifest,
                "snapshot_manifest_sha256": snapshot_hash
            }],
            "budgets": {
                "quality": {
                    "hit_at_1_max_absolute_drop": 0.06,
                    "recall_at_5_macro_max_absolute_drop": 0.04,
                    "recall_at_5_micro_max_absolute_drop": 0.04,
                    "mrr_at_5_max_absolute_drop": 0.05,
                    "exact_hit_at_1_max_absolute_drop": 0.0,
                    "exact_recall_at_5_macro_max_absolute_drop": 0.0,
                    "exact_mrr_at_5_max_absolute_drop": 0.0,
                    "ambiguous_hit_at_1_max_absolute_drop": 0.0,
                    "ambiguous_recall_at_5_macro_max_absolute_drop": 0.0,
                    "ambiguous_mrr_at_5_max_absolute_drop": 0.0
                },
                "context": {"max_relative_total_increase": 0.15},
                "latency_ms": {
                    "startup_max_by_engine": {
                        (ENGINE): 500.0,
                        (SQLITE_ENGINE): 500.0,
                        (FTS5_ENGINE): 500.0
                    },
                    "query_p95_max_by_engine": {
                        (ENGINE): 500.0,
                        (SQLITE_ENGINE): 500.0,
                        (FTS5_ENGINE): 500.0
                    },
                    "run_variation": {
                        "relative_max": 0.25,
                        "absolute_p95_ms_max": 10.0
                    }
                },
                "resources_bytes": {
                    "peak_rss_max_by_engine": {
                        (ENGINE): 536870912,
                        (SQLITE_ENGINE): 536870912,
                        (FTS5_ENGINE): 536870912
                    },
                    "index_storage_max_by_engine": {
                        (ENGINE): 0,
                        (SQLITE_ENGINE): 268435456,
                        (FTS5_ENGINE): 268435456
                    }
                },
                "indexing_ms": {
                    "full_build_max_by_engine": {
                        (SQLITE_ENGINE): 60000.0,
                        (FTS5_ENGINE): 60000.0
                    },
                    "incremental_step_max_relative_to_full_build": 1.0
                }
            }
        });
        let protocol_path = directory.path().join("protocol.json");
        fs::write(
            &protocol_path,
            format!("{}\n", serde_json::to_string_pretty(&protocol).unwrap()),
        )
        .unwrap();

        let cargo_lock = fs::read(manifest_dir.join("Cargo.lock")).unwrap();
        let tool = json!({
            "version": env!("CARGO_PKG_VERSION"),
            "git_revision": "0".repeat(40),
            "binary_sha256": "1".repeat(64),
            "rust_version": "test-rust",
            "cargo_lock_sha256": sha256::digest_hex(&cargo_lock),
            "profile": "release"
        });
        let provenance_path = directory.path().join("provenance.json");
        fs::write(
            &provenance_path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&json!({
                    "tool": tool,
                    "cargo_version": "test-cargo",
                    "process_environment": {},
                    "host_sha256": canonical_json_sha256(&protocol["measurement"]["host"]).unwrap(),
                    "host_verified": true,
                    "isolation": {
                        "denylist_sha256": "2".repeat(64),
                        "verified_before_path_resolution": true
                    }
                }))
                .unwrap()
            ),
        )
        .unwrap();
        let observations = [
            directory.path().join("observation-1.json"),
            directory.path().join("observation-2.json"),
        ];
        for run in [1, 2] {
            observe(ObserveRequest {
                protocol_path: protocol_path.clone(),
                input_id: "stable-v1".to_owned(),
                engine: ENGINE.to_owned(),
                run,
                provenance_path: provenance_path.clone(),
                ready_file: directory.path().join(format!("ready-{run}")),
                output: observations[usize::from(run - 1)].clone(),
            })
            .unwrap();
        }

        let protocol_hash = sha256::digest_hex(&fs::read(&protocol_path).unwrap());
        let observation_hash = sha256::digest_hex(&fs::read(&observations[0]).unwrap());
        let measurements = json!({
            "schema_version": 1,
            "protocol_sha256": protocol_hash,
            "input_id": "stable-v1",
            "engine": ENGINE,
            "run": 1,
            "observation_sha256": observation_hash,
            "tool": tool,
            "environment": {
                "os": "test-os",
                "architecture": "arm64",
                "machine": "test-machine",
                "cpu": "test-cpu",
                "logical_cpus": 1,
                "memory_bytes": 1,
                "load_average": [0.0, 0.0, 0.0],
                "sqlite_version": null,
                "sqlite_compile_options_sha256": null
            },
            "timing_ms": {"startup": 1.0, "end_to_end": 100.0},
            "resources_bytes": {
                "peak_rss": 1,
                "index_logical": 0,
                "index_allocated": 0,
                "shared_model": 0
            },
            "started_at": "2026-01-01T00:00:00Z",
            "finished_at": "2026-01-01T00:00:01Z"
        });
        let measurements_path = directory.path().join("measurements.json");
        fs::write(
            &measurements_path,
            format!("{}\n", serde_json::to_string_pretty(&measurements).unwrap()),
        )
        .unwrap();
        let report_path = directory.path().join("report.json");
        finalize(FinalizeRequest {
            protocol_path,
            observation_path: observations[0].clone(),
            other_observation_path: observations[1].clone(),
            measurements_path,
            report_tag: "engine-bakeoff-v1-stable-v1-lexical-bm25-v1-run-1".to_owned(),
            output: report_path.clone(),
        })
        .unwrap();

        let report: Value = serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
        assert_eq!(report["schema_version"], BAKEOFF_REPORT_SCHEMA_VERSION);
        assert_eq!(report["engine"], ENGINE);
        for (name, passed) in report["budget_checks"].as_object().unwrap() {
            if name != "run_variation" {
                assert_eq!(passed, true, "budget check failed: {name}");
            }
        }
        assert_eq!(report["determinism"]["matches_other_run"], true);
        assert_eq!(report["fallback"]["behavior"], "not-applicable");
        assert!(report["indexing"]["full_build_ms"].is_null());
        assert_eq!(report["queries"].as_array().unwrap().len(), 20);
        assert_eq!(
            report_path.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );

        let mut tampered: Value =
            serde_json::from_slice(&fs::read(&observations[0]).unwrap()).unwrap();
        tampered["queries"][0]["results"][0]["excerpt"] = Value::String("tampered".to_owned());
        let tampered_path = directory.path().join("tampered-observation.json");
        fs::write(
            &tampered_path,
            format!("{}\n", serde_json::to_string_pretty(&tampered).unwrap()),
        )
        .unwrap();
        let error = finalize(FinalizeRequest {
            protocol_path: directory.path().join("protocol.json"),
            observation_path: tampered_path,
            other_observation_path: observations[1].clone(),
            measurements_path: directory.path().join("measurements.json"),
            report_tag: "engine-bakeoff-v1-stable-v1-lexical-bm25-v1-run-1".to_owned(),
            output: directory.path().join("tampered-report.json"),
        })
        .unwrap_err();
        assert!(error.to_string().contains("excerpt"));

        let mut tampered_fault: Value =
            serde_json::from_slice(&fs::read(&observations[0]).unwrap()).unwrap();
        tampered_fault["fault_injection_passed"] = Value::Bool(false);
        let tampered_fault_path = directory.path().join("tampered-fault-observation.json");
        fs::write(
            &tampered_fault_path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&tampered_fault).unwrap()
            ),
        )
        .unwrap();
        let error = finalize(FinalizeRequest {
            protocol_path: directory.path().join("protocol.json"),
            observation_path: tampered_fault_path,
            other_observation_path: observations[1].clone(),
            measurements_path: directory.path().join("measurements.json"),
            report_tag: "engine-bakeoff-v1-stable-v1-lexical-bm25-v1-run-1".to_owned(),
            output: directory.path().join("tampered-fault-report.json"),
        })
        .unwrap_err();
        assert!(error.to_string().contains("fault injection"));

        crate::sqlite_cache::set_test_cache_home(Some(directory.path().join("cache")));
        let sqlite_observations = [
            directory.path().join("sqlite-observation-1.json"),
            directory.path().join("sqlite-observation-2.json"),
        ];
        for run in [1, 2] {
            observe(ObserveRequest {
                protocol_path: directory.path().join("protocol.json"),
                input_id: "stable-v1".to_owned(),
                engine: SQLITE_ENGINE.to_owned(),
                run,
                provenance_path: provenance_path.clone(),
                ready_file: directory.path().join(format!("sqlite-ready-{run}")),
                output: sqlite_observations[usize::from(run - 1)].clone(),
            })
            .unwrap();
        }
        let sqlite_observation: Value =
            serde_json::from_slice(&fs::read(&sqlite_observations[0]).unwrap()).unwrap();
        let index_path = PathBuf::from(sqlite_observation["index_path"].as_str().unwrap());
        let index_metadata = fs::metadata(&index_path).unwrap();
        let mut sqlite_measurements = measurements.clone();
        sqlite_measurements["engine"] = Value::String(SQLITE_ENGINE.to_owned());
        sqlite_measurements["observation_sha256"] = Value::String(sha256::digest_hex(
            &fs::read(&sqlite_observations[0]).unwrap(),
        ));
        sqlite_measurements["environment"]["sqlite_version"] =
            sqlite_observation["sqlite_runtime"]["version"].clone();
        sqlite_measurements["environment"]["sqlite_compile_options_sha256"] =
            sqlite_observation["sqlite_runtime"]["compile_options_sha256"].clone();
        sqlite_measurements["resources_bytes"]["index_logical"] = json!(index_metadata.len());
        sqlite_measurements["resources_bytes"]["index_allocated"] =
            json!(index_metadata.blocks() * 512);
        let sqlite_measurements_path = directory.path().join("sqlite-measurements.json");
        fs::write(
            &sqlite_measurements_path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&sqlite_measurements).unwrap()
            ),
        )
        .unwrap();
        let sqlite_report_path = directory.path().join("sqlite-report.json");
        finalize(FinalizeRequest {
            protocol_path: directory.path().join("protocol.json"),
            observation_path: sqlite_observations[0].clone(),
            other_observation_path: sqlite_observations[1].clone(),
            measurements_path: sqlite_measurements_path,
            report_tag: "engine-bakeoff-v1-stable-v1-sqlite-cache-bm25-v1-run-1".to_owned(),
            output: sqlite_report_path.clone(),
        })
        .unwrap();
        let sqlite_report: Value =
            serde_json::from_slice(&fs::read(sqlite_report_path).unwrap()).unwrap();
        assert_eq!(sqlite_report["engine"], SQLITE_ENGINE);
        assert_eq!(
            sqlite_report["fallback"]["behavior"],
            "explicit-direct-bm25"
        );
        assert_eq!(
            sqlite_report["indexing"]["incremental_steps"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        assert_eq!(sqlite_report["indexing"]["corruption_detected"], true);

        let fts5_observations = [
            directory.path().join("fts5-observation-1.json"),
            directory.path().join("fts5-observation-2.json"),
        ];
        for run in [1, 2] {
            observe(ObserveRequest {
                protocol_path: directory.path().join("protocol.json"),
                input_id: "stable-v1".to_owned(),
                engine: FTS5_ENGINE.to_owned(),
                run,
                provenance_path: provenance_path.clone(),
                ready_file: directory.path().join(format!("fts5-ready-{run}")),
                output: fts5_observations[usize::from(run - 1)].clone(),
            })
            .unwrap();
        }
        let fts5_observation: Value =
            serde_json::from_slice(&fs::read(&fts5_observations[0]).unwrap()).unwrap();
        let fts5_index_path = PathBuf::from(fts5_observation["index_path"].as_str().unwrap());
        let fts5_index_metadata = fs::metadata(&fts5_index_path).unwrap();
        let mut fts5_measurements = measurements;
        fts5_measurements["engine"] = Value::String(FTS5_ENGINE.to_owned());
        fts5_measurements["observation_sha256"] = Value::String(sha256::digest_hex(
            &fs::read(&fts5_observations[0]).unwrap(),
        ));
        fts5_measurements["environment"]["sqlite_version"] =
            fts5_observation["sqlite_runtime"]["version"].clone();
        fts5_measurements["environment"]["sqlite_compile_options_sha256"] =
            fts5_observation["sqlite_runtime"]["compile_options_sha256"].clone();
        fts5_measurements["resources_bytes"]["index_logical"] = json!(fts5_index_metadata.len());
        fts5_measurements["resources_bytes"]["index_allocated"] =
            json!(fts5_index_metadata.blocks() * 512);
        let fts5_measurements_path = directory.path().join("fts5-measurements.json");
        fs::write(
            &fts5_measurements_path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&fts5_measurements).unwrap()
            ),
        )
        .unwrap();
        let fts5_report_path = directory.path().join("fts5-report.json");
        finalize(FinalizeRequest {
            protocol_path: directory.path().join("protocol.json"),
            observation_path: fts5_observations[0].clone(),
            other_observation_path: fts5_observations[1].clone(),
            measurements_path: fts5_measurements_path,
            report_tag: "engine-bakeoff-v1-stable-v1-fts5-v1-run-1".to_owned(),
            output: fts5_report_path.clone(),
        })
        .unwrap();
        let fts5_report: Value =
            serde_json::from_slice(&fs::read(fts5_report_path).unwrap()).unwrap();
        assert_eq!(fts5_report["engine"], FTS5_ENGINE);
        assert_eq!(fts5_report["fallback"]["behavior"], "explicit-fail-closed");
        assert_eq!(
            fts5_report["indexing"]["incremental_steps"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        assert_eq!(fts5_report["indexing"]["corruption_detected"], true);
        assert!(
            fts5_report["queries"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|query| query["results"].as_array().unwrap())
                .all(|result| result["source_ranks"]["fts5"].is_number())
        );
        crate::sqlite_cache::set_test_cache_home(None);
    }

    fn lexical_configuration() -> Value {
        json!({
            "storage": "direct-markdown-scan-per-query",
            "ranking": {
                "k1": 1.2,
                "b": 0.75,
                "body_weight": 1.0,
                "heading_weight": 2.0,
                "path_weight": 3.0,
                "idf": "ln(1+(N-df+0.5)/(df+0.5))",
                "minimum_should_match": "one-term=1;otherwise=ceil(0.6*distinct-meaningful-query-terms)",
                "phrase_boosts": {"body": 2.5, "heading": 3.0, "path": 2.0},
                "score_round_decimals": 6
            },
            "candidate_depth": "all-chunks",
            "abstention": "empty-only-when-no-chunk-meets-minimum-should-match"
        })
    }

    fn sqlite_configuration() -> Value {
        json!({
            "storage": "sqlite-persistent-chunk-and-token-cache",
            "sqlite": {
                "rusqlite": "=0.40.2",
                "features": ["bundled"],
                "expected_sqlite_version": "3.53.2",
                "runtime_checks": [
                    "sqlite_version_equals_expected",
                    "compile_option_ENABLE_FTS5",
                    "fts5_create-insert-match-bm25-smoke"
                ]
            },
            "schema": "metadata(key-text-primary-key,value-text);files(path-text-primary-key,file-hash-text,bytes-integer);chunks(chunk-hash-text-primary-key,file-hash-text,path-text,heading-text,line-start-integer,line-end-integer,text-text,tokens-json-text,normalized-text)",
            "invalidation": "rebuild-atomically-when-corpus-fingerprint-or-parser-version-or-engine-config-hash-differs",
            "write_policy": "temporary-database-fsync-then-atomic-rename;never-mutate-valid-index-in-place",
            "ranking": {
                "k1": 1.2,
                "b": 0.75,
                "body_weight": 1.0,
                "heading_weight": 2.0,
                "path_weight": 3.0,
                "idf": "ln(1+(N-df+0.5)/(df+0.5))",
                "minimum_should_match": "one-term=1;otherwise=ceil(0.6*distinct-meaningful-query-terms)",
                "phrase_boosts": {"body": 2.5, "heading": 3.0, "path": 2.0},
                "score_round_decimals": 6
            },
            "candidate_depth": "all-cached-chunks",
            "abstention": "empty-only-when-no-chunk-meets-minimum-should-match"
        })
    }

    fn fts5_configuration() -> Value {
        crate::fts5::frozen_configuration()
    }

    fn embedding_configuration() -> Value {
        crate::embeddings::frozen_configuration()
    }
}
