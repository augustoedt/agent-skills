# Relatórios de avaliação

Esta pasta preserva medições derivadas e reproduzíveis do `docs-search`. Os documentos Markdown do
projeto continuam sendo a fonte da verdade; relatórios registram apenas o comportamento de uma
versão específica do motor sobre um estado identificado do corpus e do dataset.

## Baseline do corpus real

`agent-skills-lexical-bm25-v1-baseline.json` é a primeira medição sem tuning do corpus real
`agent-skills`. Ela foi produzida pelo `docs-search 0.2.0` sobre o estado anterior à documentação do
próprio resultado, no commit fonte `de77945`, com:

- engine: `lexical-bm25-v1`;
- dataset: schema v2, hash
  `7c068e714df8b59ada8af85a235522b41f77eb23860a463ef30ac3fd6d62485f`;
- corpus: 7 arquivos, 87 chunks, fingerprint
  `2efca8740d95cebe6829749cb21c17140d3e5647ac0f8279a3edc66aece56dd4`;
- configuração: `limit=5`, `max_excerpt_chars=1200`;
- execução: varredura completa do corpus a cada consulta.

### Resultado global

| Métrica | Resultado |
| --- | ---: |
| Consultas executadas | 20/20 |
| Hit@1 | 0,647059 |
| Recall@5 macro | 0,588235 |
| Recall@5 micro | 0,619048 |
| MRR@5 | 0,647059 |
| Falsos positivos no-answer | 0/3 |
| Latência média | 22,009 ms |
| Latência p50 / p95 | 22,108 / 23,199 ms |
| Caracteres de contexto | 18.326 |
| Contexto médio / p95 | 916,3 / 2.439 caracteres |

Latência é uma observação da máquina da execução, não um snapshot determinístico. Qualidade,
configuração, hashes e evidências por consulta são os campos primários para comparação.

### Resultado por categoria

| Categoria | Casos | Hit@1 | Recall@5 macro | Recall@5 micro | MRR@5 |
| --- | ---: | ---: | ---: | ---: | ---: |
| exact | 8 | 0,875000 | 0,812500 | 0,800000 | 0,875000 |
| semantic | 6 | 0,166667 | 0,166667 | 0,166667 | 0,166667 |
| ambiguous | 3 | 1,000000 | 0,833333 | 0,800000 | 1,000000 |
| no_answer | 3 | n/a | n/a | n/a | n/a |

### Diagnóstico inicial

- O lexical respondeu bem aos casos exatos e ambíguos, mas encontrou somente 1 dos 6 casos
  semânticos (`semantic-03`).
- Houve seis misses path-level: `exact-05`, `semantic-01`, `semantic-02`, `semantic-04`,
  `semantic-05` e `semantic-06`.
- `exact-08` e `ambiguous-03` recuperaram apenas um dos dois paths esperados no top 5.
- Os três casos no-answer retornaram vazio, mas essa amostra ainda é pequena demais para uma
  conclusão forte sobre abstention.
- Somente 4 das 17 consultas respondíveis recuperaram um heading esperado. Parte dessa lacuna vem
  da regressão conhecida em que uma linha `# ...` dentro de bloco cercado altera breadcrumbs do
  `README.md`; parte vem de seção incorreta ou miss completo.
- O contraste entre `exact` e `semantic` registra uma limitação lexical concreta, mas ainda não
  justifica embeddings sem ampliar casos ambíguos/no-answer e medir um segundo corpus.

## Reprodução

A partir de `tools/docs-search`, gere um relatório novo fora do baseline preservado:

```bash
cargo run --quiet -- evaluate \
  --root ../.. \
  --queries evaluation/queries.json \
  --limit 5 \
  --max-excerpt-chars 1200 \
  --output /tmp/agent-skills-lexical-bm25-v1.json
```

O relatório deve passar em `../report.schema.json`. Mudanças posteriores do corpus alteram seu
fingerprint, e latências podem variar. Não sobrescrever o baseline: novos motores, datasets,
configurações ou estados relevantes recebem outro arquivo e uma justificativa explícita.
