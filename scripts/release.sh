#!/usr/bin/env bash
# Compila el binario optimizado y muestra dónde quedó.
source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

info "Compilando release de chatoss…"
cargo build --release -p chatoss-ui --bin chatoss

bin="$REPO_ROOT/target/release/chatoss"
[[ -x "$bin" ]] || die "No se encontró el binario en $bin"

size="$(du -h "$bin" | cut -f1)"
ok "Binario listo: $bin ($size)"
info "Ejecutalo directamente con: $bin"
