# Runbook — Buscar documentação de projetos

> **Verificado em:** 2026-10-13 — `docs-search --version` retornou `0.6.0`; no corpus real de um
> projeto, uma consulta longa multifacetada retornou vazio, enquanto as consultas atômicas
> `retificar.py`, `regras retificação`, `produto replicável` e `estado atual projeto` retornaram
> os documentos esperados com `schema_version = 2` e `engine = lexical-bm25-v1`. A trilha de
> desenvolvimento (`cargo test`, matrizes de features) e o verificador do freeze não foram
> reexecutados nessa data.

Documento vivo. Atualizar junto com `skills/search-project-docs/SKILL.md` sempre que o fluxo ou o
contrato do binário mudar.

## Quando

- Antes de carregar vários documentos de um projeto no contexto de um agente.
- Ao retomar trabalho e localizar checkpoint, ADR, plano, regra ou runbook relevante.
- Ao responder perguntas cuja fonte deve ser citada com path e linhas.

## Premissas

- Instalação compila com Cargo: exige toolchain Rust; o binário global é gerenciado por `asdf`.
- O corpus padrão é Markdown em `docs/**` e arquivos de raiz; `skills/**` e código ficam fora.
- O motor estável é lexical: normaliza caixa e acentos, mas não resolve sinônimos ou flexões e
  exige que um mesmo chunk contenha a maioria dos termos significativos da consulta.
- A trilha de avaliação do bake-off depende de `DOCS_SEARCH_PRIVATE_EVAL_ROOT`, um diretório
  privado local que não existe fora da máquina do autor.

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

1. Resolver a raiz do projeto, com fallback explícito para projetos sem Git:

   ```bash
   if ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
     :
   else
     ROOT="$PWD"
   fi
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
4. Decompor o pedido:
   - um assunto vira uma consulta de 1–4 termos distintivos;
   - um pedido multifacetado vira 2–4 consultas independentes, uma por faceta;
   - identificadores literais, nomes de arquivo, ADRs, comandos e mensagens de erro são os melhores
     pontos de partida;
   - não enviar uma pergunta longa que mistura estado, comportamento, regras e próximos passos.
5. Fazer cada consulta focada, em paralelo quando a interface permitir:

   ```bash
   "$DOCS_SEARCH_BIN" search --root "$ROOT" --query "<1 a 4 termos>" --limit 5 --json
   ```

6. Selecionar pelos campos `path`, `heading`, `line_start`, `line_end`, `score` e
   `matched_terms`. Na resposta v2, `selection` registra o cap efetivo e `raw_rank` permite auditar
   resultados promovidos pela diversidade.
7. Ler o trecho correspondente no arquivo original e confirmar o hash/frescor quando isso for
   relevante.
8. Para uma faceta vazia, seguir **Resultado vazio**. Só aumentar o limite quando já existem
   candidatos relevantes.
9. Conferir código e configuração atuais antes de editar.

## Resultado vazio

O filtro lexical exige cerca de 60% dos termos significativos no mesmo chunk. Como os documentos
são divididos por headings, uma consulta que mistura várias respostas pode não ter candidato algum,
mesmo quando o corpus contém todas elas em chunks diferentes.

Para cada faceta vazia:

1. reduzir a consulta ao identificador literal ou a 1–2 termos centrais;
2. tentar no máximo uma segunda variante lexical, porque `gera`, `gerar` e `geração` não são
   equivalentes e “próximos passos” não implica “Plano” ou “Pronto quando”;
3. não repetir a frase original e não aumentar só `--limit`;
4. persistindo vazio, usar `rg` no corpus permitido conforme **Não fazer** e marcar a evidência como
   fallback sem ranking.

## Verificação

```bash
docs-search --version   # versão estável atual: 0.6.0
docs-search search --root . --query "sincronizar skills" --limit 1 --json
```

A busca deve retornar `schema_version` 2, `engine` `lexical-bm25-v1` e
`selection.max_results_per_path` nulo (cap desligado por padrão).

## Corpus

A versão atual busca `docs/**/*.md` e os arquivos de raiz `README.md`, `CLAUDE.md`, `AGENTS.md` e
`pi-warden.md`. A leitura é direta e não usa índice persistente. Arquivos de código, `.env` e
`skills/**` ficam fora do corpus padrão. Isso não detecta secrets escritos nos próprios Markdown;
credenciais nunca devem ser documentadas no corpus.

## Desenvolvimento e validação

```bash
cd tools/docs-search
cargo fmt --check

# Caminho de produto: não habilita adapters rejeitados.
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets

# Histórico experimental: compila e testa isoladamente.
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Execute as matrizes sequencialmente ou defina `CARGO_TARGET_DIR` distintos; não rode builds com
features diferentes em paralelo sobre o mesmo diretório de artefatos.

A feature não default `experimental-adapters` contém SQLite/BM25, FTS5, E5, RRF e os comandos
privados `bakeoff`/`holdout`. Não a habilite para busca, instalação ou distribuição normal. A build
padrão exclui tanto os comandos quanto Candle, Tokenizers e SQLite bundled; `cargo install --locked
--path tools/docs-search` produz essa build enxuta. `--all-features` serve apenas para garantir que o
histórico auditável ainda compila e passa seus testes — nunca para reabrir medições consumidas.

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
- Não habilitar `experimental-adapters` na instalação, busca ou distribuição de produto.
- Não implementar ou medir o bake-off se o verificador do freeze falhar.
- Não acessar holdouts consumidos, contornar a denylist ou alterar parâmetros de `engine-bakeoff-v1`.
- Não invocar `docs-search` sem antes confirmar `command -v docs-search`.
- Não engolir indisponibilidade ou erro do binário; reportar a falha e usar `rg` explicitamente
  apenas no corpus permitido. O mesmo fallback vale quando consultas atômicas e uma variante
  lexical continuam vazias:

  ```bash
  paths=()
  [ -d "$ROOT/docs" ] && paths+=("$ROOT/docs")
  for name in README.md CLAUDE.md AGENTS.md pi-warden.md; do
    [ -f "$ROOT/$name" ] && paths+=("$ROOT/$name")
  done
  if [ "${#paths[@]}" -eq 0 ]; then
    echo "nenhum arquivo do corpus documental encontrado" >&2
  else
    rg -n -i --glob '*.md' '<termo1|termo2|identificador>' "${paths[@]}"
  fi
  ```
