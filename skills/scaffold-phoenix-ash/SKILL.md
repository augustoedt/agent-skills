---
name: scaffold-phoenix-ash
description: Cria o scaffold de um backend Elixir com Phoenix + Ash Framework (Ash Postgres, Ash JSON API, Ash Authentication, Ash Admin, Ash Oban, Ash State Machine, Ash Events, Live Debugger, Ash AI, usage_rules) e PostgreSQL via Docker Compose. Use quando o usuário pedir para criar um novo projeto backend Elixir/Phoenix/Ash, ou mencionar "phx.new", "igniter.install ash", "ash.setup", "ash_phoenix" ou "usage_rules".
---

# Scaffold Phoenix + Ash

Automatiza a criação de um backend Elixir com o stack padrão:

**Phoenix · Ash Framework · Ash Postgres · Ash JSON API · Ash Authentication (password) · Ash Admin · Ash Oban · Ash State Machine · Ash Events · Live Debugger · Ash AI · usage_rules**

## Parâmetro

| Parâmetro | Exemplo |
|---|---|
| `<project-name>` (kebab-case) | `campanha-intel-back` |

Pergunte o nome do projeto se o usuário não informou. O nome vira o app atom no mix.exs com underscores (`campanha-intel-back` → `:campanha_intel_back`).

## Passos

### 1. Criar o projeto Phoenix

```bash
mix phx.new <project-name>
cd <project-name>
```

Se o `mix phx.new` perguntar se quer instalar/fetch as dependências, responder sim (ou rodar `mix deps.get` depois).

### 2. Instalar a stack Ash via Igniter

```bash
mix igniter.install \
  ash \
  ash_phoenix \
  ash_json_api \
  ash_postgres \
  ash_authentication \
  ash_authentication_phoenix \
  ash_admin \
  ash_oban \
  oban_web \
  ash_state_machine \
  ash_events \
  live_debugger \
  ash_ai \
  usage_rules \
  --auth-strategy password \
  --yes
```

> **Bootstrap do igniter:** se o comando `mix igniter.install` não existir (igniter não instalado), instale o archive global primeiro:
> `mix archive.install hex igniter_new`

### 3. PostgreSQL via Docker Compose

O nome do banco vem do `config/dev.exs`. Gere o compose com o script (roda de dentro do projeto):

```bash
bash ~/.pi/agent/skills/scaffold-phoenix-ash/scripts/generate-docker-compose.sh .
```

O script lê `database: "..."` do `config/dev.exs` e cria o `docker-compose.yml` (postgres:16-alpine, container `<project>_postgres`, db `<app>_dev`, porta 5432). Se o script falhar, monte na mão usando o template em `templates/docker-compose.yml`, substituindo `{{PROJECT_NAME}}` e `{{DB_NAME}}` (extraído do dev.exs).

Suba o banco:

```bash
docker compose up -d postgres
docker compose ps   # aguardar status "healthy"
```

> Se a porta 5432 já estiver em uso por outro projeto, mapear outra porta (ex.: `"5433:5432"`) e ajustar a config do Repo no dev.exs de acordo.

### 4. Configurar skills do Ash e Phoenix (usage_rules)

**a) Editar `mix.exs`** — adicionar `usage_rules: usage_rules()` dentro de `def project do` (após `app:`, `version:`):

```elixir
def project do
  [
    app: :<app_name>,
    version: "0.1.0",
    # ...
    usage_rules: usage_rules()
  ]
end
```

**b)** Adicionar a função `usage_rules/0` no mesmo módulo, por exemplo antes de `deps/0`:

```elixir
defp usage_rules do
  [
    file: "AGENTS.md",
    usage_rules: ["usage_rules:all"],
    skills: [
      location: ".agents/skills",
      build: [
        "ash-framework": [
          description:
            "Use esta skill ao trabalhar com Ash Framework ou suas extensões.",
          usage_rules: [:ash, ~r/^ash_/]
        ],
        "phoenix-framework": [
          description:
            "Use esta skill ao trabalhar com Phoenix, controllers, LiveView e camada web.",
          usage_rules: [:phoenix, ~r/^phoenix_/]
        ]
      ]
    ]
  ]
end
```

**c)** Gerar os arquivos de skill:

```bash
printf '\n' | mix usage_rules.sync
```

> **Não-TTY:** sem terminal interativo, o igniter pode crashar com `String.trim/1` em `Igniter.Util.IO.select`. Piping `printf '\n'` (ou `echo ""`) resolve — o prompt de confirmação do diff recebe o default e o sync completa.

## Ordem de execução final

Após os passos acima, executar **nesta ordem**:

```bash
docker compose up -d          # 1. banco no ar
mix deps.compile              # 2. compila as dependências
mix usage_rules.sync          # 3. gera AGENTS.md + skills do ash/phoenix
mix ash.setup                 # 4. setup do Ash (cria Repo, migrações, etc.)
mix phx.server                # 5. sobe o servidor
```

Notas sobre o `mix ash.setup`:
- Se ele perguntar qual banco de dados, escolher **postgres**.
- Se perguntar sobre Ecto/igniter installs, aceitar as opções padrão.
- `mix phx.server` é long-running: rodar em background ou num painel separado (ex.: um painel herdr), não no mesmo terminal do passo-a-passo.

## Verificação

```bash
curl -s http://localhost:4000        # Phoenix responde HTML (página inicial)
```

Se o `ash.setup` criou migrações pendentes, aplicá-las com:

```bash
mix ash.migrate   # ou mix ash.gen.migration se precisar gerar antes
```

## Integração condicional com admin de produto

A extensão `ash_admin` instalada por esta skill é uma ferramenta técnica e não implica desenvolver um admin de produto em Phoenix.

A skill global `phoenix-ash-admin-ui` é apenas um complemento opcional. Use-a depois deste scaffold **somente se o plano aprovado definir explicitamente que o painel/admin/backoffice será implementado no próprio Phoenix LiveView**. Não a acione para API-only, frontend administrativo separado, admin fora da etapa ou pela simples presença de Phoenix, Ash ou AshAdmin.

## Notas

- O nome do banco deve sempre ser extraído do `config/dev.exs` (fonte da verdade) — não inventar.
- `.env` / segredos: o padrão Phoenix/Ash usa `config/runtime.exs` + variáveis de ambiente; não versionar segredos.
- Para novos módulos depois do scaffold: `mix ash.gen.resource <nome>` seguindo o padrão Ash (Resource + Actions + `ash.setup` para migrações).
