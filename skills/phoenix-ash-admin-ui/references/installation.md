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

### Projeto já usa daisyUI

Detecte antes de decidir a estratégia: procure `@plugin "daisyui...` e blocos
`@plugin "daisyui/packages/bundle/daisyui-theme" { name: "..."; ... }` em
`assets/css/app.css`. Se existirem, **não** ejete `admin-theme.css` como arquivo
à parte — o projeto já tem um sistema de temas nativo (OKLCH, por atributo
`data-theme`) equivalente em propósito ao do shadcn. Dois sistemas de tokens
semânticos concorrentes é exatamente o anti-padrão que este documento já pede
para evitar; com daisyUI presente, a forma de evitá-lo é reaproveitar os temas
existentes em vez de importar uma paleta nova.

Mapeamento recomendado (reaproveitar em vez de inventar):

| Conceito shadcn | Reaproveite do daisyUI |
|---|---|
| `background`/`foreground` | `base-100` / `base-content` |
| `card`/`popover` | `base-100` (ou `base-200` para leve contraste) |
| `primary`/`secondary`/`accent` (+ `-foreground`) | `primary`/`secondary`/`accent` (+ `-content`) |
| `muted` | `base-200` |
| `muted-foreground` | `base-content` com opacidade Tailwind (`text-base-content/60`), não um token novo |
| `destructive` | `error` (+ `error-content`) |
| `border`/`input` | `base-300` (utilitário `border-base-300` já existe, não crie `--color-border`) |
| `ring`/foco | `primary` (`ring-primary`) ou `base-content` |
| `radius` | os três radii nativos do daisyUI (`--radius-selector`/`-field`/`-box`) — não adicione uma escala `--radius` paralela |

Sem equivalente nativo — só esses precisam de token novo, e mesmo assim como
**adição** aos blocos de tema existentes, nunca como arquivo/paleta separada:

- `--color-sidebar*` (7 tokens do shell) — alias para cores do próprio tema,
  ex.: `--color-sidebar: var(--color-base-200); --color-sidebar-accent:
  var(--color-primary);`, não uma paleta importada;
- `--color-chart-1..5` — só quando um gráfico real for planejado; não
  adiante isso na fundação inicial (ver "Gráficos" no sistema visual).

**Armadilha verificada, não hipotética**: o tema daisyUI declara `--border`
como **largura** (ex.: `--border: 1.5px`), consumida pelo CSS interno dos
componentes daisyUI — não é uma cor. O `admin-theme.css` desta skill declara
`--border` como **cor** (`oklch(...)`) no mesmo escopo efetivo (`:root`/
`[data-theme=...]` têm a mesma especificidade; quem carrega depois vence
silenciosamente, sem erro de build). Reusar o nome `--border` para a cor do
admin quando daisyUI está presente corrompe um dos dois usos sem aviso
nenhum — bordas ficam com espessura errada ou a cor do admin nunca aplica.
Nunca redeclare `--border` neste cenário; use um utilitário daisyUI existente
(`border-base-300`) em vez de um token dedicado.

Adaptação prática: **não** copie `admin_components.ex.eex` literalmente —
ele referencia classes shadcn (`bg-background`, `border-border`,
`text-muted-foreground`, etc.). Reescreva as classes desses componentes para
os equivalentes daisyUI da tabela acima antes de integrá-lo. O
`scaffold.py` detecta daisyUI automaticamente e pula a criação de
`admin-theme.css` — os componentes ainda precisam desse ajuste manual de
classes, o script não reescreve HEEx.

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
