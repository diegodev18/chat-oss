#!/usr/bin/env bash
# Launch the NativeDesk desktop app.
#
#   ./scripts/run.sh             # release (rápido en runtime)
#   ./scripts/run.sh --debug     # build de debug (compila más rápido)
#   ./scripts/run.sh --debug --watch  # debug + reinicio al cambiar el código
#
# Cualquier otro argumento se pasa tal cual a `cargo run`.
source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

profile="--release"
watch=false
args=()
for arg in "$@"; do
  case "$arg" in
    --debug) profile="" ;;
    --watch) watch=true ;;
    *) args+=("$arg") ;;
  esac
done

if ! ollama_up; then
  warn "Ollama no responde en $OLLAMA_HOST."
  warn "Arrancalo en otra terminal con 'ollama serve' o la app no podrá chatear."
fi

run_cmd="run $profile -p nativedesk-ui --bin nativedesk"
if ((${#args[@]})); then
  run_cmd+=" ${args[*]}"
fi

if [[ "$watch" == true ]]; then
  cargo watch --version >/dev/null 2>&1 \
    || die "cargo-watch no está instalado. Instalalo con: cargo install cargo-watch"
  info "Modo desarrollo: se reiniciará al detectar cambios…"
  # shellcheck disable=SC2086
  exec cargo watch -q -x "$run_cmd"
fi

info "Lanzando NativeDesk…"
# shellcheck disable=SC2086
exec cargo $run_cmd
