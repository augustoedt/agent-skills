# Stable evaluation fixtures

`stable-v1/` is a versioned documentation corpus kept stable by project policy for deterministic
regression tests.
It preserves the paths and intended headings used by `evaluation/queries.json` while remaining
independent from the evolving `agent-skills` documentation.

Rules:

- never edit `stable-v1` merely to make a ranking experiment pass;
- create `stable-v2` for intentional corpus changes after recording why v1 is insufficient;
- keep excluded files, `.ignore` rules, and symlinks because they test corpus boundaries;
- use the real repository separately for operational measurements;
- never place credentials or production content in a fixture.

The fixture is not product documentation and must not be added to the default project corpus.
