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
