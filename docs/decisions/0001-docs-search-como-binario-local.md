# ADR 0001 — docs-search como binário local em Rust

- **Status:** aceito
- **Data:** 2026-09-28

## Contexto

Agentes diferentes precisam localizar documentação relevante sem carregar todo o corpus no
contexto. A solução deve ser independente de Claude, Gemini, GPT, Pi ou outro cliente, e a equipe
prefere distribuir uma ferramenta compilada.

## Decisão

Implementar `tools/docs-search` em Rust e expor inicialmente uma CLI com JSON versionado. A skill
`search-project-docs` será uma camada fina de instrução e não conterá o motor de busca.

O baseline fará leitura direta dos Markdown e ranking lexical. Se as métricas justificarem, um
único SQLite por projeto armazenará metadados, chunks, FTS5 e embeddings. Os documentos originais
continuam autoritativos e o banco é sempre reconstruível.

## Consequências

- qualquer agente capaz de executar um binário pode usar a mesma recuperação;
- o contrato CLI pode ser testado sem depender de um protocolo ou provedor;
- instalação e releases do binário precisam ser mantidos;
- busca semântica não existe na primeira etapa;
- integrações adicionais não fazem parte desta decisão; a interface pública permanece a CLI.
