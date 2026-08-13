---
name: phoenix-ash-admin-ui
description: Complemento opcional da skill scaffold-phoenix-ash para criar, expandir ou refatorar um admin de produto em Phoenix LiveView + Ash com visual shadcn-style. Use somente quando o plano aprovado declarar explicitamente que o painel/admin/backoffice será desenvolvido no próprio Phoenix; também para essa migração explícita de PhiaUI, Backpex ou AshBackpex. Não usar só porque o projeto tem Phoenix/Ash, AshAdmin ou pede scaffold. Optional Phoenix admin dashboard companion, never automatic.
license: MIT; veja assets/LICENSE-shadcn-svelte-MIT.txt
compatibility: Projetos Phoenix 1.8+ com LiveView 1.1+, Ash e Tailwind CSS 4, normalmente criados/evoluídos com scaffold-phoenix-ash.
---

# Phoenix + Ash Admin UI

Crie admins pertencentes à aplicação, com composição inspirada em shadcn, sem transformar uma biblioteca de componentes em dependência runtime.

## Gate de ativação — obrigatório

Esta é uma skill **complementar e condicional** à `scaffold-phoenix-ash`, não uma etapa automática dela.

Use somente quando as duas condições forem verdadeiras:

1. a aplicação usa Phoenix LiveView + Ash;
2. o plano aprovado define explicitamente um admin de produto implementado no próprio Phoenix.

Não ative por inferência ou apenas porque:

- o usuário pediu um scaffold Phoenix/Ash;
- `ash_admin` está instalado ou `/ash-admin` existe em desenvolvimento;
- há uma API Phoenix com frontend administrativo separado;
- o plano não incluiu admin nesta etapa;
- alguém mencionou genericamente dashboard sem decidir onde ele será implementado.

Se a plataforma do admin estiver indefinida, pergunte antes. Se o plano escolher SvelteKit, React, outra aplicação ou apenas AshAdmin de desenvolvimento, não use esta skill. Quando aplicável, execute-a **depois** da fundação criada pela `scaffold-phoenix-ash` e preserve as decisões do plano.

## Diretiva principal

- **Nunca instalar** `phia_ui`, `backpex`, `ash_backpex`, Svelte, shadcn-svelte, daisyUI ou um catálogo completo de componentes para construir este admin.
- Use shadcn-svelte apenas como referência de composição, tokens, estados e acessibilidade.
- Gere/ejetе somente os componentes realmente usados e mantenha-os no namespace da aplicação.
- Não remova dependências existentes sem explicar o impacto e obter concordância quando a remoção for destrutiva.
- Não copie telas de negócio da referência; adapte a composição aos dados reais do projeto.

## Referência visual padrão

Combine:

- **Shell:** `sidebar-07` — sidebar esquerda recolhível, grupos, submenus, rodapé do usuário, header compacto e conteúdo à direita.
- **Conteúdo:** `dashboard-01` — cards de métricas, painel analítico, filtros de período, tabs e tabela operacional.
- **Tema:** tokens semânticos OKLCH no padrão `background`/`foreground`, incluindo tokens próprios de sidebar e charts.

Leia antes de implementar:

- [Especificação visual](references/visual-system.md)
- [Arquitetura Phoenix/Ash](references/phoenix-ash-architecture.md)
- [Processo de instalação e migração](references/installation.md)
- [Fontes e licença](references/sources.md)

## Fluxo obrigatório

### 1. Inspecionar o projeto

Leia integralmente:

- `AGENTS.md` e skills locais relevantes;
- `mix.exs`, `config/*.exs` e `assets/css/app.css`;
- `assets/js/app.js`;
- router, autenticação, layouts e componentes existentes;
- recursos/domínios Ash e policies envolvidos;
- testes web existentes.

Determine:

- namespace web e convenção de paths;
- Tailwind 4 e mecanismo de dark mode;
- actor/current user/current scope usado pelo projeto;
- rota administrativa e pipeline de autorização;
- componentes que já existem e podem ser preservados;
- presença de `phia_ui`, Backpex, AshBackpex ou outro framework CRUD.

### 2. Definir o modo

- **Novo admin:** instalar fundação visual e criar dashboard mínimo ligado a dados reais.
- **Expansão:** reutilizar componentes existentes; não duplicar primitives.
- **Migração:** inventariar rotas, actions e testes antes de remover a UI antiga. Migrar tela a tela.
- **Somente design system:** instalar tokens e primitives sem criar rotas de negócio, mas apenas dentro de um admin Phoenix já aprovado no plano.

### 3. Gerar uma fundação conservadora

Os assets desta skill são ponto de partida, não código para sobrescrever cegamente:

```text
assets/admin-theme.css
assets/admin_components.ex.eex
assets/admin_sidebar_hook.js
```

O scaffold opcional cria arquivos somente quando eles não existem:

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

`--with-dashboard` é opcional e gera um LiveView de estado vazio, sem métricas fictícias; conecte-o a actions Ash reais e adapte rotas/autenticação. O flag de confirmação é deliberadamente obrigatório: não o use se a decisão não estiver explícita no plano aprovado.

Depois, integre manualmente:

- `@import "./admin-theme.css";` em `assets/css/app.css`;
- `AdminSidebar` no objeto `Hooks` de `assets/js/app.js`;
- `AdminComponents` nos LiveViews ou no helper web apropriado;
- rotas dentro do pipeline/live session autenticado correto.

### 4. Construir a composição padrão

Shell:

- sidebar de 16rem no desktop, recolhível para 4rem;
- drawer/off-canvas no mobile com backdrop e Escape;
- header de 3rem a 3.5rem com trigger, breadcrumb/título e ações;
- conteúdo em `max-w-screen-2xl`, padding responsivo e fundo muted discreto;
- rodapé da sidebar com identidade do usuário e logout;
- grupos de navegação com estado ativo e `aria-current="page"`.

Conteúdo de dashboard:

1. page header com título, descrição e ações;
2. grid de 1/2/4 cards de métricas;
3. painel analítico principal, sem instalar charts automaticamente;
4. toolbar com busca, filtros, tabs e ações;
5. tabela server-side;
6. paginação, empty state, loading e error state.

Não use números fictícios como se fossem reais. Se o domínio ainda não fornecer métricas, mostre estado de configuração explícito.

### 5. Regras Ash

Quando o projeto usa Ash:

- componentes HEEx nunca fazem queries/actions;
- LiveViews usam code interfaces Ash;
- actor é definido no `Ash.Query`, `Ash.Changeset`, `AshPhoenix.Form` ou input da action;
- nunca usar `authorize?: false` em fluxo administrativo normal;
- não criar schemas Ecto paralelos para recursos Ash;
- paginação, filtros e ordenação de alto volume são server-side;
- filtros vindos da URL são validados por allowlist antes de virar atoms;
- nenhuma action Ash por linha; jobs/bulk/staging para operações pesadas;
- confirmações destrutivas usam modal acessível e action explícita;
- correções de dados canônicos preferem retry, novo import ou novo release.

### 6. Acessibilidade e responsividade

Obrigatório:

- landmarks `header`, `nav`, `aside`, `main`;
- labels e nomes acessíveis em botões de ícone;
- foco visível com token `ring`;
- `aria-current`, `aria-expanded`, `aria-controls` e `aria-modal` corretos;
- modal fecha por Escape, recebe foco inicial e devolve foco ao trigger;
- tabelas têm `scope="col"`, caption acessível quando necessário e wrapper horizontal;
- touch targets mínimos de 2.25rem;
- sidebar mobile nunca bloqueia scroll após fechar;
- respeitar `prefers-reduced-motion`;
- tema claro/escuro sem flash perceptível.

### 7. Gráficos

Não adicione biblioteca de chart por padrão. Prioridade:

1. métricas e tabela funcionais;
2. placeholder semântico ou SVG simples para sparkline pequena;
3. somente instalar chart após necessidade real e aprovação do usuário.

Charts devem usar `--chart-1` a `--chart-5`, ter resumo textual e não depender apenas de cor.

### 8. Testes mínimos

Inclua testes para:

- usuário não autenticado;
- usuário autenticado sem papel administrativo;
- administrador autorizado;
- navegação ativa;
- mobile/sidebar quando houver hook relevante;
- filtros inválidos e paginação extrema;
- forms/actions com actor correto;
- ausência de create/edit/delete em dados canônicos somente leitura;
- rotas antigas após migração.

### 9. Verificação

Execute os comandos do projeto. Em stack Phoenix/Ash, normalmente:

```bash
mix format --check-formatted
mix compile --warnings-as-errors
mix ash.codegen --check
mix ash_postgres.generate_migrations --check
mix test
mix assets.build
mix precommit
```

Execute também:

```bash
bash ~/.pi/agent/skills/phoenix-ash-admin-ui/scripts/verify.sh .
```

Corrija warnings do código da aplicação. Não mascare warnings instalando dependências grandes sem necessidade.

## Critérios de aceite

- admin consistente em desktop e mobile;
- sidebar/header/conteúdo seguem o padrão desta skill;
- tokens semânticos centralizam o tema;
- nenhuma dependência proibida nova;
- componentes pertencem à aplicação;
- authorization e actor são explícitos;
- tabelas de volume são server-side;
- build, testes e assets passam;
- origem visual e licença estão documentadas.
