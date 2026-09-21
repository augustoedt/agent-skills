# Documentação do agent-skills

Repo de skills próprias do usuário (fonte oficial) + scripts de sync.
Estrutura de docs no padrão do ecossistema (skill `docs-organization`).

## runbooks/

Passo a passo **vivo** de ops do próprio repo. Um arquivo por operação;
actualizar quando a rotina mudar.

- `sincronizar-e-deduplicar-skills.md` — sync das skills via
  `scripts/install.sh` e eliminação de conflitos de nome (duplicatas) entre
  `~/.pi/agent/skills/` e `~/.agents/skills/`.

Outras pastas (`decisions/`, `checkpoints/`, `plans/`, …) seguem o padrão do
ecossistema e podem ser criadas quando surgir conteúdo.
