---
name: scaffold-sveltekit
description: Cria o scaffold de um frontend SvelteKit (TypeScript, Tailwind, ESLint, Prettier, Vitest, Playwright, Paraglide i18n pt-br/en, adapter Bun) e configura a integração com o backend Phoenix + Ash via Hey API (@hey-api/openapi-ts), gerando um SDK tipado a partir do openapi.json do Ash JSON API. Use quando o usuário pedir para criar um novo frontend SvelteKit/Svelte, ou mencionar "sv create", "sveltekit", "hey-api", "openapi-ts", "adapter-bun" ou "paraglide".
---

# Scaffold SvelteKit Frontend

Automatiza a criação de um frontend **SvelteKit** com o stack padrão do projeto:

**SvelteKit (TypeScript) · Tailwind CSS (typography + forms) · ESLint · Prettier · Vitest (unit + component) · Playwright · Paraglide (pt-br/en) · adapter Bun · Hey API (OpenAPI → SDK tipado)**

Pensado para ser o par do backend criado pela skill `scaffold-phoenix-ash`: o Ash JSON API expõe a spec OpenAPI, o `@hey-api/openapi-ts` consome essa spec e gera o SDK tipado, e as páginas SvelteKit usam esse SDK numa estrutura plain e repetitiva (`+page.server.ts` para data fetching e `+page.svelte` para renderização), sem escrever `fetch` ou validação manualmente.

## Parâmetro

| Parâmetro | Exemplo |
|---|---|
| `<project-name>` (kebab-case) | `campanha-intel-front` |

Pergunte o nome do projeto se o usuário não informou. Use **kebab-case** (o Svelte CLI aceita, mas `snake_case` como `campanha_intel_front` é incomum e deve ser normalizado).

## Passos

### 1. Criar o projeto SvelteKit

Comando one-shot (equivale às escolhas interativas: template `minimal`, TypeScript `ts`, addons `prettier eslint vitest playwright tailwindcss sveltekit-adapter ai-tools paraglide`, package manager `bun`):

```bash
bun x sv@0.17.0 create \
  --template minimal \
  --types ts \
  --add prettier eslint \
  vitest="usages:unit,component" \
  playwright \
  tailwindcss="plugins:typography,forms" \
  sveltekit-adapter="adapter:node" \
  ai-tools="ide:other" \
  paraglide="languageTags:pt-br,en+demo:yes" \
  --install bun \
  <project-name>
```

Equivalência das escolhas interativas (caso o one-shot não rode e seja preciso responder o prompt):

- Template: `SvelteKit minimal`
- TypeScript: `Yes, using TypeScript syntax`
- Addons: `prettier`, `eslint`, `vitest`, `playwright`, `tailwindcss`, `sveltekit-adapter`, `paraglide`, `ai-tools`
- vitest: `unit testing, component testing`
- tailwindcss: plugins `typography, forms`
- sveltekit-adapter: `node`
- ai-tools: `Other`
- paraglide: idiomas `pt-br, en`, incluir demo `Yes`
- package manager: `bun`

### 2. Adapter Bun (runtime e produção)

O scaffold usa o adapter `node`. Troque pelo adapter Bun para rodar no runtime Bun:

```bash
cd <project-name>
bun add -D svelte-adapter-bun
```

Edite o `vite.config.ts` — no `sv@0.17.0` o adapter fica dentro de `sveltekit({ adapter: ... })`, não em `svelte.config.js`. Troque apenas o import:

```ts
// vite.config.ts
import adapter from 'svelte-adapter-bun'; // era '@sveltejs/adapter-node'
// ...
sveltekit({ adapter: adapter() })
```

> Em projetos mais antigos que ainda têm `svelte.config.js`, a troca equivalente é:
>
> ```js
> // svelte.config.js
> import adapter from 'svelte-adapter-bun';
> export default { kit: { adapter: adapter() } };
> ```
>
> O `out` padrão é `build/` (o build gera `build/index.js`).

### 3. Comandos de dev e produção

```bash
bun --bun run dev       # desenvolvimento
bun --bun run build     # build de produção (gera ./build/)
bun ./build/index.js    # executa o build de produção
```

### 4. Integração com o backend Phoenix + Ash (Hey API)

O **Ash JSON API** gera a spec OpenAPI a partir dos recursos declarativos; o **@hey-api/openapi-ts** consome essa spec e gera o SDK tipado (funções, tipos e schemas Zod).

```text
openapi.json (Ash)
    ↓
bunx @hey-api/openapi-ts -i openapi.json -o src/lib/api
    ↓
src/lib/api/
├── services.gen.ts   ← listUsers(), createUser(), etc.
├── types.gen.ts      ← tipos TypeScript
└── schemas.gen.ts    ← schemas Zod (se o plugin zod estiver ativo)
```

**a)** Instalar o gerador:

```bash
bun add @hey-api/openapi-ts -D
```

**b)** Baixar a spec do backend (com o servidor Phoenix rodando):

```bash
curl -s http://localhost:4000/api/json/open_api -o openapi.json
```

A rota vem do `AshJsonApiRouter` do backend (`open_api: "/open_api"` montado em `/api/json`). Se o backend estiver em outra URL/porta, ajuste.

**c)** Gerar o SDK:

```bash
bunx @hey-api/openapi-ts -i openapi.json -o src/lib/api
```

> Para ativar o plugin Zod, crie um `openapi-ts.config.ts` (ver https://heyapi.dev/docs/openapi/typescript/configuration) com o plugin `zod` e rode `bunx @hey-api/openapi-ts` apontando para a config.

**d)** Estrutura plain das páginas — a IA apenas instancia o template trocando o nome do recurso, sem escrever `fetch` ou validação manual:

`src/routes/<recurso>/+page.server.ts`:

```ts
import { listMunicipalities } from '$lib/api/services.gen';

export async function load() {
  const { data } = await listMunicipalities();
  return { municipalities: data };
}
```

`src/routes/<recurso>/+page.svelte`:

```svelte
<script lang="ts">
  let { data } = $props();
</script>

<!-- renderiza data.municipalities -->
```

### 5. (Opcional) shadcn-svelte

```bash
bun x shadcn-svelte@latest init
bun x shadcn-svelte@latest add button
```

O `init` pergunta a config do `components.json`: base color `Slate`, global CSS `src/routes/layout.css`, alias de lib `$lib`, components `$lib/components`, utils `$lib/utils`, hooks `$lib/hooks`, ui `$lib/components/ui`.

Import de componente:

```svelte
<script lang="ts">
  import { Button } from '$lib/components/ui/button/index.js';
</script>
<Button>Click me</Button>
```

## Verificação

```bash
bun --bun run dev
# ou, para produção:
bun --bun run build && bun ./build/index.js
```

Abrir `http://localhost:5173` (dev) e confirmar que a página inicial renderiza. Se o SDK do Hey API foi gerado, `src/lib/api/` deve conter `services.gen.ts`, `types.gen.ts` e `schemas.gen.ts`.

## Notas

- O nome do projeto é kebab-case; ele vira o nome da pasta e do pacote no `package.json`.
- `--install bun` já instala as dependências no passo 1; o `bun add -D svelte-adapter-bun` do passo 2 é aditivo.
- `bun x sv@0.17.0` pina a versão do CLI para reprodutibilidade; sem o pin, `bun x sv create ...` usa a última versão.
- A spec OpenAPI é a fonte da verdade do contrato; se o backend mudar recursos, baixe o `openapi.json` de novo e regenere o SDK.
