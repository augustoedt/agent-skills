# Estado do projeto

## Estado atual

O repositório é a fonte oficial das skills próprias e agora também abriga
`tools/docs-search`, um binário Rust para recuperação documental independente de modelo.

O motor lexical preserva o ranking BM25 original como default, com chunking por headings,
normalização Unicode e hashes BLAKE3. O código-fonte `docs-search` v0.6.0-alpha.6 aceita somente os
contratos atuais: resposta de busca v2, dataset de consultas v2 e relatório v2. A resposta registra
`selection.max_results_per_path`, rank final e rank BM25 bruto; versões de schema diferentes são
rejeitadas em vez de manter compatibilidade legada.

A fixture estável e versionada `evaluation/fixtures/stable-v1` separa regressão determinística do
corpus real. Um smoke test completo da fixture executou 20/20 consultas sem erro: Hit@1 0,705882,
Recall@5 macro 0,617647, Recall@5 micro 0,619048, MRR@5 0,705882, zero falsos positivos em três
casos no-answer e 7.453 caracteres de contexto. Com cap 1, Recall@5 macro/micro subiu para
0,676471/0,714286 e o contexto caiu para 3.995 caracteres, sem alterar Hit@1, MRR ou no-answer.
`fmt`, `clippy` e 102 testes passam; busca v2 usa `evaluation/search-response.schema.json` e
relatórios v2 usam `evaluation/report.schema.json`. As medições públicas atuais ficam em
`tools/docs-search/evaluation/reports/` e substituem artefatos históricos incompatíveis.

Avaliações de checkouts privados são estado local da máquina: datasets, nomes, roots, headings,
fingerprints e relatórios ficam fora deste repositório público, em armazenamento local criptografado.
Um manifesto local associa IDs neutros aos paths disponíveis em cada computador. Seu contrato v1
está em `evaluation/local-manifest.schema.json`; o runner local não sobrescreve relatórios e só
libera um holdout mediante gate explícito, congelado e de execução única. O Git mantém apenas
fixtures sintéticas e corpora explicitamente públicos.

O chunker ignora comentários `#` dentro de fenced code com crases ou tils, incluindo cercas
indentadas, fechamentos compatíveis e blocos não encerrados. A fixture cobre esses casos. Avaliações
privadas permanecem fora do Git; cada holdout congelado foi executado uma única vez.

O experimento público de diversidade comparou ilimitado, cap 1 e cap 2 na fixture e no corpus do
próprio repositório. Cap 1 melhorou Recall@5 na fixture, manteve a qualidade no corpus público e usou
menos contexto que cap 2 nos dois. Avaliações adicionais ficaram integralmente no ambiente privado:
identidades, quantidade de corpora, métricas detalhadas e diagnósticos por consulta não foram
versionados. Os julgamentos de desenvolvimento e holdout foram revisados diretamente contra as
fontes; o candidato cap 1 foi congelado com commit, parâmetros, revisões e checksums. O gate executou
cada holdout uma vez e fechou automaticamente. O conjunto falhou os pisos de qualidade predefinidos,
portanto cap 1 não será promovido a default e continua somente opt-in.

A apresentação editorial `docs/apresentacoes/docs-search-arquitetura-e-avaliacao.html` explica o
escopo, fluxo de busca, contrato de evidência, avaliação, privacidade, decisões de no-go, plano do
bake-off e estado operacional atual sem expor nomes ou paths privados.

O binário instalado via Rust 1.98.1 gerenciado por `asdf` continua na versão 0.1.0; ele não foi
atualizado automaticamente. O v0.6.0-alpha.6 pode ser executado no checkout com `cargo run --
evaluate` até uma instalação ser solicitada.

A skill `search-project-docs` também está sincronizada em `~/.agents/skills/` e ligada aos agentes
locais detectados. O plano detalhado em `docs/plans/docs-search.md` divide o próximo ciclo em um
bake-off de cinco braços: BM25 direto, cache SQLite com BM25 preservado, FTS5, embeddings locais e
híbrido por RRF. A decisão está no ADR 0003; o harness e os adapters `lexical-bm25-v1`,
`sqlite-cache-bm25-v1`, `fts5-v1`, `local-embeddings-v1` e `hybrid-rrf-v1` estão implementados. A
implementação dos adapters está completa; nenhum deles foi promovido ao produto.

A Fase 4.0 está concluída e congelada. Os contratos públicos
`engine-bakeoff-protocol.schema.json` e `engine-bakeoff-report.schema.json` separam a
pré-inscrição dos resultados observados. A instância privada fixa snapshots derivados de revisões
Git, datasets e baselines, configurações das cinco engines, modelo E5 e hashes, duas rodadas,
medições, budgets eliminatórios e seleção por Pareto. O registry local e seu verificador conferem
hashes, conjunto de snapshots e permissões. Uma denylist ativa bloqueia os holdouts consumidos antes
da resolução de seus paths no runner.

A Fase 4.1 está concluída. O harness em duas etapas (`bakeoff observe` e `bakeoff finalize`) registra
timings internos, qualidade, contexto, evidência, determinismo, fault injection e budgets. O runner
privado, fixado por registry próprio, mede startup, end-to-end, RSS e disco, atesta release limpo,
`Cargo.lock`, toolchain, host, ambiente e denylist e valida o relatório operacional v1. Observações e relatórios recusam overwrite e usam
`0600`. Duas observações sintéticas pareadas passaram; nenhuma medição congelada foi executada fora
da ordem contrabalanceada. Os cinco adapters estão disponíveis no harness; o default e o binário
global permanecem inalterados.

As Fases 4.2–4.5 estão concluídas em código e testes. O cache SQLite/BM25 e o índice FTS5 usam
`rusqlite 0.40.2` e SQLite bundled 3.53.2. FTS5 adiciona query literal segura, pesos 1/2/3 e top 50.
O adapter E5 usa Candle CPU 0.9.1 e Tokenizers 0.21.1, artefatos locais verificados, 384 dimensões,
512 tokens, prefixos E5, mean pooling pela attention mask, L2, scan exato, top 50 e threshold 0,80.
Os índices privados são atômicos e reconstruíveis; add/modify/rename/remove são comparados com
rebuild limpo. Corrupção é recuperada e embeddings falham fechados quando o modelo está ausente ou
o rebuild é forçado a falhar, sem rede nem fallback lexical. Um fluxo sintético pareado com o modelo
real validou schema, determinismo, evidência, `source_ranks.embedding`, quatro updates sem stale e
fault injection; seu status foi `fail` por `no_answer` e contexto, resultado que não autoriza tuning
nem adoção. O híbrido usa top 50 de FTS5 e E5, RRF `k = 60`, deduplicação por `chunk_hash`, evidência
determinística e `source_ranks` das duas fontes. O harness soma candidatos, timings e bytes dos dois
índices; qualquer erro E5 falha fechado sem devolver resultados FTS5 parciais. Testes sintéticos
cobrem fusão, ranks, duplicatas, ordenação, abstention, propagação de erro e métricas incrementais.
Um smoke pareado com o modelo real confirmou schema, determinismo, `source_ranks` duplos, quatro
updates sem stale e fault injection; o status `fail` por `no_answer` e contexto valida as guardas e
não autoriza tuning.

A Fase 4.6 também está concluída. As cinco engines foram executadas duas vezes sobre os cinco inputs
de development, produzindo 50 observações, 50 medições e 50 relatórios privados imutáveis. Todos os
pares foram determinísticos, sem erros de consulta e com isolamento de holdout válido. Apenas
`lexical-bm25-v1` e `sqlite-cache-bm25-v1` passaram todas as guardas em todos os inputs e runs. O
cache SQLite preservou exatamente qualidade e contexto do baseline e entregou ganho significativo
de latência de consulta, em troca de startup, memória e disco maiores; por isso os dois formam a
fronteira de Pareto. FTS5, E5 e RRF foram eliminados pelas guardas congeladas, sem retuning. Nenhum
holdout foi usado.

A Fase 5.1 está concluída e o novo gate continua fechado. Novos corpora reproduzíveis foram extraídos
de objetos Git, os julgamentos foram revisados diretamente contra os Markdown sem ranking e o
protocolo one-shot congelou hashes, fingerprints, ordem, finalistas, limites absolutos e decisão. O
contrato público ativo é `evaluation/engine-holdout-protocol-v2.schema.json`; identidades,
consultas, roots e artefatos permanecem privados. O v1 foi fechado e supersedido antes de qualquer
implementação porque não fixava o hash da configuração externa das engines; v2 incorpora
configuração e hash. Nenhuma finalista foi executada nesses holdouts.

## Em andamento

- manter cap 1 apenas como opção explícita, sem alterar o default;
- preservar o freeze e os 50 relatórios privados imutáveis da Fase 4.6;
- preservar a rejeição do prefixo morfológico 7/4 sem retuning de parâmetros;
- manter `lexical-bm25-v1` e `sqlite-cache-bm25-v1` como as duas finalistas de development;
- não usar os holdouts consumidos para seleção, tuning ou mudança de julgamentos;
- preservar o novo protocolo de holdout fechado e seus datasets imutáveis;
- não integrar SQLite antes de concluir o gate novo e one-shot.

## Próximo passo

1. Implementar os contratos e o harness one-shot sem abrir o gate.
2. Validar com fixture sintética, auditar hashes, ordem, recusa de rerun e isolamento.
3. Congelar um registry com medições desabilitadas e pedir autorização explícita para abri-lo.
4. Executar cada finalista uma única vez e registrar a decisão: SQLite opt-in ou manutenção do BM25 direto.

O default continua `lexical-bm25-v1`. O plano faseado está no [`plano`](../plans/docs-search.md), a
comparação foi aprovada no [`ADR 0003`](../decisions/0003-comparar-engines-antes-da-adocao.md) e a
decisão anterior sobre cap 1 permanece no [`ADR 0002`](../decisions/0002-manter-cap-1-opt-in-apos-holdout.md).

## Armadilhas conhecidas

- `skills/` é fonte da verdade; não editar cópias em `~/.agents/skills/`.
- Os índices SQLite/BM25, FTS5, E5 e a composição RRF existem somente no bake-off; a busca padrão ainda não possui índice persistente nem embeddings.
- Não alterar tokenizer, threshold 0,80, profundidade 50, RRF `k = 60`, modelo, inputs ou budgets
  dentro de `engine-bakeoff-v1`; qualquer mudança exige protocolo e tag novos.
- Não abrir, executar ou editar os novos holdouts durante a implementação do harness one-shot;
  qualquer mudança de julgamentos, parâmetros, ordem ou budgets invalida `engine-finalists-holdout-v2`.
- O corpus padrão exclui `skills/**`; por isso avaliações devem apontar para documentação em
  `README.md` ou `docs/`, não apenas para conteúdo de `SKILL.md`.
- Resultado lexical vazio não prova ausência da informação.
- Nunca versionar nomes, paths, queries, headings, fingerprints ou relatórios derivados de projetos
  privados; nem paths absolutos, porque a configuração muda entre computadores.
- Runbook e `skills/search-project-docs/SKILL.md` descrevem o mesmo processo e devem mudar juntos.
