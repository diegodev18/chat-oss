//! Tool `datetime`: devuelve la fecha y hora actuales.

use async_trait::async_trait;
use chrono::{DateTime, Local, SecondsFormat};
use serde_json::{json, Value};

use super::{Tool, ToolResult};

/// Devuelve la fecha/hora local actual en formato RFC 3339.
///
/// El reloj es inyectable para poder testear con un instante fijo.
pub struct DateTimeTool {
    now: Box<dyn Fn() -> DateTime<Local> + Send + Sync>,
}

impl DateTimeTool {
    /// Reloj personalizado (para tests).
    pub fn with_clock<F>(now: F) -> Self
    where
        F: Fn() -> DateTime<Local> + Send + Sync + 'static,
    {
        Self { now: Box::new(now) }
    }
}

impl Default for DateTimeTool {
    fn default() -> Self {
        Self::with_clock(Local::now)
    }
}

#[async_trait]
impl Tool for DateTimeTool {
    fn name(&self) -> &str {
        "datetime"
    }

    fn description(&self) -> &str {
        "Devuelve la fecha y hora actuales (zona horaria local) en formato RFC 3339."
    }

    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }

    async fn execute(&self, _args: &Value) -> ToolResult {
        Ok((self.now)().to_rfc3339_opts(SecondsFormat::Secs, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Local, TimeZone};
    use serde_json::json;

    #[tokio::test]
    async fn returns_injected_time_in_rfc3339() {
        let fixed: DateTime<Local> = Local.with_ymd_and_hms(2026, 6, 3, 14, 30, 0).unwrap();
        let tool = DateTimeTool::with_clock(move || fixed);
        let out = tool.execute(&json!({})).await.unwrap();
        assert!(out.contains("2026-06-03"), "salida: {out}");
        // Debe ser parseable como RFC3339.
        assert!(DateTime::parse_from_rfc3339(out.trim()).is_ok(), "salida: {out}");
    }

    #[tokio::test]
    async fn default_clock_produces_parseable_time() {
        let out = DateTimeTool::default().execute(&json!({})).await.unwrap();
        assert!(DateTime::parse_from_rfc3339(out.trim()).is_ok(), "salida: {out}");
    }

    #[test]
    fn spec_has_expected_name() {
        assert_eq!(DateTimeTool::default().spec().function.name, "datetime");
        assert!(!DateTimeTool::default().requires_permission());
    }
}
