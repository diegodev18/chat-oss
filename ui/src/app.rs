//! La aplicación egui: estado de UI, manejo de eventos y render.

use chatoss_core::storage::Conversation;
use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::bridge::{Command, DisplayMessage, DisplayRole, Engine, UiEvent};

pub struct ChatossApp {
    engine: Engine,
    md_cache: CommonMarkCache,

    models: Vec<String>,
    selected_model: Option<String>,
    conversations: Vec<Conversation>,
    current_conv_id: Option<i64>,
    messages: Vec<DisplayMessage>,

    input: String,
    busy: bool,
    streaming: bool,
    tool_status: Option<String>,
    pending_permission: Option<PendingPermission>,
    error: Option<String>,
    editing_conv: Option<EditingConv>,
    edit_focus_pending: bool,
}

struct PendingPermission {
    tool: String,
    args: String,
}

struct EditingConv {
    id: i64,
    title: String,
}

impl ChatossApp {
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            md_cache: CommonMarkCache::default(),
            models: Vec::new(),
            selected_model: None,
            conversations: Vec::new(),
            current_conv_id: None,
            messages: Vec::new(),
            input: String::new(),
            busy: false,
            streaming: false,
            tool_status: None,
            pending_permission: None,
            error: None,
            editing_conv: None,
            edit_focus_pending: false,
        }
    }

    fn apply_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::Models(models) => {
                if self.selected_model.is_none() {
                    self.selected_model = models.first().cloned();
                }
                self.models = models;
            }
            UiEvent::Conversations(convs) => self.conversations = convs,
            UiEvent::ConversationOpened { id, model, messages } => {
                self.current_conv_id = (id != 0).then_some(id);
                self.selected_model = Some(model);
                // Al primer envío, submit() añade el mensaje del usuario antes de que
                // el motor responda con ConversationOpened (historial vacío). No lo borramos.
                let keep_optimistic = id == 0 && messages.is_empty() && !self.messages.is_empty();
                if !keep_optimistic {
                    self.messages = messages;
                    self.streaming = false;
                    self.tool_status = None;
                }
            }
            UiEvent::ConversationCreated(id) => self.current_conv_id = Some(id),
            UiEvent::Token(t) => {
                self.tool_status = None;
                if !self.streaming {
                    self.messages.push(DisplayMessage {
                        role: DisplayRole::Assistant,
                        text: String::new(),
                    });
                    self.streaming = true;
                }
                if let Some(last) = self.messages.last_mut() {
                    last.text.push_str(&t);
                }
            }
            UiEvent::ToolStatus(s) => self.tool_status = Some(s),
            UiEvent::AssistantDone(text) => {
                if self.streaming {
                    if let Some(last) = self.messages.last_mut() {
                        last.text = text;
                    }
                } else if !text.is_empty() {
                    self.messages.push(DisplayMessage { role: DisplayRole::Assistant, text });
                }
                self.streaming = false;
                self.tool_status = None;
            }
            UiEvent::PermissionRequest { tool, args } => {
                self.pending_permission = Some(PendingPermission { tool, args });
            }
            UiEvent::Error(e) => {
                self.error = Some(e);
                self.busy = false;
                self.streaming = false;
                self.tool_status = None;
            }
            UiEvent::Busy(b) => self.busy = b,
        }
    }

    fn submit(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() || self.busy {
            return;
        }
        if self.current_conv_id.is_none() && self.selected_model.is_none() {
            self.error = Some("Selecciona un modelo antes de enviar.".into());
            return;
        }
        // Asegura una sesión: si no hay conversación abierta, crea una en el motor.
        if self.current_conv_id.is_none() && self.messages.is_empty() {
            if let Some(model) = self.selected_model.clone() {
                self.engine.send(Command::NewConversation { model });
            }
        }
        self.messages.push(DisplayMessage { role: DisplayRole::User, text: text.clone() });
        self.streaming = false;
        self.busy = true;
        self.input.clear();
        self.engine.send(Command::SendMessage(text));
    }

    fn commit_rename(&mut self) {
        let Some(editing) = self.editing_conv.take() else { return };
        let title = editing.title.trim().to_string();
        if title.is_empty() {
            self.error = Some("El título no puede estar vacío.".into());
            self.editing_conv = Some(editing);
            return;
        }
        self.engine.send(Command::RenameConversation { id: editing.id, title });
    }

    fn cancel_rename(&mut self) {
        self.editing_conv = None;
        self.edit_focus_pending = false;
    }

    fn new_chat(&mut self) {
        self.cancel_rename();
        let model = self.selected_model.clone().unwrap_or_default();
        self.current_conv_id = None;
        self.messages.clear();
        self.streaming = false;
        self.tool_status = None;
        self.engine.send(Command::NewConversation { model });
    }
}

impl eframe::App for ChatossApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for event in self.engine.drain() {
            self.apply_event(event);
        }

        self.permission_modal(ctx);
        self.rename_modal(ctx);
        self.sidebar(ctx);
        self.top_bar(ctx);
        self.input_bar(ctx);
        self.chat_panel(ctx);
    }
}

impl ChatossApp {
    fn rename_modal(&mut self, ctx: &egui::Context) {
        enum Action {
            Save,
            Cancel,
        }

        let mut action = None;
        let focus_pending = self.edit_focus_pending;

        if let Some(editing) = self.editing_conv.as_mut() {
            egui::Window::new("Renombrar conversación")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Nombre de la sesión:");
                    ui.add_space(6.0);
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut editing.title)
                            .desired_width(320.0)
                            .hint_text("Título"),
                    );
                    if focus_pending {
                        resp.request_focus();
                    }
                    if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        action = Some(Action::Save);
                    }
                    if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        action = Some(Action::Cancel);
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Cancelar").clicked() {
                            action = Some(Action::Cancel);
                        }
                        if ui
                            .add(egui::Button::new("Guardar").fill(egui::Color32::from_rgb(40, 90, 140)))
                            .clicked()
                        {
                            action = Some(Action::Save);
                        }
                    });
                });
        }

        if focus_pending {
            self.edit_focus_pending = false;
        }

        match action {
            Some(Action::Save) => self.commit_rename(),
            Some(Action::Cancel) => self.cancel_rename(),
            None => {}
        }
    }

    fn permission_modal(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.pending_permission else { return };
        let tool = pending.tool.clone();
        let args = pending.args.clone();
        let mut decision: Option<bool> = None;

        egui::Window::new("Solicitud de permiso")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("El agente quiere ejecutar la herramienta «{tool}»:"));
                ui.add_space(6.0);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.monospace(&args);
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Denegar").clicked() {
                        decision = Some(false);
                    }
                    if ui.add(egui::Button::new("Aprobar").fill(egui::Color32::from_rgb(40, 110, 60))).clicked() {
                        decision = Some(true);
                    }
                });
            });

        if let Some(allowed) = decision {
            self.pending_permission = None;
            self.engine.send(Command::PermissionDecision(allowed));
        }
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(240.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                if ui.add_sized([ui.available_width(), 32.0], egui::Button::new("➕  Nuevo chat")).clicked() {
                    self.new_chat();
                }
                ui.separator();

                let mut to_delete: Option<i64> = None;
                let mut to_open: Option<i64> = None;

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for conv in &self.conversations {
                        let selected = self.current_conv_id == Some(conv.id);
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(selected, truncate(&conv.title, 26))
                                .clicked()
                            {
                                if selected {
                                    self.editing_conv = Some(EditingConv {
                                        id: conv.id,
                                        title: conv.title.clone(),
                                    });
                                    self.edit_focus_pending = true;
                                } else {
                                    to_open = Some(conv.id);
                                }
                            }
                            if ui.small_button("🗑").clicked() {
                                to_delete = Some(conv.id);
                            }
                        });
                    }
                });

                if let Some(id) = to_open {
                    self.engine.send(Command::SelectConversation(id));
                }
                if let Some(id) = to_delete {
                    if self.editing_conv.as_ref().is_some_and(|e| e.id == id) {
                        self.cancel_rename();
                    }
                    self.engine.send(Command::DeleteConversation(id));
                    if self.current_conv_id == Some(id) {
                        self.current_conv_id = None;
                        self.messages.clear();
                    }
                }
            });
    }

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("topbar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("chatoss");
                ui.separator();
                ui.label("Modelo:");
                let current = self.selected_model.clone().unwrap_or_else(|| "—".into());
                egui::ComboBox::from_id_salt("model_selector")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for model in &self.models {
                            let mut sel = self.selected_model.as_deref() == Some(model.as_str());
                            if ui.selectable_value(&mut sel, true, model).clicked() {
                                self.selected_model = Some(model.clone());
                            }
                        }
                    });
                if ui.button("⟳").on_hover_text("Recargar modelos").clicked() {
                    self.engine.send(Command::RefreshModels);
                }
                if self.models.is_empty() {
                    ui.colored_label(egui::Color32::YELLOW, "⚠ Ollama no responde o sin modelos");
                }
            });
            ui.add_space(4.0);
        });
    }

    fn input_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("inputbar").show(ctx, |ui| {
            ui.add_space(6.0);
            if let Some(err) = &self.error {
                ui.colored_label(egui::Color32::LIGHT_RED, format!("⚠ {err}"));
            }
            if let Some(status) = &self.tool_status {
                ui.colored_label(egui::Color32::LIGHT_BLUE, format!("🔧 {status}"));
            } else if self.busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("pensando…");
                });
            }

            ui.horizontal(|ui| {
                let hint = "Escribe un mensaje (Enter envía, Shift+Enter salto de línea)";
                let resp = ui.add_enabled(
                    !self.busy,
                    egui::TextEdit::multiline(&mut self.input)
                        .desired_rows(2)
                        .desired_width(ui.available_width() - 90.0)
                        .hint_text(hint),
                );
                let enter = resp.has_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                let clicked = ui
                    .add_enabled(!self.busy, egui::Button::new("Enviar").min_size([72.0, 0.0].into()))
                    .clicked();
                if enter || clicked {
                    self.submit();
                    ui.memory_mut(|m| m.request_focus(resp.id));
                }
            });
            ui.add_space(6.0);
        });
    }

    fn chat_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.messages.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(
                        egui::RichText::new("Empieza una conversación 👋")
                            .size(18.0)
                            .weak(),
                    );
                });
                return;
            }

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for msg in &self.messages {
                        render_message(ui, &mut self.md_cache, msg);
                        ui.add_space(8.0);
                    }
                });
        });
    }
}

fn render_message(ui: &mut egui::Ui, cache: &mut CommonMarkCache, msg: &DisplayMessage) {
    match msg.role {
        DisplayRole::User => {
            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                egui::Frame::group(ui.style())
                    .fill(egui::Color32::from_rgb(40, 60, 90))
                    .show(ui, |ui| {
                        ui.set_max_width(ui.available_width() * 0.75);
                        ui.label(egui::RichText::new(&msg.text).color(egui::Color32::WHITE));
                    });
            });
        }
        DisplayRole::Assistant => {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_max_width(ui.available_width() * 0.85);
                CommonMarkViewer::new().show(ui, cache, &msg.text);
            });
        }
        DisplayRole::Tool => {
            ui.label(egui::RichText::new(&msg.text).italics().weak());
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}
