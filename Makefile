# Atajos para las tareas comunes de chatoss.
# Ejecutá `make` o `make help` para ver los objetivos disponibles.

.DEFAULT_GOAL := help
.PHONY: help setup run dev check fmt clippy test test-live release reset-db clean

help: ## Muestra esta ayuda
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

setup: ## Verifica dependencias y descarga el modelo de Ollama
	@./scripts/setup.sh

run: ## Ejecuta la app (release)
	@./scripts/run.sh

dev: ## Ejecuta la app en debug y la reinicia al cambiar el código
	@./scripts/run.sh --debug --watch

check: ## Puerta de calidad: fmt --check + clippy + tests
	@./scripts/check.sh

fmt: ## Aplica cargo fmt a todo el workspace
	@cargo fmt --all

clippy: ## Corre clippy tratando warnings como errores
	@cargo clippy --workspace --all-targets -- -D warnings

test: ## Tests del workspace (sin Ollama)
	@./scripts/test.sh

test-live: ## Tests de integración contra un Ollama real
	@./scripts/test.sh --live

release: ## Compila el binario optimizado
	@./scripts/release.sh

reset-db: ## Borra la base de datos de conversaciones
	@./scripts/reset-db.sh

clean: ## Limpia los artefactos de compilación
	@cargo clean
