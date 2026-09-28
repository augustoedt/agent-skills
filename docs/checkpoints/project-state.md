# Estado do projeto

## Estado atual

O repositório é a fonte oficial das skills próprias e agora também abriga
`tools/docs-search`, um binário Rust para recuperação documental independente de modelo.

O motor lexical permanece sem tuning, com chunking por headings, BM25, normalização Unicode,
contrato de busca JSON v1 e hashes BLAKE3. O código-fonte `docs-search` v0.2.0 adiciona o subcomando
`evaluate`, relatório JSON v1 formalizado por schema e Hit@1, Recall@5 macro/micro, MRR@5,
falsos positivos de no-answer, latência monotônica e volume de contexto. As 20 consultas usam o
schema de avaliação v2; o parser mantém leitura do schema v1.

A fixture estável e versionada `evaluation/fixtures/stable-v1` separa regressão determinística do
corpus real. Um smoke test completo da fixture executou 20/20 consultas sem erro: Hit@1 0,705882,
Recall@5 macro 0,617647, Recall@5 micro 0,619048, MRR@5 0,705882, zero falsos positivos em três
casos no-answer e 7.453 caracteres de contexto. `fmt`, `clippy` e 30 testes passam; o relatório
gerado também passa em `evaluation/report.schema.json`.

O primeiro baseline do corpus real foi preservado em
`tools/docs-search/evaluation/reports/agent-skills-lexical-bm25-v1-baseline.json`, sobre o commit
fonte `de77945`: 7 arquivos, 87 chunks, 20/20 consultas sem erro, Hit@1 0,647059, Recall@5 macro
0,588235, Recall@5 micro 0,619048, MRR@5 0,647059, zero falsos positivos em três casos no-answer e
18.326 caracteres de contexto. Casos exatos tiveram Hit@1 0,875; semânticos, apenas 0,166667. A
análise detalhada e a política de preservação estão em
`tools/docs-search/evaluation/reports/README.md`.

O binário instalado via Rust 1.98.1 gerenciado por `asdf` continua na versão 0.1.0; ele não foi
atualizado automaticamente. O v0.2.0 pode ser executado no checkout com `cargo run -- evaluate` até
uma instalação ser solicitada.

A skill `search-project-docs` também está sincronizada em `~/.agents/skills/` e ligada aos agentes
locais detectados. O plano detalhado em `docs/plans/docs-search.md` agora define schema de avaliação,
métricas, relatórios, testes, gates e critérios para SQLite/FTS5, embeddings/RRF e distribuição.

## Em andamento

- ampliar casos ambíguos e no-answer sem ajustar o dataset para favorecer o motor atual;
- preparar a medição de um segundo corpus documental autorizado.

## Próximo passo

Adicionar casos humanos revisáveis de ambiguidade, abstention, flexão e consultas parcialmente fora
do corpus; depois medir novamente o baseline original e selecionar um segundo corpus antes de
considerar tuning lexical, SQLite ou embeddings.

## Armadilhas conhecidas

- `skills/` é fonte da verdade; não editar cópias em `~/.agents/skills/`.
- `docs-search` ainda não possui índice persistente, FTS5 nem embeddings.
- O corpus padrão exclui `skills/**`; por isso avaliações devem apontar para documentação em
  `README.md` ou `docs/`, não apenas para conteúdo de `SKILL.md`.
- Resultado lexical vazio não prova ausência da informação.
- O chunker atual trata uma linha `# ...` dentro de bloco de código como heading. O baseline tornou
  a regressão visível nos breadcrumbs do `README.md`: somente 4 de 17 consultas respondíveis
  recuperaram um heading esperado. A fixture também contém um caso dedicado para o hardening
  posterior.
- Runbook e `skills/search-project-docs/SKILL.md` descrevem o mesmo processo e devem mudar juntos.
