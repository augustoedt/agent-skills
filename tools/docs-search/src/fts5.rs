use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, OpenFlags, Transaction, params};
use serde_json::{Value, json};

use crate::corpus::{self, Chunk, Corpus};
use crate::evaluate::corpus_fingerprint;
use crate::search::{
    SearchTimings, elapsed_ms, excerpt, meaningful_query_tokens, round_score,
    select_with_path_diversity, tokens, validate_request,
};
use crate::sha256;
use crate::sqlite_cache::{
    EXPECTED_SQLITE_VERSION, IncrementalStep, IndexState, IndexingMetrics, SQLITE_RUNTIME_CHECKS,
    SqliteRuntime, cache_path, validate_runtime,
};
use crate::types::{
    CorpusSummary, SCHEMA_VERSION, SearchRequest, SearchResponse, SearchResult, SearchSelection,
};

pub const FTS5_ENGINE: &str = "fts5-v1";
pub(crate) const FTS5_CONFIG_SHA256: &str =
    "3aedaee7026d230eea9345a962f6913c9d7890dd0fe175704c29aafb10384034";
const INDEX_SCHEMA_VERSION: &str = "fts5-1";
const PARSER_VERSION: &str = "markdown-heading-fence-aware-v1";
const INDEX_FILE: &str = "fts5.sqlite3";
const FTS5_TABLE: &str = "chunks_fts";
const FTS5_TOKENIZER: &str = "unicode61 remove_diacritics 2";
const FTS5_COLUMNS: [&str; 3] = ["body", "heading", "path"];
const FTS5_WEIGHTS: [f64; 3] = [1.0, 2.0, 3.0];
const FTS5_DETAIL: &str = "full";
const FTS5_COLUMNSIZE: usize = 1;
pub(crate) const CANDIDATE_DEPTH: usize = 50;

const CREATE_SCHEMA: &str = "
CREATE TABLE metadata(key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE files(path TEXT PRIMARY KEY, file_hash TEXT, bytes INTEGER);
CREATE TABLE chunks(
  id INTEGER PRIMARY KEY,
  storage_key TEXT UNIQUE NOT NULL,
  file_hash TEXT NOT NULL,
  path TEXT NOT NULL,
  heading TEXT,
  line_start INTEGER NOT NULL,
  line_end INTEGER NOT NULL,
  chunk_hash TEXT NOT NULL
);
";

#[derive(Debug)]
pub(crate) struct Fts5Index {
    root: PathBuf,
    index_path: PathBuf,
    engine_config_sha256: String,
    state: Mutex<IndexCacheState>,
}

#[derive(Debug)]
struct IndexCacheState {
    files: usize,
    chunks: usize,
    corpus_fingerprint: String,
}

#[derive(Debug)]
struct RankedFtsChunk {
    chunk: Chunk,
    score: f64,
    matched_terms: Vec<String>,
}

#[derive(Debug, PartialEq)]
struct IndexedRow {
    storage_key: String,
    file_hash: String,
    path: String,
    heading: Option<String>,
    line_start: i64,
    line_end: i64,
    chunk_hash: String,
    body: String,
    fts_heading: String,
    fts_path: String,
}

fn expected_fts_sql() -> String {
    format!(
        "CREATE VIRTUAL TABLE {FTS5_TABLE} USING fts5(\n  {},\n  {},\n  {},\n  tokenize='{FTS5_TOKENIZER}',\n  detail={FTS5_DETAIL},\n  columnsize={FTS5_COLUMNSIZE}\n);",
        FTS5_COLUMNS[0], FTS5_COLUMNS[1], FTS5_COLUMNS[2]
    )
}

fn ranking_sql() -> String {
    format!(
        "SELECT c.file_hash, c.path, c.heading, c.line_start, c.line_end,\n\
                f.body, c.chunk_hash,\n\
                -bm25({FTS5_TABLE}, {}, {}, {}) AS score\n\
         FROM {FTS5_TABLE} AS f\n\
         JOIN chunks AS c ON c.id = f.rowid\n\
         WHERE {FTS5_TABLE} MATCH ?1\n\
         ORDER BY score DESC, c.path ASC, c.line_start ASC\n\
         LIMIT ?2",
        FTS5_WEIGHTS[0], FTS5_WEIGHTS[1], FTS5_WEIGHTS[2]
    )
}

pub(crate) fn frozen_configuration() -> Value {
    json!({
        "sqlite": {
            "rusqlite": "=0.40.2",
            "features": ["bundled"],
            "expected_sqlite_version": EXPECTED_SQLITE_VERSION,
            "runtime_checks": SQLITE_RUNTIME_CHECKS
        },
        "table": FTS5_TABLE,
        "tokenizer": FTS5_TOKENIZER,
        "columns": FTS5_COLUMNS,
        "column_weights": FTS5_WEIGHTS,
        "detail": FTS5_DETAIL,
        "columnsize": FTS5_COLUMNSIZE,
        "prefix_indexes": [],
        "query": "deduplicated-meaningful-terms-double-quoted-and-joined-with-OR;literal-double-quotes-escaped-by-doubling",
        "candidate_depth": CANDIDATE_DEPTH,
        "ranking": format!(
            "negative-sqlite-bm25({FTS5_TABLE},{:.1},{:.1},{:.1})-descending-then-path-asc-line-start-asc",
            FTS5_WEIGHTS[0], FTS5_WEIGHTS[1], FTS5_WEIGHTS[2]
        ),
        "abstention": "empty-only-when-fts5-match-has-no-row"
    })
}

impl Fts5Index {
    pub(crate) fn open_existing(
        root: &Path,
        corpus: &Corpus,
        engine_config_sha256: &str,
    ) -> Result<Self> {
        validate_runtime()?;
        let index_path = index_path_for_root(root)?;
        validate_index(&index_path, corpus, engine_config_sha256)?;
        Ok(Self::at_path(corpus, index_path, engine_config_sha256))
    }

    pub(crate) fn prepare_fresh(
        root: &Path,
        corpus: &Corpus,
        engine_config_sha256: &str,
    ) -> Result<(Self, IndexingMetrics, SqliteRuntime)> {
        let runtime = validate_runtime()?;
        let index_path = index_path_for_root(root)?;
        if let Some(parent) = index_path.parent() {
            create_private_directory(parent)?;
        }
        remove_index_files(&index_path)?;
        let started = Instant::now();
        build_atomic(&index_path, root, corpus, engine_config_sha256)?;
        let full_build_ms = elapsed_ms(started);
        validate_index(&index_path, corpus, engine_config_sha256)?;
        Ok((
            Self::at_path(corpus, index_path, engine_config_sha256),
            IndexingMetrics {
                full_build_ms: Some(full_build_ms),
                incremental_steps: Vec::new(),
                rebuild_succeeded: true,
                corruption_detected: false,
                runtime_checks: runtime.checks.clone(),
            },
            runtime,
        ))
    }

    pub(crate) fn search(&self, request: SearchRequest) -> Result<(SearchResponse, SearchTimings)> {
        self.search_with_recovery(request, false)
    }

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
                "failed to resolve FTS5 request root {}",
                request.root.display()
            )
        })?;
        if request_root != self.root {
            bail!("FTS5 request root differs from the bound index root");
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
                    )
                };
                if let Err(rebuild_error) = rebuild {
                    bail!(
                        "FTS5 index failed ({index_error:#}); atomic rebuild failed ({rebuild_error:#}); fail-closed"
                    );
                }
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| anyhow!("FTS5 cache state mutex is poisoned"))?;
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
                .map_err(|_| anyhow!("FTS5 cache state mutex is poisoned"))?;
            if state.corpus_fingerprint != current_fingerprint
                || state.files != current_hashes.len()
            {
                bail!("FTS5 corpus fingerprint is stale");
            }
            (state.files, state.chunks)
        };
        let connection = Connection::open_with_flags(
            &self.index_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("failed to open FTS5 index {}", self.index_path.display()))?;
        connection.execute_batch("PRAGMA query_only=ON;")?;
        let lookup_ms = elapsed_ms(lookup_started);

        let query_tokens = meaningful_query_tokens(&request.query);
        if query_tokens.is_empty() {
            bail!("query must contain at least one letter or number");
        }
        let match_query = fts_query(&query_tokens);
        let ranking_started = Instant::now();
        let ranking_sql = ranking_sql();
        let mut statement = connection.prepare(&ranking_sql)?;
        let rows = statement.query_map(params![match_query, CANDIDATE_DEPTH as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, f64>(7)?,
            ))
        })?;
        let mut ranked = Vec::new();
        for row in rows {
            let (file_hash, path, heading, line_start, line_end, text, chunk_hash, score) = row?;
            if !score.is_finite() {
                bail!("FTS5 returned a non-finite BM25 score");
            }
            let actual_chunk_hash = blake3::hash(text.as_bytes()).to_hex().to_string();
            if actual_chunk_hash != chunk_hash {
                bail!("FTS5 chunk hash does not match indexed text");
            }
            let matched_terms = matched_terms(&query_tokens, &text, heading.as_deref(), &path);
            ranked.push(RankedFtsChunk {
                chunk: Chunk {
                    path,
                    heading,
                    line_start: to_usize(line_start, "line_start")?,
                    line_end: to_usize(line_end, "line_end")?,
                    text,
                    file_hash,
                    chunk_hash,
                },
                score,
                matched_terms,
            });
        }
        let candidates_examined = ranked.len();
        let selected = select_with_path_diversity(
            ranked,
            request.limit,
            request.max_results_per_path,
            |ranked| ranked.chunk.path.as_str(),
        );
        let ranking_ms = elapsed_ms(ranking_started);

        let excerpt_started = Instant::now();
        let results = selected
            .into_iter()
            .enumerate()
            .map(|(index, (raw_rank, ranked))| {
                let (excerpt, line_start, line_end) =
                    excerpt(&ranked.chunk, &query_tokens, request.max_excerpt_chars);
                SearchResult {
                    rank: index + 1,
                    raw_rank,
                    path: ranked.chunk.path,
                    heading: ranked.chunk.heading,
                    line_start,
                    line_end,
                    excerpt,
                    file_hash: ranked.chunk.file_hash,
                    chunk_hash: ranked.chunk.chunk_hash,
                    score: round_score(ranked.score),
                    matched_terms: ranked.matched_terms,
                }
            })
            .collect();
        let excerpt_ms = elapsed_ms(excerpt_started);
        Ok((
            SearchResponse {
                schema_version: SCHEMA_VERSION,
                engine: FTS5_ENGINE,
                query: request.query,
                root: self.root.to_string_lossy().into_owned(),
                corpus: CorpusSummary { files, chunks },
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
        validate_index(&self.index_path, corpus, &self.engine_config_sha256)
    }

    fn at_path(corpus: &Corpus, index_path: PathBuf, engine_config_sha256: &str) -> Self {
        Self {
            root: corpus.root.clone(),
            index_path,
            engine_config_sha256: engine_config_sha256.to_owned(),
            state: Mutex::new(index_state(corpus)),
        }
    }
}

fn index_state(corpus: &Corpus) -> IndexCacheState {
    IndexCacheState {
        files: corpus.files,
        chunks: corpus.chunks.len(),
        corpus_fingerprint: corpus_fingerprint(&corpus.file_hashes),
    }
}

pub(crate) fn index_path_for_root(root: &Path) -> Result<PathBuf> {
    Ok(cache_path(root)?.with_file_name(INDEX_FILE))
}

fn fts_query(terms: &[String]) -> String {
    terms
        .iter()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn matched_terms(
    query_terms: &[String],
    body: &str,
    heading: Option<&str>,
    path: &str,
) -> Vec<String> {
    let mut indexed: HashSet<String> = tokens(body).into_iter().collect();
    indexed.extend(tokens(heading.unwrap_or_default()));
    indexed.extend(tokens(path));
    query_terms
        .iter()
        .filter(|term| indexed.contains(*term))
        .cloned()
        .collect()
}

fn build_atomic(
    index_path: &Path,
    root: &Path,
    corpus: &Corpus,
    engine_config_sha256: &str,
) -> Result<()> {
    let parent = index_path
        .parent()
        .ok_or_else(|| anyhow!("FTS5 index path has no parent"))?;
    create_private_directory(parent)?;
    let temp_path = temporary_index_path(index_path)?;
    if temp_path.exists() {
        bail!(
            "temporary FTS5 index already exists: {}",
            temp_path.display()
        );
    }
    let result = (|| -> Result<()> {
        let mut connection = Connection::open(&temp_path)
            .with_context(|| format!("failed to create FTS5 index {}", temp_path.display()))?;
        fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600))?;
        connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;")?;
        connection.execute_batch(CREATE_SCHEMA)?;
        connection.execute_batch(&expected_fts_sql())?;
        let transaction = connection.transaction()?;
        insert_metadata(&transaction, corpus, engine_config_sha256)?;
        insert_files(&transaction, root, corpus)?;
        for chunk in &corpus.chunks {
            insert_chunk(&transaction, chunk)?;
        }
        transaction.commit()?;
        connection.execute_batch(&format!(
            "INSERT INTO {FTS5_TABLE}({FTS5_TABLE}) VALUES('optimize');"
        ))?;
        drop(connection);
        sync_file(&temp_path)?;
        remove_sidecars(index_path)?;
        fs::rename(&temp_path, index_path).with_context(|| {
            format!(
                "failed to atomically install FTS5 index {}",
                index_path.display()
            )
        })?;
        fs::set_permissions(index_path, fs::Permissions::from_mode(0o600))?;
        sync_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = cleanup_temporary_index(&temp_path);
    }
    result
}

fn update_atomic(
    index_path: &Path,
    root: &Path,
    corpus: &Corpus,
    engine_config_sha256: &str,
) -> Result<()> {
    validate_update_source(index_path, engine_config_sha256)?;
    let parent = index_path
        .parent()
        .ok_or_else(|| anyhow!("FTS5 index path has no parent"))?;
    let temp_path = temporary_index_path(index_path)?;
    fs::copy(index_path, &temp_path)?;
    fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600))?;
    let result = (|| -> Result<()> {
        let mut connection = Connection::open(&temp_path)?;
        connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;")?;
        let transaction = connection.transaction()?;
        let mut statement = transaction.prepare("SELECT path, file_hash FROM files")?;
        let existing: BTreeMap<String, String> = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        drop(statement);
        for path in existing.keys() {
            if !corpus.file_hashes.contains_key(path) {
                delete_path(&transaction, path)?;
            }
        }
        for (path, file_hash) in &corpus.file_hashes {
            if existing.get(path) == Some(file_hash) {
                continue;
            }
            delete_path(&transaction, path)?;
            let bytes = fs::metadata(root.join(path))?.len();
            transaction.execute(
                "INSERT INTO files(path, file_hash, bytes) VALUES (?1, ?2, ?3)",
                params![path, file_hash, to_i64(bytes, "file bytes")?],
            )?;
            for chunk in corpus.chunks.iter().filter(|chunk| chunk.path == *path) {
                insert_chunk(&transaction, chunk)?;
            }
        }
        transaction.execute(
            "UPDATE metadata SET value = ?1 WHERE key = 'corpus_fingerprint'",
            params![corpus_fingerprint(&corpus.file_hashes)],
        )?;
        transaction.commit()?;
        drop(connection);
        sync_file(&temp_path)?;
        remove_sidecars(index_path)?;
        fs::rename(&temp_path, index_path)?;
        fs::set_permissions(index_path, fs::Permissions::from_mode(0o600))?;
        sync_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = cleanup_temporary_index(&temp_path);
    }
    result
}

fn insert_metadata(
    transaction: &Transaction<'_>,
    corpus: &Corpus,
    engine_config_sha256: &str,
) -> Result<()> {
    let fingerprint = corpus_fingerprint(&corpus.file_hashes);
    for (key, value) in [
        ("schema_version", INDEX_SCHEMA_VERSION),
        ("parser_version", PARSER_VERSION),
        ("engine_config_sha256", engine_config_sha256),
        ("corpus_fingerprint", fingerprint.as_str()),
    ] {
        transaction.execute(
            "INSERT INTO metadata(key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
    }
    Ok(())
}

fn insert_files(transaction: &Transaction<'_>, root: &Path, corpus: &Corpus) -> Result<()> {
    for (path, file_hash) in &corpus.file_hashes {
        let bytes = fs::metadata(root.join(path))?.len();
        transaction.execute(
            "INSERT INTO files(path, file_hash, bytes) VALUES (?1, ?2, ?3)",
            params![path, file_hash, to_i64(bytes, "file bytes")?],
        )?;
    }
    Ok(())
}

fn insert_chunk(transaction: &Transaction<'_>, chunk: &Chunk) -> Result<()> {
    transaction.execute(
        "INSERT INTO chunks(
           storage_key, file_hash, path, heading, line_start, line_end, chunk_hash
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            storage_chunk_key(chunk),
            chunk.file_hash,
            chunk.path,
            chunk.heading,
            to_i64(chunk.line_start as u64, "line_start")?,
            to_i64(chunk.line_end as u64, "line_end")?,
            chunk.chunk_hash,
        ],
    )?;
    let rowid = transaction.last_insert_rowid();
    transaction.execute(
        &format!(
            "INSERT INTO {FTS5_TABLE}(rowid, {}, {}, {}) VALUES (?1, ?2, ?3, ?4)",
            FTS5_COLUMNS[0], FTS5_COLUMNS[1], FTS5_COLUMNS[2]
        ),
        params![
            rowid,
            chunk.text,
            chunk.heading.as_deref().unwrap_or_default(),
            chunk.path
        ],
    )?;
    Ok(())
}

fn delete_path(transaction: &Transaction<'_>, path: &str) -> Result<()> {
    let mut statement = transaction.prepare("SELECT id FROM chunks WHERE path = ?1")?;
    let ids: Vec<i64> = statement
        .query_map(params![path], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    drop(statement);
    for id in ids {
        transaction.execute(
            &format!("DELETE FROM {FTS5_TABLE} WHERE rowid = ?1"),
            params![id],
        )?;
    }
    transaction.execute("DELETE FROM chunks WHERE path = ?1", params![path])?;
    transaction.execute("DELETE FROM files WHERE path = ?1", params![path])?;
    Ok(())
}

fn storage_chunk_key(chunk: &Chunk) -> String {
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

fn validate_update_source(index_path: &Path, engine_config_sha256: &str) -> Result<()> {
    let connection = Connection::open_with_flags(index_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    validate_metadata_contract(&connection, engine_config_sha256)
}

fn validate_metadata_contract(connection: &Connection, engine_config_sha256: &str) -> Result<()> {
    for (key, expected) in [
        ("schema_version", INDEX_SCHEMA_VERSION),
        ("parser_version", PARSER_VERSION),
        ("engine_config_sha256", engine_config_sha256),
    ] {
        let actual: String = connection.query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )?;
        if actual != expected {
            bail!("FTS5 metadata {key} is incompatible");
        }
    }
    Ok(())
}

fn validate_index(index_path: &Path, corpus: &Corpus, engine_config_sha256: &str) -> Result<()> {
    let connection = Connection::open(index_path)
        .with_context(|| format!("failed to open FTS5 index {}", index_path.display()))?;
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        bail!("FTS5 SQLite integrity check failed: {integrity}");
    }
    validate_metadata_contract(&connection, engine_config_sha256)?;
    let fingerprint: String = connection.query_row(
        "SELECT value FROM metadata WHERE key = 'corpus_fingerprint'",
        [],
        |row| row.get(0),
    )?;
    if fingerprint != corpus_fingerprint(&corpus.file_hashes) {
        bail!("FTS5 corpus fingerprint does not match current corpus");
    }
    let files: i64 = connection.query_row("SELECT count(*) FROM files", [], |row| row.get(0))?;
    let chunks: i64 = connection.query_row("SELECT count(*) FROM chunks", [], |row| row.get(0))?;
    let postings: i64 =
        connection.query_row(&format!("SELECT count(*) FROM {FTS5_TABLE}"), [], |row| {
            row.get(0)
        })?;
    if to_usize(files, "file count")? != corpus.files
        || to_usize(chunks, "chunk count")? != corpus.chunks.len()
        || to_usize(postings, "FTS row count")? != corpus.chunks.len()
    {
        bail!("FTS5 index counts do not match current corpus");
    }
    let mut file_statement = connection.prepare("SELECT path, file_hash, bytes FROM files")?;
    let stored_files: BTreeMap<String, (String, u64)> = file_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                (
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?.try_into().map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Integer,
                            Box::new(error),
                        )
                    })?,
                ),
            ))
        })?
        .collect::<std::result::Result<_, _>>()?;
    drop(file_statement);
    let expected_files: BTreeMap<String, (String, u64)> = corpus
        .file_hashes
        .iter()
        .map(|(path, hash)| {
            Ok((
                path.clone(),
                (hash.clone(), fs::metadata(corpus.root.join(path))?.len()),
            ))
        })
        .collect::<Result<_>>()?;
    if stored_files != expected_files {
        bail!("FTS5 files table differs from the current corpus");
    }
    let mut expected_projection: Vec<_> = corpus
        .chunks
        .iter()
        .map(|chunk| IndexedRow {
            storage_key: storage_chunk_key(chunk),
            file_hash: chunk.file_hash.clone(),
            path: chunk.path.clone(),
            heading: chunk.heading.clone(),
            line_start: chunk.line_start as i64,
            line_end: chunk.line_end as i64,
            chunk_hash: chunk.chunk_hash.clone(),
            body: chunk.text.clone(),
            fts_heading: chunk.heading.clone().unwrap_or_default(),
            fts_path: chunk.path.clone(),
        })
        .collect();
    expected_projection.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.line_start.cmp(&right.line_start))
            .then_with(|| left.storage_key.cmp(&right.storage_key))
    });
    if index_projection(index_path)? != expected_projection {
        bail!("FTS5 indexed content or evidence hashes differ from the current corpus");
    }
    let definition: String = connection.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
        params![FTS5_TABLE],
        |row| row.get(0),
    )?;
    if definition.trim() != expected_fts_sql().trim().trim_end_matches(';') {
        bail!("FTS5 table definition differs from the frozen schema");
    }
    connection
        .execute(
            &format!("INSERT INTO {FTS5_TABLE}({FTS5_TABLE}) VALUES('integrity-check')"),
            [],
        )
        .context("FTS5 internal integrity check failed")?;
    Ok(())
}

pub(crate) fn rebuild_at(
    index_path: &Path,
    root: &Path,
    corpus: &Corpus,
    engine_config_sha256: &str,
) -> Result<IndexState> {
    let corruption_detected =
        index_path.exists() && validate_index(index_path, corpus, engine_config_sha256).is_err();
    build_atomic(index_path, root, corpus, engine_config_sha256)?;
    validate_index(index_path, corpus, engine_config_sha256)?;
    Ok(IndexState {
        corruption_detected,
        rebuild_succeeded: true,
    })
}

pub(crate) fn run_incremental_workload(
    source_root: &Path,
    queries: &[String],
    engine_config_sha256: &str,
) -> Result<Vec<IncrementalStep>> {
    let source = corpus::load(source_root)?;
    let selected: Vec<_> = source.file_hashes.keys().take(4).cloned().collect();
    if selected.len() != 4 {
        bail!("FTS5 incremental workload requires at least four selected Markdown files");
    }
    let directory = unique_temp_directory("incremental")?;
    let root = directory.join("corpus");
    fs::create_dir(&root)?;
    for relative in &selected {
        copy_selected_file(source_root, &root, relative, relative)?;
    }
    let update_index = directory.join("update.sqlite3");
    let full_index = directory.join("full.sqlite3");
    let initial = corpus::load(&root)?;
    build_atomic(&update_index, &root, &initial, engine_config_sha256)?;

    let mut steps = Vec::new();
    for operation in ["add", "modify", "rename", "remove"] {
        apply_incremental_operation(operation, &root, &selected)?;
        let current = corpus::load(&root)?;
        let started = Instant::now();
        update_atomic(&update_index, &root, &current, engine_config_sha256)?;
        let elapsed_ms = elapsed_ms(started);
        validate_index(&update_index, &current, engine_config_sha256)?;
        if full_index.exists() {
            fs::remove_file(&full_index)?;
        }
        build_atomic(&full_index, &root, &current, engine_config_sha256)?;
        let update = Fts5Index::at_path(&current, update_index.clone(), engine_config_sha256);
        let full = Fts5Index::at_path(&current, full_index.clone(), engine_config_sha256);
        let stale_results = compare_indexes(&update, &full, &root, queries)?;
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

fn index_projection(index_path: &Path) -> Result<Vec<IndexedRow>> {
    let connection = Connection::open_with_flags(index_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let projection_sql = format!(
        "SELECT c.storage_key, c.file_hash, c.path, c.heading, c.line_start, c.line_end,
                c.chunk_hash, f.{}, f.{}, f.{}
         FROM chunks AS c
         JOIN {FTS5_TABLE} AS f ON f.rowid = c.id
         ORDER BY c.path, c.line_start, c.storage_key",
        FTS5_COLUMNS[0], FTS5_COLUMNS[1], FTS5_COLUMNS[2]
    );
    let mut statement = connection.prepare(&projection_sql)?;
    statement
        .query_map([], |row| {
            Ok(IndexedRow {
                storage_key: row.get(0)?,
                file_hash: row.get(1)?,
                path: row.get(2)?,
                heading: row.get(3)?,
                line_start: row.get(4)?,
                line_end: row.get(5)?,
                chunk_hash: row.get(6)?,
                body: row.get(7)?,
                fts_heading: row.get(8)?,
                fts_path: row.get(9)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()
        .map_err(Into::into)
}

fn compare_indexes(
    update: &Fts5Index,
    full: &Fts5Index,
    root: &Path,
    queries: &[String],
) -> Result<usize> {
    let mut stale_results =
        usize::from(index_projection(update.index_path())? != index_projection(full.index_path())?);
    for query in queries {
        let request = SearchRequest {
            root: root.to_path_buf(),
            query: query.clone(),
            limit: 5,
            max_excerpt_chars: 1_200,
            max_results_per_path: None,
        };
        let update_results = update.search_index(request.clone())?.0.results;
        let full_results = full.search_index(request)?.0.results;
        if serde_json::to_value(update_results)? != serde_json::to_value(full_results)? {
            stale_results = stale_results
                .checked_add(1)
                .ok_or_else(|| anyhow!("FTS5 stale result counter overflow"))?;
        }
    }
    Ok(stale_results)
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
            file.write_all(b"# docs-search incremental addition\n\nDeterministic FTS5 fixture.\n")?;
            file.sync_all()?;
            Ok(())
        }
        "modify" => {
            let path = root.join(&selected[1]);
            let mut file = OpenOptions::new().append(true).open(path)?;
            file.write_all(
                b"\n\n## docs-search incremental modification\n\nDeterministic FTS5 fixture.\n",
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
        _ => bail!("unsupported FTS5 incremental operation {operation}"),
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
    fs::copy(source_root.join(source_relative), destination)?;
    Ok(())
}

pub(crate) fn fault_injection_passed(root: &Path, engine_config_sha256: &str) -> bool {
    fault_injection(root, engine_config_sha256).is_ok()
}

fn fault_injection(root: &Path, engine_config_sha256: &str) -> Result<()> {
    let corpus = corpus::load(root)?;
    let directory = unique_temp_directory("fault")?;
    let index_path = directory.join(INDEX_FILE);
    build_atomic(&index_path, root, &corpus, engine_config_sha256)?;
    let index = Fts5Index::at_path(&corpus, index_path.clone(), engine_config_sha256);
    fs::write(&index_path, b"corrupt-index-header")?;
    index.search(SearchRequest {
        root: root.to_path_buf(),
        query: "fault injection".to_owned(),
        limit: 5,
        max_excerpt_chars: 1_200,
        max_results_per_path: None,
    })?;
    validate_index(&index_path, &corpus, engine_config_sha256)
        .context("automatic FTS5 corruption recovery did not install a valid index")?;
    fs::write(&index_path, b"corrupt-index-header")?;
    let error = index
        .search_with_recovery(
            SearchRequest {
                root: root.to_path_buf(),
                query: "fault injection".to_owned(),
                limit: 5,
                max_excerpt_chars: 1_200,
                max_results_per_path: None,
            },
            true,
        )
        .expect_err("forced FTS5 rebuild failure must fail closed");
    if !error.to_string().contains("fail-closed") {
        bail!("forced FTS5 rebuild failure did not report fail-closed behavior");
    }
    fs::remove_dir_all(directory)?;
    Ok(())
}

fn cleanup_temporary_index(index_path: &Path) -> Result<()> {
    match fs::remove_file(index_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    remove_sidecars(index_path)
}

fn remove_index_files(index_path: &Path) -> Result<()> {
    match fs::remove_file(index_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    remove_sidecars(index_path)
}

fn remove_sidecars(index_path: &Path) -> Result<()> {
    for path in [
        index_path.with_extension("sqlite3-journal"),
        index_path.with_extension("sqlite3-wal"),
        index_path.with_extension("sqlite3-shm"),
    ] {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn temporary_index_path(index_path: &Path) -> Result<PathBuf> {
    let name = index_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("FTS5 index filename is not UTF-8"))?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(index_path.with_file_name(format!(".{name}.tmp-{}-{nonce}", std::process::id())))
}

fn unique_temp_directory(label: &str) -> Result<PathBuf> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = env::temp_dir().join(format!(
        "docs-search-fts5-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    Ok(path)
}

fn sync_file(path: &Path) -> Result<()> {
    OpenOptions::new().read(true).open(path)?.sync_all()?;
    Ok(())
}

fn sync_directory(path: &Path) -> Result<()> {
    OpenOptions::new().read(true).open(path)?.sync_all()?;
    Ok(())
}

fn to_i64(value: u64, label: &str) -> Result<i64> {
    i64::try_from(value).with_context(|| format!("{label} exceeds SQLite INTEGER range"))
}

fn to_usize(value: i64, label: &str) -> Result<usize> {
    usize::try_from(value).with_context(|| format!("{label} is negative or too large"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use tempfile::tempdir;

    use super::*;
    use crate::sqlite_cache::set_test_cache_home;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("evaluation/fixtures/stable-v1")
    }

    fn request(root: &Path, query: &str) -> SearchRequest {
        SearchRequest {
            root: root.to_path_buf(),
            query: query.to_owned(),
            limit: 5,
            max_excerpt_chars: 1_200,
            max_results_per_path: None,
        }
    }

    #[test]
    fn query_builder_quotes_terms_and_escapes_literal_quotes() {
        assert_eq!(
            fts_query(&["alpha\"beta".to_owned(), "OR".to_owned()]),
            "\"alpha\"\"beta\" OR \"OR\""
        );
        let terms = meaningful_query_tokens("alpha OR (beta*) path:docs \"quoted\"");
        assert_eq!(
            fts_query(&terms),
            "\"alpha\" OR \"or\" OR \"beta\" OR \"path\" OR \"docs\" OR \"quoted\""
        );
    }

    #[test]
    fn fts5_search_is_deterministic_safe_and_preserves_evidence() {
        let corpus = corpus::load(&fixture()).unwrap();
        let directory = tempdir().unwrap();
        let index_path = directory.path().join(INDEX_FILE);
        build_atomic(&index_path, &fixture(), &corpus, &"a".repeat(64)).unwrap();
        let index = Fts5Index::at_path(&corpus, index_path, &"a".repeat(64));
        let first = index
            .search(request(&fixture(), "sincronizar OR (skills*)"))
            .unwrap()
            .0;
        let second = index
            .search(request(&fixture(), "sincronizar OR (skills*)"))
            .unwrap()
            .0;
        assert_eq!(
            serde_json::to_value(&first.results).unwrap(),
            serde_json::to_value(&second.results).unwrap()
        );
        assert!(!first.results.is_empty());
        for result in first.results {
            assert!(!result.matched_terms.is_empty());
            let chunk = corpus
                .chunks
                .iter()
                .find(|chunk| {
                    chunk.path == result.path
                        && chunk.heading == result.heading
                        && chunk.chunk_hash == result.chunk_hash
                        && result.line_start >= chunk.line_start
                        && result.line_end <= chunk.line_end
                })
                .unwrap();
            assert_eq!(chunk.file_hash, result.file_hash);
        }
        let none = index
            .search(request(&fixture(), "quasarxylophone nebulaunseen"))
            .unwrap()
            .0;
        assert!(none.results.is_empty());
    }

    #[test]
    fn invalid_or_cross_root_requests_never_rebuild_the_bound_index() {
        let corpus = corpus::load(&fixture()).unwrap();
        let directory = tempdir().unwrap();
        let index_path = directory.path().join(INDEX_FILE);
        build_atomic(&index_path, &fixture(), &corpus, &"a".repeat(64)).unwrap();
        let index = Fts5Index::at_path(&corpus, index_path.clone(), &"a".repeat(64));
        let before = sha256::digest_hex(&fs::read(&index_path).unwrap());
        let mut empty = request(&fixture(), "   ");
        assert!(index.search(empty.clone()).is_err());
        empty.root = directory.path().to_path_buf();
        let error = index.search(empty).unwrap_err();
        assert!(error.to_string().contains("query must not be empty"));
        let cross_root = index
            .search(request(directory.path(), "safe query"))
            .unwrap_err();
        assert!(cross_root.to_string().contains("bound index root"));
        assert_eq!(before, sha256::digest_hex(&fs::read(index_path).unwrap()));
    }

    #[test]
    fn stale_source_rebuilds_and_forced_failure_is_fail_closed() {
        let source_root = fixture();
        let source = corpus::load(&source_root).unwrap();
        let directory = tempdir().unwrap();
        let root = directory.path().join("corpus");
        fs::create_dir(&root).unwrap();
        for path in source.file_hashes.keys() {
            copy_selected_file(&source_root, &root, path, path).unwrap();
        }
        let initial = corpus::load(&root).unwrap();
        let index_path = directory.path().join(INDEX_FILE);
        build_atomic(&index_path, &root, &initial, &"a".repeat(64)).unwrap();
        let index = Fts5Index::at_path(&initial, index_path.clone(), &"a".repeat(64));
        let modified = root.join(initial.file_hashes.keys().next().unwrap());
        let mut file = OpenOptions::new().append(true).open(modified).unwrap();
        file.write_all(b"\n\n## Zephyr FTS sentinel\n\nzephyr-fts-refresh\n")
            .unwrap();
        file.sync_all().unwrap();
        let response = index
            .search(request(&root, "zephyr fts refresh"))
            .unwrap()
            .0;
        assert!(!response.results.is_empty());
        fs::write(&index_path, b"corrupt-index-header").unwrap();
        let error = index
            .search_with_recovery(request(&root, "zephyr"), true)
            .unwrap_err();
        assert!(error.to_string().contains("fail-closed"));
    }

    #[test]
    fn incremental_workload_matches_clean_rebuilds() {
        let queries = vec![
            "sincronizar skills duplicadas".to_owned(),
            "documentacao de arquitetura".to_owned(),
        ];
        let steps = run_incremental_workload(&fixture(), &queries, &"a".repeat(64)).unwrap();
        assert_eq!(steps.len(), 4);
        assert!(
            steps
                .iter()
                .all(|step| step.equivalent_to_full_rebuild && step.stale_results == 0)
        );
    }

    #[test]
    fn atomic_update_keeps_concurrent_reader_valid() {
        let source_root = fixture();
        let source = corpus::load(&source_root).unwrap();
        let directory = tempdir().unwrap();
        let root = directory.path().join("corpus");
        fs::create_dir(&root).unwrap();
        for path in source.file_hashes.keys() {
            copy_selected_file(&source_root, &root, path, path).unwrap();
        }
        let initial = corpus::load(&root).unwrap();
        let index_path = directory.path().join(INDEX_FILE);
        build_atomic(&index_path, &root, &initial, &"a".repeat(64)).unwrap();
        let reader = Arc::new(Fts5Index::at_path(
            &initial,
            index_path.clone(),
            &"a".repeat(64),
        ));
        let child = {
            let reader = Arc::clone(&reader);
            let root = root.clone();
            thread::spawn(move || {
                for _ in 0..10 {
                    let response = reader
                        .search(request(&root, "sincronizar skills"))
                        .unwrap()
                        .0;
                    assert!(!response.results.is_empty());
                }
            })
        };
        let current = corpus::load(&root).unwrap();
        update_atomic(&index_path, &root, &current, &"a".repeat(64)).unwrap();
        child.join().unwrap();
    }

    #[test]
    fn cache_path_is_distinct_and_private() {
        let directory = tempdir().unwrap();
        set_test_cache_home(Some(directory.path().to_path_buf()));
        let corpus = corpus::load(&fixture()).unwrap();
        let (index, _, _) = Fts5Index::prepare_fresh(&fixture(), &corpus, &"a".repeat(64)).unwrap();
        assert_eq!(index.index_path().file_name().unwrap(), INDEX_FILE);
        assert_eq!(
            index.index_path().metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
        set_test_cache_home(None);
    }
}
