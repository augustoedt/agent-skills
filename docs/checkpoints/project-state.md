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
`fmt`, `clippy` e 42 testes passam; busca v2 usa `evaluation/search-response.schema.json` e
relatórios v2 usam `evaluation/report.schema.json`. As medições públicas atuais ficam em
`tools/docs-search/evaluation/reports/` e substituem artefatos históricos incompatíveis.

Avaliações de checkouts privados são estado local da máquina: datasets, nomes, roots, headings,
fingerprints e relatórios ficam fora deste repositório público, em armazenamento local criptografado.
Um manifesto local associa IDs neutros aos paths disponíveis em cada computador. Seu contrato v1
está em `evaluation/local-manifest.schema.json`; o runner local bloqueia holdouts e não sobrescreve
relatórios. O Git mantém apenas fixtures sintéticas e corpora explicitamente públicos.

O chunker ignora comentários `#` dentro de fenced code com crases ou tils, incluindo cercas
indentadas, fechamentos compatíveis e blocos não encerrados. A fixture cobre esses casos. Avaliações
privadas permanecem fora do Git e nenhum holdout foi executado.

O experimento público de diversidade comparou ilimitado, cap 1 e cap 2 na fixture e no corpus do
próprio repositório. Cap 1 melhorou Recall@5 na fixture, manteve a qualidade no corpus público e usou
menos contexto que cap 2 nos dois. Avaliações adicionais ficaram integralmente no ambiente privado:
quantidade de corpora, métricas e direção dos resultados não foram versionadas. Os julgamentos de
desenvolvimento e holdout foram revisados diretamente contra as fontes; o candidato cap 1 foi
congelado com commit, parâmetros, revisões e checksums. Cap 1 continua opt-in, o gate permanece
fechado e nenhum holdout foi executado.

A apresentação editorial `docs/apresentacoes/docs-search-arquitetura-e-avaliacao.html` explica o
fluxo de busca, contrato de evidência, avaliação, privacidade, manifesto local e roadmap sem expor
nomes ou paths privados.

O binário instalado via Rust 1.98.1 gerenciado por `asdf` continua na versão 0.1.0; ele não foi
atualizado automaticamente. O v0.5.0 pode ser executado no checkout com `cargo run -- evaluate` até
uma instalação ser solicitada.

A skill `search-project-docs` também está sincronizada em `~/.agents/skills/` e ligada aos agentes
locais detectados. O plano detalhado em `docs/plans/docs-search.md` agora define schema de avaliação,
métricas, relatórios, testes, gates e critérios para SQLite/FTS5, embeddings/RRF e distribuição.

## Em andamento

- manter cap 1 como candidato opt-in enquanto o gate não for concluído;
- preservar o freeze privado do candidato e de todos os datasets;
- manter os holdouts bloqueados até autorização explícita.

## Próximo passo

Decidir explicitamente se o gate de holdout deve ser aberto para o candidato cap 1. Se autorizado,
cada holdout será executado uma única vez com a configuração congelada e sem retuning posterior.
Cap 1 não deve virar default antes dessa validação de generalização.

## Armadilhas conhecidas

- `skills/` é fonte da verdade; não editar cópias em `~/.agents/skills/`.
- `docs-search` ainda não possui índice persistente, FTS5 nem embeddings.
- O corpus padrão exclui `skills/**`; por isso avaliações devem apontar para documentação em
  `README.md` ou `docs/`, não apenas para conteúdo de `SKILL.md`.
- Resultado lexical vazio não prova ausência da informação.
- Nunca versionar nomes, paths, queries, headings, fingerprints ou relatórios derivados de projetos
  privados; nem paths absolutos, porque a configuração muda entre computadores.
- Runbook e `skills/search-project-docs/SKILL.md` descrevem o mesmo processo e devem mudar juntos.
