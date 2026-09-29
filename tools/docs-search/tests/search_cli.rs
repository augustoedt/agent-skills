use std::fs;
use std::process::Command;

use docs_search::{SearchRequest, search};
use tempfile::tempdir;

fn request(root: &std::path::Path, query: &str) -> SearchRequest {
    SearchRequest {
        root: root.to_path_buf(),
        query: query.to_owned(),
        limit: 5,
        max_excerpt_chars: 300,
        max_results_per_path: None,
    }
}

#[test]
fn ranks_the_relevant_heading_and_returns_evidence() {
    let directory = tempdir().unwrap();
    fs::create_dir(directory.path().join("docs")).unwrap();
    fs::write(
        directory.path().join("docs/runbook.md"),
        "# Deploy\nUse Railway.\n\n## Recuperação do banco\nRestaure o backup antes do restart.\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("README.md"),
        "# Example\nGeneral project information.\n",
    )
    .unwrap();

    let response = search(request(directory.path(), "recuperacao banco")).unwrap();

    assert_eq!(response.schema_version, 1);
    assert_eq!(response.results[0].path, "docs/runbook.md");
    assert_eq!(
        response.results[0].heading.as_deref(),
        Some("Deploy > Recuperação do banco")
    );
    assert!(response.results[0].excerpt.contains("Restaure o backup"));
    assert_eq!(response.results[0].matched_terms, ["recuperacao", "banco"]);
    assert_eq!(response.results[0].file_hash.len(), 64);
    assert_eq!(response.results[0].chunk_hash.len(), 64);
}

#[test]
fn returns_no_results_when_the_corpus_has_no_answer() {
    let directory = tempdir().unwrap();
    fs::create_dir(directory.path().join("docs")).unwrap();
    fs::write(directory.path().join("README.md"), "# Project\nRust CLI.\n").unwrap();
    fs::write(
        directory.path().join("docs/storage.md"),
        "# Banco local\nO índice usa SQLite.\n",
    )
    .unwrap();

    let response = search(request(directory.path(), "senha do banco de producao")).unwrap();

    assert!(response.results.is_empty());
}

#[test]
fn excludes_markdown_outside_the_documentation_corpus() {
    let directory = tempdir().unwrap();
    fs::create_dir_all(directory.path().join("skills/example")).unwrap();
    fs::write(
        directory.path().join("skills/example/SKILL.md"),
        "# Secret skill\nneedle-only-here\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("README.md"),
        "# Project\nPublic docs.\n",
    )
    .unwrap();

    let response = search(request(directory.path(), "needle only here")).unwrap();

    assert!(response.results.is_empty());
    assert_eq!(response.corpus.files, 1);
}

#[test]
fn path_diversity_limits_chunks_per_file_and_preserves_raw_rank() {
    let directory = tempdir().unwrap();
    fs::create_dir(directory.path().join("docs")).unwrap();
    fs::write(
        directory.path().join("docs/orion-nebula.md"),
        "# Orion Nebula\norion nebula orion nebula.\n\n## First\norion nebula orion nebula.\n\n## Second\norion nebula orion nebula.\n\n## Third\norion nebula orion nebula.\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("docs/reference.md"),
        "# Reference\nA short note about orion nebula.\n",
    )
    .unwrap();

    let mut diversified = request(directory.path(), "orion nebula");
    diversified.max_results_per_path = Some(1);
    let response = search(diversified).unwrap();

    assert_eq!(response.results.len(), 2);
    assert_eq!(response.results[0].rank, 1);
    assert_eq!(response.results[1].rank, 2);
    assert_ne!(response.results[0].path, response.results[1].path);
    assert!(response.results[1].raw_rank > response.results[1].rank);
}

#[test]
fn rejects_an_invalid_path_diversity_limit() {
    let directory = tempdir().unwrap();
    fs::write(directory.path().join("README.md"), "# Project\nDocs.\n").unwrap();
    let mut invalid = request(directory.path(), "project");
    invalid.max_results_per_path = Some(0);

    let error = search(invalid).unwrap_err();

    assert!(error.to_string().contains("max results per path"));
}

#[test]
fn cli_emits_the_versioned_json_contract() {
    let directory = tempdir().unwrap();
    fs::write(
        directory.path().join("README.md"),
        "# Project\nUse docs-search for project documentation.\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_docs-search"))
        .args([
            "search",
            "--root",
            directory.path().to_str().unwrap(),
            "--query",
            "project documentation",
            "--json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["engine"], "lexical-bm25-v1");
    assert_eq!(json["results"][0]["path"], "README.md");
    assert!(json["results"][0].get("raw_rank").is_none());
}

#[test]
fn cli_evaluates_the_stable_fixture_and_emits_metrics() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("evaluation/fixtures/stable-v1");
    let queries = manifest.join("evaluation/queries.json");

    let output = Command::new(env!("CARGO_BIN_EXE_docs-search"))
        .args([
            "evaluate",
            "--root",
            root.to_str().unwrap(),
            "--queries",
            queries.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["schema_version"], 2);
    assert_eq!(json["engine"], "lexical-bm25-v1");
    assert_eq!(json["tool_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(json["queries_schema_version"], 2);
    assert_eq!(
        json["config"]["max_results_per_path"],
        serde_json::Value::Null
    );
    assert_eq!(json["corpus"]["files"], 5);
    assert_eq!(json["summary"]["queries_total"], 20);
    assert_eq!(json["summary"]["executed"], 20);
    assert_eq!(json["summary"]["failed"], 0);
    assert!(json["summary"]["hit_at_1"].is_number());
    assert!(json["summary"]["latency_ms"]["p95"].is_number());
    assert!(json["summary"]["context_chars"]["total"].is_number());
    assert_eq!(json["per_category"]["exact"]["queries_total"], 8);
    assert_eq!(json["per_category"]["no_answer"]["queries_total"], 3);
    assert_eq!(
        json["per_category"]["no_answer"]["hit_at_1"],
        serde_json::Value::Null
    );
    assert_eq!(json["queries"].as_array().unwrap().len(), 20);
    assert_eq!(json["queries"][0]["id"], "exact-01");
    assert_eq!(json["queries"][19]["id"], "no-answer-03");
    assert_eq!(json["queries"][0]["results"][0]["line_start"], 17);
    assert_eq!(json["queries"][0]["results"][0]["line_end"], 25);
    assert!(json["queries"][0]["results"][0]["raw_rank"].is_number());
    assert!(json["queries"][0]["result_count"].as_u64().unwrap() > 0);
}

#[test]
fn cli_writes_an_evaluation_report_only_when_output_is_explicit() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("evaluation/fixtures/stable-v1");
    let queries = manifest.join("evaluation/queries.json");
    let directory = tempdir().unwrap();
    let report = directory.path().join("report.json");

    let output = Command::new(env!("CARGO_BIN_EXE_docs-search"))
        .args([
            "evaluate",
            "--root",
            root.to_str().unwrap(),
            "--queries",
            queries.to_str().unwrap(),
            "--max-results-per-path",
            "1",
            "--output",
            report.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Hit@1"));
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(report).unwrap()).unwrap();
    assert_eq!(json["schema_version"], 2);
    assert_eq!(json["config"]["max_results_per_path"], 1);
    assert_eq!(json["summary"]["queries_total"], 20);
}

#[test]
fn cli_reports_disabled_queries_as_skipped() {
    let directory = tempdir().unwrap();
    fs::write(
        directory.path().join("README.md"),
        "# Project\nProject documentation.\n",
    )
    .unwrap();
    let queries = directory.path().join("queries.json");
    fs::write(
        &queries,
        r#"{
          "schema_version": 2,
          "corpus": "fixture",
          "queries": [
            {
              "id": "exact-one",
              "category": "exact",
              "query": "project documentation",
              "expected_paths": ["README.md"]
            },
            {
              "id": "disabled-one",
              "category": "no_answer",
              "query": "temporarily unavailable case",
              "expected_paths": [],
              "disabled_reason": "fixture migration"
            }
          ]
        }"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_docs-search"))
        .args([
            "evaluate",
            "--root",
            directory.path().to_str().unwrap(),
            "--queries",
            queries.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["summary"]["queries_total"], 2);
    assert_eq!(json["summary"]["executed"], 1);
    assert_eq!(json["summary"]["skipped"], 1);
    assert_eq!(json["queries"][1]["status"], "skipped");
    assert_eq!(json["queries"][1]["disabled_reason"], "fixture migration");
}

#[test]
fn cli_rejects_an_invalid_evaluation_dataset() {
    let directory = tempdir().unwrap();
    fs::write(directory.path().join("README.md"), "# Project\nDocs.\n").unwrap();
    let queries = directory.path().join("invalid-queries.json");
    fs::write(
        &queries,
        r#"{"schema_version":2,"corpus":"fixture","queries":[]}"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_docs-search"))
        .args([
            "evaluate",
            "--root",
            directory.path().to_str().unwrap(),
            "--queries",
            queries.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid evaluation dataset"));
}
