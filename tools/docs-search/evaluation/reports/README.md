# Relatórios de avaliação

Esta pasta contém somente relatórios compatíveis com o contrato atual do `docs-search`. Os Markdown
do projeto continuam sendo a fonte da verdade; relatórios são artefatos derivados e reconstruíveis.

## Contratos atuais

- ferramenta: `docs-search 0.6.0-alpha.5`; o corpus público foi regenerado nesta versão; a fixture estável histórica permanece em 0.5.0;
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

Corpus medido: 9 arquivos e 96 chunks.

| Configuração | Hit@1 | Recall@5 macro | Recall@5 micro | MRR@5 | FP no-answer | Contexto |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ilimitado | 0,647059 | 0,588235 | 0,619048 | 0,647059 | 0/3 | 18.517 |
| cap 1 | 0,647059 | 0,588235 | 0,619048 | 0,647059 | 0/3 | 14.084 |
| cap 2 | 0,647059 | 0,588235 | 0,619048 | 0,647059 | 0/3 | 14.333 |

No ranking ilimitado, casos exatos tiveram Hit@1 0,875000; semânticos, 0,166667; ambíguos,
1,000000. A lacuna semântica permanece mensurável, mas não autoriza busca híbrida sem uma hipótese
isolada nos corpora de desenvolvimento e um novo gate.

Avaliações adicionais e o gate de holdout foram executados somente no ambiente privado. Identidades,
quantidade de corpora, métricas detalhadas, queries, paths, fingerprints e diagnósticos por consulta
permanecem fora do Git. Cada holdout executou uma única vez e agora está consumido.

**Decisão após o gate:** cap 1 não será promovido a default porque falhou os pisos de qualidade
predefinidos nos holdouts. A flag permanece opt-in pelos ganhos públicos de recall e contexto. A
hipótese posterior de prefixo morfológico limitado foi medida duas vezes somente em development e
rejeitada sem ajuste de parâmetros. O próximo ciclo é um bake-off faseado de engines; protocolo,
configurações, inputs e budgets já foram congelados antes da implementação. Nenhum relatório privado
é publicado aqui.

A fixture reutiliza o dataset canônico, portanto `corpus.name` permanece `agent-skills`; nome do
arquivo, root e fingerprint distinguem a execução sintética.

## Reprodução

A partir de `tools/docs-search`:

```bash
OUT="/tmp/agent-skills-lexical-bm25-v1-unlimited-v2.json"
test ! -e "$OUT" || { echo "recusando sobrescrita: $OUT" >&2; exit 1; }
cargo run --quiet --locked -- evaluate \
  --root ../.. \
  --queries evaluation/queries.json \
  --limit 5 \
  --max-excerpt-chars 1200 \
  --output "$OUT"
```

Use `--max-results-per-path 1` ou `2` para as outras configurações. Todo relatório deve passar em
`../report.schema.json`. Um novo estado relevante substitui os relatórios correntes de forma
explícita; não há compatibilidade com contratos anteriores.
