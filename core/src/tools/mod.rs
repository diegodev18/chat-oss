//! Herramientas (tools) que el agente puede invocar, y su registro.

mod calc;
mod datetime;
mod fs;
mod registry;
mod shell;
mod web;

pub use calc::CalcTool;
pub use datetime::DateTimeTool;
pub use fs::{FsReadTool, FsWriteTool};
pub use registry::ToolRegistry;
pub use shell::ShellTool;
pub use web::{SearchProvider, SearchResult, WebSearchTool};

use async_trait::async_trait;
use serde_json::Value;

use crate::ollama::ToolSpec;

/// Resultado de ejecutar una tool. Tanto el `Ok` como el `Err` se realimentan
/// al modelo como contenido de un mensaje `tool` (el modelo decide cómo seguir).
pub type ToolResult = Result<String, String>;

/// Una herramienta invocable por el modelo vía function calling.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Nombre único, tal como aparecerá en el `ToolSpec` y en las `tool_calls`.
    fn name(&self) -> &str;

    /// Descripción en lenguaje natural para que el modelo sepa cuándo usarla.
    fn description(&self) -> &str;

    /// JSON Schema de los argumentos que acepta.
    fn parameters(&self) -> Value;

    /// Si es `true`, el agente debe pedir confirmación al usuario antes de ejecutar.
    fn requires_permission(&self) -> bool {
        false
    }

    /// Ejecuta la tool con los argumentos dados por el modelo.
    async fn execute(&self, args: &Value) -> ToolResult;

    /// Construye el `ToolSpec` que se envía a Ollama.
    fn spec(&self) -> ToolSpec {
        ToolSpec::function(self.name(), self.description(), self.parameters())
    }
}
