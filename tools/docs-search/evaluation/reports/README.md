# Relatórios de avaliação

Esta pasta contém somente relatórios compatíveis com o contrato atual do `docs-search`. Os Markdown
do projeto continuam sendo a fonte da verdade; relatórios são artefatos derivados e reconstruíveis.

## Contratos atuais

- ferramenta: `docs-search 0.5.0`;
- engine: `lexical-bm25-v1`;
- resposta de busca: schema v2;
- dataset de consultas: schema v2;
- relatório de avaliação: schema v2.

Schemas antigos e relatórios incompatíveis não são mantidos enquanto o projeto está em
desenvolvimento e sem consumidores externos.

## Comparação de diversidade por path

As três configurações foram executadas sobre o mesmo dataset e estado de cada corpus. `rank` é a
posição final; `raw_rank` preserva a posição BM25 antes da seleção por path.

### Fixture estável

| Configuração | Hit@1 | Recall@5 macro | Recall@5 micro | MRR@5 | FP no-answer | Contexto |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ilimitado | 0,705882 | 0,617647 | 0,619048 | 0,705882 | 0/3 | 7.453 |
| cap 1 | 0,705882 | 0,676471 | 0,714286 | 0,705882 | 0/3 | 3.995 |
| cap 2 | 0,705882 | 0,676471 | 0,714286 | 0,705882 | 0/3 | 6.043 |

### Corpus público atual

Corpus medido: 7 arquivos e 88 chunks.

| Configuração | Hit@1 | Recall@5 macro | Recall@5 micro | MRR@5 | FP no-answer | Contexto |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ilimitado | 0,647059 | 0,588235 | 0,619048 | 0,647059 | 0/3 | 18.626 |
| cap 1 | 0,647059 | 0,588235 | 0,619048 | 0,647059 | 0/3 | 13.951 |
| cap 2 | 0,647059 | 0,588235 | 0,619048 | 0,647059 | 0/3 | 14.442 |

No ranking ilimitado, casos exatos tiveram Hit@1 0,875000; semânticos, 0,166667; ambíguos,
1,000000. A lacuna semântica permanece mensurável, mas não justifica busca híbrida antes de ampliar
casos ambíguos/no-answer e concluir o gate de holdout.

Avaliações adicionais de desenvolvimento são executadas somente no ambiente privado. Quantidade de
corpora, métricas, queries, paths, fingerprints e direção dos resultados permanecem fora do Git.
Nenhum holdout foi executado.

**Decisão de desenvolvimento:** cap 1 continua candidato porque melhora a fixture e usa menos
contexto que cap 2 nos dois corpora públicos. Ele permanece opt-in até o gate de holdout.

A fixture reutiliza o dataset canônico, portanto `corpus.name` permanece `agent-skills`; nome do
arquivo, root e fingerprint distinguem a execução sintética.

## Reprodução

A partir de `tools/docs-search`:

```bash
cargo run --quiet -- evaluate \
  --root ../.. \
  --queries evaluation/queries.json \
  --limit 5 \
  --max-excerpt-chars 1200 \
  --output evaluation/reports/agent-skills-lexical-bm25-v1-unlimited-v2.json
```

Use `--max-results-per-path 1` ou `2` para as outras configurações. Todo relatório deve passar em
`../report.schema.json`. Um novo estado relevante substitui os relatórios correntes de forma
explícita; não há compatibilidade com contratos anteriores.
