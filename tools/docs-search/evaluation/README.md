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

Path relevance drives Hit@1, Recall@5, and MRR@5. Heading relevance is a separate diagnostic that
reveals correct-file/wrong-section retrieval without changing path-level scores. Every heading key
must also be listed in `expected_paths`.

## Invariants

- IDs, expected paths, headings, and tags cannot repeat within a query.
- Runtime validation requires at least one Unicode letter or number in `query`; the published JSON
  Schema uses the more portable structural lower bound of one non-whitespace character.
- Answerable categories require at least one expected path.
- `no_answer` requires empty expected paths and no expected headings.
- Empty notes, tags, headings, and disabled reasons are invalid.
- Schema v1 remains readable by the Rust parser so the future runner can report a deliberate
  migration error or upgrade path; the canonical dataset uses schema v2.

## Corpora

- `fixtures/stable-v1/` is the versioned corpus kept stable by policy and preserves every expected
  path and heading from the canonical dataset.
- The real `agent-skills` checkout is the operational corpus and may evolve between reports.
- `fixtures/stable-v1/fixture.json` records selected and deliberately excluded paths.

## Evaluation runner

From `tools/docs-search`:

```bash
cargo run -- evaluate \
  --root evaluation/fixtures/stable-v1 \
  --queries evaluation/queries.json \
  --limit 5 \
  --max-excerpt-chars 1200 \
  --json
```

The runner validates the dataset, preserves query order, invokes the same public `search()` function
used by the CLI, and reports individual failures without aborting later queries. Invalid datasets,
global errors, and any failed query produce a non-zero exit status. Disabled cases are reported as
skipped and excluded from metric denominators.

Path metrics deduplicate repeated chunks from the same file while preserving that file's best raw
chunk rank. The cutoff is applied to raw ranks 1–5; deduplication does not compress rank gaps. Hit@1,
macro and micro Recall@5, and MRR@5 use answerable queries. No-answer false-positive rate uses
`no_answer` queries. Heading rank scans all chunks returned by the configured limit and preserves
its raw rank; it is not a cutoff metric. If a query fails, affected aggregate quality metrics are
`null` and the command exits non-zero instead of reporting an artificially improved score. Latency
uses a monotonic clock around each attempted search. Context aggregates include only successful
queries and count Unicode characters across returned excerpts. `execution_model:
full_corpus_scan_per_query`
makes explicit that baseline latency includes corpus I/O for every query.

## Evaluation report v1

`report.schema.json` formalizes a contract independent from both search-response schema v1 and
query-dataset schema v2. The report records:

- tool version, engine, dataset hash and query schema version;
- logical corpus name, caller-provided root, file/chunk counts, and a reproducible fingerprint;
- effective limits and execution model;
- global and per-category metrics;
- expected evidence, returned ranks, section diagnostics, latency, context volume, and errors for
  every query.

`--output <path>` is the only way the runner writes a report. Use a relative `--root` when the JSON
will be committed so it does not contain a machine-specific absolute path. Latency is intentionally
volatile and should not be snapshot-compared byte for byte.

## Change policy

Do not change a query merely to make a ranking experiment pass. Every relevance change needs a
human-readable reason. Keep IDs stable so reports remain comparable. Schema-breaking changes must
increment `schema_version` and preserve an explicit compatibility decision in code and docs.
