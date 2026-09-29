# docs-search

Busca documental local, determinística e independente do modelo de IA. O binário seleciona
pequenos trechos de documentação para que o agente leia apenas o contexto suficiente e depois
confira a fonte original antes de alterar código.

## Estado atual

A versão 0.4.0 mantém o baseline lexical sem banco e sem embeddings, publica a resposta de busca v2
com diversidade auditável e preserva o hardening do chunking de Markdown:

- corpus: `docs/**/*.md` e, na raiz, `README.md`, `CLAUDE.md`, `AGENTS.md` e `pi-warden.md`;
- chunking por headings Markdown, preservando breadcrumbs e ignorando headings aparentes dentro de
  blocos cercados por crases ou tils;
- ranking BM25 com reforço de heading, path e frase exata;
- normalização Unicode, inclusive acentos;
- evidência com path, heading, linhas, excerpt, hashes BLAKE3, score e termos encontrados;
- leitura direta dos documentos a cada busca: `docs/` continua sendo a fonte da verdade;
- `evaluate` com Hit@1, Recall@5 macro/micro, MRR@5, falsos positivos de no-answer, latência e
  volume de contexto;
- `--max-results-per-path N` para medir seleção diversificada sem alterar o comportamento padrão.

Arquivos fora desse corpus, incluindo `.env`, código e `skills/**`, não entram na busca padrão.
Isso não detecta secrets escritos dentro dos próprios Markdown; credenciais nunca devem ser
documentadas no corpus.

## Instalação

A partir da raiz do repositório `agent-skills`:

```bash
scripts/install-docs-search.sh
```

Ou diretamente:

```bash
cargo install --locked --force --path tools/docs-search
```

## Uso

```bash
docs-search search \
  --root /caminho/do/projeto \
  --query "como sincronizar as skills" \
  --limit 5 \
  --json
```

Sem `--json`, o binário imprime uma visualização humana. Respostas bem-sucedidas com `--json`
usam o contrato `schema_version: 2`, formalizado em `evaluation/search-response.schema.json`;
erros são escritos em stderr e retornam exit code diferente de zero. O contrato inclui:

- `engine`, `query`, `root` e resumo do corpus;
- `selection.max_results_per_path`, com `null` quando não há cap;
- `rank`, `raw_rank`, `path`, `heading`, `line_start`, `line_end` e `excerpt`;
- `file_hash`, `chunk_hash`, `score` e `matched_terms`.

As linhas são inclusivas e começam em 1. `heading` pode ser `null` para texto anterior ao primeiro
heading. `matched_terms` contém termos normalizados, sem acentos e sem stopwords comuns. O cap
experimental é opt-in; quando usado, o output mantém ranks finais contíguos, registra o rank bruto
no JSON e também o mostra no modo humano quando ele difere. Não há contrato de busca v1 arquivado,
pois a migração foi deliberadamente direta e não havia consumidores.

Para executar o benchmark versionado:

```bash
docs-search evaluate \
  --root evaluation/fixtures/stable-v1 \
  --queries evaluation/queries.json \
  --limit 5 \
  --max-excerpt-chars 1200 \
  --max-results-per-path 1 \
  --json
```

O cap é opcional; omita-o para reproduzir o ranking original. O relatório atual usa contrato
independente `schema_version: 2`, formalizado em `evaluation/report.schema.json`, e registra
`max_results_per_path`, o rank final e o rank bruto. O schema histórico v1 permanece em
`evaluation/report-v1.schema.json`. Sem `--json`, o comando mostra um resumo humano. `--output
<arquivo>` grava explicitamente o relatório JSON; nada é persistido automaticamente. O comando
continua após falhas individuais, inclui o erro no caso correspondente e termina com status não
zero se alguma consulta falhar. Para relatórios versionados, execute com um `--root` relativo para
não registrar paths absolutos específicos da máquina. Avaliações derivadas de projetos privados não
são versionadas: mantenha dataset, relatório e manifesto fora do repositório, use um identificador
neutro e passe os caminhos locais explicitamente. Veja `evaluation/README.md`.

## Desenvolvimento

```bash
cd tools/docs-search
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

O conjunto inicial de avaliação está em `evaluation/queries.json`. O schema v2 é documentado em
`evaluation/README.md` e formalizado por `evaluation/queries.schema.json`; o parser Rust também
mantém leitura do schema v1. As 20 consultas incluem casos exatos, semânticos, ambíguos e sem
resposta, com headings esperados, notas e tags diagnósticas. A fixture estável e versionada
`evaluation/fixtures/stable-v1` sustenta testes de regressão. O primeiro baseline do corpus real,
sem tuning, está preservado e analisado em [`evaluation/reports/`](evaluation/reports/README.md).
O benchmark ainda precisa de mais casos ambíguos/no-answer e de um segundo corpus antes de
justificar SQLite FTS5, embeddings ou um classificador/router.
