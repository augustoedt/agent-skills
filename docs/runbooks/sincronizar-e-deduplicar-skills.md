# Runbook — Sincronizar skills e eliminar conflitos de nome

Documento vivo. Actualizar quando a rotina mudar (paths, credenciais, API).
Não é ADR nem plano.

## Quando

- O pi avisa sobre `name "..." collision` de skills (cópias reais
  duplicadas em `~/.pi/agent/skills/` e `~/.agents/skills/`).
- Após `git pull` no repo `~/Projects/agent-skills`, para sincronizar o que
  mudou nas skills próprias.
- Após `npx skills add|update ... -g` sobrescrever um symlink de
  `~/.pi/agent/skills/` com uma cópia real.

## Contexto

`~/.agents/skills/` é o diretório **canônico** (arquivos reais). Os agentes
apontam para lá via symlink: `~/.pi/agent/skills/<skill>` →
`../../../.agents/skills/<skill>` (target relativo a `~/.pi/agent/skills/`).

O pi carrega skills de ambos os locais, mas deduplica por `realpath` (segue
symlinks). Por isso o symlink elimina o warning. Cópia real nos dois lugares
(inodes diferentes) com o mesmo `name` = colisão, e o pi mantém a primeira
que encontrar.

Os comandos abaixo são POSIX/bash puro (sem `find -printf`, sem `sed -i`,
sem `realpath`) para funcionarem igual no macOS (BSD) e no Linux (GNU).

## Passos

1. Sincronizar as skills do repo:

   ```bash
   cd ~/Projects/agent-skills && git pull
   ./scripts/install.sh
   ```

2. Listar skills que ainda são diretório real (não symlink) em
   `~/.pi/agent/skills/`:

   ```bash
   cd ~/.pi/agent/skills
   for d in */; do
     d="${d%/}"
     [ -L "$d" ] || echo "$d"
   done
   ```

3. Converter cada uma em symlink quando (a) existir igual em
   `~/.agents/skills/` e (b) for idêntica (`diff -rq`):

   ```bash
   cd ~/.pi/agent/skills
   for d in */; do
     d="${d%/}"
     [ -L "$d" ] && continue
     [ -d "$HOME/.agents/skills/$d" ] || { echo "SKIP $d (sem canônico)"; continue; }
     diff -rq "$d" "$HOME/.agents/skills/$d" >/dev/null 2>&1 || { echo "SKIP $d (difere)"; continue; }
     rm -rf "$d" && ln -s "../../../.agents/skills/$d" "$d" && echo "OK $d"
   done
   ```

   Para um conjunto fixo (ex.: as skills AWS de terceiros), trocar o
   `for d in */` por:

   ```bash
   for d in amazon-bedrock aws-* launch-with-aws signing-in-to-aws; do
   ```

4. Verificar que não sobrou diretório real nem symlink quebrado:

   ```bash
   cd ~/.pi/agent/skills
   for d in */; do d="${d%/}"; [ -L "$d" ] || echo "REAL DIR: $d"; done  # vazio
   find . -maxdepth 1 -type l ! -exec test -e {} \; -print                 # vazio
   ```

## Não fazer

- Não apagar nada em `~/.agents/skills/` (canônico). A remoção é sempre da
  cópia de `~/.pi/agent/skills/`, substituída por symlink.
- Não remover sem `diff -rq` antes: se as duas cópias diferirem, resolver a
  diferença manualmente — não sobrescrever uma pela outra.
- Não versionar skills de terceiros neste repo: `scripts/install.sh` só
  sincroniza `skills/`; terceiros continuam via `npx skills` (ver README).
- Não rodar o loop de conversão de fora de `~/.pi/agent/skills/` sem ajustar
  o target do symlink (`../../../.agents/skills/...` é relativo a essa pasta).
