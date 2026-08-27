---
name: docs-organization
description: Organiza a pasta docs/ de um projeto no padrão do ecossistema (camara-intel/campanha-intel/softtrevo-lottery). Use quando o usuário pedir para organizar/estruturar a documentação, criar a pasta docs, definir "a função de cada pasta", criar/atualizar o checkpoint do projeto (project-state.md) ou registrar uma decisão de arquitetura (ADR).
---

# Organização de documentação — padrão do ecossistema

Padrão de docs dos projetos do usuário (origem: `campanha_intel_back/docs`,
adotado por `camara-intel` e `softtrevo-lottery`). Aplica-se a qualquer
projeto novo ou existente.

## Estrutura padrão

```
docs/
  README.md          ← índice obrigatório: uma seção por pasta explicando a FUNÇÃO dela
  decisions/         ← ADRs: um arquivo por decisão (data, contexto, consequências)
  plans/             ← planos de trabalho ainda não concluídos (documentos vivos)
  checkpoints/       ← project-state.md — arquivo ÚNICO e autoritativo
  reviews/           ← revisões de código e auditorias
  issues/            ← problemas conhecidos EM ABERTO (virou plano → sai daqui)
  archive/           ← documentos de etapas concluídas (histórico)
  benchmarks/        ← medições que sustentam decisões (pode ficar vazia)
  apresentacoes/     ← material pra não-técnicos (só sob demanda)
```

Pastas de domínio são bem-vindas quando o projeto precisa — precedentes:
`contracts/` (contratos de integração entre serviços), `operations/`
(infra/rotas), `architecture/` (diagramas/explicação), `legacy/` (dumps de
sistema legado). A regra: **toda pasta tem sua função explicada no
`docs/README.md`**.

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
6. Mover com `git mv` (preserva histórico) e **atualizar todas as
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
9. **Commit separado** só com a reorganização (não misturar com features).

## Checkpoint autoritativo — como escrever

Conteúdo denso, seções fixas: **Onde estamos** (fases/commits com hash),
**Em andamento** (WIP, inclusive não commitado, com arquivos e motivo),
**Próximo passo** (lista ordenada), **Armadilhas conhecidas**,
**Referências**. Regra de ouro: responder "se eu sumir amanhã, o que quem me
substituir precisa saber nos primeiros 30 minutos?".

## Sinais de que este skill se aplica

- "organiza a pasta docs", "documenta certinho", "qual a função de cada pasta"
- "cria o checkpoint", "atualiza o project-state"
- "registra essa decisão", "cria um ADR"
- Projeto novo sem `docs/` estruturada
