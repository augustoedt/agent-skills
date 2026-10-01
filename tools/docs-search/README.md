# docs-search

Busca documental local, determinística e independente do modelo de IA. O binário seleciona
pequenos trechos de documentação para que o agente leia apenas o contexto suficiente e depois
confira a fonte original antes de alterar código.

## Estado atual

A versão de código-fonte 0.6.0-alpha.6 mantém o baseline lexical padrão sem banco e sem embeddings e usa
somente os contratos atuais: resposta de busca v2, dataset de consultas v2 e relatório v2:

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
no JSON e também o mostra no modo humano quando ele difere.

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
`max_results_per_path`, o rank final e o rank bruto. Sem `--json`, o comando mostra um resumo
humano. `--output <arquivo>` grava explicitamente o relatório JSON; nada é persistido
automaticamente. O comando
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

Os subcomandos experimentais `bakeoff observe` e `bakeoff finalize` implementam o harness comum da
Fase 4.1. Eles são exclusivos do runner privado: exigem protocolo congelado, provenance do binário,
host, ambiente e isolamento de holdout, usam somente inputs `development`, revalidam corpus,
dataset, baseline e evidências, recusam overwrite e escrevem artefatos `0600`. Os cinco adapters
estão implementados: `lexical-bm25-v1`, `sqlite-cache-bm25-v1`, `fts5-v1`,
`local-embeddings-v1` e `hybrid-rrf-v1`. O braço SQLite preserva o ranking BM25 e usa cache privado
reconstruível. O braço FTS5 usa o tokenizer e os pesos congelados, 50 candidatos e query literal
segura. O braço vetorial usa o modelo E5 local congelado, Candle CPU, mean pooling pela attention
mask, L2, scan exato, top 50 e threshold 0,80. O híbrido combina as duas listas por RRF com `k = 60`,
deduplica por `chunk_hash`, preserva os ranks das fontes e falha como um todo se E5 falhar. Os
índices são privados e atômicos, verificam atualização incremental e corrupção; FTS5, embeddings e
híbrido falham fechados quando o rebuild ou o modelo impedem uma resposta íntegra. SQLite e FTS5
usam `rusqlite 0.40.2` com SQLite bundled 3.53.2. Esses subcomandos não alteram `search`, o engine
padrão nem a instalação global.

O conjunto inicial de avaliação está em `evaluation/queries.json`. Somente o schema v2 é aceito; ele
está documentado em `evaluation/README.md` e formalizado por
`evaluation/queries.schema.json`. As 20 consultas incluem casos exatos, semânticos, ambíguos e sem
resposta, com headings esperados, notas e tags diagnósticas. A fixture estável e versionada
`evaluation/fixtures/stable-v1` sustenta testes de regressão. As medições atuais do corpus real e da
fixture estão analisadas em [`evaluation/reports/`](evaluation/reports/README.md). O primeiro gate
de holdout rejeitou a promoção do cap 1 a default. A hipótese seguinte, equivalência por prefixo
morfológico limitado 7/4, foi testada duas vezes em development e rejeitada pelas guardas
pré-registradas; seu código não entrou em `main`. O ciclo seguinte comparou de forma faseada o
BM25 direto, cache SQLite com BM25 preservado, FTS5, embeddings locais e híbrido por RRF. A Fase 4.0
já congelou protocolo, configurações, inputs, modelo, budgets e contratos públicos de protocolo e
relatório operacional v1. As Fases 4.1–4.5 implementaram o harness e todos os adapters, com
runtime, pooling, normalização, threshold, rebuild, atualização incremental, corrupção, evidência e
fallback/falha fechada cobertos por testes. A Fase 4.6 concluiu duas rodadas sobre os cinco inputs de
development. BM25 direto e cache SQLite/BM25 foram as únicas engines aprovadas em todas as guardas e
formam a fronteira de Pareto; FTS5, E5 e RRF foram eliminados sem retuning. Os detalhes permanecem
nos relatórios privados. A Fase 5.1 já reservou e congelou holdouts novos sem executar rankings; o
contrato público ativo está em `evaluation/engine-holdout-protocol-v2.schema.json`. O gate continua fechado
enquanto o harness one-shot é implementado e auditado.
