# docs-search

Busca documental local, determinística e independente do modelo de IA. O binário seleciona
pequenos trechos de documentação para que o agente leia apenas o contexto suficiente e depois
confira a fonte original antes de alterar código.

## Estado atual

A versão 0.2.0 mantém o baseline lexical sem banco e sem embeddings e adiciona um executor de
avaliação reproduzível:

- corpus: `docs/**/*.md` e, na raiz, `README.md`, `CLAUDE.md`, `AGENTS.md` e `pi-warden.md`;
- chunking por headings Markdown, preservando breadcrumbs;
- ranking BM25 com reforço de heading, path e frase exata;
- normalização Unicode, inclusive acentos;
- evidência com path, heading, linhas, excerpt, hashes BLAKE3, score e termos encontrados;
- leitura direta dos documentos a cada busca: `docs/` continua sendo a fonte da verdade;
- `evaluate` com Hit@1, Recall@5 macro/micro, MRR@5, falsos positivos de no-answer, latência e
  volume de contexto.

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
usam o contrato `schema_version: 1`; erros são escritos em stderr e retornam exit code diferente
de zero. O contrato inclui:

- `engine`, `query`, `root` e resumo do corpus;
- `rank`, `path`, `heading`, `line_start`, `line_end` e `excerpt`;
- `file_hash`, `chunk_hash`, `score` e `matched_terms`.

As linhas são inclusivas e começam em 1. `heading` pode ser `null` para texto anterior ao primeiro
heading. `matched_terms` contém termos normalizados, sem acentos e sem stopwords comuns.

Para executar o benchmark versionado:

```bash
docs-search evaluate \
  --root evaluation/fixtures/stable-v1 \
  --queries evaluation/queries.json \
  --limit 5 \
  --max-excerpt-chars 1200 \
  --json
```

O relatório usa contrato independente `schema_version: 1`, formalizado em
`evaluation/report.schema.json`. Sem `--json`, o comando mostra um resumo humano. `--output
<arquivo>` grava explicitamente o relatório JSON; nada é persistido automaticamente. O comando
continua após falhas individuais, inclui o erro no caso correspondente e termina com status não
zero se alguma consulta falhar. Para relatórios versionados, execute com um `--root` relativo para
não registrar paths absolutos específicos da máquina.

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
`evaluation/fixtures/stable-v1` sustenta testes de regressão; o repositório real será medido
separadamente. A qualidade desse baseline deve ser medida antes de adicionar SQLite FTS5,
embeddings ou um classificador/router.
