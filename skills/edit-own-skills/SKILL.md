---
name: edit-own-skills
description: Edita, cria, commita e sincroniza skills PRÓPRIAS do agente. Use quando o usuário pedir "atualiza a skill X", "melhora a skill X", "cria uma skill", "commita a skill", "sincroniza as skills", ou quando você for mexer em qualquer SKILL.md pessoal. A fonte da verdade é o repo de skills ($AGENT_SKILLS_REPO, default ~/Projects/agent-skills); nunca edite ~/.agents/skills diretamente.
---

# Editar skills próprias

As skills próprias do agente vivem versionadas num **repo git** (origin
`github.com:augustoedt/agent-skills.git`). Ele é a **fonte da verdade**; tudo o
mais é cópia sincronizada.

## Localizando o repo (muda por máquina)

Resolva nesta ordem:

1. `$AGENT_SKILLS_REPO` se estiver setado.
2. `~/Projects/agent-skills` se existir.
3. Senão, descubra pelo marcador único do repo:

   ```bash
   find "$HOME" -maxdepth 5 -path '*/agent-skills/scripts/install.sh' 2>/dev/null | head -1
   ```

Guarde numa variável pro resto da sessão:
`REPO="${AGENT_SKILLS_REPO:-$HOME/Projects/agent-skills}"` (ajuste se a máquina
usar outro caminho).

## Mapa de diretórios

| Caminho | Papel |
|---|---|
| `$REPO/skills/<nome>/` | **fonte da verdade — edite AQUI** |
| `~/.agents/skills/<nome>/` | cópia canônica (destino do `rsync --delete`) |
| `~/.pi/agent/skills/<nome>` | symlink → `~/.agents/skills/<nome>` |
| claude/codex/grok/copilot/cursor | symlinks idem (só os detectados) |

## Fluxo correto

1. Editar em `$REPO/skills/<nome>/SKILL.md` (ou criar a pasta).
2. `cd "$REPO"` + `git add` + `git commit` + `git push origin main`.
3. Sincronizar: `"$REPO/scripts/install.sh" <nome>` — ou sem argumento, para todas.

## Nunca

- **NÃO editar `~/.agents/skills/` diretamente** — é alvo de `rsync --delete`;
  o `install.sh` sobrescreve o que estiver lá.
- **NÃO commitar a partir de `~/.agents/skills/`** — não é repo git.
- **NÃO editar o symlink `~/.pi/agent/skills/<nome>`** — ele aponta para a
  cópia canônica.

## Skills de terceiro (fora deste repo)

Para skills de terceiro (use-railway, kimi-webbridge, etc.) use as skills
`add-third-party-skill` e `update-third-party-skills` — elas cuidam do
`third-party.json` e dos scripts `scripts/add-third-party.sh` /
`scripts/update-third-party.sh`. Esta skill é só para as skills próprias
(versionadas em `skills/` deste repo).
