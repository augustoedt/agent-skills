# ADR — manter diversidade cap 1 opt-in após o gate de holdout

- **Data**: 2026-09-29
- **Status**: vigente

## Contexto

O limite de um resultado por path melhorou Recall@5 e reduziu contexto na fixture pública, sem
regressão nas métricas públicas observadas. Por isso, `--max-results-per-path 1` foi congelado como
candidato: código, parâmetros, datasets e julgamentos foram fixados antes do gate.

Os holdouts privados foram julgados diretamente contra suas fontes, sem executar busca. Após
autorização explícita, cada um foi executado uma única vez. Os relatórios, corpora, métricas e
diagnósticos permanecem locais e não são publicados.

O candidato completou todas as consultas e preservou a abstention exigida, mas o conjunto de
holdouts falhou os pisos de qualidade definidos antes da execução. Usar esses resultados para retuning
transformaria os holdouts em corpora de desenvolvimento e invalidaria o gate.

## Decisão

Não promover cap 1 ao comportamento padrão do `docs-search`.

A flag `--max-results-per-path 1` permanece disponível de forma opt-in porque continua útil em
cenários medidos e não altera o ranking BM25 bruto. Os holdouts usados pelo gate são considerados
consumidos: não podem orientar tuning, mudança de julgamento nem nova tentativa do mesmo candidato.

Qualquer hipótese futura deve nascer e ser selecionada apenas nos corpora de desenvolvimento. Antes
de outro gate, novos holdouts devem ser reservados, julgados contra as fontes e congelados sem
observar rankings.

## Consequências

- (+) o default continua estável e não assume uma generalização que o gate não confirmou;
- (+) a opção de diversidade continua disponível para usos explícitos e experimentos reproduzíveis;
- (+) a separação entre development e holdout permanece íntegra;
- (−) a redução de contexto medida publicamente não passa a beneficiar todas as buscas por default;
- ⚠️ resultados dos holdouts consumidos podem documentar a decisão, mas nunca selecionar o próximo
  ajuste;
- ⚠️ SQLite/FTS5, embeddings, RRF ou outro aumento de complexidade continuam exigindo evidência
  independente nos corpora de desenvolvimento e um novo gate.

## Como verificar

`docs-search search --help` mostra `--max-results-per-path` como opcional, sem valor padrão, e uma
busca `--json` sem a flag retorna `selection.max_results_per_path` nulo.
