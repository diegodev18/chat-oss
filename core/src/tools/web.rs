//! Tool `web_search`: interfaz pluggable de búsqueda web.
//!
//! La tool delega en un [`SearchProvider`] concreto (DuckDuckGo, Tavily, Brave…)
//! que se inyecta al construirla. Aquí solo se define el contrato; el proveedor
//! real se implementa por separado (queda fuera del MVP).

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolResult};

/// Un resultado de búsqueda.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Backend de búsqueda. Se implementa por proveedor (queda pluggable).
#[async_trait]
pub trait SearchProvider: Send + Sync {
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>, String>;
}

/// Tool de búsqueda web sobre un proveedor inyectado.
pub struct WebSearchTool {
    provider: Box<dyn SearchProvider>,
    max_results: usize,
}

impl WebSearchTool {
    pub fn new(provider: Box<dyn SearchProvider>) -> Self {
        Self { provider, max_results: 5 }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Busca en internet y devuelve una lista de resultados (título, URL y extracto)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "La consulta de búsqueda." }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: &Value) -> ToolResult {
        let query = args
            .get("query")
            .and_then(Value::as_str)
            .ok_or_else(|| "falta el argumento 'query'".to_string())?;

        let results = self.provider.search(query).await?;
        if results.is_empty() {
            return Ok("sin resultados".into());
        }

        let formatted = results
            .iter()
            .take(self.max_results)
            .enumerate()
            .map(|(i, r)| format!("{}. {}\n   {}\n   {}", i + 1, r.title, r.url, r.snippet))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(formatted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeProvider;
    #[async_trait]
    impl SearchProvider for FakeProvider {
        async fn search(&self, query: &str) -> Result<Vec<SearchResult>, String> {
            Ok(vec![
                SearchResult {
                    title: format!("Resultado para {query}"),
                    url: "https://example.com/1".into(),
                    snippet: "primer extracto".into(),
                },
                SearchResult {
                    title: "Segundo".into(),
                    url: "https://example.com/2".into(),
                    snippet: "segundo extracto".into(),
                },
            ])
        }
    }

    #[tokio::test]
    async fn formats_provider_results() {
        let tool = WebSearchTool::new(Box::new(FakeProvider));
        let out = tool.execute(&json!({ "query": "rust" })).await.unwrap();
        assert!(out.contains("Resultado para rust"));
        assert!(out.contains("https://example.com/1"));
        assert!(out.contains("Segundo"));
    }

    #[tokio::test]
    async fn does_not_require_permission() {
        let tool = WebSearchTool::new(Box::new(FakeProvider));
        assert!(!tool.requires_permission());
        assert_eq!(tool.spec().function.name, "web_search");
    }

    #[tokio::test]
    async fn missing_query_is_error() {
        let tool = WebSearchTool::new(Box::new(FakeProvider));
        assert!(tool.execute(&json!({})).await.is_err());
    }
}
