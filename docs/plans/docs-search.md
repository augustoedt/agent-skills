# Plano — docs-search

## Objetivo

Entregar busca documental local, portátil entre agentes e orientada a contexto mínimo suficiente.
Os Markdown do projeto permanecem autoritativos; todo índice é cache reconstruível.

## Etapa 1 — baseline lexical em Rust

- [x] Definir corpus padrão seguro.
- [x] Implementar chunking por headings Markdown.
- [x] Implementar ranking lexical BM25 e normalização Unicode.
- [x] Definir contrato JSON versionado com evidência e hashes.
- [x] Criar 20 consultas iniciais de avaliação.
- [x] Validar `fmt`, `clippy` e testes.
- [x] Instalar e executar o binário real.
- [ ] Medir Hit@1, Recall@5, MRR, no-answer, latência e tamanho do contexto.

## Etapa 2 — índice SQLite e FTS5

Executar apenas depois de registrar o baseline. Usar um SQLite por projeto com metadados de
schema, arquivos, chunks e tabela virtual FTS5. Sincronização incremental por hash; remoções e
renomes também precisam invalidar o cache.

## Etapa 3 — embeddings e busca híbrida

Adicionar somente se consultas semânticas reais mostrarem uma lacuna relevante. Guardar vetores e
identificação do modelo no mesmo SQLite, manter o modelo fora do banco e combinar rankings lexical
e vetorial por Reciprocal Rank Fusion.

## Etapa 4 — integração e distribuição

- estabilizar compatibilidade do JSON;
- medir em um segundo corpus maior;
- publicar artefatos/binários se a instalação por Cargo deixar de ser suficiente;
- considerar MCP apenas se clientes não puderem executar CLI ou se índice residente trouxer ganho
  mensurável.

## Critérios

- `docs/` e arquivos originais nunca são substituídos pelo índice;
- resultado inclui origem verificável e hashes;
- consultas sem resposta não recebem evidência artificial;
- qualquer aumento de complexidade precisa melhorar métricas registradas.
