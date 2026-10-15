---
name: search-project-docs
description: Busca contexto mínimo e verificável na documentação de um projeto com o binário local docs-search, somente quando ele estiver disponível no PATH. Use antes de ler docs amplamente, ao retomar um projeto, localizar runbooks/ADRs/checkpoints, responder perguntas sobre decisões ou preparar mudanças que dependem das regras documentadas.
---

# Buscar documentação do projeto

Use `docs-search` para selecionar evidência antes de carregar arquivos inteiros, mas somente após
confirmar que o executável está disponível. A busca reduz o contexto inicial; ela não substitui a
leitura da fonte nem a verificação do código atual.

O motor estável é **lexical**, não semântico. Ele normaliza caixa e acentos, mas não entende
sinônimos nem flexões e exige que um mesmo chunk contenha a maioria dos termos significativos da
consulta. Por isso, uma pergunta longa com vários assuntos pode retornar zero mesmo quando cada
resposta existe em documentos diferentes.

## Fluxo

1. Determine o projeto mencionado na tarefa **antes** de escolher a raiz. Não use automaticamente
   o cwd quando ele for uma pasta agregadora de projetos ou quando a tarefa apontar para outro
   checkout. Defina `PROJECT_DIR` com o diretório do projeto identificado; use o cwd somente se ele
   for o contexto correto. Se houver vários projetos possíveis, peça a escolha, sem buscar todos:

   ```bash
   PROJECT_DIR="${PROJECT_DIR:-$PWD}" # definir explicitamente quando o projeto não for o cwd
   if ROOT="$(git -C "$PROJECT_DIR" rev-parse --show-toplevel 2>/dev/null)"; then
     :
   else
     ROOT="$PROJECT_DIR"
   fi
   ```

   Confirme que `ROOT` pertence ao projeto desejado. A busca não agrega automaticamente `docs/`
   de repositórios filhos nem pesquisa qualquer pasta de notas do Obsidian.

2. Verifique a disponibilidade antes de qualquer invocação:

   ```bash
   if command -v docs-search >/dev/null 2>&1; then
     DOCS_SEARCH_BIN="$(command -v docs-search)"
   else
     DOCS_SEARCH_BIN=""
   fi
   ```

   Se `DOCS_SEARCH_BIN` estiver vazio, não invoque `docs-search`; use a seção
   **Fallback: resultado vazio ou binário ausente**.
3. Transforme o pedido em consultas lexicais curtas:
   - um assunto: uma consulta de **1 a 4 termos distintivos**;
   - vários assuntos: **2 a 4 consultas atômicas independentes**, uma por faceta;
   - preserve identificadores literais (`retificar.py`, nome de módulo, ADR, erro, comando);
   - prefira palavras que provavelmente aparecem em títulos ou no texto; não envie a pergunta
     inteira, conectivos, justificativas ou objetivos futuros como uma única query.

   Exemplo: para “estado atual, o que `processar.py` gera, regras e como tornar replicável”, não
   faça uma busca omnibus. Use consultas como `processar.py`, `regras processamento` e
   `produto replicável`.

4. Execute cada consulta com poucos resultados. Consultas independentes podem ser disparadas em
   paralelo quando a interface disponível permitir:

   ```bash
   "$DOCS_SEARCH_BIN" search --root "$ROOT" --query "<1 a 4 termos>" --limit 5 --json
   ```

5. Antes de avaliar o ranking, confira `root`, `corpus.files` e `corpus.chunks` na resposta JSON.
   Erro do comando ou JSON inválido é falha da busca, não resultado vazio: reporte e use o fallback.
   Se `corpus.files = 0`, siga **Corpus vazio**; se há arquivos, mas `corpus.chunks = 0`, confira
   se eles contêm texto pesquisável antes de mudar os termos. Com chunks disponíveis, examine
   `path`, `heading`, linhas, `score` e `matched_terms`. Na resposta v2, `selection` registra o cap
   efetivo e `raw_rank` permite auditar promoções por diversidade. Leia no arquivo original somente
   os trechos relevantes. O excerpt é evidência de seleção, não fonte autoritativa.
6. Trate cada faceta vazia ou sem resultado relevante pelo procedimento **Resultado vazio** abaixo.
   Aumentar `--limit` não recupera candidatos quando `results` está vazio.
7. Se existem resultados relevantes, mas poucos, só então reformule com vocabulário encontrado
   nos títulos/excerpts ou aumente o limite.
8. Antes de editar, confira o código/configuração atual relacionado à documentação encontrada.

## Corpus padrão

- `docs/**/*.md`;
- `README.md`, `CLAUDE.md`, `AGENTS.md` e `pi-warden.md` na raiz.

Não entram automaticamente código, `.env` ou `skills/**`. Isso não detecta secrets escritos nos
próprios Markdown; credenciais nunca devem ser documentadas no corpus. `docs/` e os arquivos
originais continuam sendo a fonte da verdade; hashes e scores servem para rastreabilidade.

## Corpus vazio

`corpus.files = 0` significa **nenhum documento no corpus dessa raiz**, não termos ruins.
Não tente sinônimos nem aumente `--limit` nesse caso:

1. confira a raiz retornada e a existência de `docs/` e dos Markdown de raiz permitidos;
2. se a raiz estiver errada, corrija para o projeto identificado e repita a consulta focada;
3. se o projeto correto não tiver corpus, reporte isso. Se a fonte estiver fora dele, como em
   `skills/` ou num vault, leia os arquivos pertinentes à tarefa explicitamente, sem atribuir essa
   leitura ao `docs-search` nem ampliar o corpus silenciosamente.

## Resultado vazio

Com `corpus.chunks > 0`, `results: []` não significa que o corpus não contém a resposta. Para cada
faceta vazia ou sem resultado relevante:

1. reduza para o identificador literal ou 1–2 termos centrais, preferindo um único termo distintivo
   quando a combinação falhar;
2. tente no máximo mais uma variante lexical, trocando flexão ou usando vocabulário visto em
   nomes de arquivos, no índice documental ou em resultados de outra faceta. Exemplo:
   `sincronização` → `sincronizar`; ambas podem retornar documentos diferentes, não equivalentes;
3. não repita a frase longa e não aumente apenas `--limit`;
4. se continuar sem evidência relevante, use o fallback `rg` abaixo no corpus permitido e declare
   que a recuperação veio do fallback, não do ranking.

Exemplos de diferenças que importam para o motor lexical: `gera`, `gerar` e `geração` não são o
mesmo termo; “próximos passos” não encontra automaticamente títulos como “Plano” ou “Pronto
quando”.

## Fallback: resultado vazio ou binário ausente

Se `command -v docs-search` falhar, não tente executar o binário. Reporte a indisponibilidade e use
`rg`. Use o mesmo fallback depois das tentativas focadas que ainda retornarem vazio. Restrinja-o ao
corpus documental permitido:

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

Instale ou atualize somente quando a tarefa pedir isso explicitamente. Depois do instalador,
confirme novamente a disponibilidade antes de usar:

```bash
REPO="${AGENT_SKILLS_REPO:-$HOME/Projects/agent-skills}"
"$REPO/scripts/install-docs-search.sh"
command -v docs-search >/dev/null 2>&1 || {
  echo "docs-search indisponível no PATH" >&2
  exit 1
}
docs-search --version
```

A instalação normal usa a build de produto, sem a feature `experimental-adapters`. Não habilite essa
feature para busca ou instalação: ela existe somente para compilar o histórico rejeitado de
SQLite/BM25, FTS5, E5, RRF e dos harnesses privados.

## Contexto mínimo suficiente

- Comece com `--limit 5`; não despeje todos os resultados no prompt.
- Divida pedidos multifacetados; limite não compensa uma query lexical longa demais.
- Preserve path, heading e linhas ao citar uma evidência.
- Prefira uma segunda consulta focada a carregar um arquivo grande sem seleção.
- Resultado vazio após consultas atômicas e fallback significa “não encontrado neste corpus”; não
  prova que a informação inexiste.
- Nunca trate índice, excerpt ou documentação antiga como substituto da realidade do código.

## Diversidade por path

A opção `--max-results-per-path N` existe para avaliação explícita e limita quantos chunks do mesmo
arquivo entram no resultado final. Ela é opt-in: não a aplique silenciosamente à busca normal. Ao
comparar configurações, use `docs-search evaluate`; o relatório v2 preserva rank final e rank BM25
bruto para auditoria.

O runbook operacional correspondente é `docs/runbooks/buscar-documentacao-de-projetos.md` no
repositório `agent-skills`; mantenha ambos sincronizados quando o processo mudar.
