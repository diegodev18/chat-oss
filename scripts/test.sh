#!/usr/bin/env bash
# Ejecuta los tests del workspace.
#
#   ./scripts/test.sh            # unitarios + integración (no requieren Ollama)
#   ./scripts/test.sh --live     # incluye los tests #[ignore] contra Ollama real
#
# Argumentos extra se reenvían a `cargo test`.
source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

live=0
args=()
for arg in "$@"; do
  case "$arg" in
    --live) live=1 ;;
    *) args+=("$arg") ;;
  esac
done

if [[ "$live" -eq 1 ]]; then
  ollama_up || die "Ollama no responde en $OLLAMA_HOST; los tests live lo necesitan. Probá 'ollama serve'."
  if ! ollama list 2>/dev/null | awk 'NR>1{print $1}' | grep -qx "$CHATOSS_MODEL"; then
    die "Falta el modelo '$CHATOSS_MODEL'. Descargalo con: ollama pull $CHATOSS_MODEL"
  fi
  info "Tests live contra Ollama ($CHATOSS_MODEL)…"
  exec cargo test --workspace ${args:+"${args[@]}"} -- --ignored --nocapture
fi

info "Tests del workspace…"
exec cargo test --workspace ${args:+"${args[@]}"}
