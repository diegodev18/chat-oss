//! Tipos de la API de chat de Ollama (`/api/chat`, `/api/tags`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Rol de un mensaje en la conversación.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Un mensaje de la conversación, tal como lo espera/devuelve Ollama.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    /// Tool calls solicitadas por el asistente (vacío si no hay).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into(), tool_calls: Vec::new() }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into(), tool_calls: Vec::new() }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into(), tool_calls: Vec::new() }
    }

    /// Mensaje con el resultado de ejecutar una tool, para realimentar al modelo.
    pub fn tool_result(content: impl Into<String>) -> Self {
        Self { role: Role::Tool, content: content.into(), tool_calls: Vec::new() }
    }
}

/// Una invocación de herramienta solicitada por el modelo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub function: FunctionCall,
}

/// Nombre + argumentos de una tool. Ollama devuelve `arguments` como objeto JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: Value,
}

/// Especificación de una tool que se envía a Ollama en el campo `tools`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    /// JSON Schema de los parámetros.
    pub parameters: Value,
}

impl ToolSpec {
    pub fn function(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
    ) -> Self {
        Self {
            kind: "function".into(),
            function: ToolFunction {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

/// Cuerpo de la petición a `POST /api/chat`.
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolSpec>,
    pub stream: bool,
}

/// Una línea NDJSON del stream de `/api/chat`.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatStreamChunk {
    #[serde(default)]
    pub message: Option<StreamMessage>,
    #[serde(default)]
    pub done: bool,
}

/// El mensaje parcial dentro de un chunk del stream.
#[derive(Debug, Clone, Deserialize)]
pub struct StreamMessage {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
}

/// Respuesta de `GET /api/tags`.
#[derive(Debug, Clone, Deserialize)]
pub struct TagsResponse {
    pub models: Vec<ModelInfo>,
}

/// Un modelo instalado en Ollama.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelInfo {
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chat_request_serializes_with_tools_and_stream() {
        let req = ChatRequest {
            model: "llama3.1".into(),
            messages: vec![ChatMessage::user("hola")],
            tools: vec![ToolSpec::function(
                "calc",
                "Evalúa una expresión",
                json!({"type": "object", "properties": {"expr": {"type": "string"}}}),
            )],
            stream: true,
        };

        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["model"], "llama3.1");
        assert_eq!(value["stream"], true);
        assert_eq!(value["messages"][0]["role"], "user");
        assert_eq!(value["messages"][0]["content"], "hola");
        assert_eq!(value["tools"][0]["type"], "function");
        assert_eq!(value["tools"][0]["function"]["name"], "calc");
    }

    #[test]
    fn tool_result_message_serializes_as_tool_role() {
        let msg = ChatMessage::tool_result("42");
        let value = serde_json::to_value(&msg).unwrap();
        assert_eq!(value["role"], "tool");
        assert_eq!(value["content"], "42");
    }

    #[test]
    fn stream_chunk_with_content_token_parses() {
        let line = r#"{"model":"llama3.1","message":{"role":"assistant","content":"Hola"},"done":false}"#;
        let chunk: ChatStreamChunk = serde_json::from_str(line).unwrap();
        assert!(!chunk.done);
        let msg = chunk.message.unwrap();
        assert_eq!(msg.content, "Hola");
        assert!(msg.tool_calls.is_empty());
    }

    #[test]
    fn stream_chunk_with_tool_call_parses_arguments_as_object() {
        let line = r#"{"model":"llama3.1","message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"calc","arguments":{"expr":"23*19"}}}]},"done":false}"#;
        let chunk: ChatStreamChunk = serde_json::from_str(line).unwrap();
        let msg = chunk.message.unwrap();
        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0].function.name, "calc");
        assert_eq!(msg.tool_calls[0].function.arguments["expr"], "23*19");
    }

    #[test]
    fn final_done_chunk_parses() {
        let line = r#"{"model":"llama3.1","message":{"role":"assistant","content":""},"done":true}"#;
        let chunk: ChatStreamChunk = serde_json::from_str(line).unwrap();
        assert!(chunk.done);
    }

    #[test]
    fn tags_response_parses_model_names() {
        let body = r#"{"models":[{"name":"llama3.1:latest","model":"llama3.1:latest","size":1234},{"name":"qwen2.5:7b","model":"qwen2.5:7b","size":5678}]}"#;
        let tags: TagsResponse = serde_json::from_str(body).unwrap();
        let names: Vec<_> = tags.models.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["llama3.1:latest", "qwen2.5:7b"]);
    }
}
