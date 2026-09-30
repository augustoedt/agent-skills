use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::corpus;
use crate::evaluation::{EvaluationCategory, EvaluationQuery, parse_evaluation_set};
use crate::search::search;
use crate::types::{ENGINE, SearchRequest, SearchResult};

pub const EVALUATION_REPORT_SCHEMA_VERSION: u32 = 2;
pub(crate) const RECALL_CUTOFF: usize = 5;

#[derive(Debug, Clone)]
pub struct EvaluateRequest {
    pub root: PathBuf,
    pub queries_path: PathBuf,
    pub limit: usize,
    pub max_excerpt_chars: usize,
    pub max_results_per_path: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvaluationReport {
    pub schema_version: u32,
    pub tool_version: &'static str,
    pub engine: &'static str,
    pub queries_schema_version: u32,
    pub dataset_hash: String,
    pub corpus: EvaluationCorpus,
    pub config: EvaluationConfig,
    pub summary: EvaluationSummary,
    pub per_category: BTreeMap<String, EvaluationSummary>,
    pub queries: Vec<QueryEvaluation>,
}

impl EvaluationReport {
    pub fn has_failures(&self) -> bool {
        self.summary.failed > 0
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EvaluationCorpus {
    pub name: String,
    pub root: String,
    pub files: usize,
    pub chunks: usize,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvaluationConfig {
    pub limit: usize,
    pub max_excerpt_chars: usize,
    pub max_results_per_path: Option<usize>,
    pub recall_cutoff: usize,
    pub execution_model: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvaluationSummary {
    pub queries_total: usize,
    pub executed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub answerable: usize,
    pub no_answer: usize,
    pub hit_at_1: Option<f64>,
    pub recall_at_5_macro: Option<f64>,
    pub recall_at_5_micro: Option<f64>,
    pub mrr_at_5: Option<f64>,
    pub no_answer_false_positives: usize,
    pub no_answer_false_positive_rate: Option<f64>,
    pub latency_ms: LatencyStats,
    pub context_chars: ContextStats,
}

#[derive(Debug, Clone, Serialize)]
pub struct LatencyStats {
    pub mean: Option<f64>,
    pub p50: Option<f64>,
    pub p95: Option<f64>,
    pub max: Option<f64>,
    pub total: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextStats {
    pub mean: Option<f64>,
    pub p95: Option<usize>,
    pub max: Option<usize>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryEvaluation {
    pub id: String,
    pub category: EvaluationCategory,
    pub query: String,
    pub status: QueryStatus,
    pub expected_paths: Vec<String>,
    pub expected_headings: BTreeMap<String, Vec<String>>,
    pub results: Vec<EvaluationResult>,
    pub result_count: usize,
    pub first_relevant_rank: Option<usize>,
    pub first_relevant_heading_rank: Option<usize>,
    pub relevant_paths_found: Option<usize>,
    pub hit_at_1: Option<bool>,
    pub recall_at_5: Option<f64>,
    pub reciprocal_rank_at_5: Option<f64>,
    pub no_answer_false_positive: Option<bool>,
    pub latency_ms: Option<f64>,
    pub context_chars: usize,
    pub error: Option<String>,
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryStatus {
    Ok,
    Error,
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvaluationResult {
    pub rank: usize,
    pub raw_rank: usize,
    pub path: String,
    pub heading: Option<String>,
    pub line_start: usize,
    pub line_end: usize,
    pub score: f64,
    pub excerpt_chars: usize,
}

pub fn evaluate(request: EvaluateRequest) -> Result<EvaluationReport> {
    if !(RECALL_CUTOFF..=100).contains(&request.limit) {
        bail!("evaluation limit must be between {RECALL_CUTOFF} and 100 so Recall@5 is defined");
    }
    if request.max_excerpt_chars < 80 {
        bail!("max_excerpt_chars must be at least 80");
    }
    if request
        .max_results_per_path
        .is_some_and(|maximum| maximum == 0 || maximum > 100)
    {
        bail!("max_results_per_path must be between 1 and 100");
    }

    let dataset = fs::read_to_string(&request.queries_path).with_context(|| {
        format!(
            "failed to read evaluation dataset {}",
            request.queries_path.display()
        )
    })?;
    let set = parse_evaluation_set(&dataset).with_context(|| {
        format!(
            "invalid evaluation dataset {}",
            request.queries_path.display()
        )
    })?;
    let loaded = corpus::load(&request.root)?;
    let corpus_fingerprint = corpus_fingerprint(&loaded.file_hashes);
    let root = request.root.to_string_lossy().into_owned();

    let mut queries = Vec::with_capacity(set.queries.len());
    for query in set.queries {
        queries.push(evaluate_query(&request, query));
    }

    let summary = summarize(queries.iter());
    let mut per_category = BTreeMap::new();
    for (name, category) in [
        ("exact", EvaluationCategory::Exact),
        ("semantic", EvaluationCategory::Semantic),
        ("ambiguous", EvaluationCategory::Ambiguous),
        ("no_answer", EvaluationCategory::NoAnswer),
    ] {
        per_category.insert(
            name.to_owned(),
            summarize(queries.iter().filter(|query| query.category == category)),
        );
    }

    Ok(EvaluationReport {
        schema_version: EVALUATION_REPORT_SCHEMA_VERSION,
        tool_version: env!("CARGO_PKG_VERSION"),
        engine: ENGINE,
        queries_schema_version: set.schema_version,
        dataset_hash: blake3::hash(dataset.as_bytes()).to_hex().to_string(),
        corpus: EvaluationCorpus {
            name: set.corpus,
            root,
            files: loaded.files,
            chunks: loaded.chunks.len(),
            fingerprint: corpus_fingerprint,
        },
        config: EvaluationConfig {
            limit: request.limit,
            max_excerpt_chars: request.max_excerpt_chars,
            max_results_per_path: request.max_results_per_path,
            recall_cutoff: RECALL_CUTOFF,
            execution_model: "full_corpus_scan_per_query",
        },
        summary,
        per_category,
        queries,
    })
}

fn evaluate_query(request: &EvaluateRequest, query: EvaluationQuery) -> QueryEvaluation {
    if query.disabled_reason.is_some() {
        return base_query_evaluation(query, QueryStatus::Skipped);
    }

    let started = Instant::now();
    let response = search(SearchRequest {
        root: request.root.clone(),
        query: query.query.clone(),
        limit: request.limit,
        max_excerpt_chars: request.max_excerpt_chars,
        max_results_per_path: request.max_results_per_path,
    });
    let elapsed_ms = duration_ms(started.elapsed().as_secs_f64() * 1_000.0);

    match response {
        Ok(response) => successful_query_evaluation(query, response.results, elapsed_ms),
        Err(error) => {
            let mut evaluation = base_query_evaluation(query, QueryStatus::Error);
            evaluation.latency_ms = Some(elapsed_ms);
            evaluation.error = Some(error.to_string());
            evaluation
        }
    }
}

pub(crate) fn successful_query_evaluation(
    query: EvaluationQuery,
    search_results: Vec<SearchResult>,
    latency_ms: f64,
) -> QueryEvaluation {
    let context_chars = search_results
        .iter()
        .map(|result| result.excerpt.chars().count())
        .sum();
    let results: Vec<EvaluationResult> = search_results
        .iter()
        .map(|result| EvaluationResult {
            rank: result.rank,
            raw_rank: result.raw_rank,
            path: result.path.clone(),
            heading: result.heading.clone(),
            line_start: result.line_start,
            line_end: result.line_end,
            score: result.score,
            excerpt_chars: result.excerpt.chars().count(),
        })
        .collect();

    let mut evaluation = base_query_evaluation(query, QueryStatus::Ok);
    evaluation.latency_ms = Some(latency_ms);
    evaluation.context_chars = context_chars;
    evaluation.result_count = results.len();
    evaluation.results = results;

    if evaluation.category == EvaluationCategory::NoAnswer {
        evaluation.no_answer_false_positive = Some(!search_results.is_empty());
        return evaluation;
    }

    let unique = unique_paths(&search_results);
    let top_five = unique
        .iter()
        .copied()
        .filter(|result| result.rank <= RECALL_CUTOFF)
        .collect::<Vec<_>>();
    let expected: HashSet<_> = evaluation
        .expected_paths
        .iter()
        .map(String::as_str)
        .collect();
    let relevant_paths_found = top_five
        .iter()
        .filter(|result| expected.contains(result.path.as_str()))
        .count();
    let first_relevant_rank = top_five
        .iter()
        .find(|result| expected.contains(result.path.as_str()))
        .map(|result| result.rank);
    let first_relevant_heading_rank = search_results.iter().find_map(|result| {
        result
            .heading
            .as_deref()
            .filter(|heading| {
                evaluation
                    .expected_headings
                    .get(&result.path)
                    .is_some_and(|expected| expected.iter().any(|candidate| candidate == heading))
            })
            .map(|_| result.rank)
    });

    evaluation.first_relevant_rank = first_relevant_rank;
    evaluation.first_relevant_heading_rank = first_relevant_heading_rank;
    evaluation.relevant_paths_found = Some(relevant_paths_found);
    evaluation.hit_at_1 = Some(first_relevant_rank == Some(1));
    evaluation.recall_at_5 = Some(round_six(
        relevant_paths_found as f64 / evaluation.expected_paths.len() as f64,
    ));
    evaluation.reciprocal_rank_at_5 = Some(
        first_relevant_rank
            .map(|rank| round_six(1.0 / rank as f64))
            .unwrap_or(0.0),
    );
    evaluation
}

pub(crate) fn base_query_evaluation(
    query: EvaluationQuery,
    status: QueryStatus,
) -> QueryEvaluation {
    QueryEvaluation {
        id: query.id,
        category: query.category,
        query: query.query,
        status,
        expected_paths: query.expected_paths,
        expected_headings: query.expected_headings,
        results: Vec::new(),
        result_count: 0,
        first_relevant_rank: None,
        first_relevant_heading_rank: None,
        relevant_paths_found: None,
        hit_at_1: None,
        recall_at_5: None,
        reciprocal_rank_at_5: None,
        no_answer_false_positive: None,
        latency_ms: None,
        context_chars: 0,
        error: None,
        disabled_reason: query.disabled_reason,
    }
}

fn unique_paths(results: &[SearchResult]) -> Vec<&SearchResult> {
    let mut seen = HashSet::new();
    results
        .iter()
        .filter(|result| seen.insert(result.path.as_str()))
        .collect()
}

pub(crate) fn corpus_fingerprint(files: &BTreeMap<String, String>) -> String {
    let mut hasher = blake3::Hasher::new();
    for (path, hash) in files {
        hasher.update(path.as_bytes());
        hasher.update(&[0]);
        hasher.update(hash.as_bytes());
        hasher.update(&[0xff]);
    }
    hasher.finalize().to_hex().to_string()
}

pub(crate) fn summarize<'a>(
    queries: impl Iterator<Item = &'a QueryEvaluation>,
) -> EvaluationSummary {
    let queries: Vec<_> = queries.collect();
    let active: Vec<_> = queries
        .iter()
        .copied()
        .filter(|query| query.status != QueryStatus::Skipped)
        .collect();
    let answerable: Vec<_> = active
        .iter()
        .copied()
        .filter(|query| query.category != EvaluationCategory::NoAnswer)
        .collect();
    let no_answer: Vec<_> = active
        .iter()
        .copied()
        .filter(|query| query.category == EvaluationCategory::NoAnswer)
        .collect();

    let answerable_complete = answerable
        .iter()
        .all(|query| query.status == QueryStatus::Ok);
    let answerable_metrics_available = answerable_complete && !answerable.is_empty();
    let no_answer_complete = no_answer
        .iter()
        .all(|query| query.status == QueryStatus::Ok);
    let hit_at_1 = answerable_metrics_available.then(|| {
        mean(
            answerable
                .iter()
                .filter_map(|query| query.hit_at_1.map(bool_to_number)),
        )
        .unwrap_or(0.0)
    });
    let recall_at_5_macro = answerable_metrics_available
        .then(|| mean(answerable.iter().filter_map(|query| query.recall_at_5)).unwrap_or(0.0));
    let expected_total: usize = answerable
        .iter()
        .map(|query| query.expected_paths.len())
        .sum();
    let found_total: usize = answerable
        .iter()
        .filter_map(|query| query.relevant_paths_found)
        .sum();
    let recall_at_5_micro = (answerable_metrics_available && expected_total > 0)
        .then(|| round_six(found_total as f64 / expected_total as f64));
    let mrr_at_5 = answerable_metrics_available.then(|| {
        mean(
            answerable
                .iter()
                .filter_map(|query| query.reciprocal_rank_at_5),
        )
        .unwrap_or(0.0)
    });
    let no_answer_false_positives = no_answer
        .iter()
        .filter(|query| query.no_answer_false_positive == Some(true))
        .count();
    let no_answer_false_positive_rate = (no_answer_complete && !no_answer.is_empty())
        .then(|| round_six(no_answer_false_positives as f64 / no_answer.len() as f64));
    let latencies: Vec<_> = active.iter().filter_map(|query| query.latency_ms).collect();
    let contexts: Vec<_> = active
        .iter()
        .filter(|query| query.status == QueryStatus::Ok)
        .map(|query| query.context_chars)
        .collect();

    EvaluationSummary {
        queries_total: queries.len(),
        executed: active
            .iter()
            .filter(|query| query.status == QueryStatus::Ok)
            .count(),
        skipped: queries
            .iter()
            .filter(|query| query.status == QueryStatus::Skipped)
            .count(),
        failed: active
            .iter()
            .filter(|query| query.status == QueryStatus::Error)
            .count(),
        answerable: answerable.len(),
        no_answer: no_answer.len(),
        hit_at_1,
        recall_at_5_macro,
        recall_at_5_micro,
        mrr_at_5,
        no_answer_false_positives,
        no_answer_false_positive_rate,
        latency_ms: latency_stats(&latencies),
        context_chars: context_stats(&contexts),
    }
}

pub(crate) fn latency_stats(values: &[f64]) -> LatencyStats {
    LatencyStats {
        mean: mean(values.iter().copied()),
        p50: percentile_f64(values, 0.50),
        p95: percentile_f64(values, 0.95),
        max: values.iter().copied().reduce(f64::max).map(duration_ms),
        total: duration_ms(values.iter().sum()),
    }
}

pub(crate) fn context_stats(values: &[usize]) -> ContextStats {
    ContextStats {
        mean: (!values.is_empty())
            .then(|| round_six(values.iter().sum::<usize>() as f64 / values.len() as f64)),
        p95: percentile_usize(values, 0.95),
        max: values.iter().copied().max(),
        total: values.iter().sum(),
    }
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let values: Vec<_> = values.collect();
    (!values.is_empty()).then(|| round_six(values.iter().sum::<f64>() / values.len() as f64))
}

fn percentile_f64(values: &[f64], percentile: f64) -> Option<f64> {
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    percentile_index(values.len(), percentile).map(|index| duration_ms(values[index]))
}

fn percentile_usize(values: &[usize], percentile: f64) -> Option<usize> {
    let mut values = values.to_vec();
    values.sort_unstable();
    percentile_index(values.len(), percentile).map(|index| values[index])
}

fn percentile_index(length: usize, percentile: f64) -> Option<usize> {
    if length == 0 {
        return None;
    }
    Some(((length as f64 * percentile).ceil() as usize).saturating_sub(1))
}

fn bool_to_number(value: bool) -> f64 {
    if value { 1.0 } else { 0.0 }
}

pub(crate) fn round_six(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn duration_ms(value: f64) -> f64 {
    (value * 1_000.0).round() / 1_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(
        id: &str,
        category: EvaluationCategory,
        expected: &[&str],
        found: usize,
        first_rank: Option<usize>,
        context_chars: usize,
    ) -> QueryEvaluation {
        QueryEvaluation {
            id: id.to_owned(),
            category,
            query: id.to_owned(),
            status: QueryStatus::Ok,
            expected_paths: expected.iter().map(|value| (*value).to_owned()).collect(),
            expected_headings: BTreeMap::new(),
            results: Vec::new(),
            result_count: 0,
            first_relevant_rank: first_rank,
            first_relevant_heading_rank: None,
            relevant_paths_found: (category != EvaluationCategory::NoAnswer).then_some(found),
            hit_at_1: (category != EvaluationCategory::NoAnswer).then_some(first_rank == Some(1)),
            recall_at_5: (category != EvaluationCategory::NoAnswer)
                .then(|| found as f64 / expected.len() as f64),
            reciprocal_rank_at_5: (category != EvaluationCategory::NoAnswer)
                .then(|| first_rank.map(|rank| 1.0 / rank as f64).unwrap_or(0.0)),
            no_answer_false_positive: (category == EvaluationCategory::NoAnswer).then_some(false),
            latency_ms: Some(10.0),
            context_chars,
            error: None,
            disabled_reason: None,
        }
    }

    #[test]
    fn summary_calculates_path_metrics_and_context() {
        let queries = [
            query("one", EvaluationCategory::Exact, &["a.md"], 1, Some(1), 100),
            query(
                "two",
                EvaluationCategory::Semantic,
                &["a.md", "b.md"],
                1,
                Some(2),
                300,
            ),
            query("none", EvaluationCategory::NoAnswer, &[], 0, None, 0),
        ];

        let summary = summarize(queries.iter());
        assert_eq!(summary.hit_at_1, Some(0.5));
        assert_eq!(summary.recall_at_5_macro, Some(0.75));
        assert_eq!(summary.recall_at_5_micro, Some(0.666667));
        assert_eq!(summary.mrr_at_5, Some(0.75));
        assert_eq!(summary.no_answer_false_positive_rate, Some(0.0));
        assert_eq!(summary.context_chars.total, 400);
        assert_eq!(summary.context_chars.p95, Some(300));
    }

    #[test]
    fn path_metrics_deduplicate_chunks_and_preserve_best_rank() {
        let evaluation = successful_query_evaluation(
            EvaluationQuery {
                id: "duplicate-paths".to_owned(),
                category: EvaluationCategory::Exact,
                query: "target".to_owned(),
                expected_paths: vec!["target.md".to_owned()],
                expected_headings: BTreeMap::from([(
                    "target.md".to_owned(),
                    vec!["Target".to_owned()],
                )]),
                notes: None,
                tags: Vec::new(),
                disabled_reason: None,
            },
            vec![
                result(1, "distractor.md", Some("One")),
                result(2, "distractor.md", Some("Two")),
                result(3, "target.md", Some("Target")),
            ],
            1.0,
        );

        assert_eq!(evaluation.first_relevant_rank, Some(3));
        assert_eq!(evaluation.first_relevant_heading_rank, Some(3));
        assert_eq!(evaluation.hit_at_1, Some(false));
        assert_eq!(evaluation.reciprocal_rank_at_5, Some(0.333333));
    }

    #[test]
    fn diversified_metrics_use_selected_rank_and_preserve_raw_rank() {
        let mut relevant = result(2, "target.md", Some("Target"));
        relevant.raw_rank = 7;
        let evaluation = successful_query_evaluation(
            EvaluationQuery {
                id: "diversified-rank".to_owned(),
                category: EvaluationCategory::Exact,
                query: "target".to_owned(),
                expected_paths: vec!["target.md".to_owned()],
                expected_headings: BTreeMap::new(),
                notes: None,
                tags: Vec::new(),
                disabled_reason: None,
            },
            vec![result(1, "distractor.md", Some("Distractor")), relevant],
            1.0,
        );

        assert_eq!(evaluation.first_relevant_rank, Some(2));
        assert_eq!(evaluation.reciprocal_rank_at_5, Some(0.5));
        assert_eq!(evaluation.results[1].raw_rank, 7);
    }

    #[test]
    fn no_answer_results_are_counted_as_false_positives() {
        let evaluation = successful_query_evaluation(
            EvaluationQuery {
                id: "no-answer".to_owned(),
                category: EvaluationCategory::NoAnswer,
                query: "missing".to_owned(),
                expected_paths: Vec::new(),
                expected_headings: BTreeMap::new(),
                notes: None,
                tags: Vec::new(),
                disabled_reason: None,
            },
            vec![result(1, "distractor.md", Some("Distractor"))],
            1.0,
        );
        let summary = summarize(std::iter::once(&evaluation));

        assert_eq!(evaluation.no_answer_false_positive, Some(true));
        assert_eq!(summary.hit_at_1, None);
        assert_eq!(summary.no_answer_false_positives, 1);
        assert_eq!(summary.no_answer_false_positive_rate, Some(1.0));
    }

    #[test]
    fn failed_queries_make_aggregate_quality_metrics_incomplete() {
        let mut answerable = query(
            "answerable-error",
            EvaluationCategory::Exact,
            &["a.md"],
            0,
            None,
            100,
        );
        answerable.status = QueryStatus::Error;
        answerable.hit_at_1 = None;
        answerable.recall_at_5 = None;
        answerable.reciprocal_rank_at_5 = None;
        answerable.relevant_paths_found = None;

        let mut no_answer = query(
            "no-answer-error",
            EvaluationCategory::NoAnswer,
            &[],
            0,
            None,
            200,
        );
        no_answer.status = QueryStatus::Error;
        no_answer.no_answer_false_positive = None;

        let queries = [answerable, no_answer];
        let summary = summarize(queries.iter());
        assert_eq!(summary.failed, 2);
        assert_eq!(summary.hit_at_1, None);
        assert_eq!(summary.recall_at_5_macro, None);
        assert_eq!(summary.recall_at_5_micro, None);
        assert_eq!(summary.mrr_at_5, None);
        assert_eq!(summary.no_answer_false_positive_rate, None);
        assert_eq!(summary.context_chars.total, 0);
    }

    #[test]
    fn percentiles_use_nearest_rank_and_handle_empty_inputs() {
        assert_eq!(percentile_usize(&[], 0.95), None);
        assert_eq!(percentile_usize(&[1, 2, 3, 4], 0.50), Some(2));
        assert_eq!(percentile_usize(&[1, 2, 3, 4], 0.95), Some(4));
        assert_eq!(percentile_f64(&[1.0, 2.0, 3.0], 0.50), Some(2.0));
    }

    fn result(rank: usize, path: &str, heading: Option<&str>) -> SearchResult {
        SearchResult {
            rank,
            raw_rank: rank,
            path: path.to_owned(),
            heading: heading.map(str::to_owned),
            line_start: 1,
            line_end: 1,
            excerpt: "text".to_owned(),
            file_hash: "f".repeat(64),
            chunk_hash: "c".repeat(64),
            score: 1.0,
            matched_terms: vec!["text".to_owned()],
        }
    }
}
