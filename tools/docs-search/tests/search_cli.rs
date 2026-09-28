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
}
