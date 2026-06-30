//! Pruebas de integración contra un Ollama real.
//!
//! Requieren `ollama serve` y el modelo `llama3.1:8b` (`ollama pull llama3.1:8b`).
//! Por eso están marcadas `#[ignore]`; se ejecutan con:
//!   cargo test -p nativedesk-core --test live_ollama -- --ignored --nocapture

use async_trait::async_trait;
use nativedesk_core::agent::{Agent, PermissionGate};
use nativedesk_core::ollama::{ChatMessage, HttpOllamaClient, OllamaClient};
use nativedesk_core::tools::{CalcTool, ToolRegistry};
use serde_json::Value;

struct AllowAll;
#[async_trait]
impl PermissionGate for AllowAll {
    async fn request(&self, _tool: &str, _args: &Value) -> bool {
        true
    }
}

const HOST: &str = "http://localhost:11434";
const MODEL: &str = "llama3.1:8b";

#[tokio::test]
#[ignore = "requiere Ollama real"]
async fn lists_installed_models() {
    let client = HttpOllamaClient::new(HOST);
    let models = client.list_models().await.expect("Ollama debe responder");
    assert!(!models.is_empty(), "debe haber al menos un modelo instalado");
    println!("modelos: {:?}", models.iter().map(|m| &m.name).collect::<Vec<_>>());
}

#[tokio::test]
#[ignore = "requiere Ollama real"]
async fn agent_uses_calc_tool_end_to_end() {
    let client = HttpOllamaClient::new(HOST);
    let mut reg = ToolRegistry::new();
    reg.register(CalcTool);
    let agent = Agent::new(client, reg);

    let mut history = vec![
        ChatMessage::system(
            "Eres un asistente. Para cualquier cálculo aritmético DEBES usar la tool 'calc'.",
        ),
        ChatMessage::user("¿Cuánto es 23 multiplicado por 19? Usa la herramienta."),
    ];

    let mut tool_used = false;
    let mut on_event = |e: nativedesk_core::agent::AgentEvent| {
        if let nativedesk_core::agent::AgentEvent::ToolStarted { name, args } = &e {
            println!("[tool] {name} {args}");
            if name == "calc" {
                tool_used = true;
            }
        }
    };

    let answer = agent
        .run_turn(MODEL, &mut history, &AllowAll, &mut on_event)
        .await
        .expect("el turno debe completarse");

    println!("respuesta final: {answer}");
    assert!(tool_used, "el modelo debió invocar la tool calc");
    assert!(answer.contains("437"), "la respuesta debe contener 437: {answer}");
}
