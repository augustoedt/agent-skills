---
name: search-project-docs
description: Busca contexto mínimo e verificável na documentação de um projeto com o binário local docs-search, somente quando ele estiver disponível no PATH. Use antes de ler docs amplamente, ao retomar um projeto, localizar runbooks/ADRs/checkpoints, responder perguntas sobre decisões ou preparar mudanças que dependem das regras documentadas.
---

# Buscar documentação do projeto

Use `docs-search` para selecionar evidência antes de carregar arquivos inteiros, mas somente após
confirmar que o executável está disponível. A busca reduz o contexto inicial; ela não substitui a
leitura da fonte nem a verificação do código atual.

## Fluxo

1. Determine a raiz do projeto. Em um repositório Git, prefira:

   ```bash
   ROOT="$(git rev-parse --show-toplevel)"
   ```

2. Verifique a disponibilidade antes de qualquer invocação:

   ```bash
   if command -v docs-search >/dev/null 2>&1; then
     DOCS_SEARCH_BIN="$(command -v docs-search)"
   else
     DOCS_SEARCH_BIN=""
   fi
   ```

   Se `DOCS_SEARCH_BIN` estiver vazio, não invoque `docs-search`; use o fallback da seção
   **Binário ausente**.
3. Faça uma pergunta específica e peça poucos resultados:

   ```bash
   "$DOCS_SEARCH_BIN" search --root "$ROOT" --query "<pergunta objetiva>" --limit 5 --json
   ```

4. Examine primeiro `path`, `heading`, linhas, `score` e `matched_terms`. Na resposta v2,
   `selection` registra o cap efetivo e `raw_rank` permite auditar promoções por diversidade. Leia
   no arquivo original somente os trechos relevantes. O excerpt é evidência de seleção, não fonte
   autoritativa.
5. Se a evidência for insuficiente, reformule uma vez com termos do domínio ou aumente o limite.
   Só depois recorra a uma leitura documental ampla.
6. Antes de editar, confira o código/configuração atual relacionado à documentação encontrada.

## Corpus padrão

- `docs/**/*.md`;
- `README.md`, `CLAUDE.md`, `AGENTS.md` e `pi-warden.md` na raiz.

Não entram automaticamente código, `.env` ou `skills/**`. Isso não detecta secrets escritos nos
próprios Markdown; credenciais nunca devem ser documentadas no corpus. `docs/` e os arquivos
originais continuam sendo a fonte da verdade; hashes e scores servem para rastreabilidade.

## Binário ausente

Se `command -v docs-search` falhar, não tente executar o binário. Reporte que ele não está
disponível e use `rg` somente no corpus documental permitido:

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

## Contexto mínimo suficiente

- Comece com `--limit 5`; não despeje todos os resultados no prompt.
- Preserve path, heading e linhas ao citar uma evidência.
- Prefira uma segunda consulta focada a carregar um arquivo grande sem seleção.
- Resultado vazio significa “não encontrado neste corpus”, não prova que a informação inexiste.
- Nunca trate índice, excerpt ou documentação antiga como substituto da realidade do código.

## Diversidade por path

A opção `--max-results-per-path N` existe para avaliação explícita e limita quantos chunks do mesmo
arquivo entram no resultado final. Ela é opt-in: não a aplique silenciosamente à busca normal. Ao
comparar configurações, use `docs-search evaluate`; o relatório v2 preserva rank final e rank BM25
bruto para auditoria.

O runbook operacional correspondente é `docs/runbooks/buscar-documentacao-de-projetos.md` no
repositório `agent-skills`; mantenha ambos sincronizados quando o processo mudar.
