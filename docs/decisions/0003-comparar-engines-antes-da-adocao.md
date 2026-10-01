# ADR — comparar engines antes da adoção

- **Data**: 2026-09-29
- **Status**: vigente

## Contexto

O baseline `lexical-bm25-v1` é estável e dois candidatos incrementais já foram avaliados: diversidade
cap 1 e prefixo morfológico 7/4. O primeiro não generalizou no gate de holdout e o segundo falhou
as guardas de development. Continuar alterando pequenos parâmetros lexicais não permite comparar o
custo e o benefício das principais arquiteturas possíveis.

SQLite, FTS5, embeddings e RRF resolvem problemas diferentes. Implementá-los como uma única feature
confundiria persistência, ranking lexical, recuperação vetorial e fusão. Integrar todos diretamente
ao produto também criaria custo permanente antes de existir um vencedor medido.

## Decisão

Executar um bake-off com protótipos de qualidade de benchmark, todos atrás do mesmo harness e sem
alterar o default durante a comparação. Serão medidos cinco braços:

1. leitura direta com `lexical-bm25-v1`, como baseline congelado;
2. cache SQLite preservando exatamente o mesmo BM25;
3. recuperação lexical por FTS5;
4. recuperação por embeddings locais;
5. FTS5 e embeddings combinados por RRF.

Corpus, chunking, datasets, julgamentos, limites e contrato de evidência serão iguais. Cada braço
terá configuração pré-registrada e duas execuções em development. A comparação incluirá qualidade,
`no_answer`, contexto, latência, startup, indexação, RAM e disco.

A Fase 4.0 foi congelada antes da implementação. O protocolo fixa SQLite bundled 3.53.2 via
`rusqlite 0.40.2`; FTS5 com tokenizer `unicode61 remove_diacritics 2`, pesos body/heading/path 1/2/3
e 50 candidatos; `intfloat/multilingual-e5-small` por revisão e hashes, com contrato E5, vetores de
384 dimensões, truncamento em 512 tokens, busca exata e threshold 0,80; e RRF com listas de 50 e
`k = 60`. Também fixa duas rodadas, instrumentação, budgets eliminatórios e seleção por Pareto sem
score agregado opaco.

O protocolo e o relatório operacional possuem schemas v1 independentes. A instância, snapshots,
modelo e checksums permanecem privados. O runner aplica uma denylist antes de resolver qualquer
path de holdout consumido, e um verificador local confere hashes e permissões do freeze.

Os holdouts já usados permanecem proibidos. No máximo duas variantes poderão avançar para um gate
de execução única com holdouts inteiramente novos. Somente a vencedora aprovada será integrada ao
produto; o resultado válido também pode ser manter o baseline atual.

A Fase 4.1 materializa essa decisão com uma fronteira interna de engine e dois comandos exclusivos
do runner privado: `bakeoff observe` e `bakeoff finalize`. O primeiro produz observações imutáveis;
o segundo revalida corpus, dataset, ranking, baseline, evidência, provenance e o par de runs antes
de produzir o relatório operacional v1. O runner atesta binário release, revisão Git, `Cargo.lock`,
host, ambiente de processo e denylist. A Fase 4.2 adicionou o adapter SQLite/BM25 com ranking
idêntico, índice atômico reconstruível, atualização incremental verificada, corrupção e fallback
explícito. A Fase 4.3 adicionou FTS5 com configuração ligada ao runtime, query literal segura,
projeção integral verificável e falha fechada. A Fase 4.4 adicionou E5 local com artefatos
verificados, Candle CPU, pooling/normalização congelados, scan exato, threshold 0,80, índice atômico
e falha fechada sem rede ou fallback lexical. A Fase 4.5 compôs FTS5 e E5 por RRF `k = 60`,
com deduplicação por `chunk_hash`, ranks das duas fontes, contabilidade conjunta dos índices e falha
fechada sem resultado FTS5 parcial quando E5 falha. Os cinco adapters estão implementados, mas isso
não altera o default nem implica adoção.

A Fase 4.6 executou a matriz congelada completa em development: cinco inputs, cinco engines e dois
runs, com 50 relatórios privados validados. BM25 direto e cache SQLite/BM25 passaram todas as
guardas; SQLite preservou exatamente qualidade e contexto e obteve ganho significativo de latência
de consulta, mas adicionou custo de startup, memória e disco. Os dois são não dominados e avançam
como finalistas. FTS5, embeddings e RRF falharam guardas eliminatórias e foram descartados sem
retuning. Nenhum holdout foi acessado; a decisão de produto continua condicionada a holdouts novos.

A Fase 5.1 reservou e congelou um conjunto inteiramente novo antes de qualquer ranking. Snapshots
vieram de objetos Git; julgamentos foram revisados contra a fonte; revisões, manifests, hashes,
fingerprints, ordem contrabalanceada, limites absolutos e a regra de decisão ficaram fixos no
protocolo `engine-finalists-holdout-v2`. O v1 foi preservado fechado e supersedido antes de
implementação porque não vinculava por hash a configuração externa das engines; v2 incorpora os
objetos congelados e seus hashes. SQLite só poderá avançar como opt-in se preservar evidência,
métricas e contexto exatamente, passar todas as guardas e repetir o ganho operacional congelado; o
default não muda neste gate.

A Fase 5.2 implementou o harness one-shot sem abrir esse gate. Autorização, observação, relatório e
decisão têm schemas independentes; o binário vincula protocolo, configurações, revisão, sequência e provenance,
e finaliza a observação sem executar retrieval novamente. O coordenador privado restringe roots,
nega holdouts consumidos antes de resolver paths, registra a tentativa antes do processo e recusa
overwrite, ordem incorreta e qualquer retry, inclusive após falha. O fluxo completo foi exercitado
somente em fixture sintética. O registry real permanece fechado, com medições desabilitadas e sem
artefatos dos novos holdouts; abri-lo exige autorização explícita e outro registry imutável.

A Fase 5.3 foi aberta por autorização explícita e consumiu exatamente as quatro tentativas na ordem
congelada. As finalistas foram equivalentes em evidência, métricas e contexto, sem erros de consulta
ou falsos positivos `no_answer`; SQLite passou a exigência operacional de latência. Contudo, ambas
falharam os pisos absolutos de qualidade nos dois novos inputs. Como qualquer guarda falha encerra o
gate, a decisão final é manter somente `lexical-bm25-v1`. SQLite/BM25 não avança como opt-in, os
holdouts estão consumidos e nenhum resultado poderá orientar retuning ou rerun.

## Consequências

- (+) armazenamento, ranking lexical, semântica e fusão serão avaliados separadamente;
- (+) alternativas dominadas poderão ser descartadas antes de receber integração de produção;
- (+) o baseline e os contratos atuais permanecem estáveis durante o experimento;
- (+) resultados negativos continuarão auditáveis;
- (+) parâmetros, budgets, inputs e contratos ficaram verificáveis antes da primeira engine;
- (+) o gate final preservou o default simples e rejeitou uma otimização que não generalizou nos
  pisos absolutos de qualidade;
- (−) o bake-off exige harness operacional e instrumentação de RAM, disco, startup e indexação;
- (−) embeddings adicionam dependência de modelo, licença, distribuição e custo computacional;
- ⚠️ parâmetros e limites devem ser congelados antes de observar os rankings;
- ⚠️ nenhuma variante pode ser promovida usando os holdouts consumidos;
- ⚠️ SQLite/BM25 está rejeitado para este ciclo, apesar do ganho operacional;
- ⚠️ implementar um protótipo não implica adoção nem manutenção futura.
