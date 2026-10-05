---
name: docs-organization
description: Organiza e compacta docs/ como um grafo enxuto de Markdown no padrão documental do ecossistema. Use ao organizar documentação, reduzir docs sem perder decisões, criar docs/, definir relações entre documentos, atualizar project-state.md, registrar ADR, criar ou atualizar docs/runbooks/ (rotinas padrão de debug/ops), documentar branch experimental ou de demonstração, ou quando pedirem apresentação HTML em docs/apresentacoes/ (infra, API, módulos, fluxo) sob demanda.
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
  branches/          ← só dentro de branch experimental/demo (ver seção própria)
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

### Roteamento por tarefa

Documento que ninguém sabe *quando* ler não é lido: o índice dizer o que cada
arquivo **é** não basta. O `docs/README.md` tem, logo após "Como navegar", a
tabela **Antes de… → leia**, uma linha por tarefa recorrente (rodar/testar,
release/deploy, demo, migração de dados, mexer em autorização, operação de
produção…) apontando para o runbook, ADR ou documento de arquitetura que deve
ser lido antes. `AGENTS.md`/`CLAUDE.md` apontam para essa tabela em vez de
repetir a lista.

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
- detectar links quebrados e nós órfãos antes de encerrar a reorganização;
- documento não copia o que o código já diz (moduledoc, `--help`, saída de
  task, schema): aponta para o comando ou arquivo que é a fonte.

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
6. **runbooks/ são vivos e verificáveis**: um arquivo por operação de
   debug/ops. Não arquivar quando a rotina muda — **editar o mesmo arquivo**.
   Novo tipo de acesso (SSH, sqlite, ZIP, backfill, …) = arquivo novo + linha
   no `docs/README.md`. Todo runbook declara premissas, um passo de
   verificação e quando foi executado de verdade pela última vez; runbook
   nunca executado é marcado **não verificado**. Template:
   [templates/runbook.md](templates/runbook.md).
7. Mover com `git mv` (preserva histórico) e **atualizar todas as
   referências cruzadas** (código, testes, outros docs). Proteger WIP não
   commitado: nunca commitar arquivos de trabalho alheio junto com a
   reorganização.
8. **Estado fora do Git também é documentado**: mudança feita em painel ou
   CLI de plataforma (domínio, variável, serviço, credencial, DNS, cron,
   webhook, permissão de integração) atualiza o runbook correspondente **no
   mesmo passo**. Registrar a fonte da verdade e como consultá-la, não o valor
   volátil: "domínio: `<comando ou tela>`" em vez do domínio literal.
9. **Docs andam com a mudança**: atualização incremental de checkpoint,
   runbook ou ADR causada por uma mudança vai **no mesmo commit** dela.
   Commit só de docs é para reorganização/compactação ou registro sem código.

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
   [templates/docs-README.md](templates/docs-README.md), incluindo a tabela
   **Antes de… → leia** (ver Roteamento por tarefa).
6. **Criar/atualizar `docs/checkpoints/project-state.md`** a partir de
   [templates/project-state.md](templates/project-state.md) — conteúdo real:
   estado, em andamento, próximo passo, armadilhas.
7. **Extrair ADRs**: decisões tomadas em conversa/plano que não estão
   escritas viram arquivos em `decisions/` — template em
   [templates/adr.md](templates/adr.md).
8. **Apontar nos arquivos de agentes**: `CLAUDE.md`/`AGENTS.md` devem mandar
   ler `docs/README.md` e o `project-state.md` antes de retomar trabalho, e
   consultar a tabela **Antes de… → leia** antes de cada tarefa.
9. **Validar o grafo**: conferir links relativos, listar `.md` não alcançáveis
   pelo índice e revisar ciclos/duplicações nos hubs. Também: pastas presentes
   no índice mas inexistentes (ou o inverso), lacunas na numeração de ADRs sem
   explicação e números contraditórios no checkpoint. Arquivos deliberadamente
   privados ou templates podem ser exceções documentadas.
10. **Compactar ao fechar**: trocar narrativa de execução por resultado,
    decisão, evidência e limites; mover histórico útil com `git mv`.
11. **Commits**: reorganização/compactação em commit próprio, sem features;
    atualizações incrementais seguem a regra inviolável 9. Se push dispara deploy ou CI,
    configurar o filtro de caminho para ignorar `docs/**` (watch paths,
    `paths-ignore`, ignored build step…) e registrar isso no runbook de deploy.
12. **Teste de retomada**: lendo só `docs/README.md` e o checkpoint, responder:
    (1) como rodo localmente? (2) onde está produção e como faço deploy?
    (3) o que está pela metade? (4) qual o próximo passo? (5) o que não posso
    fazer? Resposta que exige ler código ou o chat = corrigir o doc agora.

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
superadas.

Ao atualizar, **reescrever a seção afetada**; nunca só acrescentar um item por
commit — isso transforma o snapshot em diário. Números que envelhecem rápido
(contagem de testes, versões, métricas) só entram se condicionam o próximo
passo, com data, e nunca em duas versões no mesmo arquivo. Regra de ouro: responder "se eu sumir amanhã, o que quem me
substituir precisa saber nos primeiros 30 minutos?".

## Runbooks — rotinas de debug/ops

Quando o agente (ou o utilizador) descobrir um acesso que vai repetir —
SSH a uma VM, sqlite local, baixar objecto privado, inspeccionar um
ficheiro, backfill — **não deixar só no chat**. Criar ou actualizar
`docs/runbooks/<kebab-da-operacao>.md`.

Conteúdo: Quando / Premissas / Passos (comandos reais, paths reais) /
Verificação / Não fazer, com a linha **Verificado em** no topo. Sem secrets.
Rotina mudou → editar o mesmo arquivo, não criar `…-v2.md`.

- **Premissas** explicitam do que a rotina depende e que pode ser diferente
  em outro ambiente: fuso horário do servidor, domínios/hosts, credenciais
  externas obrigatórias ou opcionais, provedores (email, SMS, pagamento),
  permissões de integração (ex.: plataforma sem acesso à organização Git).
  Escrever a premissa força a pergunta "e se não for assim?" antes do deploy.
- **Verificação** é um comando ou passo que prova que a rotina funcionou; no
  runbook de deploy, um teste por fluxo crítico (checklist pós-deploy).
- **Verificado em** registra data e contexto da última execução real. Runbook
  que descreve algo nunca executado (ex.: release que nunca subiu) fica
  marcado **não verificado** — não descrever como pronto o que nunca rodou.
- Valores voláteis (IP, hostname, domínio, IDs) ficam no checkpoint ou são
  consultados na fonte; o runbook diz como obtê-los.

## Planos — resumo no topo

Todo plano em `plans/` começa com um bloco **Resumo** de até ~15 linhas:
status, fase atual, pendências/gates, próximo passo e o que é proibido sem
aprovação, seguido de "detalhes abaixo só para quem vai executar o plano".
Quem precisa apenas do estado para no resumo. Template:
[templates/plan.md](templates/plan.md).

## ADRs — substituição parcial e verificação

- Decisão parcialmente substituída: o ADR antigo ganha no topo um bloco
  **Vigente hoje** (o que ainda vale, em poucas linhas) e cada trecho que
  deixou de valer é marcado no próprio texto apontando para o sucessor. O
  leitor não deve precisar filtrar mentalmente regra vigente de regra morta.
- Todo ADR tem **Como verificar**: teste, comando ou inspeção que prova que a
  decisão continua valendo no código.
- Lacuna de numeração (ADR nunca criado ou número pulado) é explicada no
  índice; ADR não é apagado (regra inviolável 4).

## Branches experimentais ou de demonstração

Branch descartável (MVP para mostrar, spike, experimento) não altera o grafo
da linha principal: checkpoint, ADRs, runbooks e índice da `main` continuam
descrevendo o produto real.

- Registrar tudo num único `docs/branches/<nome-da-branch>.md`, ligado ao
  `docs/README.md` **dentro da própria branch**, começando por: "Branch
  descartável — não aplicável à `main`".
- Listar ali as exceções a ADRs/armadilhas vigentes (qual regra é violada e
  por quê), como rodar/publicar a demo e as premissas do ambiente dela.
- Se a branch virar produto, as exceções viram ADRs novos e o documento sai
  para `archive/` no merge.

## Sinais de que este skill se aplica

- "organiza a pasta docs", "documenta certinho", "qual a função de cada pasta"
- "cria o checkpoint", "atualiza o project-state"
- "registra essa decisão", "cria um ADR"
- "runbook", "passo a passo", "como acesso a VM", "como baixo o ZIP", debug/ops
- "apresentação HTML", "explica o sistema", "página em apresentacoes/"
- "branch de demo", "MVP descartável", "spike", experimento fora da `main`
- Projeto novo sem `docs/` estruturada
