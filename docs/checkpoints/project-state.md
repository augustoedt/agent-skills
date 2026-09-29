# Estado do projeto

## Estado atual

O repositório é a fonte oficial das skills próprias e agora também abriga
`tools/docs-search`, um binário Rust para recuperação documental independente de modelo.

O motor lexical permanece sem tuning, com chunking por headings, BM25, normalização Unicode,
contrato de busca JSON v1 e hashes BLAKE3. O código-fonte `docs-search` v0.2.1 inclui o subcomando
`evaluate`, relatório JSON v1 formalizado por schema e Hit@1, Recall@5 macro/micro, MRR@5,
falsos positivos de no-answer, latência monotônica e volume de contexto. As 20 consultas usam o
schema de avaliação v2; o parser mantém leitura do schema v1.

A fixture estável e versionada `evaluation/fixtures/stable-v1` separa regressão determinística do
corpus real. Um smoke test completo da fixture executou 20/20 consultas sem erro: Hit@1 0,705882,
Recall@5 macro 0,617647, Recall@5 micro 0,619048, MRR@5 0,705882, zero falsos positivos em três
casos no-answer e 7.453 caracteres de contexto. `fmt`, `clippy` e 37 testes passam; o relatório
gerado também passa em `evaluation/report.schema.json`.

O primeiro baseline do corpus real foi preservado em
`tools/docs-search/evaluation/reports/agent-skills-lexical-bm25-v1-baseline.json`, sobre o commit
fonte `de77945`: 7 arquivos, 87 chunks, 20/20 consultas sem erro, Hit@1 0,647059, Recall@5 macro
0,588235, Recall@5 micro 0,619048, MRR@5 0,647059, zero falsos positivos em três casos no-answer e
18.326 caracteres de contexto. Casos exatos tiveram Hit@1 0,875; semânticos, apenas 0,166667. A
análise detalhada e a política de preservação estão em
`tools/docs-search/evaluation/reports/README.md`.

Avaliações de checkouts privados são estado local da máquina: datasets, nomes, roots, headings,
fingerprints e relatórios ficam fora deste repositório público, em armazenamento local criptografado.
Um manifesto local associa IDs neutros aos paths disponíveis em cada computador. Seu contrato v1
está em `evaluation/local-manifest.schema.json`; o runner local bloqueia holdouts e não sobrescreve
relatórios. O Git mantém apenas fixtures sintéticas e corpora explicitamente públicos.

O hardening de fenced code no v0.2.1 impede comentários `#` dentro de blocos com crases/tils de
virarem headings. No corpus público real, chunks caíram de 87 para 86 e headings esperados encontrados
subiram de 4/17 para 9/17, sem alterar Hit@1, Recall@5, MRR@5 ou falsos positivos; contexto passou de
18.326 para 18.927 caracteres. A fixture estável manteve todas as métricas e 7.453 caracteres. Os
corpora privados de desenvolvimento também foram medidos localmente sem regressão nas métricas
primárias; nenhum holdout foi executado e os relatórios ficaram fora do Git.

O binário instalado via Rust 1.98.1 gerenciado por `asdf` continua na versão 0.1.0; ele não foi
atualizado automaticamente. O v0.2.0 pode ser executado no checkout com `cargo run -- evaluate` até
uma instalação ser solicitada.

A skill `search-project-docs` também está sincronizada em `~/.agents/skills/` e ligada aos agentes
locais detectados. O plano detalhado em `docs/plans/docs-search.md` agora define schema de avaliação,
métricas, relatórios, testes, gates e critérios para SQLite/FTS5, embeddings/RRF e distribuição.

## Em andamento

- ampliar casos ambíguos e no-answer sem ajustar o dataset para favorecer o motor atual;
- preparar a medição local de outros corpora autorizados, sem versionar informação privada.

## Próximo passo

Medir uma estratégia genérica de diversidade por path para evitar que chunks repetidos do mesmo
arquivo ocupem o top 5. Executar primeiro na fixture e nos corpora de desenvolvimento; holdouts
continuam bloqueados até existir uma mudança candidata.

## Armadilhas conhecidas

- `skills/` é fonte da verdade; não editar cópias em `~/.agents/skills/`.
- `docs-search` ainda não possui índice persistente, FTS5 nem embeddings.
- O corpus padrão exclui `skills/**`; por isso avaliações devem apontar para documentação em
  `README.md` ou `docs/`, não apenas para conteúdo de `SKILL.md`.
- Resultado lexical vazio não prova ausência da informação.
- Nunca versionar nomes, paths, queries, headings, fingerprints ou relatórios derivados de projetos
  privados; nem paths absolutos, porque a configuração muda entre computadores.
- O baseline v0.2.0 preserva a regressão histórica de headings dentro de fenced code; não o
  sobrescrever. O v0.2.1 corrige o parser e novos relatórios devem identificar essa versão.
- Runbook e `skills/search-project-docs/SKILL.md` descrevem o mesmo processo e devem mudar juntos.
