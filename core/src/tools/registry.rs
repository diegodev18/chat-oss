//! Registro de tools: despacha por nombre y genera los `ToolSpec` para Ollama.

use std::collections::HashMap;
use std::sync::Arc;

use crate::ollama::ToolSpec;
use crate::tools::Tool;

/// Conjunto de tools disponibles para el agente, indexadas por nombre.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registra una tool. Si ya existía una con el mismo nombre, la reemplaza.
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_string(), Arc::new(tool));
    }

    /// Devuelve la tool con ese nombre, si existe.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// `ToolSpec` de todas las tools, para enviar a Ollama.
    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|t| t.spec()).collect()
    }

    /// `true` si no hay ninguna tool registrada.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{CalcTool, DateTimeTool};
    use serde_json::json;

    #[test]
    fn specs_lists_every_registered_tool() {
        let mut reg = ToolRegistry::new();
        reg.register(CalcTool);
        reg.register(DateTimeTool::default());

        let names: Vec<_> = reg.specs().iter().map(|s| s.function.name.clone()).collect();
        assert!(names.contains(&"calc".to_string()));
        assert!(names.contains(&"datetime".to_string()));
    }

    #[tokio::test]
    async fn get_dispatches_to_the_named_tool() {
        let mut reg = ToolRegistry::new();
        reg.register(CalcTool);

        let tool = reg.get("calc").expect("debe encontrar calc");
        let out = tool.execute(&json!({ "expr": "2+2" })).await.unwrap();
        assert_eq!(out, "4");
    }

    #[test]
    fn get_unknown_tool_returns_none() {
        let reg = ToolRegistry::new();
        assert!(reg.get("inexistente").is_none());
    }
}
