#!/usr/bin/env bash
# Compila el binario optimizado y muestra dónde quedó.
source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

info "Compilando release de NativeDesk…"
cargo build --release -p nativedesk-ui --bin nativedesk

bin="$REPO_ROOT/target/release/nativedesk"
[[ -x "$bin" ]] || die "No se encontró el binario en $bin"

size="$(du -h "$bin" | cut -f1)"
ok "Binario listo: $bin ($size)"
info "Ejecutalo directamente con: $bin"
