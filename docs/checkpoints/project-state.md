# Estado do projeto

## Estado atual

O repositório é a fonte oficial das skills próprias e agora também abriga
`tools/docs-search`, um binário Rust para recuperação documental independente de modelo.

O motor lexical preserva o ranking BM25 original como default, com chunking por headings,
normalização Unicode e hashes BLAKE3. O código-fonte `docs-search` v0.4.0 publica resposta de busca
JSON v2 com `selection.max_results_per_path`, rank final e rank BM25 bruto. A quebra foi direta, sem
arquivar o contrato de busca v1, porque não havia consumidores. O relatório de avaliação permanece
em v2; seu schema v1 continua arquivado somente para validar baselines históricos. As 20 consultas
usam o schema de dataset v2; o parser mantém leitura do dataset v1.

A fixture estável e versionada `evaluation/fixtures/stable-v1` separa regressão determinística do
corpus real. Um smoke test completo da fixture executou 20/20 consultas sem erro: Hit@1 0,705882,
Recall@5 macro 0,617647, Recall@5 micro 0,619048, MRR@5 0,705882, zero falsos positivos em três
casos no-answer e 7.453 caracteres de contexto. Com cap 1, Recall@5 macro/micro subiu para
0,676471/0,714286 e o contexto caiu para 3.995 caracteres, sem alterar Hit@1, MRR ou no-answer.
`fmt`, `clippy` e 44 testes passam; busca v2 usa `evaluation/search-response.schema.json` e
relatórios v2 usam `evaluation/report.schema.json`.

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

O experimento público de diversidade comparou ilimitado, cap 1 e cap 2 na fixture e no corpus do
próprio repositório. Cap 1 melhorou Recall@5 na fixture, manteve a qualidade no corpus público e usou
menos contexto que cap 2 nos dois. Avaliações adicionais ficaram integralmente no ambiente privado:
quantidade de corpora, métricas e direção dos resultados não foram versionadas. Cap 1 é candidato,
ainda opt-in; nenhum holdout foi executado.

A apresentação editorial `docs/apresentacoes/docs-search-arquitetura-e-avaliacao.html` explica o
fluxo de busca, contrato de evidência, avaliação, privacidade, manifesto local e roadmap sem expor
nomes ou paths privados.

O binário instalado via Rust 1.98.1 gerenciado por `asdf` continua na versão 0.1.0; ele não foi
atualizado automaticamente. O v0.4.0 pode ser executado no checkout com `cargo run -- evaluate` até
uma instalação ser solicitada.

A skill `search-project-docs` também está sincronizada em `~/.agents/skills/` e ligada aos agentes
locais detectados. O plano detalhado em `docs/plans/docs-search.md` agora define schema de avaliação,
métricas, relatórios, testes, gates e critérios para SQLite/FTS5, embeddings/RRF e distribuição.

## Em andamento

- revisar os julgamentos humanos pendentes de um dataset local sem observar rankings;
- manter cap 1 como candidato opt-in enquanto o gate não for concluído;
- ampliar casos ambíguos e no-answer sem ajustar o dataset para favorecer o motor atual.

## Próximo passo

Concluir a revisão humana local, congelar código, configuração e datasets e então decidir
explicitamente a abertura do gate de holdout para cap 1. Holdouts continuam bloqueados até essa
decisão; cap 1 não deve virar default antes da validação de generalização.

## Armadilhas conhecidas

- `skills/` é fonte da verdade; não editar cópias em `~/.agents/skills/`.
- `docs-search` ainda não possui índice persistente, FTS5 nem embeddings.
- O corpus padrão exclui `skills/**`; por isso avaliações devem apontar para documentação em
  `README.md` ou `docs/`, não apenas para conteúdo de `SKILL.md`.
- Resultado lexical vazio não prova ausência da informação.
- Nunca versionar nomes, paths, queries, headings, fingerprints ou relatórios derivados de projetos
  privados; nem paths absolutos, porque a configuração muda entre computadores.
- O baseline v0.2.0 preserva a regressão histórica de headings dentro de fenced code; não o
  sobrescrever. O v0.2.1 corrige o parser; relatórios v2 de diversidade identificam o v0.3.0 e a
  resposta de busca autocontida entra no v0.4.0.
- Runbook e `skills/search-project-docs/SKILL.md` descrevem o mesmo processo e devem mudar juntos.
