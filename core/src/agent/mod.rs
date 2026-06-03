//! Loop agéntico: orquesta Ollama + tools hasta producir una respuesta final.

use async_trait::async_trait;
use serde_json::Value;

use crate::ollama::{
    ChatMessage, ChatRequest, ChatStreamChunk, OllamaClient, OllamaError, Role, ToolCall,
};
use crate::tools::{ToolRegistry, ToolResult};

/// Eventos que el agente emite durante un turno, para que la UI los muestre.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    /// Un fragmento de texto del asistente (streaming).
    Token(String),
    /// El agente empezó a ejecutar una tool.
    ToolStarted { name: String, args: Value },
    /// La tool terminó (Ok = salida, Err = mensaje de error / denegación).
    ToolFinished { name: String, result: ToolResult },
}

/// Decide si una tool que requiere permiso puede ejecutarse.
///
/// La UI implementa esto mostrando un diálogo; los tests usan variantes que
/// siempre permiten o siempre deniegan.
#[async_trait]
pub trait PermissionGate: Send + Sync {
    async fn request(&self, tool_name: &str, args: &Value) -> bool;
}

/// Errores no recuperables de un turno del agente.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error(transparent)]
    Ollama(#[from] OllamaError),
    #[error("se alcanzó el tope de {0} iteraciones sin respuesta final")]
    MaxIterations(usize),
}

/// El agente: combina un cliente de Ollama y un registro de tools.
///
/// Es agnóstico del modelo: este se indica por turno en [`Agent::run_turn`],
/// porque cada conversación puede usar un modelo distinto.
pub struct Agent<C> {
    client: C,
    tools: ToolRegistry,
    max_iterations: usize,
}

impl<C: OllamaClient> Agent<C> {
    pub fn new(client: C, tools: ToolRegistry) -> Self {
        Self { client, tools, max_iterations: 10 }
    }

    pub fn with_max_iterations(mut self, n: usize) -> Self {
        self.max_iterations = n;
        self
    }

    /// Ejecuta un turno completo con el modelo `model`: llama a Ollama, ejecuta
    /// las tools que pida y repite hasta obtener una respuesta final en texto.
    /// Va añadiendo al `history` los mensajes del asistente y los resultados de
    /// las tools.
    pub async fn run_turn(
        &self,
        model: &str,
        history: &mut Vec<ChatMessage>,
        gate: &dyn PermissionGate,
        on_event: &mut (dyn FnMut(AgentEvent) + Send),
    ) -> Result<String, AgentError> {
        for _ in 0..self.max_iterations {
            let request = ChatRequest {
                model: model.to_string(),
                messages: history.clone(),
                tools: self.tools.specs(),
                stream: true,
            };

            // Acumula la respuesta del modelo mientras la transmite token a token.
            let mut content = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            {
                let mut on_chunk = |chunk: ChatStreamChunk| {
                    if let Some(msg) = chunk.message {
                        if !msg.content.is_empty() {
                            on_event(AgentEvent::Token(msg.content.clone()));
                            content.push_str(&msg.content);
                        }
                        tool_calls.extend(msg.tool_calls);
                    }
                };
                self.client.chat_stream(request, &mut on_chunk).await?;
            }

            // Sin tool calls => es la respuesta final.
            if tool_calls.is_empty() {
                history.push(ChatMessage::assistant(content.clone()));
                return Ok(content);
            }

            // Registra el mensaje del asistente con sus tool calls...
            history.push(ChatMessage {
                role: Role::Assistant,
                content: content.clone(),
                tool_calls: tool_calls.clone(),
            });

            // ...y ejecuta cada tool, realimentando el resultado al modelo.
            for call in &tool_calls {
                let outcome = self
                    .run_tool(&call.function.name, &call.function.arguments, gate, on_event)
                    .await;
                let text = match outcome {
                    Ok(s) => s,
                    Err(s) => s,
                };
                history.push(ChatMessage::tool_result(text));
            }
        }

        Err(AgentError::MaxIterations(self.max_iterations))
    }

    /// Resuelve, autoriza y ejecuta una única tool. Tanto el éxito como el error
    /// se devuelven como texto que se realimentará al modelo.
    async fn run_tool(
        &self,
        name: &str,
        args: &Value,
        gate: &dyn PermissionGate,
        on_event: &mut (dyn FnMut(AgentEvent) + Send),
    ) -> ToolResult {
        let Some(tool) = self.tools.get(name) else {
            let msg = format!("error: la tool '{name}' no existe");
            on_event(AgentEvent::ToolFinished { name: name.into(), result: Err(msg.clone()) });
            return Err(msg);
        };

        if tool.requires_permission() && !gate.request(name, args).await {
            let msg = format!("el usuario denegó el permiso para ejecutar '{name}'");
            on_event(AgentEvent::ToolFinished { name: name.into(), result: Err(msg.clone()) });
            return Err(msg);
        }

        on_event(AgentEvent::ToolStarted { name: name.into(), args: args.clone() });
        let result = tool.execute(args).await;
        on_event(AgentEvent::ToolFinished { name: name.into(), result: result.clone() });
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ollama::{
        ChatRequest, ChatStreamChunk, FunctionCall, ModelInfo, OllamaClient, OllamaError,
        StreamMessage, ToolCall,
    };
    use crate::tools::{CalcTool, Tool, ToolRegistry, ToolResult};
    use async_trait::async_trait;
    use serde_json::{json, Value};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    // --- Mock de Ollama: respuestas guionizadas + captura de peticiones ---

    struct MockOllama {
        scripted: Mutex<VecDeque<Vec<ChatStreamChunk>>>,
        seen: Mutex<Vec<ChatRequest>>,
    }

    impl MockOllama {
        fn new(responses: Vec<Vec<ChatStreamChunk>>) -> Self {
            Self {
                scripted: Mutex::new(responses.into()),
                seen: Mutex::new(Vec::new()),
            }
        }
        fn calls(&self) -> usize {
            self.seen.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl OllamaClient for MockOllama {
        async fn list_models(&self) -> Result<Vec<ModelInfo>, OllamaError> {
            Ok(vec![])
        }
        async fn chat_stream(
            &self,
            request: ChatRequest,
            on_chunk: &mut (dyn FnMut(ChatStreamChunk) + Send),
        ) -> Result<(), OllamaError> {
            self.seen.lock().unwrap().push(request);
            let next = self.scripted.lock().unwrap().pop_front().unwrap_or_default();
            for chunk in next {
                on_chunk(chunk);
            }
            Ok(())
        }
    }

    fn token(s: &str) -> ChatStreamChunk {
        ChatStreamChunk {
            message: Some(StreamMessage { content: s.into(), tool_calls: vec![] }),
            done: false,
        }
    }

    fn tool_call(name: &str, args: Value) -> ChatStreamChunk {
        ChatStreamChunk {
            message: Some(StreamMessage {
                content: String::new(),
                tool_calls: vec![ToolCall {
                    function: FunctionCall { name: name.into(), arguments: args },
                }],
            }),
            done: false,
        }
    }

    fn done() -> ChatStreamChunk {
        ChatStreamChunk { message: None, done: true }
    }

    fn calc_registry() -> ToolRegistry {
        let mut reg = ToolRegistry::new();
        reg.register(CalcTool);
        reg
    }

    fn collect_events() -> (Arc<Mutex<Vec<AgentEvent>>>, impl FnMut(AgentEvent) + Send) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = {
            let log = log.clone();
            move |e: AgentEvent| log.lock().unwrap().push(e)
        };
        (log, sink)
    }

    // --- Tests ---

    #[tokio::test]
    async fn returns_final_answer_without_tools() {
        let mock = MockOllama::new(vec![vec![token("Hola "), token("mundo"), done()]]);
        let agent = Agent::new(mock, ToolRegistry::new()).with_max_iterations(5);
        let mut history = vec![crate::ollama::ChatMessage::user("hi")];
        let (log, mut sink) = collect_events();

        let out = agent
            .run_turn("m", &mut history, &AllowAll, &mut sink)
            .await
            .unwrap();

        assert_eq!(out, "Hola mundo");
        // El asistente debe quedar añadido al historial.
        assert_eq!(history.last().unwrap().content, "Hola mundo");
        // Se emitieron tokens en streaming.
        let tokens: Vec<_> = log
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Token(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(tokens, vec!["Hola ", "mundo"]);
    }

    #[tokio::test]
    async fn executes_tool_then_produces_final_answer() {
        let mock = MockOllama::new(vec![
            vec![tool_call("calc", json!({"expr": "23*19"})), done()],
            vec![token("El resultado es 437"), done()],
        ]);
        let agent = Agent::new(mock, calc_registry());
        let mut history = vec![crate::ollama::ChatMessage::user("cuanto es 23*19?")];
        let (log, mut sink) = collect_events();

        let out = agent
            .run_turn("m", &mut history, &AllowAll, &mut sink)
            .await
            .unwrap();

        assert_eq!(out, "El resultado es 437");
        // El historial debe contener el resultado de la tool ("437").
        assert!(
            history.iter().any(|m| m.content == "437"),
            "historial: {history:?}"
        );
        // Se emitieron eventos de inicio y fin de tool.
        let events = log.lock().unwrap();
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolStarted { name, .. } if name == "calc")));
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolFinished { name, .. } if name == "calc")));
    }

    #[tokio::test]
    async fn denied_permission_skips_execution_and_feeds_back() {
        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(SpyTool { executed: executed.clone() });

        let mock = MockOllama::new(vec![
            vec![tool_call("danger", json!({})), done()],
            vec![token("entendido, no lo hago"), done()],
        ]);
        let agent = Agent::new(mock, reg);
        let mut history = vec![crate::ollama::ChatMessage::user("borra todo")];
        let (_log, mut sink) = collect_events();

        let out = agent
            .run_turn("m", &mut history, &DenyAll, &mut sink)
            .await
            .unwrap();

        assert_eq!(out, "entendido, no lo hago");
        assert!(!executed.load(Ordering::SeqCst), "la tool NO debió ejecutarse");
        assert!(
            history.iter().any(|m| m.content.contains("denegó")
                || m.content.contains("denegado")),
            "debe realimentarse la denegación: {history:?}"
        );
    }

    #[tokio::test]
    async fn unknown_tool_is_fed_back_as_error_and_loop_continues() {
        let mock = MockOllama::new(vec![
            vec![tool_call("inexistente", json!({})), done()],
            vec![token("ups"), done()],
        ]);
        let agent = Agent::new(mock, calc_registry());
        let mut history = vec![crate::ollama::ChatMessage::user("x")];
        let (_log, mut sink) = collect_events();

        let out = agent.run_turn("m", &mut history, &AllowAll, &mut sink).await.unwrap();
        assert_eq!(out, "ups");
        assert!(history.iter().any(|m| m.content.contains("inexistente")));
    }

    #[tokio::test]
    async fn stops_after_max_iterations() {
        // El modelo siempre pide una tool, nunca da respuesta final.
        let mock = MockOllama::new(vec![
            vec![tool_call("calc", json!({"expr": "1+1"})), done()],
            vec![tool_call("calc", json!({"expr": "1+1"})), done()],
            vec![tool_call("calc", json!({"expr": "1+1"})), done()],
            vec![tool_call("calc", json!({"expr": "1+1"})), done()],
        ]);
        let agent = Agent::new(mock, calc_registry()).with_max_iterations(2);
        let mut history = vec![crate::ollama::ChatMessage::user("loop")];
        let (_log, mut sink) = collect_events();

        let err = agent.run_turn("m", &mut history, &AllowAll, &mut sink).await;
        assert!(matches!(err, Err(AgentError::MaxIterations(2))));
    }

    #[tokio::test]
    async fn request_includes_tool_specs_and_history() {
        let mock = Arc::new(MockOllama::new(vec![vec![token("ok"), done()]]));
        let agent = Agent::new(mock.clone(), calc_registry());
        let mut history = vec![crate::ollama::ChatMessage::user("hi")];
        let (_log, mut sink) = collect_events();

        agent.run_turn("modelo-x", &mut history, &AllowAll, &mut sink).await.unwrap();

        assert_eq!(mock.calls(), 1);
        let seen = mock.seen.lock().unwrap();
        assert_eq!(seen[0].model, "modelo-x");
        assert!(seen[0].stream);
        assert!(seen[0].tools.iter().any(|t| t.function.name == "calc"));
    }

    // --- Auxiliares de test ---

    struct AllowAll;
    #[async_trait]
    impl PermissionGate for AllowAll {
        async fn request(&self, _tool: &str, _args: &Value) -> bool {
            true
        }
    }

    struct DenyAll;
    #[async_trait]
    impl PermissionGate for DenyAll {
        async fn request(&self, _tool: &str, _args: &Value) -> bool {
            false
        }
    }

    struct SpyTool {
        executed: Arc<AtomicBool>,
    }
    #[async_trait]
    impl Tool for SpyTool {
        fn name(&self) -> &str {
            "danger"
        }
        fn description(&self) -> &str {
            "tool peligrosa de prueba"
        }
        fn parameters(&self) -> Value {
            json!({"type": "object", "properties": {}})
        }
        fn requires_permission(&self) -> bool {
            true
        }
        async fn execute(&self, _args: &Value) -> ToolResult {
            self.executed.store(true, Ordering::SeqCst);
            Ok("hecho".into())
        }
    }
}
