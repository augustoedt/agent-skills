# Agent Skills

Repositório dedicado ao versionamento e à sincronização de skills próprias para agentes de programação.

## Estrutura

- `skills/`: fonte oficial das skills próprias.
- `scripts/`: scripts de instalação, verificação e remoção segura.

## Como as skills são instaladas

`scripts/install.sh` mantém **uma única cópia física** das skills próprias em
`~/.agents/skills/` (sincronizada do repo via rsync) e cria **symlinks por
skill** em todos os agentes detectados na máquina — pi (`~/.pi/agent/skills`),
Claude Code (`~/.claude/skills`), Codex (`~/.codex/skills`), Grok
(`~/.grok/skills`), Copilot CLI (`~/.copilot/skills`), Cursor
(`~/.cursor/skills`) e `~/.agent/skills`, quando a pasta do agente existir.
Agentes ausentes são pulados; skills de terceiros nunca são tocadas.

## Skills de terceiros

Skills de terceiros **não são versionadas aqui** — continuam gerenciadas por
seus respectivos fornecedores e devem ser instaladas/atualizadas a partir das
fontes originais. Hoje usamos, entre outras:

- **`use-railway`** (Railway) — infra/deploy: projetos, serviços, bancos,
  domains, troubleshooting
- **`kimi-webbridge`** (Moonshot/Kimi) — controle do browser real do usuário
  via daemon local

Para instalar ou atualizar uma delas numa máquina nova:

```bash
npx skills find use-railway      # localiza o pacote oficial no marketplace
npx skills add <owner/repo@skill> -g -y
```

Referência do marketplace: <https://skills.sh/>
