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

O binário instalado via Rust 1.98.1 gerenciado por `asdf` continua na versão 0.1.0; ele não foi
atualizado automaticamente. O v0.2.0 pode ser executado no checkout com `cargo run -- evaluate` até
uma instalação ser solicitada.

A skill `search-project-docs` também está sincronizada em `~/.agents/skills/` e ligada aos agentes
locais detectados. O plano detalhado em `docs/plans/docs-search.md` agora define schema de avaliação,
métricas, relatórios, testes, gates e critérios para SQLite/FTS5, embeddings/RRF e distribuição.

## Em andamento

- medir o corpus real sem alterar o motor `lexical-bm25-v1`;
- registrar o relatório reproduzível e interpretar os misses por consulta.

## Próximo passo

Executar `tools/docs-search/evaluation/queries.json` contra a raiz real de `agent-skills`, usando
path relativo, e registrar Hit@1, Recall@5, MRR, falsos positivos de no-answer, latência e caracteres
retornados antes de qualquer tuning lexical.

## Armadilhas conhecidas

- `skills/` é fonte da verdade; não editar cópias em `~/.agents/skills/`.
- `docs-search` ainda não possui índice persistente, FTS5 nem embeddings.
- O corpus padrão exclui `skills/**`; por isso avaliações devem apontar para documentação em
  `README.md` ou `docs/`, não apenas para conteúdo de `SKILL.md`.
- Resultado lexical vazio não prova ausência da informação.
- O chunker atual trata uma linha `# ...` dentro de bloco de código como heading. O dataset v2
  preserva os headings corretos do corpus real para tornar a regressão visível no futuro runner; a
  fixture também contém um caso dedicado para o hardening posterior.
- Runbook e `skills/search-project-docs/SKILL.md` descrevem o mesmo processo e devem mudar juntos.
