# Agent Skills

Repositório dedicado ao versionamento e à sincronização de skills próprias para agentes de programação.

## Estrutura

- `skills/`: fonte oficial das skills próprias.
- `scripts/`: scripts de instalação, verificação e remoção segura.
- `docs/`: documentação interna do repo — `docs/runbooks/` guarda as rotinas
  de ops (sync/dedup de skills).

## Como as skills são instaladas

`scripts/install.sh` mantém **uma única cópia física** das skills próprias em
`~/.agents/skills/` (sincronizada do repo via rsync) e cria **symlinks por
skill** em todos os agentes detectados na máquina — pi (`~/.pi/agent/skills`),
Claude Code (`~/.claude/skills`), Codex (`~/.codex/skills`), Grok
(`~/.grok/skills`), Copilot CLI (`~/.copilot/skills`), Cursor
(`~/.cursor/skills`) e `~/.agent/skills`, quando a pasta do agente existir.
Agentes ausentes são pulados; skills de terceiros nunca são tocadas.

## Repo portátil (`AGENT_SKILLS_REPO`)

O caminho deste repo **muda por máquina**. Scripts e skills que precisam do
caminho resolvem nesta ordem:

1. `$AGENT_SKILLS_REPO` (env var), se setada;
2. `~/Projects/agent-skills`, se existir;
3. descoberta pelo marcador `scripts/install.sh` (ver `skills/edit-own-skills`).

Setar a env var no shell evita ambiguidade:

```bash
# ~/.zshrc ou ~/.bashrc
export AGENT_SKILLS_REPO="$HOME/Projects/agent-skills"
```

## Clonar em outra máquina

```bash
git clone git@github.com:augustoedt/agent-skills.git ~/Projects/agent-skills  # ou outro caminho
export AGENT_SKILLS_REPO=~/Projects/agent-skills   # recomendado, se não for o default
cd "$AGENT_SKILLS_REPO"
./scripts/install.sh   # cria ~/.agents/skills/ + symlinks nos agentes
```

## Editar e sincronizar uma skill própria

A **fonte da verdade** é `skills/` deste repo — **nunca** edite
`~/.agents/skills/` direto (o `install.sh` sobrescreve via `rsync --delete`).
Fluxo: editar em `skills/<nome>/`, commitar, pushar e rodar
`./scripts/install.sh <nome>`. A skill `edit-own-skills` (neste repo) documenta
esse fluxo e é carregada automaticamente pelo pi ao pedir pra mexer em skill.

## Compatibilidade (macOS + Linux)

Os scripts são bash puro compatível com bash 3.2 (padrão do macOS) e bash 5
(Linux), usando só ferramentas POSIX/portáveis — sem `find -printf`,
`sed -i`, `realpath` nem `stat -c` (GNU-only). `rsync`, `diff`, `ln` e
`readlink` são usados com flags comuns aos dois. Rodar `scripts/install.sh`
numa máquina nova (macOS ou Linux) produz o mesmo layout: cópia canônica em
`~/.agents/skills/` + symlinks nos agentes.

## Skills de terceiros

Skills de terceiros **não são versionadas aqui** — o conteúdo continua gerenciado
por seus respectivos fornecedores. O que este repo guarda é só a **proveniência**:
um catálogo em `third-party.json` na raiz, com nome, fonte e o comando de
update de cada uma — nunca o conteúdo da skill em si.

```json
{
  "use-railway": {
    "description": "...",
    "source": "skills.sh marketplace: railwayapp/railway-skills@use-railway",
    "update_cmd": "npx skills add railwayapp/railway-skills@use-railway -g -y",
    "installed_at": "2026-08-06"
  }
}
```

Pra atualizar todas as skills de terceiros catalogadas (cada uma via seu
próprio mecanismo de update — marketplace, CLI do fornecedor, etc.):

```bash
scripts/update-third-party.sh                # todas
scripts/update-third-party.sh use-railway     # só uma
```

Hoje catalogamos:

- **`use-railway`** (Railway) — infra/deploy: projetos, serviços, bancos,
  domains, troubleshooting
- **`kimi-webbridge`** (Moonshot/Kimi) — controle do browser real do usuário
  via daemon local

Pra instalar uma skill de terceiro nova (e depois catalogá-la em
`third-party.json`):

```bash
npx skills find <nome>            # localiza o pacote oficial no marketplace
npx skills add <owner/repo@skill> -g -y
```

Referência do marketplace: <https://skills.sh/>
