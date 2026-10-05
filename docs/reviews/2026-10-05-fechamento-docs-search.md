# Review — fechamento do docs-search 0.6.0

> **Data:** 2026-10-05. Texto movido do checkpoint sem reescrita, para que o checkpoint volte a ser
> um snapshot. Nenhum fato foi removido; o plano faseado completo está em
> [`../archive/plano-docs-search.md`](../archive/plano-docs-search.md).

## Relações

- **Decidido por:** [ADR 0001](../decisions/0001-docs-search-como-binario-local.md),
  [ADR 0002](../decisions/0002-manter-cap-1-opt-in-apos-holdout.md),
  [ADR 0003](../decisions/0003-comparar-engines-antes-da-adocao.md),
  [ADR 0004](../decisions/0004-isolar-adapters-rejeitados-da-build-de-produto.md)
- **Implementa:** [plano arquivado](../archive/plano-docs-search.md)

## Resultado

`docs-search` 0.6.0 estável com `lexical-bm25-v1` como única engine de produto. Cap 1 ficou opt-in;
o bake-off terminou em `lexical-retained`; os adapters rejeitados ficaram isolados em
`experimental-adapters`.

## Histórico de fases e métricas

O repositório é a fonte oficial das skills próprias e agora também abriga
`tools/docs-search`, um binário Rust para recuperação documental independente de modelo.

O motor lexical preserva o ranking BM25 original como default, com chunking por headings,
normalização Unicode e hashes BLAKE3. O código-fonte `docs-search` v0.6.0 aceita somente os
contratos atuais: resposta de busca v2, dataset de consultas v2 e relatório v2. A resposta registra
`selection.max_results_per_path`, rank final e rank BM25 bruto; versões de schema diferentes são
rejeitadas em vez de manter compatibilidade legada.

A fixture estável e versionada `evaluation/fixtures/stable-v1` separa regressão determinística do
corpus real. Um smoke test completo da fixture executou 20/20 consultas sem erro: Hit@1 0,705882,
Recall@5 macro 0,617647, Recall@5 micro 0,619048, MRR@5 0,705882, zero falsos positivos em três
casos no-answer e 7.453 caracteres de contexto. Com cap 1, Recall@5 macro/micro subiu para
0,676471/0,714286 e o contexto caiu para 3.995 caracteres, sem alterar Hit@1, MRR ou no-answer.
`fmt`, Clippy e as duas matrizes passam: 47 testes no produto default e 104 com o arquivo
experimental habilitado; busca v2 usa `evaluation/search-response.schema.json` e
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

O binário global gerenciado por `asdf` está na versão estável 0.6.0. A atualização foi explicitamente
autorizada e usa a build de produto com `--no-default-features`; `bakeoff`, `holdout` e as
dependências dos adapters rejeitados não fazem parte da instalação.

A skill `search-project-docs` também está sincronizada em `~/.agents/skills/` e ligada aos agentes
locais detectados. O plano concluído em `docs/plans/docs-search.md` documenta o bake-off de cinco
braços: BM25 direto, cache SQLite com BM25 preservado, FTS5, embeddings locais e
híbrido por RRF. A decisão está no ADR 0003; o harness e os adapters `lexical-bm25-v1`,
`sqlite-cache-bm25-v1`, `fts5-v1`, `local-embeddings-v1` e `hybrid-rrf-v1` estão implementados. A
implementação dos adapters está completa; nenhum deles foi promovido ao produto. Os quatro
adapters rejeitados e os harnesses agora estão isolados pela feature não default
`experimental-adapters`.

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
da ordem contrabalanceada. Os cinco adapters continuam disponíveis somente no harness compilado com
`experimental-adapters`; o default e o binário global permanecem inalterados.

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

A Fase 5.1 congelou o novo gate inicialmente fechado. Novos corpora reproduzíveis foram extraídos
de objetos Git, os julgamentos foram revisados diretamente contra os Markdown sem ranking e o
protocolo one-shot congelou hashes, fingerprints, ordem, finalistas, limites absolutos e decisão. O
contrato público ativo é `evaluation/engine-holdout-protocol-v2.schema.json`; identidades,
consultas, roots e artefatos permanecem privados. O v1 foi fechado e supersedido antes de qualquer
implementação porque não fixava o hash da configuração externa das engines; v2 incorpora
configuração e hash. Nenhuma finalista foi executada nesses holdouts.

A Fase 5.2 também está concluída sem abrir o gate. O build experimental contém `holdout observe`,
`holdout finalize` e `holdout decide`, com autorização hash-bound, sequência exata, somente as duas
finalistas, revalidação sem segunda execução de retrieval e saída imutável. O coordenador privado
nega paths consumidos antes de resolução, exige roots exatas, grava uma tentativa append-only antes
do processo e bloqueia rerun inclusive após falha. Contratos públicos independentes formalizam
autorização, observação, relatório e decisão. Testes sintéticos cobrem as duas engines, gate fechado, relatório,
decisão, ordem, overwrite e tentativa consumida. O registry operacional continua fechado, com
medições desabilitadas; naquele marco ainda não existiam artefatos reais dos novos holdouts.

A Fase 5.3 foi autorizada e concluída em quatro tentativas, uma por engine/input, na ordem
congelada. BM25 direto e SQLite/BM25 produziram evidência, métricas e contexto exatamente iguais,
sem erros de consulta ou falsos positivos `no_answer`; SQLite também passou a guarda operacional de
latência. As duas engines, porém, falharam os pisos absolutos de qualidade nos novos inputs. Como o
gate é eliminatório, a decisão final é `lexical-retained`: SQLite não avança nem como opt-in,
`lexical-bm25-v1` permanece sozinho e os holdouts estão consumidos sem possibilidade de rerun ou
retuning.

A Etapa 6 foi concluída. A build default usa features vazias e não compila nem expõe SQLite/BM25,
FTS5, E5, RRF, Candle, Tokenizers, SQLite bundled, `bakeoff` ou `holdout`. A build explícita com
`--all-features` preserva os 104 testes históricos; o produto default passa 47 testes. Uma medição
release local reduziu o binário de aproximadamente 10,96 MB para 3,09 MB. Instalação limpa,
atualização 0.1.0 → 0.6.0, rollback para 0.1.0 e nova atualização para 0.6.0 passaram em roots
isoladas antes da atualização global.
