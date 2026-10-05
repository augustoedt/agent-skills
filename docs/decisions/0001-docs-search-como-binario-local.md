# ADR 0001 — docs-search como binário local em Rust

- **Status:** aceito; trecho sobre SQLite superado pelos ADRs 0003 e 0004
- **Data:** 2026-09-28

> **Vigente hoje:** `docs-search` é um binário Rust local com CLI e JSON versionado; a skill
> `search-project-docs` é só instrução; os Markdown originais são a fonte da verdade. O caminho
> condicional "um único SQLite por projeto com FTS5 e embeddings" foi avaliado e **rejeitado** — ver
> [ADR 0003](0003-comparar-engines-antes-da-adocao.md) e
> [ADR 0004](0004-isolar-adapters-rejeitados-da-build-de-produto.md).

## Contexto

Agentes diferentes precisam localizar documentação relevante sem carregar todo o corpus no
contexto. A solução deve ser independente de Claude, Gemini, GPT, Pi ou outro cliente, e a equipe
prefere distribuir uma ferramenta compilada.

## Decisão

Implementar `tools/docs-search` em Rust e expor inicialmente uma CLI com JSON versionado. A skill
`search-project-docs` será uma camada fina de instrução e não conterá o motor de busca.

O baseline fará leitura direta dos Markdown e ranking lexical. ~~Se as métricas justificarem, um
único SQLite por projeto armazenará metadados, chunks, FTS5 e embeddings.~~ *(Superado: as métricas
não justificaram; ver ADRs 0003 e 0004.)* Os documentos originais continuam autoritativos.

## Consequências

- qualquer agente capaz de executar um binário pode usar a mesma recuperação;
- o contrato CLI pode ser testado sem depender de um protocolo ou provedor;
- instalação e releases do binário precisam ser mantidos;
- busca semântica não existe na primeira etapa;
- integrações adicionais não fazem parte desta decisão; a interface pública permanece a CLI.

## Como verificar

```bash
docs-search --version
docs-search search --root . --query "<termo>" --limit 1 --json
```

O binário responde sem rede nem índice persistente, com `schema_version` 2 e `engine`
`lexical-bm25-v1`.
