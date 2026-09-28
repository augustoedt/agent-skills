use std::collections::{BTreeMap, HashSet};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

pub const EVALUATION_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationSet {
    pub schema_version: u32,
    pub corpus: String,
    pub queries: Vec<EvaluationQuery>,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationCategory {
    Exact,
    Semantic,
    Ambiguous,
    NoAnswer,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationQuery {
    pub id: String,
    pub category: EvaluationCategory,
    pub query: String,
    pub expected_paths: Vec<String>,
    #[serde(default)]
    pub expected_headings: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub disabled_reason: Option<String>,
}

pub fn parse_evaluation_set(input: &str) -> Result<EvaluationSet> {
    let set: EvaluationSet = serde_json::from_str(input)?;
    validate_evaluation_set(&set)?;
    Ok(set)
}

pub fn validate_evaluation_set(set: &EvaluationSet) -> Result<()> {
    if !matches!(set.schema_version, 1 | EVALUATION_SCHEMA_VERSION) {
        bail!(
            "unsupported evaluation schema_version {}; expected 1 or {}",
            set.schema_version,
            EVALUATION_SCHEMA_VERSION
        );
    }
    if set.corpus.trim().is_empty() {
        bail!("evaluation corpus must not be empty");
    }
    if set.queries.is_empty() {
        bail!("evaluation set must contain at least one query");
    }

    let mut ids = HashSet::new();
    for query in &set.queries {
        validate_query(query)?;
        if !ids.insert(query.id.as_str()) {
            bail!("duplicate evaluation query id {:?}", query.id);
        }
    }
    Ok(())
}

fn validate_query(query: &EvaluationQuery) -> Result<()> {
    if !is_kebab_case_id(&query.id) {
        bail!(
            "evaluation query id {:?} must be non-empty lowercase kebab-case",
            query.id
        );
    }
    if !query.query.chars().any(char::is_alphanumeric) {
        bail!(
            "evaluation query {:?} must contain at least one letter or number",
            query.id
        );
    }
    if query
        .expected_paths
        .iter()
        .any(|path| path.trim().is_empty())
    {
        bail!("evaluation query {:?} has an empty expected path", query.id);
    }
    if query.expected_paths.iter().collect::<HashSet<_>>().len() != query.expected_paths.len() {
        bail!("evaluation query {:?} repeats an expected path", query.id);
    }

    match query.category {
        EvaluationCategory::NoAnswer => {
            if !query.expected_paths.is_empty() || !query.expected_headings.is_empty() {
                bail!(
                    "no_answer query {:?} must not define expected paths or headings",
                    query.id
                );
            }
        }
        _ if query.expected_paths.is_empty() => {
            bail!(
                "answerable evaluation query {:?} must define expected paths",
                query.id
            );
        }
        _ => {}
    }

    for (path, headings) in &query.expected_headings {
        if !query.expected_paths.contains(path) {
            bail!(
                "evaluation query {:?} defines headings for unexpected path {:?}",
                query.id,
                path
            );
        }
        if headings.is_empty() || headings.iter().any(|heading| heading.trim().is_empty()) {
            bail!(
                "evaluation query {:?} must define non-empty headings for {:?}",
                query.id,
                path
            );
        }
        if headings.iter().collect::<HashSet<_>>().len() != headings.len() {
            bail!(
                "evaluation query {:?} repeats an expected heading for {:?}",
                query.id,
                path
            );
        }
    }

    if query.tags.iter().any(|tag| tag.trim().is_empty()) {
        bail!("evaluation query {:?} has an empty tag", query.id);
    }
    if query.tags.iter().collect::<HashSet<_>>().len() != query.tags.len() {
        bail!("evaluation query {:?} repeats a tag", query.id);
    }
    if query
        .notes
        .as_deref()
        .is_some_and(|notes| notes.trim().is_empty())
    {
        bail!("evaluation query {:?} has empty notes", query.id);
    }
    if query
        .disabled_reason
        .as_deref()
        .is_some_and(|reason| reason.trim().is_empty())
    {
        bail!(
            "evaluation query {:?} has an empty disabled_reason",
            query.id
        );
    }

    Ok(())
}

fn is_kebab_case_id(id: &str) -> bool {
    id.split('-').all(|segment| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_schema_v1_for_runner_compatibility() {
        let input = r#"{
          "schema_version": 1,
          "corpus": "fixture",
          "queries": [{
            "id": "exact-01",
            "category": "exact",
            "query": "where is the runbook",
            "expected_paths": ["README.md"]
          }]
        }"#;
        let set = parse_evaluation_set(input).expect("schema v1 should remain readable");
        assert_eq!(set.schema_version, 1);
        assert!(set.queries[0].expected_headings.is_empty());
    }

    #[test]
    fn rejects_duplicate_ids() {
        let input = r#"{
          "schema_version": 2,
          "corpus": "fixture",
          "queries": [
            {"id":"same","category":"exact","query":"one","expected_paths":["README.md"]},
            {"id":"same","category":"exact","query":"two","expected_paths":["README.md"]}
          ]
        }"#;
        let error = parse_evaluation_set(input).expect_err("duplicate ids must fail");
        assert!(error.to_string().contains("duplicate"));
    }

    #[test]
    fn rejects_headings_for_an_unexpected_path() {
        let input = r#"{
          "schema_version": 2,
          "corpus": "fixture",
          "queries": [{
            "id": "exact-01",
            "category": "exact",
            "query": "where is the runbook",
            "expected_paths": ["README.md"],
            "expected_headings": {"docs/other.md": ["Other"]}
          }]
        }"#;
        let error = parse_evaluation_set(input).expect_err("unexpected heading path must fail");
        assert!(error.to_string().contains("unexpected path"));
    }

    #[test]
    fn rejects_expected_evidence_for_no_answer() {
        let input = r#"{
          "schema_version": 2,
          "corpus": "fixture",
          "queries": [{
            "id": "no-answer-01",
            "category": "no_answer",
            "query": "missing topic",
            "expected_paths": ["README.md"]
          }]
        }"#;
        let error = parse_evaluation_set(input).expect_err("no-answer evidence must fail");
        assert!(error.to_string().contains("must not define"));
    }

    #[test]
    fn rejects_invalid_structural_and_metadata_fields() {
        let cases = [
            (
                "unsupported schema",
                r#"{"schema_version":3,"corpus":"fixture","queries":[{"id":"exact-01","category":"exact","query":"one","expected_paths":["README.md"]}]}"#,
            ),
            (
                "empty corpus",
                r#"{"schema_version":2,"corpus":" ","queries":[{"id":"exact-01","category":"exact","query":"one","expected_paths":["README.md"]}]}"#,
            ),
            (
                "empty query set",
                r#"{"schema_version":2,"corpus":"fixture","queries":[]}"#,
            ),
            (
                "invalid id",
                r#"{"schema_version":2,"corpus":"fixture","queries":[{"id":"Exact 01","category":"exact","query":"one","expected_paths":["README.md"]}]}"#,
            ),
            (
                "empty query text",
                r#"{"schema_version":2,"corpus":"fixture","queries":[{"id":"exact-01","category":"exact","query":" ","expected_paths":["README.md"]}]}"#,
            ),
            (
                "query without searchable terms",
                r#"{"schema_version":2,"corpus":"fixture","queries":[{"id":"exact-01","category":"exact","query":"!!!","expected_paths":["README.md"]}]}"#,
            ),
            (
                "answerable without paths",
                r#"{"schema_version":2,"corpus":"fixture","queries":[{"id":"exact-01","category":"exact","query":"one","expected_paths":[]}]}"#,
            ),
            (
                "duplicate paths",
                r#"{"schema_version":2,"corpus":"fixture","queries":[{"id":"exact-01","category":"exact","query":"one","expected_paths":["README.md","README.md"]}]}"#,
            ),
            (
                "duplicate headings",
                r#"{"schema_version":2,"corpus":"fixture","queries":[{"id":"exact-01","category":"exact","query":"one","expected_paths":["README.md"],"expected_headings":{"README.md":["One","One"]}}]}"#,
            ),
            (
                "duplicate tags",
                r#"{"schema_version":2,"corpus":"fixture","queries":[{"id":"exact-01","category":"exact","query":"one","expected_paths":["README.md"],"tags":["pt-BR","pt-BR"]}]}"#,
            ),
            (
                "empty notes",
                r#"{"schema_version":2,"corpus":"fixture","queries":[{"id":"exact-01","category":"exact","query":"one","expected_paths":["README.md"],"notes":" "}]}"#,
            ),
            (
                "empty disabled reason",
                r#"{"schema_version":2,"corpus":"fixture","queries":[{"id":"exact-01","category":"exact","query":"one","expected_paths":["README.md"],"disabled_reason":" "}]}"#,
            ),
        ];

        for (label, input) in cases {
            assert!(
                parse_evaluation_set(input).is_err(),
                "{label} should be rejected"
            );
        }
    }
}
