# Plano — docs-search

## Estado do plano

- baseline lexical: implementado e preservado sem tuning;
- schema de avaliação v2: implementado e validado;
- executor e métricas: implementados; somente relatório v2 é suportado;
- baseline real do `lexical-bm25-v1`: medido e preservado no relatório de referência;
- diversidade por path: cap 1 medido, mantido opt-in e rejeitado para promoção a default;
- julgamentos de desenvolvimento e holdout: revisados diretamente contra as fontes e congelados;
- gate de holdout: concluído uma única vez e fechado; holdouts consumidos não orientam retuning;
- prefixo morfológico limitado 7/4: testado duas vezes em development e rejeitado sem ajustes;
- bake-off de engines: aprovado; Fase 4.0 congelada e harness comum da Fase 4.1 implementado;
- adapters disponíveis no bake-off: `lexical-bm25-v1` e `sqlite-cache-bm25-v1`; FTS5, E5 e RRF ainda recusados;
- protocolo e relatório operacional: contratos públicos v1 formalizados, com instância, snapshots,
  checksums e verificador mantidos no ambiente privado;
- variantes congeladas: BM25 direto, cache SQLite com BM25, FTS5, embeddings locais e híbrido por RRF;
- adoção: proibida antes da comparação em development e de um novo gate com holdouts frescos.

## Objetivo

Entregar busca documental local, portátil entre agentes e orientada a contexto mínimo suficiente.
Os Markdown do projeto permanecem autoritativos; índices, relatórios e excerpts são artefatos
derivados e reconstruíveis.

O sistema deve responder a três perguntas antes de ganhar complexidade:

1. encontra o documento e a seção corretos?
2. sabe não retornar evidência quando a resposta não existe no corpus?
3. reduz contexto e latência o bastante para justificar seu custo operacional?

## Princípios e restrições

- Verificar `command -v docs-search` antes de qualquer invocação externa do binário.
- Nunca indexar código, `.env`, `skills/**` ou credenciais no corpus padrão.
- Manter avaliações de projetos privados fora do Git, com IDs e roots em manifesto local por máquina.
- Preservar path, heading, linhas e hashes para conferir a fonte original.
- Não tratar score, excerpt, relatório ou índice como fonte da verdade.
- Não adotar SQLite, FTS5, embeddings, RRF ou outro motor sem comparação registrada com o baseline.
- Não copiar thresholds entre motores ou modelos sem calibração própria.
- Versionar mudanças incompatíveis do contrato JSON e manter somente o schema atual enquanto não houver consumidores.
- Atualizar juntos o runbook e `skills/search-project-docs/SKILL.md` quando o fluxo operacional mudar.

## Baseline já entregue

- [x] Corpus seguro: `docs/**/*.md` e arquivos de instrução Markdown na raiz.
- [x] Chunking por headings com breadcrumbs e linhas inclusivas.
- [x] Normalização Unicode e remoção de acentos.
- [x] Ranking BM25 com pesos para body, heading, path e frase exata.
- [x] Cobertura mínima de termos para reduzir evidência acidental.
- [x] Contrato JSON v2 autocontido com seleção, rank bruto e hashes BLAKE3.
- [x] Saída humana e saída JSON.
- [x] 20 consultas de avaliação migradas para schema v2 com headings, notas e tags.
- [x] Parser/validador estrito para schema v2.
- [x] Fixture documental estável e versionada `stable-v1`, separada do corpus real.
- [x] `fmt`, `clippy` e 52 testes.
- [x] Instalador compatível com Rust gerenciado por `asdf`.
- [x] Skill e runbook com preflight obrigatório do binário.
- [x] Baseline atual sem tuning, com relatório JSON v2 e análise por categoria.

---

## Etapa 1 — contrato de avaliação e executor

### Objetivo

Transformar `evaluation/queries.json` em benchmark executável e reproduzível, sem alterar o motor de
busca durante a primeira medição.

### 1.1 Versionar o schema das consultas — concluído

O dataset usa somente schema v2; versões diferentes são rejeitadas antes da avaliação. Cada consulta tem:

- `id` único e estável;
- `category`: `exact`, `semantic`, `ambiguous` ou `no_answer`;
- `query`;
- `expected_paths`.

Adicionar, em uma versão nova do schema, campos opcionais:

- `expected_headings`: headings aceitáveis por path;
- `notes`: justificativa humana da relevância;
- `tags`: domínio, idioma ou dificuldade;
- `disabled_reason`: apenas para casos temporariamente inválidos, sem exclusão silenciosa.

Relevância primária continua sendo por path. Heading é diagnóstico separado para revelar quando o
arquivo correto foi recuperado pela seção errada, sem alterar as métricas path-level.

### 1.2 Estabilizar os corpora — fixture v1 concluída

A fixture `evaluation/fixtures/stable-v1` foi criada e validada contra o dataset v2. Manter duas modalidades:

1. **fixture versionada e pequena** — conteúdo mantido estável por política para testes de regressão;
2. **corpus real** — o próprio `agent-skills`, para medir comportamento operacional.

A fixture deve conter casos de:

- múltiplas seções no mesmo arquivo;
- dois arquivos relevantes para a mesma consulta;
- resposta inexistente;
- headings com acentos e caracteres `#`;
- texto que parece heading dentro de bloco de código cercado;
- arquivos fora do corpus permitido;
- symlinks que não podem ser seguidos;
- documentos longos com match próximo ao início, meio e fim.

O corpus real pode mudar ao longo do tempo; por isso cada relatório deve registrar hashes ou uma
identificação reproduzível do estado avaliado.

### 1.3 Implementar o executor em Rust

**Status:** concluído, com saída humana/JSON, `--output` explícito e continuação após falhas
individuais.

Adicionar um subcomando no mesmo binário, evitando scripts que reimplementem o motor:

```text
docs-search evaluate \
  --root <projeto> \
  --queries <queries.json> \
  --limit 5 \
  --max-excerpt-chars 1200 \
  --json
```

Responsabilidades do executor:

- validar schema, ids duplicados, categorias e paths esperados;
- chamar diretamente a função pública `search()` para cada consulta;
- continuar a avaliação após falha individual, registrando o erro por consulta;
- retornar exit code diferente de zero para dataset inválido, erro global ou consulta não executada;
- preservar a ordem definida no dataset;
- produzir resumo global, por categoria e por consulta;
- não modificar os documentos avaliados;
- permitir gravar relatório em arquivo de forma explícita, sem escrita automática oculta.

Quando o executor for chamado por skill, script externo ou agente, o chamador continua obrigado a
confirmar `command -v docs-search` antes da invocação.

### 1.4 Definir as métricas sem ambiguidade

**Status:** concluído no relatório de avaliação v2; testes cobrem agregação, deduplicação de paths,
no-answer e percentis.

Para métricas por path, resultados repetidos do mesmo arquivo devem ser deduplicados preservando a
melhor posição no ranking bruto de chunks. O cutoff considera ranks brutos de 1 a 5; remover uma
duplicata não comprime as posições seguintes. Headings permanecem diagnóstico separado e não
alteram as métricas primárias por path.

#### Hit@1

Somente consultas respondíveis:

```text
Hit@1 = consultas cujo primeiro path é relevante / consultas respondíveis
```

#### Recall@5

Para cada consulta respondível:

```text
Recall@5 = paths esperados distintos encontrados no top 5 / total de paths esperados
```

Registrar média macro e agregado micro.

#### MRR@5

Para cada consulta respondível:

```text
RR = 1 / posição do primeiro resultado relevante
```

Usar zero quando nenhum resultado relevante aparecer no top 5. MRR é a média desses valores.

#### Falso positivo de no-answer

```text
FP no-answer = consultas no_answer com pelo menos um resultado / total de no_answer
```

Também registrar quantidade absoluta e o path/score responsável por cada falso positivo.

#### Latência

Medir com relógio monotônico:

- tempo de cada chamada `search()`;
- média, p50, p95, máximo e total;
- separação entre custo de leitura do corpus e ranking quando a arquitetura permitir.

No baseline atual, cada busca lê o corpus novamente; o relatório deve deixar isso explícito para
não atribuir ao ranking o custo de I/O.

#### Volume de contexto

Contar caracteres Unicode dos excerpts retornados:

- total por consulta;
- média e p95;
- total do benchmark;
- número de resultados por consulta.

Bytes podem ser registrados como diagnóstico, mas a métrica principal deve acompanhar o limite de
caracteres usado pelo CLI.

### 1.5 Versionar o relatório

**Status:** somente o contrato v2 é suportado e formalizado em
`evaluation/report.schema.json`.

O JSON de avaliação deve ter schema próprio e independente do schema de busca. Campos mínimos:

```text
schema_version
engine
queries_schema_version
corpus: nome, root fornecida pelo chamador, files, chunks e identificação do estado
config: limit e max_excerpt_chars
summary: totais e métricas globais
per_category: métricas por categoria
queries: resultados e métricas de cada caso
```

Cada item de consulta deve registrar:

- id, categoria e query;
- paths/headings esperados;
- resultados retornados com rank, path, heading, score, linhas e tamanho do excerpt;
- primeiro rank relevante;
- Hit@1, Recall@5 e reciprocal rank;
- latência e caracteres retornados;
- erro, quando houver.

Não incluir paths absolutos específicos da máquina em relatórios commitados. Timestamps e latência
podem variar; snapshots de teste devem normalizar campos voláteis.

### 1.6 Testes da etapa

Adicionar testes unitários para:

- cálculo com um e múltiplos paths esperados;
- resultado repetido do mesmo path;
- miss completo;
- consulta `no_answer` vazia e com falso positivo;
- percentis com amostras pares, ímpares e vazias;
- dataset inválido e ids duplicados;
- ordenação determinística do relatório.

Adicionar integração com fixture para:

- execução completa do subcomando `evaluate`;
- JSON válido e versionado;
- erro por schema inválido;
- estabilidade de paths, headings e linhas;
- nenhum acesso fora do corpus permitido.

### Gate da Etapa 1

Não iniciar SQLite ou embeddings até que:

- [x] todas as 20 consultas sejam executadas automaticamente;
- [x] o runner e suas métricas tenham testes;
- [x] exista relatório JSON do baseline lexical;
- [x] Hit@1, Recall@5, MRR, no-answer, latência e contexto estejam registrados;
- [x] `project-state.md` contenha o resumo medido;
- [x] este plano marque o baseline como concluído;
- [x] `fmt`, `clippy` e testes passem.

---

## Etapa 2 — ampliar o benchmark e registrar o baseline confiável

### 2.1 Executar a medição sem tuning — concluído

O relatório `tools/docs-search/evaluation/reports/agent-skills-lexical-bm25-v1-unlimited-v2.json`
é a medição corrente sem diversidade. Ele usa somente o contrato v2 e serve como referência para os
relatórios cap 1 e cap 2 gerados no mesmo estado do corpus. Artefatos de contratos antigos não são
mantidos neste projeto em desenvolvimento. A análise está em
`tools/docs-search/evaluation/reports/README.md`.

Analisar por categoria, não apenas o total. Em especial:

- consultas exatas devem expor regressões de tokenização/ranking;
- consultas semânticas medem a limitação real do lexical;
- consultas ambíguas medem precisão com pouco contexto;
- `no_answer` mede capacidade de abstention.

### 2.2 Expandir casos insuficientes

O conjunto atual tem 8 consultas exatas, 6 semânticas, 3 ambíguas e 3 sem resposta. Antes de usar
percentuais para decisões arquiteturais:

- aumentar consultas ambíguas e `no_answer` para reduzir a granularidade excessiva;
- incluir português e inglês quando o corpus suportar ambos;
- adicionar termos presentes em documentos irrelevantes para desafiar abstention;
- adicionar consultas longas com alguns termos ausentes;
- adicionar singular/plural, flexões e sinônimos;
- evitar casos que só podem ser respondidos por `skills/**`, pois esse diretório é excluído.

Toda inclusão exige justificativa de relevância revisável por humano.

### 2.3 Medir um segundo corpus

Selecionar um projeto documental maior, sem credenciais e com autorização para avaliação. Se o
projeto não for público, dataset, manifesto e relatório permanecem em armazenamento local
criptografado, fora deste Git; somente fixtures sintéticas ou corpora explicitamente públicos podem
ser versionados. Usar o mesmo schema e métricas. Registrar localmente e em separado:

- tamanho do corpus;
- distribuição de documentos e chunks;
- idiomas;
- categorias de consulta;
- resultados por engine.

O segundo corpus é obrigatório antes de justificar embeddings: o `agent-skills` é pequeno e pode
produzir métricas artificialmente altas ou instáveis.

### Gate da Etapa 2

- [x] baseline atual medido sem tuning no contrato v2;
- [x] benchmark ampliado com cobertura suficiente de no-answer e ambiguidade;
- [x] segundo corpus medido localmente sem versionar metadados privados;
- [x] limitações lexicais documentadas com exemplos concretos;
- [x] decisão registrada: manter baseline e cap 1 opt-in, sem promover o candidato.

---

## Etapa 3 — hardening e tuning lexical medido

Executar apenas depois do primeiro relatório.

### 3.1 Completar testes de fronteira

Adicionar cobertura para:

- `line_start` e `line_end`;
- recorte de excerpt próximo aos limites do documento;
- query vazia;
- `limit` fora de `1..=100`;
- `max_excerpt_chars < 80`;
- corpus vazio;
- symlink de arquivo e diretório;
- arquivo ignorado pelo Git;
- erro de leitura e Markdown inválido tolerável;
- [x] linha iniciada por `#` dentro de bloco cercado, sem alterar breadcrumbs;
- instalador quando o binário não fica no `PATH`.

O item de fenced code foi concluído para cercas de crases e tils, incluindo fechamento com marcador
compatível, comprimento suficiente, indentação de até três espaços e fence não encerrada. A fixture
e os testes focados cobrem esses casos; os relatórios atuais registram o efeito no corpus público.
A correção permaneceu inalterada durante o gate posterior de holdout.

### 3.2 Avaliar melhorias lexicais isoladamente

Alterar uma variável por experimento:

- cobertura mínima conforme o tamanho da query;
- tokenização de hífen, underscore e paths;
- stopwords em português e inglês;
- pesos de body, heading e path;
- bônus de frase;
- prefix matching;
- score mínimo para abstention;
- tratamento de flexões simples, somente se mensurável.

Cada experimento deve gerar relatório comparável. Não manter mudanças que melhorem uma categoria
escondendo regressão relevante em outra.

#### Hipótese pré-registrada — prefixo morfológico limitado

A evidência de desenvolvimento mostra categorias exatas e ambíguas fortes, mas consultas semânticas
frequentemente ficam sem candidatos porque query e documento usam flexões ou nominalizações
diferentes. Reduzir `minimum_should_match` seria uma mudança ampla e arriscaria abstention sem criar
sobreposição lexical real.

O próximo experimento altera somente a equivalência entre tokens normalizados:

- match exato continua válido;
- match morfológico exige prefixo comum com pelo menos sete caracteres;
- depois do prefixo, cada token pode ter no máximo quatro caracteres restantes;
- os mesmos critérios valem para document frequency, body, heading, path e seleção do excerpt;
- fórmula e constantes BM25, `minimum_should_match`, pesos, bônus de frase, stopwords, chunking e
  desempate não mudam; frequências e IDF refletem a nova equivalência de forma consistente;
- o modo é opt-in; desligado, deve reproduzir byte a byte o comportamento atual;
- o baseline do experimento usa ranking ilimitado, sem combinar a variável com diversidade por path.

A configuração `7/4` é única e congelada para o experimento; não haverá variação por corpus. Ela
cobre diferenças morfológicas longas sem aproximar tokens curtos ou introduzir dicionários,
sinônimos, modelos ou regras de domínio.

O conjunto exato de avaliação será congelado localmente antes da implementação: corpus e fixture
públicos mais todos os aliases então marcados como `development`, com revisão fonte, hash do dataset
e fingerprint do baseline. Nenhum corpus poderá entrar ou sair durante o experimento.

Critérios de aceitação apenas nesse conjunto de development, definidos antes da implementação:

1. nenhum corpus aumenta falsos positivos `no_answer`;
2. Hit@1, Recall@5 e MRR@5 de `exact` e `ambiguous` não diminuem em nenhum corpus;
3. Recall@5 semântico melhora na maioria dos corpora e a macro entre corpora sobe ao menos 0,03;
4. Hit@1 e MRR@5 globais não caem mais de 0,02;
5. nenhuma consulta respondível já correta se torna sem resposta;
6. contexto médio não cresce mais de 25%; p95 de latência não cresce mais de 25% ou 50 ms,
   prevalecendo o limite maior;
7. duas execuções idênticas produzem ranking idêntico;
8. datasets e julgamentos permanecem byte a byte inalterados.

Falhar qualquer item rejeita a hipótese sem ajuste de parâmetros. Mesmo se aprovada em development,
ela só poderia chegar a outro gate depois que novos holdouts fossem reservados e congelados; os
holdouts consumidos não seriam consultados.

**Resultado:** rejeitada. Duas execuções em todo o conjunto congelado produziram rankings
determinísticos e sinal de ganho na categoria-alvo, mas falharam guardas pré-registradas de
segurança, qualidade e custo. Os parâmetros 7/4 não foram ajustados, datasets permaneceram
imutáveis, nenhum holdout foi consultado e o código ficou somente em branch experimental local para
auditoria. O default `lexical-bm25-v1` não mudou.

### 3.3 Diversidade por path — candidato medido

O `docs-search 0.6.0-alpha.3` oferece `--max-results-per-path N` como seleção opt-in após o ranking BM25. O
seletor pode avançar além dos cinco primeiros chunks brutos, limita contribuições repetidas de um
path e devolve ranks finais contíguos. Relatório e resposta de busca v2 preservam `raw_rank`; a
resposta também expõe `selection.max_results_per_path`. O comportamento sem flag não muda.

Foram comparados ilimitado, cap 1 e cap 2 na fixture e no corpus público. Cap 1:

- melhorou Recall@5 na fixture;
- manteve Hit@1, MRR e falsos positivos no-answer sem regressão nos dois corpora;
- reduziu contexto na fixture e no corpus público;
- entregou a mesma qualidade que cap 2 com menos contexto nos dois corpora.

Avaliações adicionais foram executadas somente no ambiente privado; identidades, quantidade de
corpora, métricas detalhadas e diagnósticos por consulta não são versionados. Os julgamentos de
desenvolvimento e holdout foram revisados contra as fontes, e o candidato cap 1 foi congelado com
código, parâmetros, revisões e checksums. Após autorização explícita, cada holdout executou uma única
vez e o gate fechou automaticamente. O conjunto falhou os pisos de qualidade predefinidos. Cap 1
permanece opt-in, não será o default e os holdouts consumidos não podem orientar retuning. A decisão
está no [ADR 0002](../decisions/0002-manter-cap-1-opt-in-apos-holdout.md).

### Critério de aceitação do tuning

Uma mudança lexical só entra quando:

- não reduz Hit@1 ou MRR global sem justificativa registrada;
- não aumenta falsos positivos de no-answer;
- melhora a categoria-alvo nos corpora de desenvolvimento ou explica claramente a diferença, sem
  consultar holdouts durante tuning;
- não aumenta contexto retornado de forma desproporcional;
- mantém determinismo e contrato JSON.

O bake-off posterior pode concluir que o lexical já é suficiente; nesse caso, preservar os
protótipos apenas como evidência e não adotar a complexidade adicional.

---

## Etapa 4 — bake-off experimental de engines

### Objetivo e estado

Comparar alternativas de armazenamento e recuperação sobre a mesma base documental antes de
escolher qualquer arquitetura nova. A Fase 4.0 e o harness comum da Fase 4.1 estão concluídos; as
engines experimentais ainda não foram implementadas. O `lexical-bm25-v1` continua sendo o default
durante todo o bake-off.

As cinco variantes são:

| ID experimental | Variante | Variável isolada |
| --- | --- | --- |
| `lexical-bm25-v1` | leitura direta + BM25 atual | baseline congelado |
| `sqlite-cache-bm25-v1` | chunks em SQLite + mesmo BM25 | persistência e custo de leitura |
| `fts5-v1` | recuperação lexical por FTS5 | motor lexical indexado |
| `local-embeddings-v1` | recuperação vetorial local | similaridade semântica |
| `hybrid-rrf-v1` | FTS5 + embeddings por RRF | fusão dos dois rankings |

Nenhuma variante experimental será instalada globalmente, conectada ao fluxo padrão ou mantida
como implementação de produção antes da decisão final.

### Fase 4.0 — protocolo e freeze

**Status:** concluída antes da implementação das engines.

Foram congelados:

- snapshots dos inputs de development extraídos de revisões Git, datasets, fingerprints e baselines;
- chunking, normalização, corpus, top 5, excerpts de até 1.200 caracteres e evidência verificável;
- `rusqlite = 0.40.2` com SQLite bundled 3.53.2 e validação runtime de versão, FTS5 e smoke test;
- FTS5 `unicode61 remove_diacritics 2`, colunas body/heading/path com pesos 1/2/3, lista de 50
  candidatos e abstention quando `MATCH` não produz linha;
- `intfloat/multilingual-e5-small` na revisão imutável registrada, com 384 dimensões, máximo de 512
  tokens, prefixos E5, truncamento à direita, mean pooling pela attention mask, L2 e dot product;
- busca vetorial exata sobre todos os chunks, top 50 e abstention abaixo de similaridade 0,80;
- RRF com listas de 50 candidatos, deduplicação por hash de chunk e `k = 60`;
- duas rodadas em ordens inversas, processos novos, zero warmup e nenhuma cache de resultados;
- limites eliminatórios de qualidade, `no_answer`, contexto, startup, p95, indexação, memória, disco,
  determinismo, rebuild, atualização incremental, evidência e fallback;
- seleção por fronteira de Pareto, ganho significativo explícito e no máximo duas finalistas.

Os contratos públicos independentes são
`evaluation/engine-bakeoff-protocol.schema.json` e
`evaluation/engine-bakeoff-report.schema.json`. A instância do protocolo, snapshots, modelo,
relatórios-base e registry de checksums ficam somente no armazenamento privado. Um verificador local
confere hashes, conjunto exato de snapshots e permissões. O runner consulta uma denylist ativa antes
de resolver paths de datasets consumidos; esses holdouts permanecem inacessíveis durante o bake-off.

Qualquer mudança de modelo, threshold, tokenizer, profundidade, RRF, orçamento ou input exige outro
protocolo/tag. A Fase 4.1 não pode reinterpretar o freeze depois de observar resultados.

### Fase 4.1 — harness comum e instrumentação

**Status:** concluída para o harness comum e o adapter `lexical-bm25-v1`; validada com duas
observações sintéticas pareadas e relatório operacional conforme o schema. As medições congeladas
de development continuam sujeitas à ordem contrabalanceada do protocolo.

Foi criada uma fronteira interna de engine sem alterar o comportamento da CLI padrão. O harness
executa exatamente as mesmas queries e registra:

- Hit@1, Recall@5 macro/micro e MRR@5, globais e por categoria;
- falsos positivos absolutos e taxa de `no_answer`;
- headings corretos, resultados por query e contexto total/médio/p95;
- latência de startup, leitura/index lookup, ranking e total, separadamente;
- tempo de build completo e atualização incremental do índice;
- pico de RAM e tamanho em disco;
- erros, fallback, versão da engine e configuração integral.

Cada variante executará duas vezes sobre os mesmos inputs congelados. Rankings devem ser idênticos;
campos voláteis, como tempos, serão comparados por estatística e não por igualdade byte a byte.

Entregue nesta fase:

- `bakeoff observe` com timings internos, candidatos examinados, evidência e projeções determinísticas;
- `bakeoff finalize` com revalidação do engine lexical, baseline exato, métricas, budgets e par de runs;
- runner privado fixado por registry próprio, com release limpo, provenance do binário e
  `Cargo.lock`, toolchain, host congelado, ambiente, startup, end-to-end, RSS, isolamento de holdout
  e validação do relatório público;
- escrita imutável `create_new`, `0600`, recusa de overwrite e quarentena sem sobrescrever;
- falha explícita para cada engine ainda não implementada até sua respectiva fase.

### Fase 4.2 — cache SQLite com BM25 preservado

**Status:** concluída em código e testes; medições congeladas continuam fechadas até os cinco
adapters existirem na mesma revisão limpa.

O braço `sqlite-cache-bm25-v1` foi implementado sem mudar tokenização, frequências, IDF, fórmula,
pesos, desempate ou seleção. O objetivo permanece isolar o efeito de persistência.

O banco ficará fora do repositório:

```text
${XDG_CACHE_HOME:-~/.cache}/docs-search/<hash-da-raiz>/index.sqlite3
```

Ele é descartável e reconstruível. Foram implementados criação e rebuild atômicos, atualização
transacional por cópia temporária, arquivo novo/alterado/removido/renomeado, schema incompatível,
invalidação por fingerprint/parser/configuração, corrupção, leitores concorrentes durante troca
atômica e fallback explícito para BM25 direto sob falha forçada de rebuild. Resultados não voláteis
e hashes de evidência equivalem ao baseline direto. Chunks com texto repetido usam uma chave interna
determinística composta para satisfazer a chave primária, mas o hash BLAKE3 original é reconstruído
do texto e permanece no contrato de evidência.

O runtime confere SQLite 3.53.2, `ENABLE_FTS5` e um smoke test FTS5/BM25. O workload incremental
executa, nesta ordem, add, modify, rename e remove sobre uma cópia isolada dos quatro primeiros
Markdown selecionados; cada passo é comparado com um rebuild limpo e exige zero resultado stale.

### Fase 4.3 — recuperação FTS5

Implementar `fts5-v1` sobre os mesmos chunks e metadados. Queries do usuário nunca serão tratadas
como sintaxe FTS bruta. O relatório deve distinguir ganho de latência de qualquer mudança de
qualidade e preservar path, heading, linhas e hashes verificáveis.

### Fase 4.4 — embeddings locais

Implementar `local-embeddings-v1` com o único modelo congelado na Fase 4.0. Registrar id, versão,
licença, dimensão, normalização, hash, requisitos de CPU/RAM e algoritmo de distância. Pesos ficam
fora do banco; vetores incluem modelo e hash do chunk para invalidação reproduzível.

Não haverá seleção de modelo ou ajuste de threshold depois de observar os resultados congelados.
Uma nova configuração exigirá outro protocolo e outra tag.

### Fase 4.5 — híbrido por RRF

Implementar `hybrid-rrf-v1` como composição de FTS5 e embeddings, sem um terceiro mecanismo oculto
de score. O relatório registra constante `k`, profundidade das listas, contribuição de cada ranking
e candidatos antes da fusão. A lógica de RRF terá testes com rankings sintéticos antes da inferência
real.

### Fase 4.6 — comparação em development

Comparar as variantes em pares que preservem interpretação:

1. SQLite + BM25 versus leitura direta: efeito de persistência;
2. FTS5 versus SQLite + BM25: efeito do motor lexical;
3. embeddings versus FTS5: diferença entre recuperação vetorial e lexical;
4. híbrido versus FTS5 e embeddings isolados: contribuição da fusão.

A seleção será por fronteira de Pareto, sem score composto opaco. São guardas eliminatórias:

- regressão além do limite pré-registrado em qualidade geral, exata ou ambígua;
- qualquer piora proibida de `no_answer`;
- evidência sem path, heading, linhas ou hashes verificáveis;
- ranking não determinístico;
- contexto, latência, RAM ou disco acima do orçamento congelado;
- índice stale apresentado como fonte atual;
- falha de rebuild ou fallback.

Resultados negativos também serão preservados. Uma variante só vira finalista se entregar ganho
relevante de recuperação ou custo que não seja dominado por uma opção mais simples.

### Gate da Etapa 4

- protocolo, inputs e parâmetros congelados antes do código experimental;
- cinco variantes executadas duas vezes em todo o conjunto de development;
- nenhuma consulta ou relatório de holdout consumido acessado;
- relatórios imutáveis e comparação reproduzível;
- default e instalação global inalterados;
- no máximo duas variantes escolhidas como finalistas, ou nenhuma se todas falharem.

---

## Etapa 5 — holdout novo e decisão

### Fase 5.1 — reservar o gate

Somente após escolher finalistas em development:

- reservar holdouts inteiramente novos;
- revisar julgamentos diretamente contra as fontes sem executar rankings;
- congelar datasets, revisões, fingerprints, engines, parâmetros e critérios;
- registrar aprovação explícita e execução única no gate local.

Os holdouts anteriores permanecem consumidos e proibidos. Cada finalista executará uma única vez;
o gate fecha automaticamente e seus resultados não poderão orientar retuning.

### Fase 5.2 — decisão

Registrar um ADR com uma destas saídas:

- promover uma engine vencedora;
- manter `lexical-bm25-v1` e preservar todas as alternativas apenas como experimento;
- rejeitar todas as variantes e encerrar o ciclo.

Passar em development não garante promoção. Qualquer falha em holdout resulta em no-go para aquela
configuração, sem ajuste posterior de parâmetros.

---

## Etapa 6 — integrar somente o vencedor

Esta etapa só começa depois do ADR da Etapa 5. Não será construída uma plataforma de produção para
engines descartadas.

### 6.1 Produto e compatibilidade

- integrar somente a engine aprovada, inicialmente de forma opt-in;
- preservar o motor lexical direto como fallback;
- manter documentos como fonte autoritativa e todo índice reconstruível;
- versionar qualquer mudança incompatível dos contratos;
- adicionar testes de corrupção, rebuild, atualização e rollback aplicáveis ao vencedor.

### 6.2 CI e distribuição

- executar `cargo fmt --check`, Clippy com warnings negados e todos os testes;
- avaliar a fixture estável e o conjunto público permitido;
- testar instalador, atualização e rollback;
- manter `cargo install --locked` enquanto for suficiente;
- publicar binários somente se a instalação por Cargo se tornar barreira medida;
- publicar checksums e testar arquiteturas suportadas quando houver artefatos.

### 6.3 Documentação operacional

Ao mudar fluxo ou contrato, atualizar juntos:

- `tools/docs-search/README.md`;
- `docs/runbooks/buscar-documentacao-de-projetos.md`;
- `skills/search-project-docs/SKILL.md`;
- `docs/checkpoints/project-state.md`;
- apresentação, ADRs e este plano.

---

## Dependências entre etapas

```text
Etapas 1–3: baseline, avaliação e experimentos lexicais concluídos
    ↓
Etapa 4.0: protocolo e inputs congelados ✓
    ↓
Etapas 4.1–4.5: harness + quatro engines experimentais
    ↓
Etapa 4.6: comparação em development
    ├── nenhuma finalista → manter BM25 direto
    └── até duas finalistas
              ↓
Etapa 5: holdouts novos, gate one-shot e ADR
    ├── no-go → manter BM25 direto
    └── vencedora aprovada
              ↓
Etapa 6: integrar e distribuir somente a vencedora
```

O bake-off compara todas as alternativas planejadas sem confundir armazenamento, ranking lexical,
recuperação vetorial e fusão. A adoção continua condicionada a evidência; implementar um protótipo
não o torna parte do produto.

## Riscos a acompanhar

- **Corpus pequeno:** pode inflar métricas e justificar complexidade indevida.
- **Relevância somente por arquivo:** pode esconder seção incorreta; usar headings diagnósticos.
- **Consultas longas:** cobertura mínima pode produzir falsos negativos.
- **No-answer fraco:** poucos casos tornam a taxa instável; ampliar antes de calibrar threshold.
- **Latência confundida:** separar leitura, ranking e startup quando possível.
- **Drift documental:** relatórios precisam identificar o estado do corpus.
- **Vazamento de credenciais:** Markdown do corpus nunca deve conter secrets.
- **Cache stale:** hashes e fallback precisam impedir que índice antigo pareça autoritativo.
- **Modelo de embeddings:** versão, licença e custo operacional precisam permanecer reproduzíveis.
- **Divergência de instruções:** skill e runbook devem continuar sincronizados.

## Definição de conclusão do projeto

O `docs-search` só pode ser considerado estável quando:

- qualidade e abstention são medidas em pelo menos dois corpora;
- regressões são detectadas automaticamente;
- o contexto retornado é pequeno e rastreável;
- falhas nunca são apresentadas como evidência positiva;
- documentos originais permanecem autoritativos;
- qualquer cache pode ser apagado e reconstruído;
- instalação, atualização e fallback são testados;
- complexidade adotada possui ganho mensurável registrado.
