use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

use docs_search::corpus;
use docs_search::{EVALUATION_SCHEMA_VERSION, EvaluationCategory, parse_evaluation_set};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct FixtureManifest {
    schema_version: u32,
    name: String,
    query_dataset: String,
    expected_selected_paths: Vec<String>,
    excluded_paths: Vec<String>,
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("evaluation/fixtures/stable-v1")
}

#[test]
fn canonical_evaluation_dataset_uses_schema_v2_and_has_expected_coverage() {
    let input = include_str!("../evaluation/queries.json");
    let set = parse_evaluation_set(input).expect("canonical evaluation dataset should be valid");

    assert_eq!(set.schema_version, EVALUATION_SCHEMA_VERSION);
    assert_eq!(set.corpus, "agent-skills");
    assert_eq!(set.queries.len(), 20);

    let mut categories = HashMap::new();
    for query in &set.queries {
        let label = match query.category {
            EvaluationCategory::Exact => "exact",
            EvaluationCategory::Semantic => "semantic",
            EvaluationCategory::Ambiguous => "ambiguous",
            EvaluationCategory::NoAnswer => "no_answer",
        };
        *categories.entry(label).or_insert(0usize) += 1;

        assert!(
            query.notes.is_some(),
            "{} should explain relevance",
            query.id
        );
        assert!(
            !query.tags.is_empty(),
            "{} should have diagnostic tags",
            query.id
        );
        if query.category == EvaluationCategory::NoAnswer {
            assert!(query.expected_headings.is_empty());
        } else {
            assert_eq!(
                query.expected_headings.len(),
                query.expected_paths.len(),
                "{} should define headings for every expected path",
                query.id
            );
        }
    }

    assert_eq!(categories.get("exact"), Some(&8));
    assert_eq!(categories.get("semantic"), Some(&6));
    assert_eq!(categories.get("ambiguous"), Some(&3));
    assert_eq!(categories.get("no_answer"), Some(&3));
}

#[test]
fn stable_fixture_preserves_corpus_boundaries() {
    let root = fixture_root();
    let manifest: FixtureManifest = serde_json::from_str(
        &fs::read_to_string(root.join("fixture.json")).expect("fixture manifest should exist"),
    )
    .expect("fixture manifest should be valid");
    let loaded = corpus::load(&root).expect("stable fixture should load");
    let paths: BTreeSet<_> = loaded
        .chunks
        .iter()
        .map(|chunk| chunk.path.as_str())
        .collect();
    let expected: BTreeSet<_> = manifest
        .expected_selected_paths
        .iter()
        .map(String::as_str)
        .collect();

    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.name, "agent-skills-stable-v1");
    assert!(root.join(&manifest.query_dataset).is_file());
    assert_eq!(loaded.files, expected.len());
    assert_eq!(paths, expected);

    for excluded in manifest.excluded_paths {
        assert!(
            fs::symlink_metadata(root.join(&excluded)).is_ok(),
            "excluded fixture {excluded} should exist"
        );
        assert!(
            !paths.contains(excluded.as_str()),
            "excluded fixture {excluded} entered the corpus"
        );
    }
    let linked_notes = root.join("docs/reference/linked-notes.md");
    assert!(
        fs::symlink_metadata(&linked_notes)
            .expect("fixture symlink should exist")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(&linked_notes).expect("fixture symlink target should be readable"),
        PathBuf::from("../../notes.md")
    );
}

#[test]
fn stable_fixture_contains_every_expected_path_and_heading() {
    let set = parse_evaluation_set(include_str!("../evaluation/queries.json"))
        .expect("canonical evaluation dataset should be valid");
    let loaded = corpus::load(&fixture_root()).expect("stable fixture should load");

    let paths: BTreeSet<_> = loaded
        .chunks
        .iter()
        .map(|chunk| chunk.path.as_str())
        .collect();
    let headings: BTreeSet<_> = loaded
        .chunks
        .iter()
        .filter_map(|chunk| {
            chunk
                .heading
                .as_deref()
                .map(|heading| (chunk.path.as_str(), heading))
        })
        .collect();

    for query in set.queries {
        for path in query.expected_paths {
            assert!(paths.contains(path.as_str()), "{} missing {path}", query.id);
        }
        for (path, expected) in query.expected_headings {
            for heading in expected {
                assert!(
                    headings.contains(&(path.as_str(), heading.as_str())),
                    "{} missing {path} heading {heading:?}",
                    query.id
                );
            }
        }
    }
}

#[test]
fn published_json_schema_tracks_the_runtime_contract() {
    let schema: Value = serde_json::from_str(include_str!("../evaluation/queries.schema.json"))
        .expect("published evaluation schema should be valid JSON");

    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        EVALUATION_SCHEMA_VERSION
    );
    assert_eq!(
        schema["$defs"]["query"]["properties"]["id"]["pattern"],
        "^[a-z0-9]+(?:-[a-z0-9]+)*$"
    );

    let categories = schema["$defs"]["query"]["properties"]["category"]["enum"]
        .as_array()
        .expect("category enum should be an array");
    assert_eq!(
        categories,
        &["exact", "semantic", "ambiguous", "no_answer"]
            .map(Value::from)
            .to_vec()
    );

    let required = schema["$defs"]["query"]["required"]
        .as_array()
        .expect("required fields should be an array");
    for field in ["id", "category", "query", "expected_paths"] {
        assert!(required.contains(&Value::from(field)), "missing {field}");
    }
}
