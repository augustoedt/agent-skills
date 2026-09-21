---
name: update-third-party-skills
description: Atualiza as skills de terceiro catalogadas em third-party.json (use-railway, kimi-webbridge, etc.), cada uma pelo seu próprio mecanismo de update. Use quando o usuário pedir "atualiza as skills de terceiro", "atualiza as skills externas", ou "atualiza a skill <nome>" para uma skill catalogada (não uma skill própria deste repo).
---

# Atualizar skills de terceiro

Roda o mecanismo de update de cada skill de terceiro catalogada em `third-party.json` — o
script não sabe nem precisa saber como cada uma funciona por dentro, só executa o `update_cmd`
gravado no cadastro de cada uma.

## Passos

1. Rodar o script, sem argumento pra atualizar todas, ou com o nome pra atualizar só uma:
   ```bash
   ~/Projects/agent-skills/scripts/update-third-party.sh                # todas
   ~/Projects/agent-skills/scripts/update-third-party.sh use-railway    # só uma
   ```
2. Reportar ao usuário, por skill: se atualizou com sucesso ou se deu erro (o script já imprime
   isso por entrada, com o comando rodado).
3. Se alguma falhar, **não tentar corrigir sozinho** — reportar o erro exato e perguntar como o
   usuário quer prosseguir (pode ser um problema do fornecedor, não da automação).

## Observações

- Isso **não** atualiza as skills próprias deste repo (as que ficam em `skills/`) — essas são
  editadas diretamente no repo e sincronizadas com `scripts/install.sh`, não têm "update" externo.
- Se o usuário pedir pra atualizar uma skill que não está em `third-party.json`, ela provavelmente
  é uma skill própria (usar `install.sh`) ou ainda não foi catalogada (usar a skill
  `add-third-party-skill` primeiro, que já cataloga e instala).
