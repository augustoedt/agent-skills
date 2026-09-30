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

Os holdouts já usados permanecem proibidos. No máximo duas variantes poderão avançar para um gate
de execução única com holdouts inteiramente novos. Somente a vencedora aprovada será integrada ao
produto; o resultado válido também pode ser manter o baseline atual.

## Consequências

- (+) armazenamento, ranking lexical, semântica e fusão serão avaliados separadamente;
- (+) alternativas dominadas poderão ser descartadas antes de receber integração de produção;
- (+) o baseline e os contratos atuais permanecem estáveis durante o experimento;
- (+) resultados negativos continuarão auditáveis;
- (−) o bake-off exige harness operacional e instrumentação de RAM, disco, startup e indexação;
- (−) embeddings adicionam dependência de modelo, licença, distribuição e custo computacional;
- ⚠️ parâmetros e limites devem ser congelados antes de observar os rankings;
- ⚠️ nenhuma variante pode ser promovida usando os holdouts consumidos;
- ⚠️ implementar um protótipo não implica adoção nem manutenção futura.
