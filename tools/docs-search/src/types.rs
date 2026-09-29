use std::path::PathBuf;

use serde::Serialize;

pub const SCHEMA_VERSION: u32 = 2;
pub const ENGINE: &str = "lexical-bm25-v1";

#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub root: PathBuf,
    pub query: String,
    pub limit: usize,
    pub max_excerpt_chars: usize,
    /// Optional post-ranking cap; `None` preserves the original ranking.
    pub max_results_per_path: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CorpusSummary {
    pub files: usize,
    pub chunks: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub rank: usize,
    /// Pre-diversity BM25 position for auditing the final selection.
    pub raw_rank: usize,
    pub path: String,
    pub heading: Option<String>,
    pub line_start: usize,
    pub line_end: usize,
    pub excerpt: String,
    pub file_hash: String,
    pub chunk_hash: String,
    pub score: f64,
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchSelection {
    pub max_results_per_path: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub schema_version: u32,
    pub engine: &'static str,
    pub query: String,
    pub root: String,
    pub corpus: CorpusSummary,
    pub selection: SearchSelection,
    pub results: Vec<SearchResult>,
}
