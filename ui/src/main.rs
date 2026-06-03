//! chatoss-ui: aplicación de escritorio (egui/eframe) para el agente de chat.

mod app;
mod bridge;

use std::path::PathBuf;

use eframe::egui;

use app::ChatossApp;
use bridge::Engine;

const DEFAULT_HOST: &str = "http://localhost:11434";

fn main() -> eframe::Result<()> {
    let host = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
    let db_path = data_dir().join("chatoss.db");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([700.0, 500.0])
            .with_title("chatoss"),
        ..Default::default()
    };

    eframe::run_native(
        "chatoss",
        options,
        Box::new(move |cc| {
            let engine = Engine::start(host, db_path, cc.egui_ctx.clone());
            Ok(Box::new(ChatossApp::new(engine)))
        }),
    )
}

/// Directorio de datos: `~/.chatoss` (o el directorio actual como respaldo).
fn data_dir() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join(".chatoss");
    let _ = std::fs::create_dir_all(&dir);
    dir
}
