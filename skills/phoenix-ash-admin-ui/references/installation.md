# Instalação e migração

## Gate de aplicação

Esta skill complementa `scaffold-phoenix-ash` somente quando o plano aprovado determina explicitamente que o admin de produto será implementado em Phoenix LiveView. Não execute este processo para API-only, frontend administrativo separado, admin fora da etapa ou simples AshAdmin de desenvolvimento.

## Princípio

A skill não é um instalador de biblioteca. Ela ajuda o agente a ejetar uma fundação pequena para dentro do projeto e adaptá-la conscientemente.

## Pré-requisitos

- Phoenix 1.8+;
- LiveView 1.1+;
- Tailwind CSS 4 configurado;
- Heroicons/core `<.icon>` do Phoenix;
- Ash/AshPhoenix apenas quando o projeto usa Ash.

## Scaffold opcional

```bash
python3 ~/.pi/agent/skills/phoenix-ash-admin-ui/scripts/scaffold.py \
  --project . \
  --web-module MyAppWeb \
  --web-path my_app_web \
  --brand "My App" \
  --initials MA \
  --with-dashboard \
  --confirm-admin-in-phoenix-plan
```

A confirmação é obrigatória e representa uma decisão já registrada no plano; ela não autoriza o agente a inferir essa decisão.

Cria, se ausentes:

```text
assets/css/admin-theme.css
assets/js/admin_sidebar_hook.js
lib/<web_path>/components/admin_components.ex
lib/<web_path>/live/admin/dashboard_live.ex  # somente com --with-dashboard
```

O dashboard opcional é deliberadamente vazio: substitua rotas placeholder e conecte-o a uma action Ash autorizada que retorne dados reais.

Se qualquer destino existir, o script falha sem sobrescrever nada. Nesse caso, inspecione e integre manualmente.

## Integração CSS

No `assets/css/app.css`, após imports/base do projeto:

```css
@import "./admin-theme.css";
```

Confirme que Tailwind 4 escaneia `lib/<web_path>`.

Não mantenha dois sistemas semânticos concorrentes. Se já houver tokens `--background`, `--primary`, etc., faça merge token a token em vez de importar o arquivo inteiro.

## Integração JS

No `assets/js/app.js`:

```javascript
import AdminSidebar from "./admin_sidebar_hook"

const Hooks = {AdminSidebar}
```

Se já houver Hooks:

```javascript
const Hooks = {...existingHooks, AdminSidebar}
```

Conecte `hooks: Hooks` ao `LiveSocket` conforme a estrutura existente.

## Router/autorização

Nunca copie uma live session genérica sem entender autenticação local.

Exemplo conceitual:

```elixir
scope "/admin", MyAppWeb.Admin do
  pipe_through [:browser, :admin_required]

  live_session :admin,
    on_mount: [{MyAppWeb.LiveUserAuth, :admin_required}] do
    live "/", DashboardLive, :index
  end
end
```

Em AshAuthentication Phoenix, preserve macros/live sessions geradas pelo projeto.

## Dashboard inicial

O dashboard inicial deve usar dados reais e baratos:

- counts limitados/aggregates adequados;
- último batch/release/job;
- alertas abertos;
- atalhos operacionais.

Não introduza quatro queries por card se uma action/aggregate pode retornar um resumo.

## Migração de PhiaUI

1. Liste imports/referências `PhiaUI` e dependency no lock.
2. Identifique somente components realmente usados.
3. Reimplemente ou adapte no namespace da aplicação.
4. Preserve licença se copiar porções substanciais.
5. Migre páginas e testes.
6. Remova hooks/assets/imports da lib.
7. Remova dependency e rode build limpo.
8. Verifique que nenhum `PhiaUI` permanece.

Não remova antes de as páginas compilarem com a substituição.

## Migração de Backpex/AshBackpex

1. Inventarie routes/resources/actions/fields.
2. Crie code interfaces Ash necessárias.
3. Recrie leitura/listagem primeiro.
4. Recrie forms somente quando edição é válida.
5. Recrie resource actions com confirmação e actor.
6. Teste policies independentemente da UI.
7. Remova imports, formatter deps, config e package.
8. Execute codegen/migration checks.

Não duplique resource Ash com schema Ecto para facilitar a UI.

## Verificação

```bash
bash ~/.pi/agent/skills/phoenix-ash-admin-ui/scripts/verify.sh .
```

Depois use os comandos do projeto. O script verifica apenas invariantes da skill; não substitui compile/test/assets.
