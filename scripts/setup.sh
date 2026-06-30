#!/usr/bin/env bash
# Verifica las dependencias del proyecto y descarga el modelo de Ollama.
#
#   ./scripts/setup.sh            # usa el modelo por defecto (llama3.1:8b)
#   NATIVEDESK_MODEL=qwen2.5:7b ./scripts/setup.sh
source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

info "Comprobando toolchain de Rust…"
have cargo || die "cargo no está instalado. Instalá Rust desde https://rustup.rs"
ok "$(cargo --version)"

if have rustc; then
  ok "$(rustc --version)"
fi

info "Comprobando componentes opcionales (rustfmt, clippy)…"
have rustfmt || warn "rustfmt no encontrado. Instalalo con: rustup component add rustfmt"
cargo clippy --version >/dev/null 2>&1 || warn "clippy no encontrado. Instalalo con: rustup component add clippy"

info "Comprobando herramientas de desarrollo…"
cargo watch --version >/dev/null 2>&1 \
  || warn "cargo-watch no encontrado (necesario para 'make dev'). Instalalo con: cargo install cargo-watch"

info "Comprobando Ollama…"
if ! have ollama; then
  warn "ollama no está instalado. Descargalo desde https://ollama.com"
else
  ok "$(ollama --version 2>&1 | head -1)"
  if ollama_up; then
    ok "Ollama responde en $OLLAMA_HOST"
    if ollama list 2>/dev/null | awk 'NR>1{print $1}' | grep -qx "$NATIVEDESK_MODEL"; then
      ok "El modelo '$NATIVEDESK_MODEL' ya está descargado"
    else
      info "Descargando modelo '$NATIVEDESK_MODEL' (puede tardar)…"
      ollama pull "$NATIVEDESK_MODEL"
      ok "Modelo '$NATIVEDESK_MODEL' listo"
    fi
  else
    warn "Ollama no responde en $OLLAMA_HOST. Arrancalo con: ollama serve"
  fi
fi

info "Compilando el workspace para validar dependencias…"
cargo build --workspace
ok "Setup completo. Ejecutá la app con: ./scripts/run.sh"
