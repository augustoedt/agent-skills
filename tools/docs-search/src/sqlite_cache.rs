use std::collections::BTreeMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
#[cfg(test)]
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, OpenFlags, Transaction, params};
use serde::{Deserialize, Serialize};

use crate::corpus::{self, Chunk, Corpus};
use crate::evaluate::corpus_fingerprint;
use crate::search::{SearchDocument, SearchTimings, search_documents, search_with_timings};
use crate::sha256;
use crate::types::{SearchRequest, SearchResponse};

pub const SQLITE_ENGINE: &str = "sqlite-cache-bm25-v1";
pub const EXPECTED_SQLITE_VERSION: &str = "3.53.2";
pub(crate) const SQLITE_RUNTIME_CHECKS: [&str; 3] = [
    "sqlite_version_equals_expected",
    "compile_option_ENABLE_FTS5",
    "fts5_create-insert-match-bm25-smoke",
];
const INDEX_SCHEMA_VERSION: &str = "1";
const PARSER_VERSION: &str = "markdown-heading-fence-aware-v1";
const INDEX_FILE: &str = "index.sqlite3";

#[cfg(test)]
static TEST_CACHE_HOME: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

const CREATE_SCHEMA: &str = "
CREATE TABLE metadata(key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE files(path TEXT PRIMARY KEY, file_hash TEXT, bytes INTEGER);
CREATE TABLE chunks(
  chunk_hash TEXT PRIMARY KEY,
  file_hash TEXT,
  path TEXT,
  heading TEXT,
  line_start INTEGER,
  line_end INTEGER,
  text TEXT,
  tokens_json TEXT,
  normalized_text TEXT
);
";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct IncrementalStep {
    pub operation: String,
    pub elapsed_ms: f64,
    pub equivalent_to_full_rebuild: bool,
    pub stale_results: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct IndexingMetrics {
    pub full_build_ms: Option<f64>,
    pub incremental_steps: Vec<IncrementalStep>,
    pub rebuild_succeeded: bool,
    pub corruption_detected: bool,
    pub runtime_checks: BTreeMap<String, bool>,
}

impl IndexingMetrics {
    pub(crate) fn direct() -> Self {
        Self {
            full_build_ms: None,
            incremental_steps: Vec::new(),
            rebuild_succeeded: true,
            corruption_detected: false,
            runtime_checks: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SqliteRuntime {
    pub version: String,
    pub compile_options_sha256: String,
    pub checks: BTreeMap<String, bool>,
}

#[derive(Debug)]
pub(crate) struct SqliteCache {
    root: PathBuf,
    index_path: PathBuf,
    engine_config_sha256: String,
    state: Mutex<CacheState>,
}

#[derive(Debug)]
struct CacheState {
    files: usize,
    corpus_fingerprint: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct IndexState {
    pub corruption_detected: bool,
    pub rebuild_succeeded: bool,
}

impl SqliteCache {
    pub(crate) fn open_existing(
        root: &Path,
        corpus: &Corpus,
        engine_config_sha256: &str,
    ) -> Result<Self> {
        validate_runtime()?;
        let index_path = cache_path(root)?;
        validate_index(&index_path, corpus, engine_config_sha256)?;
        Ok(Self {
            root: corpus.root.clone(),
            index_path,
            engine_config_sha256: engine_config_sha256.to_owned(),
            state: cache_state(corpus),
        })
    }

    pub(crate) fn prepare_fresh(
        root: &Path,
        corpus: &Corpus,
        engine_config_sha256: &str,
    ) -> Result<(Self, IndexingMetrics, SqliteRuntime)> {
        let runtime = validate_runtime()?;
        let index_path = cache_path(root)?;
        if let Some(parent) = index_path.parent() {
            create_private_directory(parent)?;
        }
        remove_index_files(&index_path)?;
        let started = Instant::now();
        build_atomic(&index_path, root, corpus, engine_config_sha256)?;
        let full_build_ms = elapsed_ms(started);
        validate_index(&index_path, corpus, engine_config_sha256)?;
        Ok((
            Self {
                root: corpus.root.clone(),
                index_path,
                engine_config_sha256: engine_config_sha256.to_owned(),
                state: cache_state(corpus),
            },
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
        let (response, timings, _) = self.search_with_recovery(request, false)?;
        Ok((response, timings))
    }

    pub(crate) fn search_with_recovery(
        &self,
        request: SearchRequest,
        force_rebuild_failure: bool,
    ) -> Result<(SearchResponse, SearchTimings, bool)> {
        match self.search_cached(request.clone()) {
            Ok((response, timings)) => Ok((response, timings, false)),
            Err(cache_error) => {
                let corpus = corpus::load(&request.root)?;
                let rebuilt = !force_rebuild_failure
                    && rebuild_at(
                        &self.index_path,
                        &request.root,
                        &corpus,
                        &self.engine_config_sha256,
                    )
                    .is_ok();
                if rebuilt {
                    let mut state = self
                        .state
                        .lock()
                        .map_err(|_| anyhow!("SQLite cache state mutex is poisoned"))?;
                    state.files = corpus.files;
                    state.corpus_fingerprint = corpus_fingerprint(&corpus.file_hashes);
                    drop(state);
                    return self
                        .search_cached(request)
                        .map(|(response, timings)| (response, timings, false));
                }
                let (response, timings) = search_with_timings(request).with_context(|| {
                    format!(
                        "SQLite cache failed ({cache_error:#}); rebuild failed; direct BM25 fallback also failed"
                    )
                })?;
                Ok((response, timings, true))
            }
        }
    }

    fn search_cached(&self, request: SearchRequest) -> Result<(SearchResponse, SearchTimings)> {
        let total_started = Instant::now();
        let lookup_started = Instant::now();
        let (_, current_hashes) = corpus::file_hashes(&self.root)?;
        let current_fingerprint = corpus_fingerprint(&current_hashes);
        let files = {
            let state = self
                .state
                .lock()
                .map_err(|_| anyhow!("SQLite cache state mutex is poisoned"))?;
            if state.corpus_fingerprint != current_fingerprint
                || state.files != current_hashes.len()
            {
                bail!("SQLite cache corpus fingerprint is stale");
            }
            state.files
        };
        let documents = load_documents(&self.index_path).with_context(|| {
            format!(
                "SQLite cache lookup failed for {}",
                self.index_path.display()
            )
        })?;
        let lookup_ms = elapsed_ms(lookup_started);
        search_documents(
            request,
            SQLITE_ENGINE,
            self.root.to_string_lossy().into_owned(),
            files,
            documents,
            lookup_ms,
            total_started,
        )
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
            state: cache_state(corpus),
        }
    }
}

fn cache_state(corpus: &Corpus) -> Mutex<CacheState> {
    Mutex::new(CacheState {
        files: corpus.files,
        corpus_fingerprint: corpus_fingerprint(&corpus.file_hashes),
    })
}

pub(crate) fn validate_runtime() -> Result<SqliteRuntime> {
    let version = rusqlite::version().to_owned();
    let version_ok = version == EXPECTED_SQLITE_VERSION;
    let connection = Connection::open_in_memory().context("failed to open SQLite runtime check")?;
    let mut statement = connection.prepare("PRAGMA compile_options")?;
    let mut options: Vec<String> = statement
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    options.sort();
    let fts5_enabled = options.iter().any(|option| option == "ENABLE_FTS5");
    let compile_options_sha256 = sha256::digest_hex(options.join("\n").as_bytes());
    let smoke = fts5_smoke(&connection).is_ok();
    let checks = BTreeMap::from([
        (SQLITE_RUNTIME_CHECKS[0].to_owned(), version_ok),
        (SQLITE_RUNTIME_CHECKS[1].to_owned(), fts5_enabled),
        (SQLITE_RUNTIME_CHECKS[2].to_owned(), smoke),
    ]);
    if !checks.values().all(|passed| *passed) {
        bail!("SQLite runtime contract failed: version={version}, checks={checks:?}");
    }
    Ok(SqliteRuntime {
        version,
        compile_options_sha256,
        checks,
    })
}

fn fts5_smoke(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE VIRTUAL TABLE temp.docs_search_fts5_smoke USING fts5(body);
         INSERT INTO docs_search_fts5_smoke(body) VALUES ('alpha beta');",
    )?;
    let score: f64 = connection.query_row(
        "SELECT bm25(docs_search_fts5_smoke) FROM docs_search_fts5_smoke
         WHERE docs_search_fts5_smoke MATCH 'alpha'",
        [],
        |row| row.get(0),
    )?;
    if !score.is_finite() {
        bail!("FTS5 smoke test returned a non-finite BM25 score");
    }
    Ok(())
}

pub(crate) fn cache_path(root: &Path) -> Result<PathBuf> {
    let canonical = root
        .canonicalize()
        .with_context(|| format!("failed to resolve cache root {}", root.display()))?;
    let key = sha256::digest_hex(canonical.to_string_lossy().as_bytes());
    #[cfg(test)]
    if let Some(base) = TEST_CACHE_HOME
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| anyhow!("test cache mutex is poisoned"))?
        .clone()
    {
        return Ok(base.join("docs-search").join(key).join(INDEX_FILE));
    }
    let base = if let Some(value) = env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(value)
    } else {
        let home = env::var_os("HOME")
            .ok_or_else(|| anyhow!("HOME or XDG_CACHE_HOME is required for the SQLite cache"))?;
        PathBuf::from(home).join(".cache")
    };
    Ok(base.join("docs-search").join(key).join(INDEX_FILE))
}

#[cfg(test)]
pub(crate) fn set_test_cache_home(path: Option<PathBuf>) {
    *TEST_CACHE_HOME
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("test cache mutex") = path;
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

fn update_atomic(
    index_path: &Path,
    root: &Path,
    corpus: &Corpus,
    engine_config_sha256: &str,
) -> Result<()> {
    validate_update_source(index_path, engine_config_sha256)?;
    let parent = index_path
        .parent()
        .ok_or_else(|| anyhow!("SQLite index path has no parent"))?;
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
                transaction.execute("DELETE FROM chunks WHERE path = ?1", params![path])?;
                transaction.execute("DELETE FROM files WHERE path = ?1", params![path])?;
            }
        }
        for (path, file_hash) in &corpus.file_hashes {
            if existing.get(path) == Some(file_hash) {
                continue;
            }
            transaction.execute("DELETE FROM chunks WHERE path = ?1", params![path])?;
            transaction.execute("DELETE FROM files WHERE path = ?1", params![path])?;
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
    if result.is_err() && temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn build_atomic(
    index_path: &Path,
    root: &Path,
    corpus: &Corpus,
    engine_config_sha256: &str,
) -> Result<()> {
    let parent = index_path
        .parent()
        .ok_or_else(|| anyhow!("SQLite index path has no parent"))?;
    create_private_directory(parent)?;
    let temp_path = temporary_index_path(index_path)?;
    if temp_path.exists() {
        bail!(
            "temporary SQLite index already exists: {}",
            temp_path.display()
        );
    }

    let result = (|| -> Result<()> {
        let mut connection = Connection::open(&temp_path)
            .with_context(|| format!("failed to create SQLite index {}", temp_path.display()))?;
        fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600))?;
        connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;")?;
        connection.execute_batch(CREATE_SCHEMA)?;
        let transaction = connection.transaction()?;
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
        for (path, file_hash) in &corpus.file_hashes {
            let bytes = fs::metadata(root.join(path))?.len();
            transaction.execute(
                "INSERT INTO files(path, file_hash, bytes) VALUES (?1, ?2, ?3)",
                params![path, file_hash, to_i64(bytes, "file bytes")?],
            )?;
        }
        for chunk in &corpus.chunks {
            insert_chunk(&transaction, chunk)?;
        }
        transaction.commit()?;
        connection.execute_batch("PRAGMA optimize;")?;
        drop(connection);
        sync_file(&temp_path)?;
        remove_sidecars(index_path)?;
        fs::rename(&temp_path, index_path).with_context(|| {
            format!(
                "failed to atomically install SQLite index {}",
                index_path.display()
            )
        })?;
        fs::set_permissions(index_path, fs::Permissions::from_mode(0o600))?;
        sync_directory(parent)?;
        Ok(())
    })();

    if result.is_err() && temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }
    result
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

fn insert_chunk(transaction: &Transaction<'_>, chunk: &Chunk) -> Result<()> {
    let document = SearchDocument::from_chunk(chunk.clone());
    transaction.execute(
        "INSERT INTO chunks(
            chunk_hash, file_hash, path, heading, line_start, line_end,
            text, tokens_json, normalized_text
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            storage_chunk_key(chunk),
            chunk.file_hash,
            chunk.path,
            chunk.heading,
            to_i64(chunk.line_start as u64, "line_start")?,
            to_i64(chunk.line_end as u64, "line_end")?,
            chunk.text,
            serde_json::to_string(&document.tokens)?,
            document.normalized,
        ],
    )?;
    Ok(())
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
            bail!("SQLite metadata {key} is incompatible");
        }
    }
    Ok(())
}

fn validate_index(index_path: &Path, corpus: &Corpus, engine_config_sha256: &str) -> Result<()> {
    let connection = Connection::open_with_flags(index_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("failed to open SQLite index {}", index_path.display()))?;
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        bail!("SQLite integrity check failed: {integrity}");
    }
    let expected = BTreeMap::from([
        ("schema_version", INDEX_SCHEMA_VERSION.to_owned()),
        ("parser_version", PARSER_VERSION.to_owned()),
        ("engine_config_sha256", engine_config_sha256.to_owned()),
        (
            "corpus_fingerprint",
            corpus_fingerprint(&corpus.file_hashes),
        ),
    ]);
    for (key, expected) in expected {
        let actual: String = connection
            .query_row(
                "SELECT value FROM metadata WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .with_context(|| format!("SQLite metadata {key} is missing"))?;
        if actual != expected {
            bail!("SQLite metadata {key} does not match the current corpus");
        }
    }
    let files: i64 = connection.query_row("SELECT count(*) FROM files", [], |row| row.get(0))?;
    let chunks: i64 = connection.query_row("SELECT count(*) FROM chunks", [], |row| row.get(0))?;
    if to_usize(files, "file count")? != corpus.files
        || to_usize(chunks, "chunk count")? != corpus.chunks.len()
    {
        bail!("SQLite index counts do not match the current corpus");
    }
    Ok(())
}

fn load_documents(index_path: &Path) -> Result<Vec<SearchDocument>> {
    let connection = Connection::open_with_flags(index_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut statement = connection.prepare(
        "SELECT chunk_hash, file_hash, path, heading, line_start, line_end,
                text, tokens_json, normalized_text
         FROM chunks ORDER BY path, line_start, chunk_hash",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;
    rows.map(|row| {
        let (storage_key, file_hash, path, heading, line_start, line_end, text, tokens, normalized) =
            row?;
        let chunk_hash = blake3::hash(text.as_bytes()).to_hex().to_string();
        if !storage_key.starts_with(&format!("{chunk_hash}:")) {
            bail!("SQLite storage chunk key does not match cached text");
        }
        let chunk = Chunk {
            path,
            heading,
            line_start: to_usize(line_start, "line_start")?,
            line_end: to_usize(line_end, "line_end")?,
            text,
            file_hash,
            chunk_hash,
        };
        let tokens: Vec<String> = serde_json::from_str(&tokens)?;
        Ok(SearchDocument::from_cached(chunk, normalized, tokens))
    })
    .collect()
}

pub(crate) fn run_incremental_workload(
    source_root: &Path,
    queries: &[String],
    engine_config_sha256: &str,
) -> Result<Vec<IncrementalStep>> {
    let source = corpus::load(source_root)?;
    let selected: Vec<_> = source.file_hashes.keys().take(4).cloned().collect();
    if selected.len() != 4 {
        bail!("incremental workload requires at least four selected Markdown files");
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
        if full_index.exists() {
            fs::remove_file(&full_index)?;
        }
        build_atomic(&full_index, &root, &current, engine_config_sha256)?;
        let update = SqliteCache::at_path(&current, update_index.clone(), engine_config_sha256);
        let full = SqliteCache::at_path(&current, full_index.clone(), engine_config_sha256);
        let stale_results = compare_caches(&update, &full, &root, queries)?;
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

fn compare_caches(
    update: &SqliteCache,
    full: &SqliteCache,
    root: &Path,
    queries: &[String],
) -> Result<usize> {
    let mut stale_results = 0usize;
    for query in queries {
        let request = SearchRequest {
            root: root.to_path_buf(),
            query: query.clone(),
            limit: 5,
            max_excerpt_chars: 1_200,
            max_results_per_path: None,
        };
        let update_results = update.search(request.clone())?.0.results;
        let full_results = full.search(request)?.0.results;
        let update = serde_json::to_value(update_results)?;
        let full = serde_json::to_value(full_results)?;
        if update != full {
            stale_results = stale_results
                .checked_add(1)
                .ok_or_else(|| anyhow!("stale result counter overflow"))?;
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
            let text = "# docs-search incremental addition\n\nDeterministic fixture derived from the first sorted source document.\n";
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            Ok(())
        }
        "modify" => {
            let path = root.join(&selected[1]);
            let mut file = OpenOptions::new().append(true).open(&path)?;
            file.write_all(
                b"\n\n## docs-search incremental modification\n\nDeterministic fixture.\n",
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
        _ => bail!("unsupported incremental operation {operation}"),
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

pub(crate) fn fault_injection_passed(root: &Path, engine_config_sha256: &str) -> bool {
    fault_injection(root, engine_config_sha256).is_ok()
}

fn fault_injection(root: &Path, engine_config_sha256: &str) -> Result<()> {
    let corpus = corpus::load(root)?;
    let directory = unique_temp_directory("fault")?;
    let index_path = directory.join(INDEX_FILE);
    build_atomic(&index_path, root, &corpus, engine_config_sha256)?;
    fs::write(&index_path, b"corrupt-index-header")?;
    let state = rebuild_at(&index_path, root, &corpus, engine_config_sha256)?;
    if !state.corruption_detected || !state.rebuild_succeeded {
        bail!("corrupt SQLite index was not detected and rebuilt");
    }

    fs::write(&index_path, b"corrupt-index-header")?;
    let request = SearchRequest {
        root: root.to_path_buf(),
        query: "fault injection".to_owned(),
        limit: 5,
        max_excerpt_chars: 1_200,
        max_results_per_path: None,
    };
    let direct = search_with_timings(request.clone())?.0;
    let cache = SqliteCache::at_path(&corpus, index_path.clone(), engine_config_sha256);
    let (fallback, _, fallback_used) = cache.search_with_recovery(request, true)?;
    if !fallback_used {
        bail!("forced rebuild failure did not activate direct BM25 fallback");
    }
    if direct.results.len() != fallback.results.len()
        || direct
            .results
            .iter()
            .zip(&fallback.results)
            .any(|(left, right)| {
                left.rank != right.rank
                    || left.path != right.path
                    || left.chunk_hash != right.chunk_hash
                    || left.score != right.score
            })
    {
        bail!("forced rebuild failure did not use equivalent direct BM25 fallback");
    }
    fs::remove_dir_all(&directory)?;
    Ok(())
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
        .ok_or_else(|| anyhow!("SQLite index filename is not UTF-8"))?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(index_path.with_file_name(format!(".{name}.tmp-{}-{nonce}", std::process::id())))
}

fn unique_temp_directory(label: &str) -> Result<PathBuf> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = env::temp_dir().join(format!(
        "docs-search-sqlite-{label}-{}-{nonce}",
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

fn elapsed_ms(started: Instant) -> f64 {
    (started.elapsed().as_secs_f64() * 1_000_000.0).round() / 1_000.0
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tempfile::tempdir;

    use super::*;
    use crate::search::search_with_timings;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("evaluation/fixtures/stable-v1")
    }

    #[test]
    fn bundled_runtime_matches_the_frozen_contract() {
        let runtime = validate_runtime().unwrap();
        assert_eq!(runtime.version, EXPECTED_SQLITE_VERSION);
        assert!(runtime.checks.values().all(|passed| *passed));
        assert_eq!(runtime.compile_options_sha256.len(), 64);
    }

    #[test]
    fn storage_keys_preserve_duplicate_chunk_hashes_without_losing_results() {
        let directory = tempdir().unwrap();
        fs::create_dir(directory.path().join("docs")).unwrap();
        fs::write(directory.path().join("README.md"), "Same chunk.\n").unwrap();
        fs::write(directory.path().join("docs/duplicate.md"), "Same chunk.\n").unwrap();
        let corpus = corpus::load(directory.path()).unwrap();
        let index = directory.path().join(INDEX_FILE);
        build_atomic(&index, directory.path(), &corpus, &"a".repeat(64)).unwrap();
        let cache = SqliteCache::at_path(&corpus, index, &"a".repeat(64));
        let request = SearchRequest {
            root: directory.path().to_path_buf(),
            query: "same chunk".to_owned(),
            limit: 5,
            max_excerpt_chars: 1_200,
            max_results_per_path: None,
        };
        let direct = search_with_timings(request.clone()).unwrap().0;
        let cached = cache.search(request).unwrap().0;
        assert_eq!(direct.results.len(), 2);
        assert_eq!(
            serde_json::to_value(direct.results).unwrap(),
            serde_json::to_value(cached.results).unwrap()
        );
    }

    #[test]
    fn cached_search_is_exactly_equivalent_to_direct_bm25() {
        let root = fixture();
        let corpus = corpus::load(&root).unwrap();
        let directory = tempdir().unwrap();
        let index = directory.path().join(INDEX_FILE);
        build_atomic(&index, &root, &corpus, "a".repeat(64).as_str()).unwrap();
        let cache = SqliteCache::at_path(&corpus, index, &"a".repeat(64));
        let request = SearchRequest {
            root: root.clone(),
            query: "sincronizar skills duplicadas".to_owned(),
            limit: 5,
            max_excerpt_chars: 1_200,
            max_results_per_path: None,
        };
        let direct = search_with_timings(request.clone()).unwrap().0;
        let cached = cache.search(request).unwrap().0;
        assert_eq!(
            serde_json::to_value(direct.results).unwrap(),
            serde_json::to_value(cached.results).unwrap()
        );
    }

    #[test]
    fn stale_source_fingerprint_triggers_atomic_rebuild_before_results() {
        let source_root = fixture();
        let source = corpus::load(&source_root).unwrap();
        let directory = tempdir().unwrap();
        let root = directory.path().join("corpus");
        fs::create_dir(&root).unwrap();
        for path in source.file_hashes.keys() {
            copy_selected_file(&source_root, &root, path, path).unwrap();
        }
        let initial = corpus::load(&root).unwrap();
        let index = directory.path().join(INDEX_FILE);
        build_atomic(&index, &root, &initial, &"a".repeat(64)).unwrap();
        let cache = SqliteCache::at_path(&initial, index.clone(), &"a".repeat(64));
        let modified = root.join(initial.file_hashes.keys().next().unwrap());
        let mut file = OpenOptions::new().append(true).open(modified).unwrap();
        file.write_all(b"\n\n## Zephyr sentinel\n\nzephyr-cache-refresh\n")
            .unwrap();
        file.sync_all().unwrap();
        let response = cache
            .search(SearchRequest {
                root: root.clone(),
                query: "zephyr cache refresh".to_owned(),
                limit: 5,
                max_excerpt_chars: 1_200,
                max_results_per_path: None,
            })
            .unwrap()
            .0;
        assert!(!response.results.is_empty());
        let current = corpus::load(&root).unwrap();
        validate_index(&index, &current, &"a".repeat(64)).unwrap();
    }

    #[test]
    fn concurrent_readers_share_an_immutable_index() {
        let root = fixture();
        let corpus = corpus::load(&root).unwrap();
        let directory = tempdir().unwrap();
        let index = directory.path().join(INDEX_FILE);
        build_atomic(&index, &root, &corpus, &"a".repeat(64)).unwrap();
        let cache = Arc::new(SqliteCache::at_path(&corpus, index, &"a".repeat(64)));
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let cache = Arc::clone(&cache);
                let root = root.clone();
                std::thread::spawn(move || {
                    cache
                        .search(SearchRequest {
                            root,
                            query: "sincronizar skills duplicadas".to_owned(),
                            limit: 5,
                            max_excerpt_chars: 1_200,
                            max_results_per_path: None,
                        })
                        .unwrap()
                        .0
                        .results
                })
            })
            .collect();
        let projections: Vec<_> = handles
            .into_iter()
            .map(|handle| serde_json::to_value(handle.join().unwrap()).unwrap())
            .collect();
        assert!(projections.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn atomic_update_does_not_break_concurrent_readers() {
        let source_root = fixture();
        let source = corpus::load(&source_root).unwrap();
        let directory = tempdir().unwrap();
        let root = directory.path().join("corpus");
        fs::create_dir(&root).unwrap();
        for path in source.file_hashes.keys() {
            copy_selected_file(&source_root, &root, path, path).unwrap();
        }
        let initial = corpus::load(&root).unwrap();
        let index = directory.path().join(INDEX_FILE);
        build_atomic(&index, &root, &initial, &"a".repeat(64)).unwrap();
        let cache = Arc::new(SqliteCache::at_path(
            &initial,
            index.clone(),
            &"a".repeat(64),
        ));
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let cache = Arc::clone(&cache);
                let root = root.clone();
                std::thread::spawn(move || {
                    for _ in 0..10 {
                        cache
                            .search(SearchRequest {
                                root: root.clone(),
                                query: "sincronizar skills duplicadas".to_owned(),
                                limit: 5,
                                max_excerpt_chars: 1_200,
                                max_results_per_path: None,
                            })
                            .unwrap();
                    }
                })
            })
            .collect();
        let modified = root.join(source.file_hashes.keys().next().unwrap());
        let mut file = OpenOptions::new().append(true).open(modified).unwrap();
        file.write_all(b"\nAtomic update fixture.\n").unwrap();
        file.sync_all().unwrap();
        let current = corpus::load(&root).unwrap();
        update_atomic(&index, &root, &current, &"a".repeat(64)).unwrap();
        for handle in handles {
            handle.join().unwrap();
        }
        validate_index(&index, &current, &"a".repeat(64)).unwrap();
    }

    #[test]
    fn incompatible_metadata_and_corruption_trigger_atomic_rebuild() {
        let root = fixture();
        let corpus = corpus::load(&root).unwrap();
        let directory = tempdir().unwrap();
        let index = directory.path().join(INDEX_FILE);
        build_atomic(&index, &root, &corpus, &"a".repeat(64)).unwrap();
        let connection = Connection::open(&index).unwrap();
        connection
            .execute(
                "UPDATE metadata SET value = 'old' WHERE key = 'parser_version'",
                [],
            )
            .unwrap();
        drop(connection);
        assert!(validate_index(&index, &corpus, &"a".repeat(64)).is_err());
        assert!(validate_index(&index, &corpus, &"b".repeat(64)).is_err());
        let state = rebuild_at(&index, &root, &corpus, &"a".repeat(64)).unwrap();
        assert!(state.corruption_detected);
        assert!(state.rebuild_succeeded);
        fs::write(&index, b"broken").unwrap();
        fs::write(index.with_extension("sqlite3-journal"), b"stale journal").unwrap();
        fs::write(index.with_extension("sqlite3-wal"), b"stale wal").unwrap();
        fs::write(index.with_extension("sqlite3-shm"), b"stale shm").unwrap();
        let state = rebuild_at(&index, &root, &corpus, &"a".repeat(64)).unwrap();
        assert!(state.corruption_detected);
        assert!(state.rebuild_succeeded);
        assert!(!index.with_extension("sqlite3-journal").exists());
        assert!(!index.with_extension("sqlite3-wal").exists());
        assert!(!index.with_extension("sqlite3-shm").exists());
    }

    #[test]
    fn incremental_workload_covers_all_operations_without_stale_results() {
        let steps = run_incremental_workload(
            &fixture(),
            &[
                "sincronizar skills duplicadas".to_owned(),
                "informacao que nao existe".to_owned(),
            ],
            &"a".repeat(64),
        )
        .unwrap();
        assert_eq!(
            steps
                .iter()
                .map(|step| step.operation.as_str())
                .collect::<Vec<_>>(),
            ["add", "modify", "rename", "remove"]
        );
        assert!(steps.iter().all(|step| {
            step.equivalent_to_full_rebuild && step.stale_results == 0 && step.elapsed_ms >= 0.0
        }));
    }

    #[test]
    fn fault_injection_rebuilds_corruption_and_uses_explicit_fallback() {
        assert!(fault_injection_passed(&fixture(), &"a".repeat(64)));
    }
}
