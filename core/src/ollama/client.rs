//! Cliente HTTP de Ollama y el buffer NDJSON para el streaming.

use async_trait::async_trait;
use futures_util::StreamExt;

use super::types::{ChatRequest, ChatStreamChunk, ModelInfo, TagsResponse};

/// Errores del cliente de Ollama.
#[derive(Debug, thiserror::Error)]
pub enum OllamaError {
    #[error("no se pudo conectar con Ollama en {host}: {source}")]
    Connection {
        host: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("Ollama respondió con error {status}: {body}")]
    Status { status: u16, body: String },
    #[error("respuesta de Ollama ilegible: {0}")]
    Decode(String),
}

/// Acumula bytes del stream y emite los chunks NDJSON completos.
///
/// Maneja líneas partidas entre dos paquetes de red: guarda el resto incompleto
/// hasta que llega el `\n` que la cierra. Las líneas en blanco se ignoran y las
/// líneas que no parsean se descartan en silencio (Ollama nunca las emite, pero
/// no queremos tumbar el stream por una línea corrupta).
#[derive(Default)]
pub struct NdjsonBuffer {
    pending: String,
}

impl NdjsonBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Añade un fragmento de bytes y devuelve los chunks ya completos.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<ChatStreamChunk> {
        self.pending.push_str(&String::from_utf8_lossy(bytes));
        let mut out = Vec::new();

        while let Some(newline) = self.pending.find('\n') {
            let line: String = self.pending.drain(..=newline).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(chunk) = serde_json::from_str::<ChatStreamChunk>(line) {
                out.push(chunk);
            }
        }
        out
    }
}

/// Abstracción del backend de Ollama, para poder mockearlo en tests del agente.
#[async_trait]
pub trait OllamaClient: Send + Sync {
    /// Lista los modelos instalados (`GET /api/tags`).
    async fn list_models(&self) -> Result<Vec<ModelInfo>, OllamaError>;

    /// Llama a `POST /api/chat` con streaming, invocando `on_chunk` por cada
    /// chunk NDJSON recibido. Devuelve cuando el stream termina (`done`).
    async fn chat_stream(
        &self,
        request: ChatRequest,
        on_chunk: &mut (dyn FnMut(ChatStreamChunk) + Send),
    ) -> Result<(), OllamaError>;
}

/// Permite usar `Arc<dyn OllamaClient>` (o `Arc<T>`) allí donde se pide un
/// `OllamaClient`: la UI comparte un único cliente entre la UI y el agente.
#[async_trait]
impl<T: OllamaClient + ?Sized> OllamaClient for std::sync::Arc<T> {
    async fn list_models(&self) -> Result<Vec<ModelInfo>, OllamaError> {
        (**self).list_models().await
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
        on_chunk: &mut (dyn FnMut(ChatStreamChunk) + Send),
    ) -> Result<(), OllamaError> {
        (**self).chat_stream(request, on_chunk).await
    }
}

/// Implementación real sobre `reqwest`.
pub struct HttpOllamaClient {
    host: String,
    http: reqwest::Client,
}

impl HttpOllamaClient {
    /// `host` p.ej. `http://localhost:11434` (sin barra final).
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            host: host.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl OllamaClient for HttpOllamaClient {
    async fn list_models(&self) -> Result<Vec<ModelInfo>, OllamaError> {
        let url = format!("{}/api/tags", self.host);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|source| OllamaError::Connection { host: self.host.clone(), source })?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(OllamaError::Status { status, body });
        }

        let tags: TagsResponse = resp
            .json()
            .await
            .map_err(|e| OllamaError::Decode(e.to_string()))?;
        Ok(tags.models)
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
        on_chunk: &mut (dyn FnMut(ChatStreamChunk) + Send),
    ) -> Result<(), OllamaError> {
        let url = format!("{}/api/chat", self.host);
        let resp = self
            .http
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|source| OllamaError::Connection { host: self.host.clone(), source })?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(OllamaError::Status { status, body });
        }

        let mut buffer = NdjsonBuffer::new();
        let mut stream = resp.bytes_stream();
        while let Some(item) = stream.next().await {
            let bytes = item.map_err(|e| OllamaError::Decode(e.to_string()))?;
            for chunk in buffer.push(&bytes) {
                on_chunk(chunk);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_line_yields_one_chunk() {
        let mut buf = NdjsonBuffer::new();
        let chunks = buf.push(b"{\"message\":{\"content\":\"Hola\"},\"done\":false}\n");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].message.as_ref().unwrap().content, "Hola");
    }

    #[test]
    fn two_lines_in_one_push_yield_two_chunks() {
        let mut buf = NdjsonBuffer::new();
        let data = b"{\"message\":{\"content\":\"a\"},\"done\":false}\n{\"message\":{\"content\":\"b\"},\"done\":false}\n";
        let chunks = buf.push(data);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].message.as_ref().unwrap().content, "a");
        assert_eq!(chunks[1].message.as_ref().unwrap().content, "b");
    }

    #[test]
    fn partial_line_is_buffered_until_complete() {
        let mut buf = NdjsonBuffer::new();
        let first = buf.push(b"{\"message\":{\"content\":\"Ho");
        assert_eq!(first.len(), 0, "línea incompleta no debe emitir chunk");
        let second = buf.push(b"la\"},\"done\":false}\n");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].message.as_ref().unwrap().content, "Hola");
    }

    #[test]
    fn blank_lines_are_ignored() {
        let mut buf = NdjsonBuffer::new();
        let chunks = buf.push(b"\n\n{\"done\":true}\n\n");
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].done);
    }
}
