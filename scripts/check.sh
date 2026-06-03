#!/usr/bin/env bash
# Puerta de calidad local: formato, clippy y tests. Ideal antes de commitear.
#
#   ./scripts/check.sh           # falla si el formato no está aplicado
#   ./scripts/check.sh --fix     # aplica `cargo fmt` en lugar de solo verificar
source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

fix=0
[[ "${1:-}" == "--fix" ]] && fix=1

if [[ "$fix" -eq 1 ]]; then
  info "Formateando (cargo fmt)…"
  cargo fmt --all
  ok "Formato aplicado"
else
  info "Verificando formato (cargo fmt --check)…"
  cargo fmt --all --check || die "Formato incorrecto. Corregilo con: ./scripts/check.sh --fix"
  ok "Formato correcto"
fi

info "Clippy (warnings = errores)…"
cargo clippy --workspace --all-targets -- -D warnings
ok "Clippy sin advertencias"

info "Tests…"
cargo test --workspace
ok "Todos los checks pasaron 🎉"
