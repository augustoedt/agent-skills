#!/usr/bin/env bash
# generate-docker-compose.sh <project-dir>
#
# Gera o docker-compose.yml do PostgreSQL com base no config/dev.exs do projeto
# Phoenix/Ash. Extrai o nome do banco de `database: "..."` no dev.exs e monta
# o compose seguindo o padrão do stack (postgres:16-alpine, container
# <project>_postgres, db <app>_dev).
set -euo pipefail

DIR="${1:-.}"
DEV_EXS="$DIR/config/dev.exs"

if [[ ! -f "$DEV_EXS" ]]; then
  echo "erro: $DEV_EXS não encontrado. Rode de dentro do projeto ou passe o diretório como argumento." >&2
  exit 1
fi

DB_NAME="$(grep -oE 'database:[[:space:]]*"[^"]+"' "$DEV_EXS" | head -1 | sed -E 's/.*"([^"]+)"/\1/')"
if [[ -z "$DB_NAME" ]]; then
  echo "erro: não encontrei 'database: \"...\"' em $DEV_EXS" >&2
  exit 1
fi

PROJECT_NAME="$(basename "$(cd "$DIR" && pwd)")"

cat > "$DIR/docker-compose.yml" <<EOF
services:
  postgres:
    image: postgres:16-alpine
    container_name: "${PROJECT_NAME}_postgres"
    restart: unless-stopped

    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: "${DB_NAME}"

    ports:
      - "5432:5432"

    volumes:
      - postgres_data:/var/lib/postgresql/data

    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres -d ${DB_NAME}"]
      interval: 5s
      timeout: 5s
      retries: 10

volumes:
  postgres_data:
EOF

echo "✅ docker-compose.yml criado em $DIR"
echo "   container_name: ${PROJECT_NAME}_postgres"
echo "   POSTGRES_DB:    ${DB_NAME} (extraído de $DEV_EXS)"
echo ""
echo "Suba o banco com: docker compose up -d postgres"
