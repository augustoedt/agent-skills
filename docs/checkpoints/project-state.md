# Estado do projeto

> **Atualizado em 2026-10-05.** Checkpoint autoritativo e snapshot, não diário: ao retomar, comece
> por aqui e pelo [`índice`](../README.md). O histórico do `docs-search` está na
> [`review de fechamento`](../reviews/2026-10-05-fechamento-docs-search.md).

## Onde estamos

- Repositório público com a fonte oficial das skills próprias em `skills/`. `scripts/install.sh`
  sincroniza cada skill para `~/.agents/skills/` e cria symlinks nos agentes detectados (pi, Claude
  Code, Codex, Grok, Copilot CLI, Cursor). Skills de terceiro ficam fora do repo, catalogadas em
  `third-party.json`.
- `tools/docs-search` está **concluído e estável na versão 0.6.0** (tag `docs-search-v0.6.0`), com
  instalação global via `asdf` usando a build de produto (`default = []`):
  - engine única `lexical-bm25-v1`, comandos `search` e `evaluate`, contrato JSON v2;
  - diversidade por path (`--max-results-per-path`) existe só como opt-in
    ([ADR 0002](../decisions/0002-manter-cap-1-opt-in-apos-holdout.md));
  - o bake-off de cinco engines terminou em `lexical-retained`; SQLite/BM25, FTS5, E5 e RRF ficaram
    isolados na feature não default `experimental-adapters`
    ([ADR 0003](../decisions/0003-comparar-engines-antes-da-adocao.md),
    [ADR 0004](../decisions/0004-isolar-adapters-rejeitados-da-build-de-produto.md));
  - uso real tem sido raro (relato do usuário em 2026-10-05).
- A skill `search-project-docs` é a camada de instrução sobre o binário e está sincronizada nos
  agentes.
- A skill `docs-organization` foi revisada em 2026-10-05 com lições de uso em dois projetos
  (roteamento por tarefa, runbooks verificáveis, estado fora do Git, docs no commit da mudança,
  resumo de plano, ADR com "Vigente hoje"/"Como verificar", branches de demonstração e teste de
  retomada). Esta documentação foi reorganizada no mesmo padrão.
- Medições públicas atuais em `tools/docs-search/evaluation/reports/`; avaliações de projetos
  privados ficam fora do Git, em armazenamento local criptografado.

## Em andamento ⚠️

Nada em andamento.

## Próximo passo

O `docs-search` está concluído. Trabalho futuro é opcional:

1. Monitorar o uso real do baseline lexical e regressões da fixture pública.
2. Só formular outra hipótese de busca com justificativa independente em development e novos
   holdouts (ver ADRs 0002 e 0003).
3. Considerar binários pré-compilados apenas se a instalação por Cargo se tornar uma barreira medida.

## Armadilhas conhecidas

- `skills/` é fonte da verdade; não editar cópias em `~/.agents/skills/`.
- Manter `lexical-bm25-v1` como única engine do produto e default; cap 1 apenas como opção explícita.
- Os índices SQLite/BM25, FTS5, E5 e a composição RRF existem somente com a feature não default
  `experimental-adapters`; ela nunca entra na instalação ou distribuição normal, e a busca padrão
  não possui índice persistente nem embeddings. Não integrar SQLite/BM25: o gate final rejeitou sua
  promoção.
- Não execute builds Cargo com conjuntos de features diferentes em paralelo no mesmo `target/`;
  rode as matrizes sequencialmente ou use `CARGO_TARGET_DIR` distintos.
- Não alterar tokenizer, threshold 0,80, profundidade 50, RRF `k = 60`, modelo, inputs ou budgets
  dentro de `engine-bakeoff-v1`; qualquer mudança exige protocolo e tag novos.
- Holdouts consumidos (gate do cap 1 e Fase 5.3) não podem ser reabertos, rerodados, editados nem
  usados para seleção, tuning ou mudança de julgamentos; nova hipótese exige outros holdouts e outro
  protocolo/tag. Preservar o freeze, os 50 relatórios privados imutáveis da Fase 4.6, o protocolo,
  as tentativas e a decisão one-shot, e a rejeição do prefixo morfológico 7/4 sem retuning.
- O corpus padrão exclui `skills/**`; por isso avaliações devem apontar para documentação em
  `README.md` ou `docs/`, não apenas para conteúdo de `SKILL.md`.
- Resultado lexical vazio não prova ausência da informação.
- Nunca versionar nomes, paths, queries, headings, fingerprints ou relatórios derivados de projetos
  privados; nem paths absolutos, porque a configuração muda entre computadores.
- Runbook e `skills/search-project-docs/SKILL.md` descrevem o mesmo processo e devem mudar juntos.

## Referências

- **Índice:** [`docs/README.md`](../README.md)
- **Verificado em:** [review de fechamento do docs-search](../reviews/2026-10-05-fechamento-docs-search.md)
- **Decidido por:** [ADRs](../decisions/) 0001–0004
- **Runbook:** [sincronizar skills](../runbooks/sincronizar-e-deduplicar-skills.md) ·
  [buscar documentação](../runbooks/buscar-documentacao-de-projetos.md)
- **Histórico:** [plano do docs-search arquivado](../archive/plano-docs-search.md)
