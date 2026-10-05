# Documentação do agent-skills

Repo de skills próprias do usuário (fonte oficial) + scripts de sync, no padrão da skill
`docs-organization`. Toda pasta tem sua função explicada aqui.

## Como navegar

Caminho principal: [`checkpoints/project-state.md`](checkpoints/project-state.md) → decisões →
runbooks. Não há plano ativo; o histórico concluído fica em `reviews/` e `archive/`.

## Antes de… → leia

| Antes de… | Leia |
|---|---|
| editar ou criar uma skill própria | [`../skills/edit-own-skills/SKILL.md`](../skills/edit-own-skills/SKILL.md) e [`runbooks/sincronizar-e-deduplicar-skills.md`](runbooks/sincronizar-e-deduplicar-skills.md) |
| sincronizar skills ou resolver colisão de nomes | [`runbooks/sincronizar-e-deduplicar-skills.md`](runbooks/sincronizar-e-deduplicar-skills.md) |
| adicionar ou atualizar skill de terceiro | [`../skills/add-third-party-skill/SKILL.md`](../skills/add-third-party-skill/SKILL.md) e [`../skills/update-third-party-skills/SKILL.md`](../skills/update-third-party-skills/SKILL.md) |
| instalar, usar ou alterar o `docs-search` | [`runbooks/buscar-documentacao-de-projetos.md`](runbooks/buscar-documentacao-de-projetos.md) |
| propor nova hipótese de busca (ranking, engine, cap) | ADRs [0002](decisions/0002-manter-cap-1-opt-in-apos-holdout.md), [0003](decisions/0003-comparar-engines-antes-da-adocao.md) e [0004](decisions/0004-isolar-adapters-rejeitados-da-build-de-produto.md) |

## checkpoints/

Estado autoritativo para retomada do trabalho.

- [`project-state.md`](checkpoints/project-state.md) — estado atual, próximo passo e armadilhas do repositório.

## decisions/

Decisões arquiteturais permanentes e suas consequências.

- [`0001-docs-search-como-binario-local.md`](decisions/0001-docs-search-como-binario-local.md) — adoção de Rust, CLI/JSON e evolução medida do motor de busca.
- [`0002-manter-cap-1-opt-in-apos-holdout.md`](decisions/0002-manter-cap-1-opt-in-apos-holdout.md) — não promoção do cap 1 após o gate e preservação dos holdouts consumidos.
- [`0003-comparar-engines-antes-da-adocao.md`](decisions/0003-comparar-engines-antes-da-adocao.md) — bake-off isolado de persistência, recuperação lexical, embeddings e RRF antes de integrar um vencedor.
- [`0004-isolar-adapters-rejeitados-da-build-de-produto.md`](decisions/0004-isolar-adapters-rejeitados-da-build-de-produto.md) — mantém os protótipos auditáveis atrás de uma feature não default e fora da distribuição normal.

## plans/

Planos vivos de trabalho ainda não concluído. Nenhum plano ativo no momento.

## runbooks/

Passo a passo **vivo** de ops do próprio repo. Um arquivo por operação; atualizar quando a rotina
mudar.

- [`buscar-documentacao-de-projetos.md`](runbooks/buscar-documentacao-de-projetos.md) — instalar e usar o binário `docs-search` com contexto mínimo suficiente.
- [`sincronizar-e-deduplicar-skills.md`](runbooks/sincronizar-e-deduplicar-skills.md) — sync das skills via `scripts/install.sh` e eliminação de conflitos de nome entre `~/.pi/agent/skills/` e `~/.agents/skills/`.

## reviews/

Revisões e fechamentos concluídos.

- [`2026-10-05-fechamento-docs-search.md`](reviews/2026-10-05-fechamento-docs-search.md) — resultado do `docs-search` 0.6.0 e histórico de fases e métricas movido do checkpoint.

## archive/

Documentos de etapas concluídas, fora do caminho principal de retomada.

- [`plano-docs-search.md`](archive/plano-docs-search.md) — plano faseado completo do `docs-search`, concluído na 0.6.0.

## apresentacoes/

Material HTML editorial criado sob demanda para explicar arquitetura, fluxos e estado do projeto.

- [`docs-search-arquitetura-e-avaliacao.html`](apresentacoes/docs-search-arquitetura-e-avaliacao.html) — escopo, fluxo da busca, contrato de evidência, avaliação, plano faseado do bake-off, decisões e estado operacional atual.
