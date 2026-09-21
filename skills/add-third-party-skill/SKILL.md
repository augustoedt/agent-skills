---
name: add-third-party-skill
description: Instala uma skill de terceiro (do marketplace skills.sh ou de outra fonte) e cataloga em third-party.json deste repo, sem versionar o conteúdo dela. Use quando o usuário pedir "adicione a skill de terceiro <nome>", "instala a skill X", ou "cataloga essa skill de terceiro". Roda scripts/add-third-party.sh depois de resolver o identificador exato com o usuário.
---

# Adicionar skill de terceiro

Instala uma skill de terceiro pelo mecanismo do próprio fornecedor (normalmente o marketplace
skills.sh) e registra a proveniência em `third-party.json` (nunca o conteúdo da skill — ver
`README.md` deste repo para o porquê dessa separação).

## Passos

1. **Buscar no marketplace**: `npx skills find <nome que o usuário deu>`.
2. **Resolver o identificador exato com o usuário**:
   - Se vier **um resultado só** e o nome bater claramente com o que foi pedido, pode seguir
     direto (mas ainda assim mostrar qual foi encontrado antes de instalar).
   - Se vier **mais de um resultado** (comum — nomes de skill não são únicos no marketplace,
     ex: buscar "use-railway" retorna também `davila7/claude-code-templates@railway-deploy`,
     `membranedev/application-skills@railway`, etc.), **listar as opções e perguntar qual é a
     certa** — nunca escolher por conta própria.
   - Se **não vier nenhum resultado**, informar ao usuário e perguntar se ele tem a fonte exata
     (`owner/repo@skill`) ou outra forma de instalação (não é tudo que está no skills.sh).
3. **Definir a descrição**: usar a descrição que o marketplace mostra, ou pedir pro usuário se não
   vier nada útil.
4. **Chamar o script**, que instala de verdade, cataloga e commita local (nunca dá push):
   ```bash
   ~/Projects/agent-skills/scripts/add-third-party.sh "<nome>" "<owner/repo@skill>" "<descrição>"
   ```
   O `update_cmd` default do script é `npx skills add <owner/repo@skill> -g -y` — só passar um
   4º argumento se essa skill específica precisar de outro mecanismo de update (como o
   `kimi-webbridge`, que usa `~/.kimi-webbridge/bin/kimi-webbridge upgrade` em vez do
   marketplace).
5. **Sincronizar pros agentes** rodando `scripts/install.sh` **não é necessário aqui** — skills de
   terceiro instaladas via `npx skills add -g` já se auto-instalam em todos os agentes
   detectados, diferente das skills próprias deste repo (que passam pelo `install.sh`).
6. Reportar ao usuário: nome, fonte, e que o commit local foi feito (mencionar o hash curto que o
   script imprime).

## Regras de segurança

- Nunca escolher entre múltiplos resultados ambíguos do marketplace sem confirmar com o usuário.
- Nunca dar `git push` — o script só commita local, e isso é intencional.
- Se o `npx skills add` falhar (pacote não encontrado, erro de rede, etc.), reportar o erro exato
  e não tentar contornar escrevendo o `third-party.json` manualmente sem a skill de fato instalada.
