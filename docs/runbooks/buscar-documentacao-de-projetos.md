# Runbook — Buscar documentação de projetos

Documento vivo. Atualizar junto com `skills/search-project-docs/SKILL.md` sempre que o fluxo ou o
contrato do binário mudar.

## Quando

- Antes de carregar vários documentos de um projeto no contexto de um agente.
- Ao retomar trabalho e localizar checkpoint, ADR, plano, regra ou runbook relevante.
- Ao responder perguntas cuja fonte deve ser citada com path e linhas.

## Instalar ou atualizar o binário

Faça isso somente quando a tarefa pedir instalação ou atualização. Resolva a localização do
repositório e execute o instalador:

```bash
REPO="${AGENT_SKILLS_REPO:-$HOME/Projects/agent-skills}"
"$REPO/scripts/install-docs-search.sh"
```

Verifique que o comando ficou disponível no `PATH` antes de qualquer uso:

```bash
command -v docs-search >/dev/null 2>&1 || {
  echo "docs-search indisponível no PATH" >&2
  exit 1
}
docs-search --version
```

## Buscar

1. Resolver a raiz do projeto:

   ```bash
   ROOT="$(git rev-parse --show-toplevel)"
   ```

2. Verifique a disponibilidade e não invoque o binário se a verificação falhar:

   ```bash
   if command -v docs-search >/dev/null 2>&1; then
     DOCS_SEARCH_BIN="$(command -v docs-search)"
   else
     DOCS_SEARCH_BIN=""
   fi
   ```

3. Se `DOCS_SEARCH_BIN` estiver vazio, use o fallback explícito da seção **Não fazer**.
4. Fazer uma consulta focada:

   ```bash
   "$DOCS_SEARCH_BIN" search --root "$ROOT" --query "<pergunta objetiva>" --limit 5 --json
   ```

5. Selecionar pelos campos `path`, `heading`, `line_start`, `line_end`, `score` e
   `matched_terms`. Na resposta v2, `selection` registra o cap efetivo e `raw_rank` permite auditar
   resultados promovidos pela diversidade.
6. Ler o trecho correspondente no arquivo original e confirmar o hash/frescor quando isso for
   relevante.
7. Se necessário, reformular a consulta uma vez; só então ampliar a leitura.
8. Conferir código e configuração atuais antes de editar.

## Corpus

A versão atual busca `docs/**/*.md` e os arquivos de raiz `README.md`, `CLAUDE.md`, `AGENTS.md` e
`pi-warden.md`. A leitura é direta e não usa índice persistente. Arquivos de código, `.env` e
`skills/**` ficam fora do corpus padrão. Isso não detecta secrets escritos nos próprios Markdown;
credenciais nunca devem ser documentadas no corpus.

## Desenvolvimento e validação

```bash
cd tools/docs-search
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

As consultas de avaliação ficam em `tools/docs-search/evaluation/queries.json`. Medir Hit@1,
Recall@5, MRR, falsos positivos em consultas sem resposta, latência e caracteres retornados antes
de trocar o motor de busca. Para um experimento explícito de diversidade por arquivo:

```bash
cargo run -- evaluate \
  --root evaluation/fixtures/stable-v1 \
  --queries evaluation/queries.json \
  --max-results-per-path 1 \
  --json
```

O cap é opt-in e não deve ser aplicado silenciosamente à busca normal. Relatórios v2 registram o
rank final e o rank BM25 bruto; outras versões de contrato não são aceitas.

### Validar o freeze do bake-off

Esta rotina é somente para desenvolvimento do `docs-search`; não faz parte da busca normal. Defina
o diretório privado local sem gravá-lo no Git e execute o verificador antes de implementar ou medir
qualquer braço:

```bash
PRIVATE_EVAL_ROOT="${DOCS_SEARCH_PRIVATE_EVAL_ROOT:?defina o diretório privado}"
"$PRIVATE_EVAL_ROOT/verify-engine-bakeoff-freeze.py"
```

A saída esperada é `engine-bakeoff-v1 freeze verified`. O verificador confere protocolo, schemas,
snapshots, datasets, baselines, modelo, checksums e permissões sem abrir holdouts consumidos. O
runner privado consulta a denylist antes de resolver os paths desses holdouts. Falha de hash,
permissão ou conjunto de inputs bloqueia qualquer implementação ou medição do bake-off; não
regenere ou sobrescreva artefatos para fazer a verificação passar. Uma mudança de configuração
exige outra tag e outro protocolo.

## Não fazer

- Não tratar excerpt, score ou cache futuro como fonte da verdade.
- Não carregar todos os resultados sem necessidade.
- Não incluir secrets ou `.env` no corpus.
- Não adicionar embeddings, classificador ou infraestrutura sem comparar métricas com o baseline.
- Não implementar ou medir o bake-off se o verificador do freeze falhar.
- Não acessar holdouts consumidos, contornar a denylist ou alterar parâmetros de `engine-bakeoff-v1`.
- Não invocar `docs-search` sem antes confirmar `command -v docs-search`.
- Não engolir indisponibilidade ou erro do binário; reportar a falha e usar `rg` explicitamente
  apenas no corpus permitido:

  ```bash
  paths=()
  [ -d "$ROOT/docs" ] && paths+=("$ROOT/docs")
  for name in README.md CLAUDE.md AGENTS.md pi-warden.md; do
    [ -f "$ROOT/$name" ] && paths+=("$ROOT/$name")
  done
  if [ "${#paths[@]}" -eq 0 ]; then
    echo "nenhum arquivo do corpus documental encontrado" >&2
  else
    rg -n -i --glob '*.md' '<termos>' "${paths[@]}"
  fi
  ```
