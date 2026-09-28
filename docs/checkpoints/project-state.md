# Estado do projeto

## Estado atual

O repositório é a fonte oficial das skills próprias e agora também abriga
`tools/docs-search`, um binário Rust para recuperação documental independente de modelo.

O baseline lexical v0.1.0 está implementado com chunking por headings, BM25, normalização
Unicode, contrato JSON v1, hashes BLAKE3 e 20 consultas de avaliação. `fmt`, `clippy` e 11 testes
passam. O binário foi instalado no Rust 1.98.1 gerenciado por `asdf` e uma consulta real retornou
evidência válida deste repositório.

A skill `search-project-docs` também está sincronizada em `~/.agents/skills/` e ligada aos agentes
locais detectados.

## Em andamento

- criar o executor das consultas de avaliação;
- medir qualidade, latência e volume de contexto do baseline.

## Próximo passo

Executar `tools/docs-search/evaluation/queries.json` e registrar Hit@1, Recall@5, MRR, falsos
positivos de no-answer, latência e caracteres retornados.

## Armadilhas conhecidas

- `skills/` é fonte da verdade; não editar cópias em `~/.agents/skills/`.
- `docs-search` ainda não possui índice persistente, FTS5 nem embeddings.
- O corpus padrão exclui `skills/**`; por isso avaliações devem apontar para documentação em
  `README.md` ou `docs/`, não apenas para conteúdo de `SKILL.md`.
- Resultado lexical vazio não prova ausência da informação.
- Runbook e `skills/search-project-docs/SKILL.md` descrevem o mesmo processo e devem mudar juntos.
