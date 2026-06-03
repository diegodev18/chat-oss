//! chatoss-core: núcleo UI-agnóstico del agente de chat sobre Ollama.
//!
//! Expone el cliente de Ollama, el loop agéntico, las herramientas (tools),
//! la persistencia en SQLite y los tipos de eventos/comandos que conectan la
//! lógica con cualquier capa de presentación.

pub mod agent;
pub mod ollama;
pub mod storage;
pub mod tools;
