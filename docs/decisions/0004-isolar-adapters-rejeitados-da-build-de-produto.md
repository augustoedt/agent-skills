# ADR — isolar adapters rejeitados da build de produto

- **Data**: 2026-10-01
- **Status**: vigente

## Contexto

O bake-off e o gate one-shot encerraram com `lexical-retained`. SQLite/BM25, FTS5, E5 e RRF não
foram aprovados para adoção. Apesar disso, seus módulos, comandos privados e dependências pesadas
ainda faziam parte de toda compilação do `docs-search`. Isso aumentava o binário e o grafo de build
de um produto que usa somente BM25 direto.

Apagar imediatamente os protótipos reduziria o custo de produção, mas também eliminaria testes e
código úteis para auditar como os resultados históricos foram produzidos. Mantê-los no caminho
normal confundiria implementação experimental com suporte de produto.

## Decisão

A build de produto passa a ter `default = []`. Os módulos SQLite/BM25, FTS5, embeddings, RRF,
`bakeoff`, `holdout` e o SHA-256 usado pelo harness ficam atrás da feature não default
`experimental-adapters`.

Candle, Tokenizers e `rusqlite` com SQLite bundled tornam-se dependências opcionais ligadas somente
a essa feature. O CLI padrão expõe apenas os comandos de produto; `bakeoff` e `holdout` só existem
numa compilação explícita com a feature.

A validação terá duas trilhas:

1. build default, Clippy e testes comprovam o produto sem adapters rejeitados;
2. build `--all-features`, Clippy e testes preservam o arquivo experimental auditável.

A segunda trilha não autoriza rerun de holdouts, retuning, distribuição nem nova adoção. Os
artefatos consumidos e suas revisões permanecem imutáveis.

A decisão foi estabilizada em `docs-search 0.6.0`. Antes da atualização global, passaram instalação
limpa, atualização desde 0.1.0, rollback para 0.1.0 e nova atualização para 0.6.0 em roots isoladas.

## Consequências

- (+) a instalação normal não compila nem distribui Candle, Tokenizers ou SQLite bundled;
- (+) comandos privados de experimento não aparecem no CLI de produto;
- (+) o ranking, os schemas e o comportamento de `search` permanecem inalterados;
- (+) o código histórico continua compilável e coberto por testes;
- (+) a build release local caiu de aproximadamente 10,96 MB para 3,09 MB neste ambiente;
- (−) a matriz de CI precisa validar builds default e `--all-features` separadamente;
- (−) o arquivo experimental ainda exige manutenção mínima enquanto permanecer na árvore;
- ⚠️ `experimental-adapters` nunca deve ser habilitada pelo instalador ou pela distribuição normal;
- ⚠️ compilar o histórico não reabre protocolos, registries ou holdouts consumidos.

## Como verificar

- `tools/docs-search/Cargo.toml` tem `default = []` em `[features]`.
- `docs-search --help` lista apenas `search` e `evaluate`, sem `bakeoff` nem `holdout`.
- As duas trilhas de testes do runbook
  [`buscar-documentacao-de-projetos.md`](../runbooks/buscar-documentacao-de-projetos.md) passam:
  default e `--all-features`, rodadas em sequência.
