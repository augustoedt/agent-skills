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
- Only schema v2 is accepted; older or future versions fail before evaluation starts.

## Corpora

- `fixtures/stable-v1/` is the versioned corpus kept stable by policy and preserves every expected
  path and heading from the canonical dataset.
- The real `agent-skills` checkout is the operational corpus and may evolve between reports.
- `fixtures/stable-v1/fixture.json` records selected and deliberately excluded paths.
- [`reports/`](reports/README.md) preserves immutable baseline/experiment reports and their
  human-readable interpretation.

## Evaluation runner

From `tools/docs-search`:

```bash
cargo run -- evaluate \
  --root evaluation/fixtures/stable-v1 \
  --queries evaluation/queries.json \
  --limit 5 \
  --max-excerpt-chars 1200 \
  --json

# Experimento opt-in: no máximo um chunk por arquivo no resultado final
cargo run -- evaluate \
  --root evaluation/fixtures/stable-v1 \
  --queries evaluation/queries.json \
  --max-results-per-path 1 \
  --json
```

The runner validates the dataset, preserves query order, invokes the same public `search()` function
used by the CLI, and reports individual failures without aborting later queries. Invalid datasets,
global errors, and any failed query produce a non-zero exit status. Disabled cases are reported as
skipped and excluded from metric denominators.

Path metrics deduplicate repeated chunks from the same file without renumbering the selected result
list. In report schema v2, `rank` is the final position presented to the caller and drives Hit@1,
Recall@5 and MRR@5; `raw_rank` preserves the position before optional path diversity. Without a cap,
`rank` and `raw_rank` are equal, preserving the baseline semantics. With
`--max-results-per-path N`, the selector scans beyond the initial top 5, keeps at most `N` chunks per
path and then assigns contiguous final ranks. Heading diagnostics use the final selected rank while
retaining `raw_rank` per result for audit.

Hit@1, macro and micro Recall@5, and MRR@5 use answerable queries. No-answer false-positive rate uses
`no_answer` queries. If a query fails, affected aggregate quality metrics are `null` and the command
exits non-zero instead of reporting an artificially improved score. Latency uses a monotonic clock
around each attempted search. Context aggregates include only successful queries and count Unicode
characters across returned excerpts. `execution_model: full_corpus_scan_per_query` makes explicit
that baseline latency includes corpus I/O for every query.

## Evaluation report v2

`report.schema.json` formalizes the current contract independently from search-response schema v2
and query-dataset schema v2. Report v2 records `config.max_results_per_path` and
`results[].raw_rank` so path-diversity experiments remain reproducible. Search-response schema v2 is
formalized separately in `search-response.schema.json`. No legacy report schema is supported. The
report records:

- tool version, engine, dataset hash and query schema version;
- logical corpus name, caller-provided root, file/chunk counts, and a reproducible fingerprint;
- effective limits, path-diversity cap and execution model;
- global and per-category metrics;
- expected evidence, final rank, raw rank, section diagnostics, latency, context volume, and errors
  for every query.

`--output <path>` is the only way the runner writes a report. Use a relative `--root` when the JSON
will be committed so it does not contain a machine-specific absolute path. Latency is intentionally
volatile and should not be snapshot-compared byte for byte. The first real-corpus baseline is
[`reports/agent-skills-lexical-bm25-v1-unlimited-v2.json`](reports/agent-skills-lexical-bm25-v1-unlimited-v2.json).

## Engine bake-off contracts

The engine comparison has contracts independent from normal search response and evaluation report
v2:

- `engine-bakeoff-protocol.schema.json` validates the immutable preregistration: engines, exact
  configurations, development inputs, model artifacts, measurement method, budgets, selection rule,
  and consumed-holdout isolation;
- `engine-bakeoff-report.schema.json` validates one observed engine/input/run report: provenance,
  baseline deltas, per-query evidence, quality, timing phases, indexing workload, RAM, disk,
  determinism, rebuild, fallback, errors, and budget checks.

`engine-bakeoff-v1` fixes top 5 with 1,200-character excerpts and no path cap. It compares direct
BM25, SQLite-cached BM25, FTS5, exact local E5 vectors, and FTS5/E5 fused with RRF. Each input/engine
pair runs twice in fresh processes. The default engine and global installation cannot change during
the experiment.

Phase 4.1 adds a two-stage common harness. `bakeoff observe` loads one frozen input, executes its
queries, records lookup/ranking/excerpt/total timing, validates evidence against source chunks, and
writes an immutable private observation. `bakeoff finalize` revalidates hashes, corpus, dataset,
lexical ranking, baseline, provenance, and the paired run before combining external startup,
end-to-end, RSS, and disk measurements into the operational v1 report. A separate private harness
registry pins the runner and public revision. The runner attests the clean release binary, frozen
Rust toolchain and host, process environment, and denylist before path resolution.
Writes use `create_new`, mode `0600`, and refuse overwrite. The `lexical-bm25-v1` and
`sqlite-cache-bm25-v1` and `fts5-v1` adapters are implemented. SQLite/BM25 uses an atomic persistent
chunk/token cache and exact direct ranking. FTS5 uses the frozen tokenizer, column weights, top 50,
safe literal query construction, complete index projection checks and fail-closed rebuild policy.
Both execute four-operation incremental verification, corruption recovery and bundled-runtime
checks. E5 and RRF remain explicitly unavailable. Frozen measurements stay closed until all
adapters share one clean revision.

Only the schemas are public. Protocol instances, snapshot manifests, private datasets, baseline
reports, model paths, and checksum registries stay in machine-local encrypted storage. Frozen
Markdown comes from Git objects rather than live working trees. A local verifier must pass before
implementation or measurement, and the private runner denies consumed holdouts before resolving
their dataset paths. Changing any frozen parameter requires a new protocol tag.

## Private and machine-local corpora

Datasets and reports derived from private checkouts must stay outside this public repository. Project
names, local roots, queries, expected paths/headings, fingerprints, revisions, and metrics can reveal
private information even when source documents are not copied.

Use a directory owned by the current user outside every Git checkout, preferably on encrypted local
storage. Public regression coverage must use synthetic fixtures or explicitly public corpora.
`evaluation/corpora/`, `evaluation/private/`, local manifests, and untracked report JSON are ignored
defensively; do not force-add them.

### `manifest.local.json`

The local manifest is an inventory and path mapping for one computer. It is not read automatically
by the `docs-search` CLI and must never be committed. A private wrapper can resolve an ID from the
manifest and pass the resulting external paths to `--root`, `--queries`, and `--output`.

Its public structural contract is
[`local-manifest.schema.json`](local-manifest.schema.json). Example with invented values:

```json
{
  "schema_version": 1,
  "scope": "machine-local-only",
  "reports_dir": "reports",
  "corpora": [
    {
      "id": "corpus-001",
      "role": "development",
      "root": "/absolute/path/on-this-machine/project-a",
      "queries": "corpus-001/queries.json",
      "baseline_report": "reports/corpus-001-baseline.json"
    },
    {
      "id": "corpus-002",
      "role": "holdout",
      "root": "/another/local/path/project-b"
    }
  ]
}
```

Top-level fields:

- `schema_version`: manifest contract version; currently `1`;
- `scope`: must be `machine-local-only`, making non-portability explicit;
- `notes`: optional local operational note;
- `reports_dir`: directory relative to the manifest location for new reports;
- `corpora`: non-empty list of local corpus registrations.

Corpus fields:

- `id`: neutral stable alias such as `corpus-001`; never encode a client, product, repository, or
  domain name;
- `role`: `development`, `holdout`, `deferred`, or `excluded`;
- `root`: absolute path to the checkout on this computer;
- `queries`: optional dataset path relative to the manifest directory;
- `baseline_report`: optional pointer to an immutable local baseline, also relative to the manifest
  directory. It requires `queries` and is descriptive; new experiments receive new report files.

IDs must be unique. Relative paths must stay inside the private evaluation directory and must not use
`..` to escape it. A safe local runner executes `development` normally and only releases a frozen
holdout through an explicit one-shot gate; after success, that holdout is consumed and rerun remains
blocked. Report tags should be kebab-case and outputs must not silently overwrite a previous
measurement.

### Lifecycle across computers

- Project moved on the same computer: edit only its `root`.
- Different computer: create a new manifest with the roots and projects available there; do not
  commit or assume the old paths.
- Reusing a private dataset: transfer it only through an approved encrypted channel and map it to a
  new neutral ID/root locally.
- Removing a project: delete its local registration and data; no public Git change is required.
- Public report: create a separate sanitized benchmark from synthetic or explicitly public content;
  never copy a private report and try to redact it afterward.

Recommended permissions are `0700` for directories and executables owned by the user, and `0600`
for manifests, datasets, mappings, and reports. Full-disk encryption protects data at rest; an
encrypted mounted volume adds isolation while preserving the same manifest contract.

## Change policy

Do not change a query merely to make a ranking experiment pass. Every relevance change needs a
human-readable reason. Keep IDs stable so reports remain comparable. Schema-breaking changes must increment `schema_version`; unsupported older versions fail explicitly
instead of keeping compatibility code.
