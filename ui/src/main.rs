//! nativedesk-ui: native desktop app (egui/eframe) for the chat agent.

mod app;
mod bridge;

use std::path::PathBuf;

use eframe::egui;

use app::NativeDeskApp;
use bridge::Engine;

const DEFAULT_HOST: &str = "http://localhost:11434";
const DATA_DIR_NAME: &str = ".nativedesk";
const DB_FILE_NAME: &str = "nativedesk.db";
const LEGACY_DATA_DIR: &str = ".chatoss";
const LEGACY_DB_FILE: &str = "chatoss.db";

fn main() -> eframe::Result<()> {
    let host = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
    let db_path = db_path();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([700.0, 500.0])
            .with_title("NativeDesk"),
        ..Default::default()
    };

    eframe::run_native(
        "NativeDesk",
        options,
        Box::new(move |cc| {
            let engine = Engine::start(host, db_path, cc.egui_ctx.clone());
            Ok(Box::new(NativeDeskApp::new(engine)))
        }),
    )
}

/// Data directory: `~/.nativedesk` (falls back to the current directory).
fn data_dir() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join(DATA_DIR_NAME);
    if !dir.exists() {
        let legacy_dir = base.join(LEGACY_DATA_DIR);
        if legacy_dir.exists() {
            let _ = std::fs::rename(&legacy_dir, &dir);
        }
    }
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn db_path() -> PathBuf {
    let dir = data_dir();
    let path = dir.join(DB_FILE_NAME);
    if !path.exists() {
        let legacy = dir.join(LEGACY_DB_FILE);
        if legacy.exists() {
            let _ = std::fs::rename(&legacy, &path);
        }
    }
    path
}
