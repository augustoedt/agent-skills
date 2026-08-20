---
name: elixir-beam-memory-tuning
description: Reduz drasticamente (50-80%) o consumo de memória RAM de apps Elixir/Phoenix/Ash deployados em containers (Railway, Fly.io, Docker, VPS). Aplica tuning de VM BEAM via variáveis de ambiente (schedulers, alocadores, lazy code loading, pool do banco) e auditoria de dependências pesadas não usadas do ecossistema Ash. Use quando o usuário mencionar consumo alto de memória/RAM de app Elixir, custo alto de infra, OOM em container, ou ao preparar/otimizar deploy de Phoenix/Ash. Os defaults da BEAM assumem máquina dedicada — em container com muitas vCPUs eles desperdiçam centenas de MB.
---

# Elixir BEAM Memory Tuning para Deploys em Container

> Medido em produção (Railway, ago/2026): app Ash+Phoenix de 627 MB → 120 MB; app Phoenix puro de 199 MB → 105 MB. Apenas env vars + remoção de deps não usadas. Zero mudança de comportamento funcional.

## Por que apps Elixir "comem tanta RAM" em container

A BEAM assume por default que roda numa máquina dedicada e grande. Em container isso gera 3 desperdícios:

1. **Schedulers = nº de vCPUs visíveis.** Plataformas como Railway expõem 32+ vCPUs ao container. A BEAM sobe 32 schedulers, e cada um mantém **caches de memória próprios (MBCS carriers)**. Resultado típico: RSS fica **~300+ MB acima** do que a VM realmente usa (`:erlang.memory(:total)`). App low-traffic não precisa de mais de 2-4 schedulers.
2. **Release roda em `embedded` mode** — carrega TODOS os módulos de TODAS as apps no boot. Um app Ash típico carrega 100+ MB de código de uma vez. `mix phx.server` (modo `interactive`) carrega sob demanda.
3. **Pool do DB com 10 conexões** (`POOL_SIZE` default do `phx.new`) — cada conexão Ecto custa alguns MB. Apps pequenos ficam ótimos com 5.

## Receita (variáveis de ambiente)

### A) Deploy via `mix release` (Dockerfile multi-stage, padrão `phx.gen.release`)

```bash
RELEASE_MODE=interactive                                  # lazy code loading — o script do release lê essa var (RELEASE_MODE="${RELEASE_MODE:-"embedded"}")
ELIXIR_ERL_OPTIONS="+S 4:4 +MBas aobf +MBlmbcs 512"       # 4 schedulers + alocadores enxutos
MALLOC_ARENA_MAX=2                                        # arenas glibc p/ NIFs (bcrypt, picosat...)
POOL_SIZE=5                                               # pool Ecto (runtime.exs já lê essa var no padrão phx.new)
```

### B) Deploy via nixpacks / `mix phx.server` (sem release)

`mix phx.server` já roda em interactive — `RELEASE_MODE` não se aplica. Use `ERL_FLAGS` (o script `elixir`/`mix` repassa ao `erl`):

```bash
ERL_FLAGS="+S 4:4 +MBas aobf +MBlmbcs 512"
MALLOC_ARENA_MAX=2
POOL_SIZE=5
```

### O que cada flag faz

| Flag | Efeito |
|---|---|
| `+S 4:4` | Limita a 4 schedulers (+4 dirty CPU). Derruba o cache de carrier por scheduler — o maior ganho |
| `+MBas aobf` | Estratégia "address order best fit": menos fragmentação de heap |
| `+MBlmbcs 512` | Carrier multibloco máximo de 512 KB (default: vários MB): memória volta pro SO mais rápido |
| `MALLOC_ARENA_MAX=2` | Limita arenas do glibc malloc (usado por NIFs) — menos RSS fantasma |
| `POOL_SIZE=5` | 5 conexões Ecto em vez de 10 (~50-60 MB a menos) |

## Trade-offs (seja honesto com o usuário)

- **`+S 4:4` é o único com trade-off real**: cap de paralelismo CPU. Irrelevante para apps I/O-bound (99% dos Phoenix: esperam DB/HTTP). Só importa em burst CPU-bound pesado e simultâneo (geração massiva de PDFs, hashes bcrypt em massa). Se o app crescer: subir para `+S 8:8` é trocar uma env var.
- **Alocadores/arenas**: overhead de nanossegundos por alocação — nunca aparece em latência de request de app I/O-bound.
- **`interactive` mode**: primeiro hit em cada rota paga alguns ms de code loading (uma vez só). Steady state idêntico.

## Auditoria de dependências (apps Ash)

Apps Ash acumulam deps pesadas instaladas "por via das dúvidas". Cada uma custa MB de código no release e OTP apps no boot. Verifique uso real com grep antes de remover:

```bash
cd <app>
for pkg in AshAi ReqLLM AshEvents AshOban Oban.Web ObanWeb PhiaUI AshStateMachine OpenApiSpex; do
  echo "$pkg: $(grep -rl "$pkg" lib config --include='*.ex' --include='*.exs' | wc -l) arquivos"
done
```

Suspeitos comuns (achados reais):

| Dep | Função | Padrão comum |
|---|---|---|
| `ash_ai` (+ `req_llm`, ~200 arquivos de providers LLM) | MCP server p/ agentes de IA | Só usado via plug `AshAi.Mcp.Dev` em dev → marcar `only: [:dev, :test]` (tira `req_llm` do release de prod) |
| `oban_web` | dashboard LiveView de jobs Oban | Frequentemente montado só em dev (`dev_routes`) ou nem montado → remover ou dev-only |
| `ash_events` | event sourcing/audit log | Muitas vezes zero resources com `events do` → remover |
| `phia_ui` | componentes LiveView shadcn-style | Às vezes instalado e nunca usado (resta só `phia_hooks/` vazio em assets) → remover + limpar import no `app.js` |
| `dns_cluster` | clustering multi-nó via DNS | Inútil com 1 réplica (mas minúsculo) |

**MANTER (parecem removíveis mas são usados):** `oban`/`ash_oban` se algum resource tem `oban do`; `ash_state_machine` se algum resource tem `state_machine do`; `swoosh` (senders de auth); `picosat_elixir` (NIF do Ash Policy Authorizer); `gettext`; `bcrypt_elixir`; `jose`.

### ⚠️ Pegadinha de compile-time (aprendida na prática)

`import` é macro e **precisa do módulo disponível em tempo de compilação, mesmo dentro de branch morta de `if`**. Ex.: `import Oban.Web.Router` dentro de `if Application.compile_env(:app, :dev_routes)` quebra `MIX_ENV=prod mix compile` se `oban_web` for `only: [:dev, :test]`. Opções: remover o mount junto com a dep, ou `runtime: false` (compila em todos os envs mas não entra no release). Já `plug Modulo` (ex.: `plug Tidewave`) NÃO carrega o módulo em compile time — por isso deps dev-only funcionam no endpoint.

### Checklist de remoção

1. Remover dep do `mix.exs` (ou marcar `only: [:dev, :test]`)
2. `mix deps.unlock <dep>` + `mix deps.get` + `mix deps.unlock --unused` (limpa transitivos órfãos)
3. **Compilar nos dois envs**: `mix compile --warnings-as-errors` **e** `MIX_ENV=prod mix compile --warnings-as-errors` (o prod pega os imports/refs que dev mascararia)
4. Commit separado das mudanças de infra

## Verificação pós-deploy (não pule)

1. **Boot saudável**: logs mostram migrations + endpoint no ar.
2. **Modo de código** (release): `bin/<app> rpc 'IO.inspect(:code.get_mode())'` → deve ser `:interactive`.
3. **Memória interna da VM**: `bin/<app> rpc 'IO.inspect(:erlang.memory())'` — compare `total` e `code` com o baseline.
4. **RSS na plataforma**: métricas (Railway: `railway metrics --service <svc> --memory --json`). Aguardar alguns minutos — averages incluem o container antigo na janela.
5. **Smoke test**: 1 request em rota pública crítica (webhook, health, login).

### Acesso remoto rápido (Railway)

```bash
railway ssh keys add --key ~/.ssh/<key>.pub --name debug   # 1x por conta
railway ssh config --service <svc>                          # escreve bloco no ~/.ssh/config
ssh -i ~/.ssh/<key> -o StrictHostKeyChecking=accept-new railway-<svc> "bin/<app> rpc 'IO.inspect(:erlang.memory())'"
```

## Referência rápida: baseline esperado após tuning

| Tipo de app | RSS típico pós-tuning |
|---|---|
| Phoenix puro (Phoenix + Ecto + LiveView) | ~90-130 MB |
| Ash + Phoenix (stack completo: json_api, admin, auth, oban) | ~120-200 MB |

Se ficar muito acima disso, investigar leaks: top processos (`process_info(:memory)`), top ETS (`:ets.info(t, :memory)`), `:recon` se disponível.
