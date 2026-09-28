# Runbook — Sincronizar skills e eliminar conflitos de nome

Este runbook restaura uma cópia canônica e elimina conflitos de nome causados por skills
duplicadas.

## Quando

Use após alterar uma skill própria, depois de atualizar o repositório ou quando o Pi avisar sobre
colisão de nome causada por cópias físicas duplicadas.

## Contexto

`~/.agents/skills/` é o diretório físico canônico. Os diretórios de cada agente apontam para ele
com symlinks. Uma cópia real em dois locais pode causar conflito; uma cópia canônica com symlinks
evita cópias duplicadas no Pi.

## Passos

1. Execute o instalador do repositório para sincronizar skills próprias.
2. Liste diretórios de skills que são cópias reais em vez de symlinks.
3. Compare as duas cópias antes de substituir uma duplicata por symlink.
4. Verifique que não restou diretório real duplicado nem symlink quebrado.

Esses passos explicam como sincronizar skills próprias, eliminar conflitos de nome e verificar um
symlink quebrado sem apagar a fonte canônica.

## Não fazer

Não sobrescreva uma skill diferente sem revisar o diff. Não apague a cópia canônica. Não versione
o conteúdo de skills de terceiros no repositório de skills próprias.
