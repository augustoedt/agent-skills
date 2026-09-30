//! Phase 4.5 — `hybrid-rrf-v1`.
//!
//! Deterministic reciprocal-rank fusion of the two frozen retrieval sources:
//! `fts5-v1` (lexical) and `local-embeddings-v1` (semantic). The hybrid never
//! introduces a third scoring mechanism — it only merges the two source lists
//! by chunk hash with `1/(k + one_based_rank)` contributions.
//!
//! Missing embedding artifacts fail closed before any FTS5 work happens, so a
//! missing model can never leave a partially built lexical index behind. Any
//! source error propagates untouched: no partial response is ever returned.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::time::Instant;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::corpus::Corpus;
use crate::embeddings::{
    EMBEDDINGS_CONFIG_SHA256, EMBEDDINGS_ENGINE, EmbeddingsIndex, ModelSpec,
    fault_injection_passed as embeddings_fault_injection_passed,
    run_incremental_workload as run_embeddings_incremental_workload, validate_model_artifacts,
};
use crate::fts5::{
    FTS5_CONFIG_SHA256, FTS5_ENGINE, Fts5Index,
    fault_injection_passed as fts5_fault_injection_passed,
    run_incremental_workload as run_fts5_incremental_workload,
};
use crate::search::{
    SearchTimings, elapsed_ms, round_score, select_with_path_diversity, validate_request,
};
use crate::sqlite_cache::{IncrementalStep, IndexingMetrics, SqliteRuntime};
use crate::types::{SCHEMA_VERSION, SearchRequest, SearchResponse, SearchResult, SearchSelection};

pub(crate) const HYBRID_ENGINE: &str = "hybrid-rrf-v1";
pub(crate) const SOURCE_CANDIDATE_DEPTH: usize = 50;
pub(crate) const RRF_K: usize = 60;

/// Frozen engine configuration for `hybrid-rrf-v1`.
///
/// The returned value must equal `#/$defs/hybridConfig` from the frozen protocol
/// schema. The two source engine identifiers come from the runtime adapters so
/// the fused contract cannot drift from the sources it composes.
pub(crate) fn frozen_configuration() -> Value {
    json!({
        "sources": [FTS5_ENGINE, EMBEDDINGS_ENGINE],
        "source_candidate_depth": SOURCE_CANDIDATE_DEPTH,
        "fusion": "reciprocal-rank-fusion",
        "rrf_k": RRF_K,
        "deduplication": "chunk-hash",
        "ranking": "sum(1/(60+one-based-source-rank))-descending-then-path-asc-line-start-asc",
        "abstention": "return-empty-only-when-fts5-has-no-row-and-top-embedding-dot-product-is-less-than-0.80"
    })
}

/// First one-based rank observed for each source, per published result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HybridSourceRanks {
    pub fts5: Option<usize>,
    pub embedding: Option<usize>,
}

/// Complete result of one fused hybrid search.
#[derive(Debug)]
pub(crate) struct HybridSearchOutput {
    pub response: SearchResponse,
    pub timings: SearchTimings,
    /// One entry per published result, aligned with `response.results`.
    pub source_ranks: Vec<HybridSourceRanks>,
}

/// A fused candidate before diversity selection.
#[derive(Debug)]
pub(crate) struct FusedCandidate {
    pub(crate) result: SearchResult,
    pub(crate) score: f64,
    pub(crate) source_ranks: HybridSourceRanks,
}

#[derive(Debug)]
struct Accumulator {
    score: f64,
    representative: SearchResult,
    matched_terms: BTreeSet<String>,
    fts5_rank: Option<usize>,
    embedding_rank: Option<usize>,
}

impl Accumulator {
    fn new(result: &SearchResult) -> Self {
        Self {
            score: 0.0,
            representative: result.clone(),
            matched_terms: result.matched_terms.iter().cloned().collect(),
            fts5_rank: None,
            embedding_rank: None,
        }
    }

    fn into_candidate(self) -> FusedCandidate {
        let mut result = self.representative;
        result.matched_terms = self.matched_terms.into_iter().collect();
        FusedCandidate {
            result,
            score: self.score,
            source_ranks: HybridSourceRanks {
                fts5: self.fts5_rank,
                embedding: self.embedding_rank,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Source {
    Fts5,
    Embedding,
}

/// Owns both frozen source indexes and exposes the fused search contract.
pub(crate) struct HybridIndex {
    primary: Fts5Index,
    secondary: EmbeddingsIndex,
}

impl HybridIndex {
    /// Build both source indexes from scratch.
    ///
    /// Model artifacts are validated before any FTS5 work so a missing model
    /// fails closed without leaving a partially built lexical index behind.
    pub(crate) fn prepare_fresh(
        root: &Path,
        corpus: &Corpus,
        model_spec: &ModelSpec,
        model_directory: &Path,
    ) -> Result<(Self, IndexingMetrics, SqliteRuntime)> {
        validate_model_artifacts(model_spec, model_directory)?;
        let (primary, fts5_metrics, runtime) =
            Fts5Index::prepare_fresh(root, corpus, FTS5_CONFIG_SHA256)?;
        let (secondary, embeddings_metrics) = EmbeddingsIndex::prepare_fresh(
            root,
            corpus,
            EMBEDDINGS_CONFIG_SHA256,
            model_spec,
            model_directory,
        )?;
        let metrics = merge_indexing_metrics(fts5_metrics, embeddings_metrics);
        Ok((Self { primary, secondary }, metrics, runtime))
    }

    /// Open both source indexes that must already exist and be current.
    pub(crate) fn open_existing(
        root: &Path,
        corpus: &Corpus,
        model_spec: &ModelSpec,
        model_directory: &Path,
    ) -> Result<Self> {
        validate_model_artifacts(model_spec, model_directory)?;
        let primary = Fts5Index::open_existing(root, corpus, FTS5_CONFIG_SHA256)?;
        let secondary = EmbeddingsIndex::open_existing(
            root,
            corpus,
            EMBEDDINGS_CONFIG_SHA256,
            model_spec,
            model_directory,
        )?;
        Ok(Self { primary, secondary })
    }

    /// Search both sources and fuse their rankings by chunk hash.
    pub(crate) fn search(&self, request: SearchRequest) -> Result<HybridSearchOutput> {
        validate_request(&request)?;
        let total_started = Instant::now();
        let source_request = SearchRequest {
            root: request.root.clone(),
            query: request.query.clone(),
            limit: SOURCE_CANDIDATE_DEPTH,
            max_excerpt_chars: request.max_excerpt_chars,
            max_results_per_path: None,
        };
        let fts5_source = self.primary.search(source_request.clone());
        let embedding_source = self.secondary.search(source_request);
        let ((fts5_response, fts5_timings), (embedding_response, embedding_timings)) =
            combine_source_outputs(fts5_source, embedding_source)?;
        ensure_matching_corpus(&fts5_response, &embedding_response)?;
        ensure_source_depth(&fts5_response, &embedding_response)?;

        let fusion_started = Instant::now();
        let candidates = order_candidates(fuse_source_lists(
            &fts5_response.results,
            &embedding_response.results,
        ));
        let selected = select_with_path_diversity(
            candidates,
            request.limit,
            request.max_results_per_path,
            |candidate| candidate.result.path.as_str(),
        );
        let fusion_ms = elapsed_ms(fusion_started);

        let mut results = Vec::with_capacity(selected.len());
        let mut source_ranks = Vec::with_capacity(selected.len());
        for (index, (raw_rank, mut candidate)) in selected.into_iter().enumerate() {
            candidate.result.rank = index + 1;
            candidate.result.raw_rank = raw_rank;
            candidate.result.score = round_score(candidate.score);
            results.push(candidate.result);
            source_ranks.push(candidate.source_ranks);
        }

        let response = SearchResponse {
            schema_version: SCHEMA_VERSION,
            engine: HYBRID_ENGINE,
            query: request.query,
            root: fts5_response.root,
            corpus: fts5_response.corpus,
            selection: SearchSelection {
                max_results_per_path: request.max_results_per_path,
            },
            results,
        };
        let timings = SearchTimings {
            lookup_ms: fts5_timings.lookup_ms + embedding_timings.lookup_ms,
            ranking_ms: fts5_timings.ranking_ms + embedding_timings.ranking_ms + fusion_ms,
            excerpt_ms: fts5_timings.excerpt_ms + embedding_timings.excerpt_ms,
            total_ms: elapsed_ms(total_started),
            candidates_examined: fts5_timings.candidates_examined
                + embedding_timings.candidates_examined,
        };
        Ok(HybridSearchOutput {
            response,
            timings,
            source_ranks,
        })
    }

    pub(crate) fn verify_current(&self, corpus: &Corpus) -> Result<()> {
        self.primary.verify_current(corpus)?;
        self.secondary.verify_current(corpus)
    }

    /// FTS5 source index path.
    pub(crate) fn primary_index_path(&self) -> &Path {
        self.primary.index_path()
    }

    /// Embeddings source index path.
    pub(crate) fn secondary_index_path(&self) -> &Path {
        self.secondary.index_path()
    }

    /// Run the four-operation incremental workload on both sources and merge
    /// the per-operation evidence.
    pub(crate) fn run_incremental_workload(
        &self,
        source_root: &Path,
        queries: &[String],
    ) -> Result<Vec<IncrementalStep>> {
        let fts5_steps = run_fts5_incremental_workload(source_root, queries, FTS5_CONFIG_SHA256)?;
        let embeddings_steps =
            run_embeddings_incremental_workload(&self.secondary, source_root, queries)?;
        Ok(merge_incremental_steps(fts5_steps, embeddings_steps))
    }

    /// Both source fault injections must pass for the hybrid to pass.
    pub(crate) fn fault_injection_passed(
        &self,
        root: &Path,
        model_spec: &ModelSpec,
        model_directory: &Path,
    ) -> bool {
        fts5_fault_injection_passed(root, FTS5_CONFIG_SHA256)
            && embeddings_fault_injection_passed(&self.secondary, model_spec, model_directory)
    }
}

/// Resolve both source searches, propagating either error with no partial
/// response. The helper is deliberately pure so error propagation can be tested
/// without loading the embedding model.
pub(crate) fn combine_source_outputs(
    fts5: Result<(SearchResponse, SearchTimings)>,
    embedding: Result<(SearchResponse, SearchTimings)>,
) -> Result<(
    (SearchResponse, SearchTimings),
    (SearchResponse, SearchTimings),
)> {
    Ok((fts5?, embedding?))
}

/// Both sources must describe the exact same corpus/root before fusing.
pub(crate) fn ensure_matching_corpus(
    fts5: &SearchResponse,
    embedding: &SearchResponse,
) -> Result<()> {
    if fts5.root != embedding.root {
        bail!("hybrid sources disagree on the corpus root");
    }
    if fts5.corpus.files != embedding.corpus.files || fts5.corpus.chunks != embedding.corpus.chunks
    {
        bail!("hybrid sources disagree on the corpus summary");
    }
    Ok(())
}

/// Reciprocal-rank contribution for a one-based source rank.
pub(crate) fn ensure_source_depth(fts5: &SearchResponse, embedding: &SearchResponse) -> Result<()> {
    for (name, response) in [(FTS5_ENGINE, fts5), (EMBEDDINGS_ENGINE, embedding)] {
        if response.results.len() > SOURCE_CANDIDATE_DEPTH
            || response
                .results
                .iter()
                .any(|result| !(1..=SOURCE_CANDIDATE_DEPTH).contains(&result.raw_rank))
        {
            bail!("hybrid source {name} exceeded the frozen candidate depth");
        }
    }
    Ok(())
}

pub(crate) fn reciprocal_rank(rank: usize) -> f64 {
    1.0 / (RRF_K as f64 + rank as f64)
}

/// Deduplicate each source list by chunk hash, keeping the first one-based
/// rank, and fuse contributions per source.
pub(crate) fn fuse_source_lists(
    fts5: &[SearchResult],
    embedding: &[SearchResult],
) -> Vec<FusedCandidate> {
    let mut accumulators: BTreeMap<String, Accumulator> = BTreeMap::new();
    add_source(&mut accumulators, fts5, Source::Fts5);
    add_source(&mut accumulators, embedding, Source::Embedding);
    accumulators
        .into_values()
        .map(Accumulator::into_candidate)
        .collect()
}

fn add_source(
    accumulators: &mut BTreeMap<String, Accumulator>,
    results: &[SearchResult],
    source: Source,
) {
    let mut seen: HashSet<&str> = HashSet::new();
    for result in results {
        let rank = result.raw_rank;
        let first_for_source = seen.insert(result.chunk_hash.as_str());
        let accumulator = accumulators
            .entry(result.chunk_hash.clone())
            .or_insert_with(|| Accumulator::new(result));
        if is_canonical(result, &accumulator.representative) {
            accumulator.representative = result.clone();
        }
        for term in &result.matched_terms {
            accumulator.matched_terms.insert(term.clone());
        }
        if first_for_source {
            accumulator.score += reciprocal_rank(rank);
            match source {
                Source::Fts5 => accumulator.fts5_rank = Some(rank),
                Source::Embedding => accumulator.embedding_rank = Some(rank),
            }
        }
    }
}

/// Canonical representative ordering: path asc, line_start asc, line_end asc,
/// then file_hash asc.
fn is_canonical(candidate: &SearchResult, current: &SearchResult) -> bool {
    (
        candidate.path.as_str(),
        candidate.line_start,
        candidate.line_end,
        candidate.file_hash.as_str(),
    ) < (
        current.path.as_str(),
        current.line_start,
        current.line_end,
        current.file_hash.as_str(),
    )
}

/// Sort fused candidates deterministically before diversity selection.
pub(crate) fn order_candidates(mut candidates: Vec<FusedCandidate>) -> Vec<FusedCandidate> {
    candidates.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.result.path.cmp(&right.result.path))
            .then_with(|| left.result.line_start.cmp(&right.result.line_start))
            .then_with(|| left.result.line_end.cmp(&right.result.line_end))
            .then_with(|| left.result.file_hash.cmp(&right.result.file_hash))
            .then_with(|| left.result.chunk_hash.cmp(&right.result.chunk_hash))
    });
    candidates
}

/// Merge two sets of indexing metrics. Full build elapsed is summed, runtime
/// check names are distinct so they are unioned, and the boolean/corruption
/// flags combine conservatively.
pub(crate) fn merge_indexing_metrics(
    primary: IndexingMetrics,
    secondary: IndexingMetrics,
) -> IndexingMetrics {
    let full_build_ms = match (primary.full_build_ms, secondary.full_build_ms) {
        (Some(primary), Some(secondary)) => Some(primary + secondary),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    };
    let mut runtime_checks = primary.runtime_checks;
    runtime_checks.extend(secondary.runtime_checks);
    IndexingMetrics {
        full_build_ms,
        incremental_steps: merge_incremental_steps(
            primary.incremental_steps,
            secondary.incremental_steps,
        ),
        rebuild_succeeded: primary.rebuild_succeeded && secondary.rebuild_succeeded,
        corruption_detected: primary.corruption_detected || secondary.corruption_detected,
        runtime_checks,
    }
}

const OPERATION_ORDER: [&str; 4] = ["add", "modify", "rename", "remove"];

/// Merge incremental steps operation-wise: elapsed sums, equivalence ANDs and
/// stale counts sum.
pub(crate) fn merge_incremental_steps(
    primary: Vec<IncrementalStep>,
    secondary: Vec<IncrementalStep>,
) -> Vec<IncrementalStep> {
    let mut merged: BTreeMap<String, IncrementalStep> = BTreeMap::new();
    for step in primary.into_iter().chain(secondary) {
        match merged.entry(step.operation.clone()) {
            Entry::Vacant(slot) => {
                slot.insert(step);
            }
            Entry::Occupied(mut slot) => {
                let existing = slot.get_mut();
                existing.elapsed_ms += step.elapsed_ms;
                existing.equivalent_to_full_rebuild &= step.equivalent_to_full_rebuild;
                existing.stale_results += step.stale_results;
            }
        }
    }
    let mut result = Vec::with_capacity(merged.len());
    for operation in OPERATION_ORDER {
        if let Some(step) = merged.remove(operation) {
            result.push(step);
        }
    }
    result.extend(merged.into_values());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CorpusSummary;
    use anyhow::anyhow;
    use tempfile::tempdir;

    fn result(
        path: &str,
        line_start: usize,
        line_end: usize,
        chunk_hash: &str,
        file_hash: &str,
        matched_terms: &[&str],
    ) -> SearchResult {
        SearchResult {
            rank: 1,
            raw_rank: 1,
            path: path.to_owned(),
            heading: None,
            line_start,
            line_end,
            excerpt: format!("excerpt for {chunk_hash}"),
            file_hash: file_hash.to_owned(),
            chunk_hash: chunk_hash.to_owned(),
            score: 0.0,
            matched_terms: matched_terms
                .iter()
                .map(|term| (*term).to_owned())
                .collect(),
        }
    }

    fn simple(path: &str, line: usize, chunk_hash: &str) -> SearchResult {
        result(path, line, line + 1, chunk_hash, &"f".repeat(64), &[])
    }

    fn at_rank(mut result: SearchResult, rank: usize) -> SearchResult {
        result.rank = rank;
        result.raw_rank = rank;
        result
    }

    fn response_with(results: Vec<SearchResult>) -> SearchResponse {
        SearchResponse {
            schema_version: SCHEMA_VERSION,
            engine: HYBRID_ENGINE,
            query: "query".to_owned(),
            root: "/tmp/hybrid-root".to_owned(),
            corpus: CorpusSummary {
                files: 1,
                chunks: results.len(),
            },
            selection: SearchSelection {
                max_results_per_path: None,
            },
            results,
        }
    }

    fn timings() -> SearchTimings {
        SearchTimings {
            lookup_ms: 0.0,
            ranking_ms: 0.0,
            excerpt_ms: 0.0,
            total_ms: 0.0,
            candidates_examined: 0,
        }
    }

    #[test]
    fn frozen_configuration_matches_the_published_schema() {
        let schema: Value = serde_json::from_str(include_str!(
            "../evaluation/engine-bakeoff-protocol.schema.json"
        ))
        .expect("published bake-off protocol schema should be valid JSON");
        let expected = &schema["$defs"]["hybridConfig"]["properties"];
        let ours = frozen_configuration();

        for field in [
            "sources",
            "source_candidate_depth",
            "fusion",
            "rrf_k",
            "deduplication",
            "ranking",
            "abstention",
        ] {
            assert_eq!(
                ours[field], expected[field]["const"],
                "hybridConfig.{field} must match the frozen schema"
            );
        }
    }

    #[test]
    fn frozen_configuration_is_exact() {
        assert_eq!(
            frozen_configuration(),
            json!({
                "sources": ["fts5-v1", "local-embeddings-v1"],
                "source_candidate_depth": 50,
                "fusion": "reciprocal-rank-fusion",
                "rrf_k": 60,
                "deduplication": "chunk-hash",
                "ranking": "sum(1/(60+one-based-source-rank))-descending-then-path-asc-line-start-asc",
                "abstention": "return-empty-only-when-fts5-has-no-row-and-top-embedding-dot-product-is-less-than-0.80"
            })
        );
        assert_eq!(SOURCE_CANDIDATE_DEPTH, 50);
        assert_eq!(RRF_K, 60);
        assert_eq!(HYBRID_ENGINE, "hybrid-rrf-v1");
    }

    #[test]
    fn reciprocal_rank_uses_the_frozen_k() {
        assert_eq!(reciprocal_rank(1), 1.0 / 61.0);
        assert_eq!(reciprocal_rank(50), 1.0 / 110.0);
        assert!(reciprocal_rank(1) > reciprocal_rank(2));
    }

    #[test]
    fn source_depth_is_enforced_before_fusion() {
        let fts5 = response_with(vec![simple("a.md", 1, "hash-a")]);
        let embedding = response_with(vec![]);
        ensure_source_depth(&fts5, &embedding).unwrap();

        let mut invalid_rank = response_with(vec![simple("a.md", 1, "hash-a")]);
        invalid_rank.results[0].raw_rank = SOURCE_CANDIDATE_DEPTH + 1;
        assert!(ensure_source_depth(&invalid_rank, &embedding).is_err());

        let too_many = response_with(
            (1..=SOURCE_CANDIDATE_DEPTH + 1)
                .map(|rank| at_rank(simple("a.md", rank, &format!("hash-{rank}")), rank))
                .collect(),
        );
        assert!(ensure_source_depth(&too_many, &embedding).is_err());
    }

    #[test]
    fn fusion_sums_one_contribution_per_source() {
        let fts5 = vec![simple("a.md", 1, "hash-a")];
        let embedding = vec![simple("a.md", 1, "hash-a")];
        let fused = fuse_source_lists(&fts5, &embedding);
        assert_eq!(fused.len(), 1);
        assert_eq!(fused[0].score, reciprocal_rank(1) + reciprocal_rank(1));
        assert_eq!(
            fused[0].source_ranks,
            HybridSourceRanks {
                fts5: Some(1),
                embedding: Some(1),
            }
        );

        let fts5 = vec![
            simple("x.md", 1, "hash-x"),
            at_rank(simple("a.md", 1, "hash-a"), 2),
        ];
        let embedding = vec![simple("a.md", 1, "hash-a")];
        let fused = fuse_source_lists(&fts5, &embedding);
        let target = fused
            .iter()
            .find(|candidate| candidate.result.chunk_hash == "hash-a")
            .unwrap();
        assert_eq!(target.score, reciprocal_rank(2) + reciprocal_rank(1));
        assert_eq!(
            target.source_ranks,
            HybridSourceRanks {
                fts5: Some(2),
                embedding: Some(1),
            }
        );
    }

    #[test]
    fn fusion_keeps_source_only_hashes() {
        let fts5 = vec![simple("a.md", 1, "hash-a")];
        let embedding = vec![simple("b.md", 1, "hash-b")];
        let fused = fuse_source_lists(&fts5, &embedding);
        assert_eq!(fused.len(), 2);

        let a = fused
            .iter()
            .find(|candidate| candidate.result.chunk_hash == "hash-a")
            .unwrap();
        assert_eq!(a.score, reciprocal_rank(1));
        assert_eq!(
            a.source_ranks,
            HybridSourceRanks {
                fts5: Some(1),
                embedding: None,
            }
        );

        let b = fused
            .iter()
            .find(|candidate| candidate.result.chunk_hash == "hash-b")
            .unwrap();
        assert_eq!(b.score, reciprocal_rank(1));
        assert_eq!(
            b.source_ranks,
            HybridSourceRanks {
                fts5: None,
                embedding: Some(1),
            }
        );
    }

    #[test]
    fn fusion_dedups_repeated_hashes_within_a_source() {
        let fts5 = vec![simple("a.md", 1, "hash-a"), simple("a.md", 9, "hash-a")];
        let embedding = vec![];
        let fused = fuse_source_lists(&fts5, &embedding);
        assert_eq!(fused.len(), 1);
        assert_eq!(fused[0].score, reciprocal_rank(1));
        assert_eq!(
            fused[0].source_ranks,
            HybridSourceRanks {
                fts5: Some(1),
                embedding: None,
            }
        );
        assert_eq!(fused[0].result.line_start, 1);

        let fts5 = vec![simple("z.md", 1, "hash-a"), simple("a.md", 9, "hash-a")];
        let fused = fuse_source_lists(&fts5, &[]);
        assert_eq!(fused[0].score, reciprocal_rank(1));
        assert_eq!(fused[0].result.path, "a.md");
    }

    #[test]
    fn fusion_chooses_the_canonical_duplicate_evidence() {
        // Different paths: the lexicographically smallest path wins.
        let fts5 = vec![result("z.md", 10, 11, "hash", &"f".repeat(64), &[])];
        let embedding = vec![result("a.md", 20, 21, "hash", &"e".repeat(64), &[])];
        let fused = fuse_source_lists(&fts5, &embedding);
        assert_eq!(fused.len(), 1);
        assert_eq!(fused[0].result.path, "a.md");
        assert_eq!(fused[0].result.line_start, 20);

        // Same path: smaller line_start wins, then line_end, then file_hash.
        let fts5 = vec![result("a.md", 10, 12, "hash", &"f".repeat(64), &[])];
        let embedding = vec![result("a.md", 10, 15, "hash", &"0".repeat(64), &[])];
        let fused = fuse_source_lists(&fts5, &embedding);
        assert_eq!(fused[0].result.line_end, 12);

        let fts5 = vec![result("a.md", 10, 12, "hash", &"f".repeat(64), &[])];
        let embedding = vec![result("a.md", 10, 12, "hash", &"0".repeat(64), &[])];
        let fused = fuse_source_lists(&fts5, &embedding);
        assert_eq!(fused[0].result.file_hash, "0".repeat(64));
    }

    #[test]
    fn fusion_unions_matched_terms_sorted_and_deduplicated() {
        let fts5 = vec![result(
            "a.md",
            1,
            2,
            "hash",
            &"f".repeat(64),
            &["beta", "alpha"],
        )];
        let embedding = vec![result(
            "a.md",
            1,
            2,
            "hash",
            &"f".repeat(64),
            &["alpha", "gamma"],
        )];
        let fused = fuse_source_lists(&fts5, &embedding);
        assert_eq!(fused.len(), 1);
        assert_eq!(
            fused[0].result.matched_terms,
            vec!["alpha".to_owned(), "beta".to_owned(), "gamma".to_owned()]
        );
    }

    #[test]
    fn fused_order_breaks_ties_by_path_then_line() {
        let fts5 = vec![simple("b.md", 1, "hash-b")];
        let embedding = vec![simple("a.md", 1, "hash-a")];
        let ordered = order_candidates(fuse_source_lists(&fts5, &embedding));
        let paths: Vec<_> = ordered
            .iter()
            .map(|candidate| candidate.result.path.clone())
            .collect();
        assert_eq!(paths, vec!["a.md".to_owned(), "b.md".to_owned()]);

        let fts5 = vec![simple("a.md", 9, "hash-b")];
        let embedding = vec![simple("a.md", 2, "hash-a")];
        let ordered = order_candidates(fuse_source_lists(&fts5, &embedding));
        let lines: Vec<_> = ordered
            .iter()
            .map(|candidate| candidate.result.line_start)
            .collect();
        assert_eq!(lines, vec![2, 9]);
    }

    #[test]
    fn source_ranks_record_first_one_based_position() {
        let fts5 = vec![
            simple("x.md", 1, "hash-x"),
            at_rank(simple("a.md", 1, "hash-a"), 2),
            at_rank(simple("a.md", 5, "hash-a"), 3),
        ];
        let embedding = vec![
            simple("a.md", 1, "hash-a"),
            at_rank(simple("x.md", 1, "hash-x"), 2),
        ];
        let fused = fuse_source_lists(&fts5, &embedding);
        let a = fused
            .iter()
            .find(|candidate| candidate.result.chunk_hash == "hash-a")
            .unwrap();
        assert_eq!(
            a.source_ranks,
            HybridSourceRanks {
                fts5: Some(2),
                embedding: Some(1),
            }
        );
        let x = fused
            .iter()
            .find(|candidate| candidate.result.chunk_hash == "hash-x")
            .unwrap();
        assert_eq!(
            x.source_ranks,
            HybridSourceRanks {
                fts5: Some(1),
                embedding: Some(2),
            }
        );
    }

    #[test]
    fn abstention_is_natural_when_both_sources_are_empty() {
        assert!(fuse_source_lists(&[], &[]).is_empty());
        assert!(order_candidates(fuse_source_lists(&[], &[])).is_empty());

        let fts5 = vec![simple("a.md", 1, "hash-a")];
        assert_eq!(fuse_source_lists(&fts5, &[]).len(), 1);
        let embedding = vec![simple("b.md", 1, "hash-b")];
        assert_eq!(fuse_source_lists(&[], &embedding).len(), 1);
    }

    #[test]
    fn embedding_error_propagates_even_with_fts_results() {
        let fts5 = Ok((response_with(vec![simple("a.md", 1, "hash-a")]), timings()));
        let embedding: Result<(SearchResponse, SearchTimings)> =
            Err(anyhow!("embedding source exploded"));
        let error =
            combine_source_outputs(fts5, embedding).expect_err("embedding error must propagate");
        assert!(error.to_string().contains("embedding source exploded"));

        let fts5: Result<(SearchResponse, SearchTimings)> = Err(anyhow!("fts5 source exploded"));
        let embedding = Ok((response_with(vec![simple("a.md", 1, "hash-a")]), timings()));
        assert!(combine_source_outputs(fts5, embedding).is_err());

        let fts5 = Ok((response_with(vec![simple("a.md", 1, "hash-a")]), timings()));
        let embedding = Ok((response_with(vec![simple("a.md", 1, "hash-a")]), timings()));
        assert!(combine_source_outputs(fts5, embedding).is_ok());
    }

    #[test]
    fn missing_model_fails_before_creating_the_fts5_index() {
        let root = tempdir().unwrap();
        std::fs::write(root.path().join("README.md"), "# Synthetic\n\nEvidence.\n").unwrap();
        let corpus = crate::corpus::load(root.path()).unwrap();
        let index_path = crate::fts5::index_path_for_root(root.path()).unwrap();
        assert!(!index_path.exists());
        let missing_model = root.path().join("missing-model");
        let spec = ModelSpec {
            id: "intfloat/multilingual-e5-small".to_owned(),
            revision: "0".repeat(40),
            license: "mit".to_owned(),
            dimension: 384,
            max_tokens: 512,
            query_prefix: "query: ".to_owned(),
            passage_prefix: "passage: ".to_owned(),
            pooling: "attention-mask-mean".to_owned(),
            normalization: "l2-epsilon-1e-12".to_owned(),
            similarity: "dot-product".to_owned(),
            dtype: "f32".to_owned(),
            runtime: "candle-cpu-0.9.1".to_owned(),
            tokenizers: "0.21.1".to_owned(),
            artifacts: Vec::new(),
            artifacts_sha256_file: "SHA256SUMS".to_owned(),
        };
        assert!(HybridIndex::prepare_fresh(root.path(), &corpus, &spec, &missing_model).is_err());
        assert!(!index_path.exists());
    }

    #[test]
    fn corpus_mismatch_is_rejected_before_fusing() {
        let fts5 = response_with(vec![simple("a.md", 1, "hash-a")]);
        let mut embedding = response_with(vec![simple("a.md", 1, "hash-a")]);
        embedding.corpus.chunks += 1;
        assert!(ensure_matching_corpus(&fts5, &embedding).is_err());

        let mut other_root = response_with(vec![]);
        other_root.root = "/tmp/other-root".to_owned();
        assert!(ensure_matching_corpus(&fts5, &other_root).is_err());
        assert!(ensure_matching_corpus(&fts5, &fts5).is_ok());
    }

    fn step(operation: &str, elapsed_ms: f64, equivalent: bool, stale: usize) -> IncrementalStep {
        IncrementalStep {
            operation: operation.to_owned(),
            elapsed_ms,
            equivalent_to_full_rebuild: equivalent,
            stale_results: stale,
        }
    }

    #[test]
    fn merge_incremental_steps_is_operation_wise() {
        let primary = vec![
            step("add", 1.0, true, 0),
            step("modify", 2.0, true, 0),
            step("rename", 3.0, false, 1),
            step("remove", 4.0, true, 0),
        ];
        let secondary = vec![
            step("add", 10.0, false, 2),
            step("modify", 20.0, true, 0),
            step("rename", 30.0, true, 3),
            step("remove", 40.0, true, 0),
        ];
        let merged = merge_incremental_steps(primary, secondary);
        assert_eq!(merged.len(), 4);
        let operations: Vec<_> = merged.iter().map(|step| step.operation.as_str()).collect();
        assert_eq!(operations, vec!["add", "modify", "rename", "remove"]);
        assert_eq!(merged[0].elapsed_ms, 11.0);
        assert!(!merged[0].equivalent_to_full_rebuild);
        assert_eq!(merged[0].stale_results, 2);
        assert_eq!(merged[1].elapsed_ms, 22.0);
        assert_eq!(merged[2].elapsed_ms, 33.0);
        assert!(!merged[2].equivalent_to_full_rebuild);
        assert_eq!(merged[2].stale_results, 4);
        assert_eq!(merged[3].elapsed_ms, 44.0);
        assert!(merged[3].equivalent_to_full_rebuild);
    }

    #[test]
    fn merge_indexing_metrics_combines_phases_and_checks() {
        let primary = IndexingMetrics {
            full_build_ms: Some(100.0),
            incremental_steps: vec![step("add", 1.0, true, 0)],
            rebuild_succeeded: true,
            corruption_detected: false,
            runtime_checks: BTreeMap::from([("sqlite".to_owned(), true)]),
        };
        let secondary = IndexingMetrics {
            full_build_ms: Some(50.0),
            incremental_steps: vec![step("add", 2.0, false, 1)],
            rebuild_succeeded: false,
            corruption_detected: true,
            runtime_checks: BTreeMap::from([("model".to_owned(), true)]),
        };
        let merged = merge_indexing_metrics(primary, secondary);
        assert_eq!(merged.full_build_ms, Some(150.0));
        assert!(!merged.rebuild_succeeded);
        assert!(merged.corruption_detected);
        assert_eq!(merged.incremental_steps.len(), 1);
        assert_eq!(merged.incremental_steps[0].elapsed_ms, 3.0);
        assert!(!merged.incremental_steps[0].equivalent_to_full_rebuild);
        assert_eq!(merged.incremental_steps[0].stale_results, 1);
        assert_eq!(
            merged.runtime_checks,
            BTreeMap::from([("model".to_owned(), true), ("sqlite".to_owned(), true),])
        );
    }

    #[test]
    fn empty_full_build_elapsed_stays_none() {
        let metrics = IndexingMetrics::direct();
        let merged = merge_indexing_metrics(metrics.clone(), metrics);
        assert_eq!(merged.full_build_ms, None);
    }
}
