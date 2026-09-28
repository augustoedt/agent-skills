# Evaluation dataset

`queries.json` is the versioned, human-reviewed retrieval benchmark for `docs-search`.
`queries.schema.json` defines the editor-facing structural schema for version 2. Runtime validation
in `src/evaluation.rs` is authoritative and also enforces cross-field invariants that JSON Schema
cannot express cleanly. Tests keep the published schema's version, ID pattern, categories, and
required fields aligned with the Rust contract.

## Version 2 fields

Required per query:

- `id`: stable, unique kebab-case identifier;
- `category`: `exact`, `semantic`, `ambiguous`, or `no_answer`;
- `query`: text submitted to the search engine;
- `expected_paths`: relevant corpus paths, empty only for `no_answer`.

Optional:

- `expected_headings`: acceptable headings keyed by an expected path;
- `notes`: human rationale for the relevance judgment;
- `tags`: language, domain, or difficulty labels;
- `disabled_reason`: explicit explanation when a case must be skipped temporarily.

Path relevance is primary. Heading relevance is diagnostic and prevents a result from receiving
full section-level credit merely because it found the right file. Every heading key must also be
listed in `expected_paths`.

## Invariants

- IDs, expected paths, headings, and tags cannot repeat within a query.
- Answerable categories require at least one expected path.
- `no_answer` requires empty expected paths and no expected headings.
- Empty notes, tags, headings, and disabled reasons are invalid.
- Schema v1 remains readable by the Rust parser so the future runner can report a deliberate
  migration error or upgrade path; the canonical dataset uses schema v2.

## Corpora

- `fixtures/stable-v1/` is the immutable regression corpus and preserves every expected path and
  heading from the canonical dataset.
- The real `agent-skills` checkout is the operational corpus and may evolve between reports.
- `fixtures/stable-v1/fixture.json` records selected and deliberately excluded paths.

## Change policy

Do not change a query merely to make a ranking experiment pass. Every relevance change needs a
human-readable reason. Keep IDs stable so reports remain comparable. Schema-breaking changes must
increment `schema_version` and preserve an explicit compatibility decision in code and docs.
