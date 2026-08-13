#!/usr/bin/env bash
set -euo pipefail

project="${1:-.}"
cd "$project"

if [[ ! -f mix.exs ]]; then
  echo "error: not a Mix project: $(pwd)" >&2
  exit 2
fi

failed=0

for dependency in phia_ui backpex ash_backpex; do
  if grep -Eq "\{:${dependency},|:${dependency}," mix.exs mix.lock 2>/dev/null; then
    echo "error: forbidden admin UI dependency detected: ${dependency}" >&2
    failed=1
  fi
done

if grep -Eq '"(svelte|shadcn-svelte|lucide-svelte|@lucide/svelte)"[[:space:]]*:' \
  assets/package.json package.json 2>/dev/null; then
  echo "error: Svelte/shadcn runtime dependency detected" >&2
  failed=1
fi

forbidden_runtime_refs="$({
  find lib assets -type f \
    ! -path '*/node_modules/*' ! -path '*/vendor/*' ! -name '*.md' \
    -exec grep -InE 'bun x shadcn-svelte|@plugin[[:space:]]+"daisyui"|\.svelte' {} + 2>/dev/null || true
  grep -InE 'bun x shadcn-svelte|@plugin[[:space:]]+"daisyui"|\.svelte' mix.exs 2>/dev/null || true
})"
if [[ -n "$forbidden_runtime_refs" ]]; then
  printf '%s\n' "$forbidden_runtime_refs"
  echo "error: forbidden Svelte/shadcn/daisyUI runtime reference detected" >&2
  failed=1
fi

if grep -Eq '"daisyui"[[:space:]]*:' assets/package.json package.json 2>/dev/null; then
  echo "error: forbidden admin UI dependency detected: daisyui" >&2
  failed=1
fi

if [[ -f assets/css/admin-theme.css ]]; then
  for token in background foreground card primary muted border ring sidebar chart-1; do
    if ! grep -Eq -- "--${token}:[[:space:]]*oklch\(" assets/css/admin-theme.css; then
      echo "error: missing or non-OKLCH admin theme token --${token}" >&2
      failed=1
    fi
  done
  if ! grep -Eq '@import[[:space:]]+.*admin-theme\.css' assets/css/app.css 2>/dev/null; then
    echo "warning: assets/css/admin-theme.css is not imported by assets/css/app.css" >&2
  fi
else
  echo "warning: assets/css/admin-theme.css not found" >&2
fi

if [[ -f assets/js/admin_sidebar_hook.js ]]; then
  if ! grep -q 'AdminSidebar' assets/js/app.js 2>/dev/null; then
    echo "warning: AdminSidebar hook exists but is not referenced by assets/js/app.js" >&2
  fi
  if ! grep -Eq 'Hooks[[:space:]]*=|hooks:[[:space:]]*Hooks' assets/js/app.js 2>/dev/null; then
    echo "warning: verify AdminSidebar is registered in the LiveSocket hooks object" >&2
  fi
fi

admin_paths=()
while IFS= read -r path; do admin_paths+=("$path"); done < <(
  find lib -type f \( -path '*_web/live/admin/*' -o -path '*_web/components/admin_components.ex' \) 2>/dev/null
)

if [[ "${#admin_paths[@]}" -gt 0 ]]; then
  if grep -nH 'authorize?: false' "${admin_paths[@]}"; then
    echo "error: authorize?: false detected in admin web layer" >&2
    failed=1
  fi

  direct_data_access="$({ grep -EnH 'Repo\.|Ecto\.|Ecto\.Adapters\.SQL' "${admin_paths[@]}" || true; } | grep -v 'Ash\.Repo\.' || true)"
  if [[ -n "$direct_data_access" ]]; then
    printf '%s\n' "$direct_data_access"
    echo "error: direct Repo/Ecto/SQL usage detected in admin LiveView/components" >&2
    failed=1
  fi
fi

if [[ "$failed" -ne 0 ]]; then
  exit 1
fi

echo "Phoenix admin UI invariants passed"
