# chatoss

App de escritorio **nativa en Rust** para chatear con modelos open source vía
**Ollama**, con un agente estilo Claude capaz de **invocar herramientas**
(function calling).

- **UI 100% Rust** (egui/eframe, sin webview) con markdown y resaltado de código.
- **Agente con tool calling**: el modelo decide cuándo usar herramientas y el agente
  ejecuta un loop hasta dar la respuesta final.
- **Persistencia** de conversaciones en SQLite.
- **Permisos**: las herramientas peligrosas piden confirmación en cada uso.

## Arquitectura

Workspace con dos crates:

- **`core`** — núcleo UI-agnóstico y testeable: cliente de Ollama, loop del agente,
  herramientas, y persistencia SQLite.
- **`ui`** — app egui. Un hilo dedicado con runtime tokio ejecuta el agente; la UI y el
  motor se comunican por canales (`Command` / `UiEvent`).

## Herramientas incluidas

| Tool        | Permiso | Descripción                                  |
|-------------|:-------:|----------------------------------------------|
| `calc`      | no      | Evalúa expresiones aritméticas.              |
| `datetime`  | no      | Fecha y hora actuales.                       |
| `fs_read`   | no      | Lee archivos dentro del directorio de trabajo.|
| `fs_write`  | **sí**  | Escribe archivos dentro del directorio.      |
| `shell`     | **sí**  | Ejecuta comandos de shell.                   |
| `web_search`| no      | Interfaz pluggable (sin proveedor por defecto).|

`fs_read`/`fs_write` están acotadas al directorio de trabajo (sin path traversal).

## Requisitos

- Rust estable (1.85+).
- [Ollama](https://ollama.com) corriendo y un modelo con soporte de tools, p.ej.:

  ```sh
  ollama pull llama3.1:8b
  ```

## Ejecutar

```sh
cargo run -p chatoss-ui --release
```

El host de Ollama se puede cambiar con la variable `OLLAMA_HOST`
(por defecto `http://localhost:11434`). Las conversaciones se guardan en
`~/.chatoss/chatoss.db`.

## Tests

```sh
# Unitarios y de integración (no requieren Ollama)
cargo test --workspace

# Integración real contra Ollama (requiere llama3.1:8b)
cargo test --workspace -- --ignored
```
