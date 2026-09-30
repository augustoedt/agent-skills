# Estado do projeto

## Estado atual

O repositório é a fonte oficial das skills próprias e agora também abriga
`tools/docs-search`, um binário Rust para recuperação documental independente de modelo.

O motor lexical preserva o ranking BM25 original como default, com chunking por headings,
normalização Unicode e hashes BLAKE3. O código-fonte `docs-search` v0.5.0 aceita somente os contratos
atuais: resposta de busca v2, dataset de consultas v2 e relatório v2. A resposta registra
`selection.max_results_per_path`, rank final e rank BM25 bruto; versões de schema diferentes são
rejeitadas em vez de manter compatibilidade legada.

A fixture estável e versionada `evaluation/fixtures/stable-v1` separa regressão determinística do
corpus real. Um smoke test completo da fixture executou 20/20 consultas sem erro: Hit@1 0,705882,
Recall@5 macro 0,617647, Recall@5 micro 0,619048, MRR@5 0,705882, zero falsos positivos em três
casos no-answer e 7.453 caracteres de contexto. Com cap 1, Recall@5 macro/micro subiu para
0,676471/0,714286 e o contexto caiu para 3.995 caracteres, sem alterar Hit@1, MRR ou no-answer.
`fmt`, `clippy` e 44 testes passam; busca v2 usa `evaluation/search-response.schema.json` e
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
atualizado automaticamente. O v0.5.0 pode ser executado no checkout com `cargo run -- evaluate` até
uma instalação ser solicitada.

A skill `search-project-docs` também está sincronizada em `~/.agents/skills/` e ligada aos agentes
locais detectados. O plano detalhado em `docs/plans/docs-search.md` divide o próximo ciclo em um
bake-off de cinco braços: BM25 direto, cache SQLite com BM25 preservado, FTS5, embeddings locais e
híbrido por RRF. A decisão está no ADR 0003; nenhum protótipo foi implementado ainda.

A Fase 4.0 está concluída e congelada. Os contratos públicos
`engine-bakeoff-protocol.schema.json` e `engine-bakeoff-report.schema.json` separam a
pré-inscrição dos resultados observados. A instância privada fixa snapshots derivados de revisões
Git, datasets e baselines, configurações das cinco engines, modelo E5 e hashes, duas rodadas,
medições, budgets eliminatórios e seleção por Pareto. O registry local e seu verificador conferem
hashes, conjunto de snapshots e permissões. Uma denylist ativa bloqueia os holdouts consumidos antes
da resolução de seus paths no runner. A Fase 4.1 pode começar pelo harness comum, sem alterar o
default ou o binário global.

## Em andamento

- manter cap 1 apenas como opção explícita, sem alterar o default;
- preservar o freeze, o gate fechado e os relatórios privados imutáveis;
- preservar a rejeição do prefixo morfológico 7/4 sem retuning de parâmetros;
- preservar o freeze concluído da Fase 4.0 sem reinterpretar parâmetros ou budgets;
- iniciar somente o harness comum da Fase 4.1; nenhuma engine experimental foi implementada;
- não usar os holdouts consumidos para seleção, tuning ou mudança de julgamentos.

## Próximo passo

1. Implementar o harness comum da Fase 4.1 conforme o protocolo congelado, sem alterar o default.
2. Validar geração do relatório operacional v1, instrumentação e fault injection na fixture.
3. Construir e medir separadamente cache SQLite, FTS5, embeddings locais e híbrido por RRF.
4. Executar duas rodadas imutáveis sobre todo o conjunto congelado de development.
5. Selecionar no máximo duas finalistas em development e só então reservar holdouts novos.
6. Integrar somente uma vencedora que passe no gate; manter BM25 direto se nenhuma passar.

O default continua `lexical-bm25-v1`. O plano faseado está no [`plano`](../plans/docs-search.md), a
comparação foi aprovada no [`ADR 0003`](../decisions/0003-comparar-engines-antes-da-adocao.md) e a
decisão anterior sobre cap 1 permanece no [`ADR 0002`](../decisions/0002-manter-cap-1-opt-in-apos-holdout.md).

## Armadilhas conhecidas

- `skills/` é fonte da verdade; não editar cópias em `~/.agents/skills/`.
- `docs-search` ainda não possui índice persistente, FTS5 nem embeddings; o freeze não é implementação.
- Não alterar tokenizer, threshold 0,80, profundidade 50, RRF `k = 60`, modelo, inputs ou budgets
  dentro de `engine-bakeoff-v1`; qualquer mudança exige protocolo e tag novos.
- O corpus padrão exclui `skills/**`; por isso avaliações devem apontar para documentação em
  `README.md` ou `docs/`, não apenas para conteúdo de `SKILL.md`.
- Resultado lexical vazio não prova ausência da informação.
- Nunca versionar nomes, paths, queries, headings, fingerprints ou relatórios derivados de projetos
  privados; nem paths absolutos, porque a configuração muda entre computadores.
- Runbook e `skills/search-project-docs/SKILL.md` descrevem o mesmo processo e devem mudar juntos.
