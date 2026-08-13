# Especificação visual

## Direção

Admin operacional limpo, denso o bastante para trabalhar e espaçoso o bastante para leitura. A composição toma como referência:

- `sidebar-07` para navegação;
- `dashboard-01` para conteúdo;
- convenções de tema do shadcn-svelte.

Não tente reproduzir pixels de Svelte. Reproduza hierarquia, ritmo, estados e semântica usando HEEx/Tailwind.

## Anatomia do shell

```text
┌──────────── sidebar ────────────┬──────── header ──────────────────┐
│ marca / workspace               │ trigger · breadcrumb   ações     │
│                                 ├───────────────────────────────────┤
│ grupo                           │ page header                       │
│  item ativo                     │ metric cards                      │
│  item                           │ painel analítico                  │
│ grupo                           │ toolbar/tabs                      │
│  item + submenu                 │ tabela                            │
│                                 │ paginação                         │
│ usuário / logout                │                                   │
└─────────────────────────────────┴───────────────────────────────────┘
```

### Desktop

- sidebar: `w-64`, recolhida `w-16`;
- header: `h-12` ou `h-14`, borda inferior;
- sidebar inset: conteúdo ocupa o restante, sem largura fixa artificial;
- conteúdo: `max-w-screen-2xl mx-auto`, `p-4 sm:p-6 lg:p-8`;
- gaps principais: `gap-4 md:gap-6`.

### Mobile

- sidebar vira painel fixed/off-canvas;
- backdrop cobre a página;
- largura máxima `min(20rem, calc(100vw - 3rem))`;
- fecha por botão, backdrop, Escape e navegação;
- header mantém trigger e título; ações secundárias podem ir para menu.

## Navegação

- marca/workspace no topo;
- grupos com label curta em uppercase discreto;
- item com ícone 1rem, texto 0.875rem e altura 2.25rem;
- ativo usa `sidebar-accent` e `sidebar-accent-foreground`;
- submenu tem recuo e guia visual leve;
- rodapé mostra nome, e-mail/papel e logout;
- estado recolhido mostra tooltip, não texto truncado ilegível.

## Dashboard

### Cards de métrica

Grid:

```text
1 coluna → 2 em md → 4 em xl
```

Card:

- label muted;
- valor `text-2xl`/`text-3xl`, `tabular-nums`;
- badge delta opcional no canto;
- frase curta de interpretação;
- contexto secundário muted;
- gradiente `from-primary/5 to-card` opcional e muito sutil;
- não transformar todos os cards em links.

### Painel analítico

- card largo após métricas;
- header com título, descrição e seletor de período;
- conteúdo mínimo 15rem de altura;
- quando não houver chart, usar resumo, progress bars, distribuição ou série tabular — nunca gráfico falso;
- filtros devem funcionar server-side quando alterarem dados.

### Toolbar e tabela

- tabs no início, ações no fim;
- mobile empilha controles;
- busca com debounce razoável;
- filtros validados por allowlist;
- table header muted, linhas de 2.75rem a 3rem;
- status em badges;
- ações por linha em menu ou botão claro;
- paginação e total abaixo;
- wrapper com overflow horizontal.

## Tokens

Base recomendada: Neutral ou Zinc. Use OKLCH.

```css
:root {
  --radius: 0.625rem;
  --background: oklch(1 0 0);
  --foreground: oklch(0.145 0 0);
  --card: oklch(1 0 0);
  --card-foreground: oklch(0.145 0 0);
  --popover: oklch(1 0 0);
  --popover-foreground: oklch(0.145 0 0);
  --primary: oklch(0.205 0 0);
  --primary-foreground: oklch(0.985 0 0);
  --secondary: oklch(0.97 0 0);
  --secondary-foreground: oklch(0.205 0 0);
  --muted: oklch(0.97 0 0);
  --muted-foreground: oklch(0.556 0 0);
  --accent: oklch(0.97 0 0);
  --accent-foreground: oklch(0.205 0 0);
  --destructive: oklch(0.577 0.245 27.325);
  --border: oklch(0.922 0 0);
  --input: oklch(0.922 0 0);
  --ring: oklch(0.708 0 0);
  --chart-1: oklch(0.646 0.222 41.116);
  --chart-2: oklch(0.6 0.118 184.704);
  --chart-3: oklch(0.398 0.07 227.392);
  --chart-4: oklch(0.828 0.189 84.429);
  --chart-5: oklch(0.769 0.188 70.08);
  --sidebar: oklch(0.985 0 0);
  --sidebar-foreground: oklch(0.145 0 0);
  --sidebar-primary: oklch(0.205 0 0);
  --sidebar-primary-foreground: oklch(0.985 0 0);
  --sidebar-accent: oklch(0.97 0 0);
  --sidebar-accent-foreground: oklch(0.205 0 0);
  --sidebar-border: oklch(0.922 0 0);
  --sidebar-ring: oklch(0.708 0 0);
}
```

O asset `admin-theme.css` contém light/dark e mapeamento Tailwind 4.

## Tipografia

- use system stack por padrão; não instale fonte remota sem aprovação;
- page title: 1.5rem, semibold/bold, tracking-tight;
- section title: 1rem–1.125rem;
- body/admin: 0.875rem;
- micro labels: 0.6875rem–0.75rem;
- números: `tabular-nums`.

## Movimento

- 150–220ms para hover/menus;
- 240–300ms para sidebar/drawer;
- somente transform/opacity sempre que possível;
- evitar animação decorativa contínua;
- desativar transições com `prefers-reduced-motion`.

## Anti-padrões

- dashboard inteiro composto de cards iguais;
- sidebar com muitas cores;
- gradientes fortes e glassmorphism em admin operacional;
- ações destrutivas sem confirmação;
- tabelas client-side com milhares de linhas;
- ícones de bibliotecas adicionais quando Heroicons já existe;
- placeholders que parecem métricas reais;
- menu mobile controlado apenas por CSS sem foco/Escape/backdrop.
