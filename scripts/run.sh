#!/usr/bin/env bash
# Ejecuta la app de escritorio chatoss.
#
#   ./scripts/run.sh             # release (rápido en runtime)
#   ./scripts/run.sh --debug     # build de debug (compila más rápido)
#
# Cualquier otro argumento se pasa tal cual a `cargo run`.
source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

profile="--release"
args=()
for arg in "$@"; do
  case "$arg" in
    --debug) profile="" ;;
    *) args+=("$arg") ;;
  esac
done

if ! ollama_up; then
  warn "Ollama no responde en $OLLAMA_HOST."
  warn "Arrancalo en otra terminal con 'ollama serve' o la app no podrá chatear."
fi

info "Lanzando chatoss…"
# shellcheck disable=SC2086
exec cargo run $profile -p chatoss-ui --bin chatoss "${args[@]}"
