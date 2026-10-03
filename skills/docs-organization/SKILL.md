---
name: docs-organization
description: Organiza e compacta docs/ como um grafo enxuto de Markdown no padrão documental do ecossistema. Use ao organizar documentação, reduzir docs sem perder decisões, criar docs/, definir relações entre documentos, atualizar project-state.md, registrar ADR, criar ou atualizar docs/runbooks/ (rotinas padrão de debug/ops), ou quando pedirem apresentação HTML em docs/apresentacoes/ (infra, API, módulos, fluxo) sob demanda.
---

# Organização de documentação — padrão do ecossistema

Padrão consolidado de documentação dos projetos do usuário. Aplica-se a
qualquer projeto novo ou existente sem depender de nomes ou paths de
checkouts privados.

## Estrutura padrão

```
docs/
  README.md          ← índice obrigatório: uma seção por pasta explicando a FUNÇÃO dela
  decisions/         ← ADRs: um arquivo por decisão (data, contexto, consequências)
  plans/             ← planos de trabalho ainda não concluídos (documentos vivos)
  checkpoints/       ← project-state.md — arquivo ÚNICO e autoritativo
  runbooks/          ← passo a passo vivo de debug/ops (um arquivo por operação)
  reviews/           ← revisões de código e auditorias
  issues/            ← problemas conhecidos EM ABERTO (virou plano → sai daqui)
  archive/           ← documentos de etapas concluídas (histórico)
  benchmarks/        ← medições que sustentam decisões (pode ficar vazia)
  apresentacoes/     ← HTML editorial pra não-técnicos (só sob demanda)
```

Pastas de domínio são bem-vindas quando o projeto precisa — precedentes:
`contracts/` (contratos de integração entre serviços), `operations/`
(infra/rotas), `architecture/` (diagramas/explicação), `legacy/` (dumps de
sistema legado). A regra: **toda pasta tem sua função explicada no
`docs/README.md`**.

## Grafo de Markdown

Trate `docs/` como um grafo portátil: cada `.md` é um nó, cada link Markdown
relativo é uma aresta e as pastas classificam os nós. Não adotar banco de
grafo, frontmatter complexo nem `[[wikilinks]]` como requisito — os links devem
continuar navegáveis no GitHub e em qualquer editor.

Caminho principal de retomada:

```text
docs/README.md → checkpoints/project-state.md → plano ativo
                                      ├→ decisões/contratos
                                      ├→ runbooks
                                      └→ reviews/benchmarks
```

Regras do grafo:

- todo documento operacional deve ser alcançável a partir de `docs/README.md`;
- cada fato tem um nó canônico; outros documentos resumem em uma frase e
  apontam para ele, sem copiar blocos;
- relações importantes usam rótulos claros: **Depende de**, **Implementa**,
  **Decidido por**, **Verificado em**, **Substitui**;
- links de volta só existem quando ajudam a navegação; não criar ciclos por
  simetria automática;
- documento concluído sai do caminho principal: o resultado vai para
  `reviews/` e o material histórico para `archive/`;
- detectar links quebrados e nós órfãos antes de encerrar a reorganização.

Bloco opcional e curto para documentos com relações não óbvias:

```md
## Relações

- **Depende de:** [Contrato X](../contracts/x.md)
- **Decidido por:** [ADR Y](../decisions/y.md)
- **Verificado em:** [Review Z](../reviews/z.md)
- **Substitui:** [Plano anterior](../archive/plano-anterior.md)
```

Não repetir relações que já estejam evidentes no primeiro parágrafo ou no
índice.

## Compactação sem perda de essência

Compactar é reescrever o conjunto, não apagar contexto às cegas. Preservar:
decisões e seus motivos, contrato vigente, estado atual, evidência final,
limitações, armadilhas ainda válidas e próximo passo. Tentativas intermediárias,
logs extensos, proibições temporárias e narrativas de sessão saem do checkpoint;
se ainda tiverem valor histórico, condensar em `reviews/` ou mover para
`archive/` com `git mv`.

Orçamentos indicativos, não limites mecânicos:

- `docs/README.md`: função da pasta + uma linha por documento;
- `checkpoints/project-state.md`: preferir 150–250 linhas; exceder só quando a
  retomada realmente exigir;
- bloco `Relações`: 3–6 arestas úteis;
- plano concluído: objetivo, resultado, decisões, evidência e limites; o diário
  de execução não permanece no caminho ativo.

A pergunta de corte é: **“isso muda uma decisão, o estado, o próximo passo ou
uma armadilha?”** Se não, referenciar uma evidência ou remover a duplicação.
O Git preserva versões anteriores; não usar o checkpoint como histórico de Git.

## Regras invioláveis

1. **Raiz limpa**: só `README.md` + arquivo de agentes (`CLAUDE.md`/
   `AGENTS.md`). Todo o resto da documentação vive em `docs/`.
2. **`docs/README.md` é o índice**: seção por pasta com a função dela e a
   lista dos documentos (uma linha cada).
3. **Checkpoint autoritativo único**: `docs/checkpoints/project-state.md`.
   Não é um por data — é UM arquivo, sempre atualizado ao fechar/pausar
   etapa. Serve pra retomada após compactação de chat ou troca de modelo.
4. **ADRs nunca são apagados**: decisão nova = arquivo novo; decisão
   revertida = mesmo arquivo atualizado com o desfecho.
5. **issues/ é só o que está aberto**: auditoria concluída → `reviews/`;
   etapa concluída → `archive/`; problema que virou trabalho → `plans/`.
6. **runbooks/ são vivos**: um arquivo por operação de debug/ops. Não
   arquivar quando a rotina muda — **editar o mesmo arquivo**. Novo tipo
   de acesso (SSH, sqlite, ZIP, backfill, …) = arquivo novo + linha no
   `docs/README.md`. Template: [templates/runbook.md](templates/runbook.md).
7. Mover com `git mv` (preserva histórico) e **atualizar todas as
   referências cruzadas** (código, testes, outros docs). Proteger WIP não
   commitado: nunca commitar arquivos de trabalho alheio junto com a
   reorganização.

## Procedimento (aplicar o padrão num projeto)

1. **Inventário**: `find . -name "*.md" -not -path "./node_modules/*"` +
   ler o início de cada doc pra classificar (decisão? plano? auditoria?
   problema aberto? contrato? histórico?).
2. **Mapear** cada doc pra pasta destino conforme as funções acima.
3. **Mover com `git mv`**; `.sql`/dumps de referência → `legacy/` ou
   `archive/`. Cuidado com arquivos referenciados por path em testes/código
   (ex.: CSV de fixture) — atualizar o path ou deixar e documentar.
4. **Atualizar referências**: grep por `docs/`, `issues/`, nomes de arquivo
   em `.md`, `.ts` e README; ajustar pros paths novos.
5. **Criar/atualizar `docs/README.md`** a partir de
   [templates/docs-README.md](templates/docs-README.md).
6. **Criar/atualizar `docs/checkpoints/project-state.md`** a partir de
   [templates/project-state.md](templates/project-state.md) — conteúdo real:
   estado, em andamento, próximo passo, armadilhas.
7. **Extrair ADRs**: decisões tomadas em conversa/plano que não estão
   escritas viram arquivos em `decisions/` — template em
   [templates/adr.md](templates/adr.md).
8. **Apontar nos arquivos de agentes**: `CLAUDE.md`/`AGENTS.md` devem mandar
   ler `docs/README.md` e o `project-state.md` antes de retomar trabalho.
9. **Validar o grafo**: conferir links relativos, listar `.md` não alcançáveis
   pelo índice e revisar ciclos/duplicações nos hubs. Arquivos deliberadamente
   privados ou templates podem ser exceções documentadas.
10. **Compactar ao fechar**: trocar narrativa de execução por resultado,
    decisão, evidência e limites; mover histórico útil com `git mv`.
11. **Commit separado** só com a reorganização (não misturar com features).

### Modo compactação

Quando o pedido for “enxugar”, “reduzir” ou “organizar sem perder a essência”:

1. congelar um inventário de nós, links, tamanho e status (ativo, concluído,
   superado, evidência ou histórico);
2. escolher o nó canônico de cada assunto e mapear duplicações antes de editar;
3. compactar primeiro `project-state.md`, mantendo somente o snapshot atual;
4. retirar planos concluídos de `plans/`, criando ou atualizando uma review de
   fechamento e arquivando apenas o histórico que ainda tem valor;
5. transformar cópias de contexto em resumo de uma frase + link rotulado;
6. provar preservação com uma tabela temporária “fato essencial → nó final”;
7. validar links, alcançabilidade desde o índice e `git diff --check`;
8. reler o caminho principal como alguém retomando o projeto sem o chat.

Não compactar ADRs apagando decisões: manter o arquivo e registrar o desfecho
ou a decisão sucessora. Não esconder WIP, bloqueadores ou riscos atuais em
`archive/`.

## Apresentação HTML (sob demanda)

Não criar ao organizar `docs/`. Só quando pedirem apresentação, página
HTML, “explica o sistema”, fluxo pra não-técnico, infra/API/módulos.

Arquivo: `docs/apresentacoes/<kebab>.html` (ex. `infraestrutura-e-api.html`,
`fluxo-<projeto>.html`). Listar em `docs/README.md` na seção
`apresentacoes/`.

### Como fazer

1. Ler `docs/checkpoints/project-state.md`, ADRs e o contrato real
   (router, domínios Ash, compose, portas). Não inventar arquitetura.
2. Copiar [templates/apresentacao.html](templates/apresentacao.html)
   **com o `<style>` intacto** — essa é a família visual canônica.
   Não restilizar, não usar shadcn, não gerar slides PPTX.
3. Preencher na língua do projeto (quase sempre pt-BR).
4. Anatomia obrigatória: capa (eyebrow, h1, lede, meta) → TOC sticky →
   `section.block` (opcional `.index` + `.status.done|.wip|.todo`) →
   `.prose` (máx. ~64ch) → diagramas SVG `.dgm` → `.data-table` /
   `.next-grid` / `.callout` / `.stepper` → rodapé + script do TOC.
5. Diagramas em SVG inline (nós `.node`, fluxo `.accent`, futuro
   `.dashed` / `.amber`). Não Mermaid, não print de tela.
6. Status do stepper = checkpoint. Rotas, portas e nomes de módulo =
   código. Identificadores em `.chip-code`.

Blocos típicos (escolher o que o pedido pedir; não forçar todos):
visão geral → infraestrutura → modelagem da API → um bloco por
domínio/módulo → contrato front → próximos passos → onde estamos.

Nomes canônicos: `docs/apresentacoes/fluxo-do-sistema.html` e
`docs/apresentacoes/infraestrutura-e-api.html`.

## Checkpoint autoritativo — como escrever

É um **snapshot**, não diário. Conteúdo denso, seções fixas: **Onde estamos**
(fases/commits ainda relevantes), **Em andamento** (WIP, inclusive não
commitado, com arquivos e motivo), **Próximo passo** (lista ordenada),
**Armadilhas conhecidas**, **Referências**. Resumir fatos encerrados em uma
linha com link para review/ADR/archive; remover instruções temporárias já
superadas. Regra de ouro: responder "se eu sumir amanhã, o que quem me
substituir precisa saber nos primeiros 30 minutos?".

## Runbooks — rotinas de debug/ops

Quando o agente (ou o utilizador) descobrir um acesso que vai repetir —
SSH a uma VM, sqlite local, baixar objecto privado, inspeccionar um
ficheiro, backfill — **não deixar só no chat**. Criar ou actualizar
`docs/runbooks/<kebab-da-operacao>.md`.

Conteúdo: Quando / Passos (comandos reais, paths reais) / Não fazer.
Sem secrets. IP/hostname no checkpoint, o runbook aponta para lá.
Rotina mudou → editar o mesmo arquivo, não criar `…-v2.md`.

## Sinais de que este skill se aplica

- "organiza a pasta docs", "documenta certinho", "qual a função de cada pasta"
- "cria o checkpoint", "atualiza o project-state"
- "registra essa decisão", "cria um ADR"
- "runbook", "passo a passo", "como acesso a VM", "como baixo o ZIP", debug/ops
- "apresentação HTML", "explica o sistema", "página em apresentacoes/"
- Projeto novo sem `docs/` estruturada
