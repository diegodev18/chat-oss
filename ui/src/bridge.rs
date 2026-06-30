//! Puente entre la UI (egui, síncrona) y el núcleo async (Ollama + agente).
//!
//! Un hilo dedicado corre un runtime tokio con el "motor". La UI le manda
//! [`Command`] por un canal y recibe [`UiEvent`] por otro, que drena cada frame.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

use async_trait::async_trait;
use nativedesk_core::agent::{Agent, AgentEvent, PermissionGate};
use nativedesk_core::ollama::{ChatMessage, ChatRequest, HttpOllamaClient, OllamaClient, Role};
use nativedesk_core::storage::{Conversation, Store};
use nativedesk_core::tools::{
    CalcTool, DateTimeTool, FsReadTool, FsWriteTool, ShellTool, ToolRegistry,
};
use serde_json::Value;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex as AsyncMutex;

const SYSTEM_PROMPT: &str = "Eres un asistente útil. Cuando necesites hacer cálculos \
aritméticos, conocer la fecha/hora o leer un archivo, usa las herramientas disponibles \
en lugar de inventar la respuesta. Responde en el idioma del usuario.";

const MAX_TITLE_LEN: usize = 40;

const TITLE_SYSTEM_PROMPT: &str = "Tu única tarea es crear un título corto para el historial \
de chat. Resume en tercera persona lo que el USUARIO dijo o preguntó, no lo que tú responderías. \
Ejemplos: \"El usuario preguntó cómo estoy\", \"El usuario pidió ayuda con Rust\". \
NO respondas al mensaje. NO uses primera persona del asistente. Máximo 8 palabras. \
Responde SOLO con el título, sin comillas ni puntuación final.";

/// Mensaje listo para mostrar en la UI.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub role: DisplayRole,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayRole {
    User,
    Assistant,
    Tool,
}

/// Órdenes de la UI hacia el motor.
#[derive(Debug, Clone)]
pub enum Command {
    RefreshModels,
    NewConversation { model: String },
    SelectConversation(i64),
    DeleteConversation(i64),
    RenameConversation { id: i64, title: String },
    SendMessage(String),
    /// Respuesta del usuario a una solicitud de permiso.
    PermissionDecision(bool),
}

/// Eventos del motor hacia la UI.
#[derive(Debug, Clone)]
pub enum UiEvent {
    Models(Vec<String>),
    Conversations(Vec<Conversation>),
    ConversationOpened { id: i64, model: String, messages: Vec<DisplayMessage> },
    /// Id asignado a una conversación recién creada al primer mensaje.
    ConversationCreated(i64),
    /// Fragmento de texto del asistente (streaming).
    Token(String),
    /// Estado efímero de ejecución de una tool.
    ToolStatus(String),
    /// El asistente terminó (texto final completo).
    AssistantDone(String),
    /// Una tool peligrosa pide confirmación.
    PermissionRequest { tool: String, args: String },
    Error(String),
    Busy(bool),
}

/// Asa que la UI conserva para hablar con el motor.
pub struct Engine {
    cmd_tx: UnboundedSender<Command>,
    evt_rx: Receiver<UiEvent>,
}

impl Engine {
    /// Arranca el hilo del motor. `ctx` se usa para despertar a la UI al llegar eventos.
    pub fn start(host: String, db_path: PathBuf, ctx: egui::Context) -> Self {
        let (cmd_tx, cmd_rx) = unbounded_channel::<Command>();
        let (evt_tx, evt_rx) = std::sync::mpsc::channel::<UiEvent>();
        let sink = EventSink {
            tx: evt_tx,
            waker: Arc::new(move || ctx.request_repaint()),
        };

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("no se pudo crear el runtime tokio");
            let client: Arc<dyn OllamaClient> = Arc::new(HttpOllamaClient::new(host));
            let tools = build_registry();
            rt.block_on(worker_main(client, tools, db_path, cmd_rx, sink));
        });

        Engine { cmd_tx, evt_rx }
    }

    /// Envía una orden al motor (no bloquea).
    pub fn send(&self, cmd: Command) {
        let _ = self.cmd_tx.send(cmd);
    }

    /// Devuelve los eventos pendientes sin bloquear.
    pub fn drain(&self) -> Vec<UiEvent> {
        self.evt_rx.try_iter().collect()
    }
}

/// Envoltura del emisor de eventos que además despierta a la UI.
///
/// El "waker" está abstraído (en producción es `ctx.request_repaint()`) para
/// poder testear el motor sin una `egui::Context`.
#[derive(Clone)]
struct EventSink {
    tx: Sender<UiEvent>,
    waker: Arc<dyn Fn() + Send + Sync>,
}

impl EventSink {
    fn send(&self, event: UiEvent) {
        let _ = self.tx.send(event);
        (self.waker)();
    }
}

/// Gate de permisos: emite la solicitud a la UI y espera la decisión.
struct ChannelGate {
    sink: EventSink,
    decision: Arc<AsyncMutex<UnboundedReceiver<bool>>>,
}

#[async_trait]
impl PermissionGate for ChannelGate {
    async fn request(&self, tool_name: &str, args: &Value) -> bool {
        let pretty = serde_json::to_string_pretty(args).unwrap_or_else(|_| args.to_string());
        self.sink.send(UiEvent::PermissionRequest { tool: tool_name.into(), args: pretty });
        self.decision.lock().await.recv().await.unwrap_or(false)
    }
}

/// Estado de la conversación activa en el motor.
struct Session {
    conv_id: Option<i64>,
    model: String,
    history: Vec<ChatMessage>,
    titled: bool,
}

impl Session {
    fn fresh(model: String) -> Self {
        Self {
            conv_id: None,
            model,
            history: vec![ChatMessage::system(SYSTEM_PROMPT)],
            titled: false,
        }
    }
}

fn build_registry() -> ToolRegistry {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut reg = ToolRegistry::new();
    reg.register(CalcTool);
    reg.register(DateTimeTool::default());
    reg.register(FsReadTool::new(&cwd));
    reg.register(FsWriteTool::new(&cwd)); // requiere permiso del usuario
    reg.register(ShellTool::new(&cwd)); // requiere permiso del usuario
    reg
}

/// Convierte el historial persistido en mensajes mostrables (oculta system/tool vacíos).
fn to_display(history: &[ChatMessage]) -> Vec<DisplayMessage> {
    let mut out = Vec::new();
    for m in history {
        match m.role {
            Role::User => out.push(DisplayMessage { role: DisplayRole::User, text: m.content.clone() }),
            Role::Assistant if !m.content.is_empty() => {
                out.push(DisplayMessage { role: DisplayRole::Assistant, text: m.content.clone() })
            }
            Role::Assistant => {
                let names: Vec<_> = m.tool_calls.iter().map(|t| t.function.name.as_str()).collect();
                if !names.is_empty() {
                    out.push(DisplayMessage {
                        role: DisplayRole::Tool,
                        text: format!("🔧 usó: {}", names.join(", ")),
                    });
                }
            }
            Role::Tool | Role::System => {}
        }
    }
    out
}

async fn worker_main(
    client: Arc<dyn OllamaClient>,
    tools: ToolRegistry,
    db_path: PathBuf,
    cmd_rx: UnboundedReceiver<Command>,
    sink: EventSink,
) {
    let store = match Store::open(&db_path) {
        Ok(s) => s,
        Err(e) => {
            sink.send(UiEvent::Error(format!("no se pudo abrir la base de datos: {e}")));
            return;
        }
    };

    // Separa las decisiones de permiso del resto de comandos para no bloquear el loop
    // mientras un turno del agente está esperando la confirmación del usuario.
    let (perm_tx, perm_rx) = unbounded_channel::<bool>();
    let (main_tx, mut main_rx) = unbounded_channel::<Command>();
    let mut cmd_rx = cmd_rx;
    tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                Command::PermissionDecision(b) => {
                    let _ = perm_tx.send(b);
                }
                other => {
                    let _ = main_tx.send(other);
                }
            }
        }
    });

    let gate = ChannelGate { sink: sink.clone(), decision: Arc::new(AsyncMutex::new(perm_rx)) };
    let agent = Agent::new(client.clone(), tools);
    let mut session: Option<Session> = None;

    // Carga inicial.
    emit_models(&client, &sink).await;
    emit_conversations(&store, &sink);

    while let Some(cmd) = main_rx.recv().await {
        match cmd {
            Command::RefreshModels => emit_models(&client, &sink).await,
            Command::NewConversation { model } => {
                session = Some(Session::fresh(model.clone()));
                sink.send(UiEvent::ConversationOpened { id: 0, model, messages: vec![] });
            }
            Command::SelectConversation(id) => {
                match open_conversation(&store, id) {
                    Ok((sess, msgs)) => {
                        let model = sess.model.clone();
                        session = Some(sess);
                        sink.send(UiEvent::ConversationOpened { id, model, messages: msgs });
                    }
                    Err(e) => sink.send(UiEvent::Error(format!("no se pudo abrir la conversación: {e}"))),
                }
            }
            Command::DeleteConversation(id) => {
                if let Err(e) = store.delete_conversation(id) {
                    sink.send(UiEvent::Error(format!("no se pudo borrar: {e}")));
                }
                if session.as_ref().and_then(|s| s.conv_id) == Some(id) {
                    session = None;
                }
                emit_conversations(&store, &sink);
            }
            Command::RenameConversation { id, title } => {
                let title = title.trim();
                if title.is_empty() {
                    sink.send(UiEvent::Error("el título no puede estar vacío".into()));
                } else if let Err(e) = store.rename_conversation(id, title) {
                    sink.send(UiEvent::Error(format!("no se pudo renombrar: {e}")));
                }
                emit_conversations(&store, &sink);
            }
            Command::SendMessage(text) => {
                let Some(sess) = session.as_mut() else { continue };
                handle_send(&client, &agent, &store, &gate, &sink, sess, text).await;
                emit_conversations(&store, &sink);
            }
            Command::PermissionDecision(_) => { /* lo gestiona el dispatcher */ }
        }
    }
}

async fn emit_models(client: &Arc<dyn OllamaClient>, sink: &EventSink) {
    match client.list_models().await {
        Ok(models) => sink.send(UiEvent::Models(models.into_iter().map(|m| m.name).collect())),
        Err(e) => sink.send(UiEvent::Error(format!("no se pudo listar modelos: {e}"))),
    }
}

fn emit_conversations(store: &Store, sink: &EventSink) {
    match store.list_conversations() {
        Ok(convs) => sink.send(UiEvent::Conversations(convs)),
        Err(e) => sink.send(UiEvent::Error(format!("no se pudieron listar conversaciones: {e}"))),
    }
}

fn open_conversation(
    store: &Store,
    id: i64,
) -> Result<(Session, Vec<DisplayMessage>), nativedesk_core::storage::StorageError> {
    let model = store
        .list_conversations()?
        .into_iter()
        .find(|c| c.id == id)
        .map(|c| c.model)
        .unwrap_or_default();
    let persisted = store.load_messages(id)?;
    let display = to_display(&persisted);

    let mut history = vec![ChatMessage::system(SYSTEM_PROMPT)];
    history.extend(persisted);
    let session = Session { conv_id: Some(id), model, history, titled: true };
    Ok((session, display))
}

async fn handle_send(
    client: &Arc<dyn OllamaClient>,
    agent: &Agent<Arc<dyn OllamaClient>>,
    store: &Store,
    gate: &ChannelGate,
    sink: &EventSink,
    sess: &mut Session,
    text: String,
) {
    // Crea la conversación de forma perezosa al primer mensaje.
    if sess.conv_id.is_none() {
        match store.create_conversation("Nuevo chat", &sess.model) {
            Ok(id) => {
                sess.conv_id = Some(id);
                sink.send(UiEvent::ConversationCreated(id));
            }
            Err(e) => {
                sink.send(UiEvent::Error(format!("no se pudo crear la conversación: {e}")));
                return;
            }
        }
    }
    let conv_id = sess.conv_id.expect("acaba de asignarse");

    // Titula la conversación con una sub-llamada al modelo.
    if !sess.titled {
        let title = generate_title(client, &sess.model, &text).await;
        let _ = store.rename_conversation(conv_id, &title);
        sess.titled = true;
    }

    let user_msg = ChatMessage::user(text);
    sess.history.push(user_msg.clone());
    let _ = store.append_message(conv_id, &user_msg);

    let before = sess.history.len();
    sink.send(UiEvent::Busy(true));

    let sink_for_events = sink.clone();
    let mut on_event = move |e: AgentEvent| match e {
        AgentEvent::Token(t) => sink_for_events.send(UiEvent::Token(t)),
        AgentEvent::ToolStarted { name, .. } => {
            sink_for_events.send(UiEvent::ToolStatus(format!("ejecutando {name}…")))
        }
        AgentEvent::ToolFinished { .. } => {}
    };

    let model = sess.model.clone();
    let result = agent.run_turn(&model, &mut sess.history, gate, &mut on_event).await;

    // Persiste todo lo que el turno añadió (asistente + resultados de tools).
    for msg in &sess.history[before..] {
        let _ = store.append_message(conv_id, msg);
    }

    match result {
        Ok(final_text) => sink.send(UiEvent::AssistantDone(final_text)),
        Err(e) => sink.send(UiEvent::Error(e.to_string())),
    }
    sink.send(UiEvent::Busy(false));
}

/// Sub-llamada al modelo para generar un título conciso a partir del primer mensaje.
async fn generate_title(
    client: &Arc<dyn OllamaClient>,
    model: &str,
    user_message: &str,
) -> String {
    let request = ChatRequest {
        model: model.to_string(),
        messages: vec![
            ChatMessage::system(TITLE_SYSTEM_PROMPT),
            ChatMessage::user(format!("Mensaje del usuario:\n{user_message}")),
        ],
        tools: vec![],
        stream: true,
    };

    let mut content = String::new();
    let result = client
        .chat_stream(request, &mut |chunk| {
            if let Some(msg) = chunk.message {
                content.push_str(&msg.content);
            }
        })
        .await;

    match result {
        Ok(()) if !content.trim().is_empty() => sanitize_title(&content),
        _ => fallback_title(user_message),
    }
}

fn sanitize_title(raw: &str) -> String {
    let mut trimmed = raw.trim().replace('\n', " ");
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        trimmed = trimmed[1..trimmed.len() - 1].trim().to_string();
    }
    trimmed = trimmed.trim_end_matches(['.', '!', '?', ':', ';']).trim().to_string();
    if trimmed.is_empty() {
        return fallback_title(raw);
    }
    truncate_title(&trimmed)
}

fn fallback_title(user_message: &str) -> String {
    truncate_title(&user_message.trim().replace('\n', " "))
}

fn truncate_title(text: &str) -> String {
    if text.chars().count() <= MAX_TITLE_LEN {
        text.to_string()
    } else {
        let cut: String = text.chars().take(MAX_TITLE_LEN).collect();
        format!("{cut}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nativedesk_core::ollama::{FunctionCall, ToolCall};
    use serde_json::json;

    #[test]
    fn fallback_title_truncates_long_text() {
        let long = "a".repeat(100);
        let title = fallback_title(&long);
        assert_eq!(title.chars().count(), MAX_TITLE_LEN + 1); // +1 por el '…'
        assert!(title.ends_with('…'));
    }

    #[test]
    fn fallback_title_collapses_newlines() {
        assert_eq!(fallback_title("hola\nmundo"), "hola mundo");
    }

    #[test]
    fn sanitize_title_strips_quotes_and_punctuation() {
        assert_eq!(sanitize_title("\"Mi título.\""), "Mi título");
    }

    #[tokio::test]
    async fn generate_title_uses_model_response() {
        let client: Arc<dyn OllamaClient> = Arc::new(ScriptedClient {
            responses: StdMutex::new(VecDeque::from(vec![vec![
                token_chunk("Consulta matemática"),
                ChatStreamChunk { message: None, done: true },
            ]])),
        });
        let title = generate_title(&client, "m", "¿Cuánto es 23 por 19?").await;
        assert_eq!(title, "Consulta matemática");
    }

    #[tokio::test]
    async fn generate_title_falls_back_on_error() {
        struct FailingClient;
        #[async_trait]
        impl OllamaClient for FailingClient {
            async fn list_models(&self) -> Result<Vec<ModelInfo>, OllamaError> {
                Ok(vec![])
            }
            async fn chat_stream(
                &self,
                _request: ChatRequest,
                _on_chunk: &mut (dyn FnMut(ChatStreamChunk) + Send),
            ) -> Result<(), OllamaError> {
                Err(OllamaError::Decode("fallo".into()))
            }
        }
        let client: Arc<dyn OllamaClient> = Arc::new(FailingClient);
        let title = generate_title(&client, "m", "mensaje largo de prueba").await;
        assert_eq!(title, "mensaje largo de prueba");
    }

    #[test]
    fn to_display_hides_system_and_tool_messages() {
        let history = vec![
            ChatMessage::system("prompt"),
            ChatMessage::user("hola"),
            ChatMessage::assistant("respuesta"),
            ChatMessage::tool_result("437"),
        ];
        let shown = to_display(&history);
        assert_eq!(shown.len(), 2);
        assert_eq!(shown[0].role, DisplayRole::User);
        assert_eq!(shown[1].role, DisplayRole::Assistant);
    }

    #[test]
    fn to_display_shows_tool_use_for_empty_assistant_with_calls() {
        let history = vec![ChatMessage {
            role: Role::Assistant,
            content: String::new(),
            tool_calls: vec![ToolCall {
                function: FunctionCall { name: "calc".into(), arguments: json!({}) },
            }],
        }];
        let shown = to_display(&history);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].role, DisplayRole::Tool);
        assert!(shown[0].text.contains("calc"));
    }

    /// Conduce el motor headless contra un Ollama real: comprueba el streaming
    /// extremo a extremo y que la conversación queda persistida.
    #[test]
    #[ignore = "requiere Ollama real (llama3.1:8b)"]
    fn worker_streams_and_persists_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let (cmd_tx, cmd_rx) = unbounded_channel::<Command>();
        let (evt_tx, evt_rx) = std::sync::mpsc::channel::<UiEvent>();
        let sink = EventSink { tx: evt_tx, waker: Arc::new(|| {}) };

        cmd_tx
            .send(Command::NewConversation { model: "llama3.1:8b".into() })
            .unwrap();
        cmd_tx
            .send(Command::SendMessage(
                "¿Cuánto es 23 por 19? Usa la herramienta calc.".into(),
            ))
            .unwrap();
        drop(cmd_tx); // cierra el loop tras procesar los comandos en cola

        let client: Arc<dyn OllamaClient> = Arc::new(HttpOllamaClient::new("http://localhost:11434"));
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(worker_main(client, build_registry(), db_path.clone(), cmd_rx, sink));

        let events: Vec<UiEvent> = evt_rx.try_iter().collect();
        let streamed: String = events
            .iter()
            .filter_map(|e| match e {
                UiEvent::Token(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        let done = events
            .iter()
            .any(|e| matches!(e, UiEvent::AssistantDone(t) if t.contains("437")));
        assert!(
            streamed.contains("437") || done,
            "debe haber streameado/terminado con 437. eventos: {events:?}"
        );

        // La conversación y sus mensajes deben quedar persistidos.
        let store = Store::open(&db_path).unwrap();
        let convs = store.list_conversations().unwrap();
        assert_eq!(convs.len(), 1, "debe haberse creado una conversación");
        let msgs = store.load_messages(convs[0].id).unwrap();
        assert!(msgs.iter().any(|m| m.role == Role::User));
        assert!(msgs.iter().any(|m| m.content.contains("437")));
    }

    // --- Test determinista del flujo de permisos (sin LLM) ---

    use nativedesk_core::ollama::{
        ChatRequest, ChatStreamChunk, ModelInfo, OllamaError, StreamMessage,
    };
    use std::collections::VecDeque;
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

    /// Cliente de Ollama mockeado con respuestas guionizadas.
    struct ScriptedClient {
        responses: StdMutex<VecDeque<Vec<ChatStreamChunk>>>,
    }

    #[async_trait]
    impl OllamaClient for ScriptedClient {
        async fn list_models(&self) -> Result<Vec<ModelInfo>, OllamaError> {
            Ok(vec![])
        }
        async fn chat_stream(
            &self,
            _request: ChatRequest,
            on_chunk: &mut (dyn FnMut(ChatStreamChunk) + Send),
        ) -> Result<(), OllamaError> {
            let next = self.responses.lock().unwrap().pop_front().unwrap_or_default();
            for chunk in next {
                on_chunk(chunk);
            }
            Ok(())
        }
    }

    fn tool_call_chunk(name: &str, args: Value) -> ChatStreamChunk {
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

    fn token_chunk(s: &str) -> ChatStreamChunk {
        ChatStreamChunk {
            message: Some(StreamMessage { content: s.into(), tool_calls: vec![] }),
            done: false,
        }
    }

    /// Aprobar un permiso para `fs_write` ejecuta la escritura sin bloquear el
    /// loop de comandos, y el turno termina con la respuesta final.
    #[test]
    fn approving_permission_executes_write_without_deadlock() {
        let work = tempfile::tempdir().unwrap();
        let db = tempfile::tempdir().unwrap();
        let target = work.path().join("nota.txt");

        // El modelo pide escribir un archivo y luego, con el resultado, responde.
        let client: Arc<dyn OllamaClient> = Arc::new(ScriptedClient {
            responses: StdMutex::new(VecDeque::from(vec![
                vec![token_chunk("Crear nota"), ChatStreamChunk { message: None, done: true }],
                vec![
                    tool_call_chunk(
                        "fs_write",
                        json!({ "path": "nota.txt", "content": "contenido aprobado" }),
                    ),
                    ChatStreamChunk { message: None, done: true },
                ],
                vec![token_chunk("listo, archivo creado"), ChatStreamChunk { message: None, done: true }],
            ])),
        });
        let mut tools = ToolRegistry::new();
        tools.register(FsWriteTool::new(work.path()));

        let (cmd_tx, cmd_rx) = unbounded_channel::<Command>();
        let (evt_tx, evt_rx) = std::sync::mpsc::channel::<UiEvent>();
        let sink = EventSink { tx: evt_tx, waker: Arc::new(|| {}) };
        let db_path = db.path().join("test.db");

        // El worker corre en su propio hilo con un runtime tokio.
        let worker = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(worker_main(client, tools, db_path, cmd_rx, sink));
        });

        cmd_tx.send(Command::NewConversation { model: "m".into() }).unwrap();
        cmd_tx.send(Command::SendMessage("crea nota.txt".into())).unwrap();

        // Drena eventos hasta ver la solicitud de permiso; la aprueba; espera el fin.
        let mut approved = false;
        let mut done = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            match evt_rx.recv_timeout(Duration::from_millis(200)) {
                Ok(UiEvent::PermissionRequest { tool, .. }) => {
                    assert_eq!(tool, "fs_write");
                    cmd_tx.send(Command::PermissionDecision(true)).unwrap();
                    approved = true;
                }
                Ok(UiEvent::AssistantDone(_)) => {
                    done = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }

        drop(cmd_tx);
        worker.join().unwrap();

        assert!(approved, "debió emitirse una solicitud de permiso");
        assert!(done, "el turno debió completarse (sin deadlock)");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "contenido aprobado");
    }

    /// Denegar el permiso NO ejecuta la escritura, pero el turno termina igual.
    #[test]
    fn denying_permission_skips_write_but_completes() {
        let work = tempfile::tempdir().unwrap();
        let db = tempfile::tempdir().unwrap();
        let target = work.path().join("nota.txt");

        let client: Arc<dyn OllamaClient> = Arc::new(ScriptedClient {
            responses: StdMutex::new(VecDeque::from(vec![
                vec![token_chunk("Crear nota"), ChatStreamChunk { message: None, done: true }],
                vec![
                    tool_call_chunk("fs_write", json!({ "path": "nota.txt", "content": "x" })),
                    ChatStreamChunk { message: None, done: true },
                ],
                vec![token_chunk("entendido, no lo creo"), ChatStreamChunk { message: None, done: true }],
            ])),
        });
        let mut tools = ToolRegistry::new();
        tools.register(FsWriteTool::new(work.path()));

        let (cmd_tx, cmd_rx) = unbounded_channel::<Command>();
        let (evt_tx, evt_rx) = std::sync::mpsc::channel::<UiEvent>();
        let sink = EventSink { tx: evt_tx, waker: Arc::new(|| {}) };
        let db_path = db.path().join("test.db");

        let worker = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(worker_main(client, tools, db_path, cmd_rx, sink));
        });

        cmd_tx.send(Command::NewConversation { model: "m".into() }).unwrap();
        cmd_tx.send(Command::SendMessage("crea nota.txt".into())).unwrap();

        let mut done = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            match evt_rx.recv_timeout(Duration::from_millis(200)) {
                Ok(UiEvent::PermissionRequest { .. }) => {
                    cmd_tx.send(Command::PermissionDecision(false)).unwrap();
                }
                Ok(UiEvent::AssistantDone(_)) => {
                    done = true;
                    break;
                }
                _ => {}
            }
        }

        drop(cmd_tx);
        worker.join().unwrap();

        assert!(done, "el turno debió completarse");
        assert!(!target.exists(), "el archivo NO debió crearse al denegar");
    }
}
