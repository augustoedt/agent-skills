# Agent Skills

Esta fixture descreve um repositório portátil de skills para assistentes de programação.

## Estrutura

O repositório mantém skills próprias, scripts, ferramentas e documentação interna em pastas
separadas. `skills/` é a fonte da verdade; cópias geradas não são autoritativas.

## Como as skills são instaladas

O instalador mantém uma cópia física canônica em `~/.agents/skills/` e cria symlinks para Pi,
Claude, Codex, Grok, Copilot e Cursor. Agentes ausentes são ignorados. Assim, uma skill é
distribuída para vários assistentes sem criar cópias divergentes.

## Repo portátil (`AGENT_SKILLS_REPO`)

A pasta do repositório pode mudar entre máquinas. `AGENT_SKILLS_REPO` identifica o checkout
selecionado e evita depender de um único caminho absoluto.

## Clonar em outra máquina

Clone o repositório, configure `AGENT_SKILLS_REPO` e execute o instalador de sincronização. O
mesmo processo funciona quando a pasta do repositório muda em outra máquina.

## Editar e sincronizar uma skill própria

Altere uma skill própria somente no diretório `skills/` do repositório, que é a fonte da verdade.
Commite a mudança e execute o instalador para sincronizar. Editar diretamente uma cópia instalada
pode perder mudanças quando a sincronização a substituir.

## Compatibilidade (macOS + Linux)

Os scripts são compatíveis com macOS e Linux, usam operações shell portáteis e não dependem de
recursos exclusivos do GNU.

## Skills de terceiros

Skills de terceiros continuam gerenciadas por seus fornecedores. Encontre um pacote no
marketplace, instale com o comando do fornecedor e catalogue somente sua proveniência neste
repositório.
