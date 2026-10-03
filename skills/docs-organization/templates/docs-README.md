# Documentação do projeto

Estrutura da documentação do `<nome-do-projeto>`. Modelo do ecossistema
(origem: `campanha_intel_back/docs`). Pastas de domínio adicionais são
bem-vindas — toda pasta tem sua função explicada aqui.

## Como navegar

Caminho principal: [`checkpoints/project-state.md`](checkpoints/project-state.md)
→ plano ativo → decisões/contratos → evidências. Cada item abaixo ocupa uma
linha e aponta para o nó canônico; conteúdo concluído sai do caminho principal
para `reviews/` ou `archive/`.

## decisions/

Registros de decisão de arquitetura (ADRs) vigentes. Um arquivo por decisão,
com data, contexto e consequências. Decisão nova = arquivo novo; decisão
revertida = arquivo atualizado com o desfecho, nunca apagado.

## plans/

Planos de trabalho ainda não concluídos (documentos vivos). Listar uma linha
por plano com status; planos encerrados não permanecem aqui.

## checkpoints/

Estado atual do projeto, para retomada após compactação de chat ou troca
de modelo.

- `project-state.md` — **checkpoint autoritativo** (único arquivo, sempre
  atualizado ao fechar/pausar etapa)

## runbooks/

Passo a passo **vivo** de debug/ops. Um arquivo por operação; actualizar
quando a rotina mudar. Não é plano nem ADR. Pasta pode começar vazia.

## reviews/

Revisões de código e auditorias (concluídas ou em curso).

## issues/

Problemas conhecidos em aberto que ainda não viraram plano.

## archive/

Documentos históricos ou de etapas concluídas, preservados para referência e
fora do caminho principal de retomada.

## benchmarks/

Resultados de medição que sustentam decisões. Vazio por ora.

## apresentacoes/

Material explicativo para quem não acompanha o dia a dia técnico.
HTML editorial **só sob demanda** (skill docs-organization, template
`apresentacao.html`). Pasta pode ficar vazia.
