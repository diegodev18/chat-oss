#!/usr/bin/env bash
# Borra la base de datos de conversaciones (~/.chatoss/chatoss.db).
# Pide confirmación salvo que se pase --yes.
source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

db="${CHATOSS_DB:-$HOME/.chatoss/chatoss.db}"

if [[ ! -f "$db" ]]; then
  ok "No hay base de datos en $db (nada que borrar)"
  exit 0
fi

size="$(du -h "$db" | cut -f1)"
warn "Esto elimina TODAS las conversaciones guardadas en:"
printf '    %s (%s)\n' "$db" "$size"

if [[ "${1:-}" != "--yes" ]]; then
  read -r -p "¿Continuar? [y/N] " reply
  [[ "$reply" =~ ^[yY]$ ]] || { info "Cancelado"; exit 0; }
fi

# Borra también los archivos auxiliares de SQLite (WAL/SHM) si existen.
rm -f "$db" "$db-wal" "$db-shm"
ok "Base de datos borrada"
