# Documentação do agent-skills

Repo de skills próprias do usuário (fonte oficial) + scripts de sync.
Estrutura de docs no padrão do ecossistema (skill `docs-organization`).

## checkpoints/

Estado autoritativo para retomada do trabalho.

- `project-state.md` — estado atual, próximo passo e armadilhas do repositório.

## decisions/

Decisões arquiteturais permanentes e suas consequências.

- `0001-docs-search-como-binario-local.md` — adoção de Rust, CLI/JSON e evolução medida do motor de busca.

## plans/

Planos vivos de trabalho ainda não concluído.

- `docs-search.md` — plano detalhado, métricas, gates e etapas do baseline lexical até busca híbrida e distribuição.

## runbooks/

Passo a passo **vivo** de ops do próprio repo. Um arquivo por operação;
atualizar quando a rotina mudar.

- `buscar-documentacao-de-projetos.md` — instalar e usar o binário `docs-search` com contexto mínimo suficiente.
- `sincronizar-e-deduplicar-skills.md` — sync das skills via
  `scripts/install.sh` e eliminação de conflitos de nome (duplicatas) entre
  `~/.pi/agent/skills/` e `~/.agents/skills/`.
