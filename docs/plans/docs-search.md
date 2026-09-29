# Plano — docs-search

## Estado do plano

- baseline lexical: implementado e preservado sem tuning;
- schema de avaliação v2: implementado e validado;
- executor e métricas: implementados; relatório v2 registra diversidade e rank bruto, com v1 arquivado;
- baseline real do `lexical-bm25-v1`: medido e preservado no relatório de referência;
- diversidade por path: cap 1 selecionado como candidato nos corpora de desenvolvimento, ainda opt-in;
- revisão humana dos julgamentos locais e gate de holdout: próximos passos obrigatórios;
- SQLite/FTS5: adiado até ampliar o benchmark e uma lacuna medida justificar persistência;
- embeddings/RRF: bloqueado até existir uma lacuna semântica comprovada;
- MCP: fora do escopo enquanto a CLI atender os clientes.

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
- Não adicionar SQLite, embeddings, classificador ou MCP sem comparação registrada com o baseline.
- Não copiar thresholds entre motores ou modelos sem calibração própria.
- Manter compatibilidade do contrato JSON; mudanças incompatíveis exigem nova versão de schema.
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
- [x] Parser/validador compatível com schemas v1 e v2.
- [x] Fixture documental estável e versionada `stable-v1`, separada do corpus real.
- [x] `fmt`, `clippy` e 37 testes.
- [x] Instalador compatível com Rust gerenciado por `asdf`.
- [x] Skill e runbook com preflight obrigatório do binário.
- [x] Baseline real sem tuning, com relatório JSON v1 e análise por categoria.

---

## Etapa 1 — contrato de avaliação e executor

### Objetivo

Transformar `evaluation/queries.json` em benchmark executável e reproduzível, sem alterar o motor de
busca durante a primeira medição.

### 1.1 Versionar o schema das consultas — concluído

O dataset foi migrado para schema v2 sem perder leitura do schema v1. Cada consulta continua tendo:

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

**Status:** concluído no `docs-search` v0.2.0, com saída humana/JSON, `--output` explícito e
continuação após falhas individuais.

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

**Status:** concluído no relatório de avaliação v1; testes cobrem agregação, deduplicação de paths,
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

**Status:** contrato v1 preservado em `evaluation/report-v1.schema.json`; o contrato atual v2 está
formalizado em `evaluation/report.schema.json`.

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

### 2.1 Executar a primeira medição sem tuning — concluído

O relatório `tools/docs-search/evaluation/reports/agent-skills-lexical-bm25-v1-baseline.json`
preserva a execução sobre o commit fonte `de77945`: 20/20 consultas sem falha, Hit@1 0,647059,
Recall@5 macro 0,588235,
Recall@5 micro 0,619048, MRR@5 0,647059, zero falsos positivos em três casos no-answer e 18.326
caracteres de contexto. A análise por categoria está em
`tools/docs-search/evaluation/reports/README.md`.

Rodar a versão atual sem alterar pesos, stopwords ou cobertura mínima. Guardar o relatório como
referência imutável do motor `lexical-bm25-v1`.

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

- [x] baseline original preservado sem tuning;
- [ ] benchmark ampliado com cobertura suficiente de no-answer e ambiguidade;
- [x] segundo corpus medido localmente sem versionar metadados privados;
- [x] limitações lexicais documentadas com exemplos concretos;
- [ ] decisão registrada: manter baseline, ajustar lexical ou investigar busca híbrida.

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

O item de fenced code foi concluído no v0.2.1 para cercas de crases e tils, incluindo fechamento com
marcador compatível, comprimento suficiente, indentação de até três espaços e fence não encerrada.
No corpus público real, headings esperados encontrados passaram de 4/17 para 9/17; métricas primárias
ficaram idênticas e o contexto total subiu de 18.326 para 18.927 caracteres. Fixture e corpora locais
de desenvolvimento não regrediram nas métricas primárias; holdouts não foram executados.

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

### 3.3 Diversidade por path — candidato medido

O `docs-search 0.3.0` introduz `--max-results-per-path N` como seleção opt-in após o ranking BM25. O
seletor pode avançar além dos cinco primeiros chunks brutos, limita contribuições repetidas de um
path e devolve ranks finais contíguos. O relatório v2 preserva `raw_rank`; desde o `docs-search
0.4.0`, a resposta de busca v2 também expõe `selection.max_results_per_path` e `raw_rank`. A quebra
do contrato de busca foi direta porque não havia consumidores; o comportamento sem flag não muda.

Foram comparados ilimitado, cap 1 e cap 2 na fixture e no corpus público. Cap 1:

- melhorou Recall@5 na fixture;
- manteve Hit@1, MRR e falsos positivos no-answer sem regressão nos dois corpora;
- reduziu contexto na fixture e no corpus público;
- entregou a mesma qualidade que cap 2 com menos contexto nos dois corpora.

Avaliações adicionais foram executadas somente no ambiente privado; quantidade de corpora, métricas
e direção dos resultados não são versionadas. Cap 1 é a configuração candidata, mas continua
opt-in. Nenhum holdout foi executado. Antes do gate, concluir a revisão humana pendente dos
julgamentos locais, congelar código/configuração/datasets e usar uma tag imutável.

### Critério de aceitação do tuning

Uma mudança lexical só entra quando:

- não reduz Hit@1 ou MRR global sem justificativa registrada;
- não aumenta falsos positivos de no-answer;
- melhora a categoria-alvo nos corpora de desenvolvimento ou explica claramente a diferença, sem
  consultar holdouts durante tuning;
- não aumenta contexto retornado de forma desproporcional;
- mantém determinismo e contrato JSON.

Se o lexical atingir qualidade suficiente, registrar a decisão e não implementar embeddings.

---

## Etapa 4 — índice SQLite e FTS5

### Pré-condição

Etapas 1 e 2 concluídas. Implementar apenas se leitura repetida do corpus tiver custo relevante ou
se FTS5 demonstrar benefício mensurável.

### 4.1 Localização e ciclo de vida

Usar um SQLite por raiz canônica de projeto em cache externo ao repositório, por exemplo:

```text
${XDG_CACHE_HOME:-~/.cache}/docs-search/<hash-da-raiz>/index.sqlite3
```

Nunca transformar o banco em fonte da verdade nem gravá-lo dentro de `docs/`. Banco ausente,
corrompido ou incompatível deve ser reconstruído ou provocar fallback explícito para leitura direta.

### 4.2 Schema mínimo

- `metadata`: schema, engine, versão da ferramenta, raiz e parâmetros do chunker;
- `files`: path, hash, tamanho e metadados necessários para sincronização;
- `chunks`: arquivo, heading, linhas, texto e chunk hash;
- `chunks_fts`: tabela virtual FTS5 ligada aos chunks;
- índices auxiliares apenas quando medição justificar.

### 4.3 Sincronização incremental

- comparar hashes antes de reprocessar;
- inserir arquivos novos;
- atualizar conteúdo alterado;
- remover arquivos apagados;
- tratar rename como remoção + inclusão ou identificá-lo por hash;
- executar mudanças em transação;
- invalidar tudo quando schema ou parâmetros incompatíveis mudarem;
- definir comportamento concorrente entre leitores e sincronizador.

### 4.4 CLI e fallback

Adicionar comandos explícitos, por exemplo:

```text
docs-search index --root <projeto>
docs-search index --root <projeto> --rebuild
docs-search status --root <projeto>
```

`search` deve preservar o contrato de evidência. O campo `engine` pode mudar para identificar FTS5,
mas `schema_version: 2` só permanece se os campos continuarem compatíveis.

### 4.5 Testes

- criação e rebuild;
- arquivo novo, alterado, removido e renomeado;
- schema incompatível;
- banco corrompido;
- concorrência básica;
- equivalência de evidência com leitura direta;
- cache nunca sobrepondo documento atual;
- fallback explícito.

### Gate da Etapa 4

- qualidade igual ou superior ao melhor lexical direto;
- falso positivo de no-answer não piora;
- hashes, paths e linhas continuam verificáveis;
- ganho de latência relevante no corpus maior;
- rebuild e fallback testados;
- banco comprovadamente descartável.

Sem ganho mensurável, manter leitura direta e encerrar esta etapa sem adoção.

---

## Etapa 5 — embeddings e busca híbrida por RRF

### Pré-condição

Executar somente quando os relatórios mostrarem lacuna semântica relevante que tuning lexical e
FTS5 não resolveram. A decisão deve citar consultas concretas e métricas nos dois corpora.

### 5.1 Escolha e versionamento do modelo

Priorizar modelo local, redistribuível e adequado a português/inglês. Registrar:

- id e versão do modelo;
- dimensão e normalização do vetor;
- licença;
- tamanho e requisitos de CPU/RAM;
- hash ou mecanismo de integridade;
- estratégia de download separada do SQLite.

Pesos do modelo ficam fora do banco. O SQLite guarda somente vetores, metadados e identificação do
modelo que os produziu.

### 5.2 Schema vetorial

Adicionar metadados suficientes para invalidar vetores quando mudar:

- modelo ou versão;
- dimensão;
- normalização;
- texto/chunk hash;
- algoritmo de distância.

### 5.3 Fusão de rankings

Combinar ranking lexical e vetorial por Reciprocal Rank Fusion. Registrar no relatório:

- constante `k` do RRF;
- profundidade de cada lista candidata;
- contribuição lexical e vetorial;
- engine e modelo.

Testar a fusão com vetores mockados antes de depender de inferência real.

### Gate da Etapa 5

Adotar busca híbrida somente se:

- Recall@5 e MRR semânticos melhorarem nos dois corpora;
- no-answer não piorar;
- crescimento do contexto permanecer dentro do orçamento registrado;
- latência e memória forem aceitáveis;
- modo lexical continuar disponível como fallback;
- índices puderem ser totalmente reconstruídos dos documentos originais.

---

## Etapa 6 — integração, distribuição e operação

### 6.1 Compatibilidade e CI

Criar pipeline para:

- `cargo fmt --check`;
- `cargo clippy --all-targets --all-features -- -D warnings`;
- `cargo test`;
- avaliação na fixture estável;
- teste do instalador;
- matriz macOS/Linux e Rust MSRV/versão ativa;
- verificação do contrato JSON.

### 6.2 Distribuição

Manter `cargo install --locked` como caminho principal enquanto for suficiente. Adicionar binários
pré-compilados somente se instalação por Cargo for uma barreira medida. Nesse caso:

- publicar checksums;
- assinar ou atestar artefatos quando disponível;
- testar arquiteturas suportadas;
- preservar `--version` e compatibilidade de schema;
- documentar atualização e rollback.

### 6.3 Segundo cliente e MCP

Considerar MCP apenas se:

- clientes relevantes não puderem executar a CLI; ou
- processo residente trouxer ganho mensurável além do SQLite persistente.

MCP deve ser adaptador fino sobre a mesma biblioteca e os mesmos contratos; não deve criar um
segundo motor de ranking.

### 6.4 Documentação operacional

Ao mudar fluxo ou contrato, atualizar:

- `tools/docs-search/README.md`;
- `docs/runbooks/buscar-documentacao-de-projetos.md`;
- `skills/search-project-docs/SKILL.md`;
- `docs/checkpoints/project-state.md`;
- ADRs e este plano quando decisões arquiteturais mudarem.

---

## Dependências entre etapas

```text
Etapa 1: executor e métricas
    ↓
Etapa 2: baseline confiável + segundo corpus
    ↓
Etapa 3: hardening/tuning lexical
    ├── qualidade suficiente → manter arquitetura simples
    └── I/O/latência insuficiente → Etapa 4 SQLite/FTS5
                                   ↓
                          lacuna semântica comprovada
                                   ↓
                          Etapa 5 embeddings/RRF
                                   ↓
                          Etapa 6 distribuição/MCP
```

SQLite não depende de embeddings. Embeddings dependem de evidência obtida nas etapas anteriores.
Distribuição básica pode evoluir em paralelo, mas não deve publicar uma arquitetura ainda não
calibrada como solução definitiva.

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
