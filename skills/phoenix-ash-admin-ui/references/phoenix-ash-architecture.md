# Arquitetura Phoenix LiveView + Ash

## Camadas

```text
HEEx components
  apresentação, slots, estados visuais
        ↓
LiveView / LiveComponent
  params, events, forms, paginação, filtros
        ↓
Ash code interface / AshPhoenix.Form
  actions, policies, actor, validações
        ↓
Resource / Data layer / Oban
  regras, persistência e trabalho pesado
```

## Componentes

Componentes devem:

- receber dados prontos;
- emitir eventos/navegação;
- declarar attrs/slots;
- manter acessibilidade;
- não chamar Repo, Ecto, Ash ou HTTP.

Prefira um módulo da aplicação, por exemplo:

```elixir
MyAppWeb.AdminComponents
```

Primitives iniciais:

```text
admin_shell
nav_section
nav_item
page_header
card / card_header / card_content / card_footer
metric_card
status_badge
button
input wrapper
responsive_table
pagination_footer
empty_state
confirmation_dialog
```

Não crie um componente para cada `div`. Extraia quando houver semântica, repetição ou estado.

## Shell e LiveView

Um LiveView administrativo deve iniciar no layout/shell esperado pelo projeto. Em Phoenix 1.8, respeite a convenção local de `<Layouts.app ...>` e live sessions autenticadas.

O shell recebe no mínimo:

```elixir
current_user/current_scope
current_path
flash
page title/breadcrumb opcional
navigation groups
```

Navegação deve usar verified routes (`~p`) e `navigate`/`patch` adequadamente.

## Actor

Correto:

```elixir
Resource
|> Ash.Query.for_read(:read, %{}, actor: socket.assigns.current_user)
|> Ash.read!()
```

```elixir
AshPhoenix.Form.for_update(record, :admin_update,
  actor: socket.assigns.current_user,
  as: "record"
)
```

Evite definir actor somente na chamada final se as instruções locais do Ash exigem colocá-lo no query/changeset/input.

## Code interfaces

Exponha operações de tela no domínio:

```elixir
resource MyApp.Domain.Record do
  define :list_records, action: :read
  define :get_record_by_id, action: :read, get_by: [:id]
  define :admin_update_record, action: :admin_update
end
```

A interface não substitui policies. Toda action administrativa continua autorizada no recurso.

## Paginação e filtros

- offset pagination serve bem ao admin tradicional;
- keyset é preferível em volumes altos ou feed ordenado estável;
- limite padrão 25–50;
- clamp de página/offset;
- count somente quando necessário;
- whitelist de status/kinds/severity;
- nunca `String.to_atom/1` em params;
- se usar `String.to_existing_atom/1`, valide a string antes;
- selecione campos enxutos e evite geometria/blob/associações grandes na lista.

Exemplo de allowlist:

```elixir
defp valid_filter(value, allowed) when is_binary(value) do
  if value in Enum.map(allowed, &Atom.to_string/1), do: value, else: ""
end

defp valid_filter(_value, _allowed), do: ""
```

## Trabalho pesado

LiveView não deve:

- baixar arquivos;
- parsear ZIP/CSV grande;
- manter transação durante I/O;
- executar uma action por linha;
- montar milhares de structs só para inserir.

Use Oban, staging, COPY, bulk actions e batches limitados.

## Autorização de rota

Proteja em duas camadas:

1. pipeline/on_mount impede acesso à tela;
2. policies Ash impedem bypass da UI.

Teste ambos. Não considere esconder links como autorização.

## Formulários

- use `AshPhoenix.Form` quando houver action Ash;
- renderize erros por campo e erro global;
- disable submit durante envio;
- preserve actor entre validate e save;
- não permita editar recursos canônicos se o fluxo correto é reimportação/release;
- modais devem ter label, description, Escape e foco.

## Tema

Use `.dark` em `<html>` e variável persistida no browser. O root layout precisa aplicar a preferência cedo para evitar flash. Não use duas fontes de verdade independentes (`data-theme` e `.dark`) sem sincronização.

## Testes recomendados

```elixir
use MyAppWeb.ConnCase, async: false
import Phoenix.LiveViewTest
```

Cobrir:

- redirect de não autenticado;
- 403/redirect de papel incorreto;
- render autorizado;
- patch de filtros/página;
- evento de action e mudança persistida;
- actor/policy com `Ash.can?`;
- params duplicados/malformados;
- IDs inexistentes.
