# shellcheck shell=bash
# Funciones y variables comunes para los scripts de chatoss.
# Se importa con:  source "$(dirname "$0")/_lib.sh"

set -euo pipefail

# Raíz del repositorio (el directorio padre de scripts/).
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Colores (se desactivan si no hay TTY).
if [[ -t 1 ]]; then
  C_RESET=$'\033[0m'; C_BOLD=$'\033[1m'
  C_RED=$'\033[31m'; C_GREEN=$'\033[32m'; C_YELLOW=$'\033[33m'; C_BLUE=$'\033[34m'
else
  C_RESET=""; C_BOLD=""; C_RED=""; C_GREEN=""; C_YELLOW=""; C_BLUE=""
fi

info()  { printf '%s==>%s %s\n' "$C_BLUE$C_BOLD" "$C_RESET" "$*"; }
ok()    { printf '%s✓%s %s\n'  "$C_GREEN"        "$C_RESET" "$*"; }
warn()  { printf '%s!%s %s\n'  "$C_YELLOW"       "$C_RESET" "$*" >&2; }
die()   { printf '%s✗%s %s\n'  "$C_RED"          "$C_RESET" "$*" >&2; exit 1; }

have() { command -v "$1" >/dev/null 2>&1; }

# Host de Ollama (configurable con OLLAMA_HOST).
OLLAMA_HOST="${OLLAMA_HOST:-http://localhost:11434}"
# Modelo por defecto usado en los tests live y en setup.
CHATOSS_MODEL="${CHATOSS_MODEL:-llama3.1:8b}"

# Devuelve 0 si Ollama responde en OLLAMA_HOST.
ollama_up() {
  curl -fsS --max-time 2 "$OLLAMA_HOST/api/tags" >/dev/null 2>&1
}
