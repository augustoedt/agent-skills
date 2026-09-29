# Documentação do agent-skills

Repo de skills próprias do usuário (fonte oficial) + scripts de sync.
Estrutura de docs no padrão do ecossistema (skill `docs-organization`).

## checkpoints/

Estado autoritativo para retomada do trabalho.

- `project-state.md` — estado atual, próximo passo e armadilhas do repositório.

## decisions/

Decisões arquiteturais permanentes e suas consequências.

- `0001-docs-search-como-binario-local.md` — adoção de Rust, CLI/JSON e evolução medida do motor de busca.
- `0002-manter-cap-1-opt-in-apos-holdout.md` — não promoção do cap 1 após o gate e preservação dos holdouts consumidos.

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

## apresentacoes/

Material HTML editorial criado sob demanda para explicar arquitetura, fluxos e estado do projeto.

- `docs-search-arquitetura-e-avaliacao.html` — fluxo completo da busca documental, contrato de
  evidência, avaliação, separação público/privado, manifesto local e roadmap.
