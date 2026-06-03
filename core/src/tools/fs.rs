//! Tool `fs_read`: lee archivos dentro de un directorio de trabajo acotado.

use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolResult};

/// Límite de tamaño para evitar inundar el contexto del modelo.
const MAX_BYTES: usize = 100 * 1024;

/// Resuelve `rel` dentro de `root`, rechazando rutas absolutas o que escapen.
///
/// `root` debe existir (se canonicaliza); el destino puede no existir todavía,
/// por lo que se normaliza léxicamente en vez de canonicalizarse.
pub(crate) fn resolve_in_root(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("directorio de trabajo inválido: {e}"))?;

    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err("no se permiten rutas absolutas".into());
    }

    let mut resolved = root.clone();
    for comp in rel_path.components() {
        match comp {
            Component::Normal(part) => resolved.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
                if !resolved.starts_with(&root) {
                    return Err("la ruta sale del directorio de trabajo".into());
                }
            }
            _ => return Err("componente de ruta no permitido".into()),
        }
    }

    if !resolved.starts_with(&root) {
        return Err("la ruta sale del directorio de trabajo".into());
    }
    Ok(resolved)
}

/// Lee archivos de texto dentro de un directorio raíz, impidiendo escapar de él.
pub struct FsReadTool {
    root: PathBuf,
}

impl FsReadTool {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn resolve(&self, rel: &str) -> Result<PathBuf, String> {
        resolve_in_root(&self.root, rel)
    }
}

/// Escribe archivos de texto dentro de un directorio raíz. Requiere permiso.
pub struct FsWriteTool {
    root: PathBuf,
}

impl FsWriteTool {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl Tool for FsWriteTool {
    fn name(&self) -> &str {
        "fs_write"
    }

    fn description(&self) -> &str {
        "Escribe (o sobrescribe) un archivo de texto dentro del directorio de trabajo."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Ruta del archivo relativa al directorio de trabajo."
                },
                "content": {
                    "type": "string",
                    "description": "Contenido completo a escribir en el archivo."
                }
            },
            "required": ["path", "content"]
        })
    }

    fn requires_permission(&self) -> bool {
        true
    }

    async fn execute(&self, args: &Value) -> ToolResult {
        let rel = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "falta el argumento 'path'".to_string())?;
        let content = args
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| "falta el argumento 'content'".to_string())?;

        let path = resolve_in_root(&self.root, rel)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("no se pudo crear el directorio: {e}"))?;
        }
        std::fs::write(&path, content).map_err(|e| format!("no se pudo escribir '{rel}': {e}"))?;
        Ok(format!("escrito {} bytes en {rel}", content.len()))
    }
}

#[async_trait]
impl Tool for FsReadTool {
    fn name(&self) -> &str {
        "fs_read"
    }

    fn description(&self) -> &str {
        "Lee el contenido de un archivo de texto dentro del directorio de trabajo."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Ruta del archivo relativa al directorio de trabajo."
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: &Value) -> ToolResult {
        let rel = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "falta el argumento 'path'".to_string())?;

        let path = self.resolve(rel)?;
        let bytes =
            std::fs::read(&path).map_err(|e| format!("no se pudo leer '{rel}': {e}"))?;
        let truncated = bytes.len() > MAX_BYTES;
        let slice = &bytes[..bytes.len().min(MAX_BYTES)];
        let mut content = String::from_utf8_lossy(slice).into_owned();
        if truncated {
            content.push_str("\n…[truncado]");
        }
        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[tokio::test]
    async fn reads_file_within_root() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("nota.txt"), "hola mundo").unwrap();
        let tool = FsReadTool::new(dir.path());

        let out = tool.execute(&json!({ "path": "nota.txt" })).await.unwrap();
        assert_eq!(out, "hola mundo");
    }

    #[tokio::test]
    async fn reads_file_in_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/a.txt"), "anidado").unwrap();
        let tool = FsReadTool::new(dir.path());

        let out = tool.execute(&json!({ "path": "sub/a.txt" })).await.unwrap();
        assert_eq!(out, "anidado");
    }

    #[tokio::test]
    async fn path_traversal_outside_root_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let tool = FsReadTool::new(dir.path());

        let res = tool.execute(&json!({ "path": "../../etc/passwd" })).await;
        assert!(res.is_err(), "el traversal fuera del root debe rechazarse");
    }

    #[tokio::test]
    async fn missing_file_is_error_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let tool = FsReadTool::new(dir.path());

        assert!(tool.execute(&json!({ "path": "noexiste.txt" })).await.is_err());
    }

    #[tokio::test]
    async fn missing_path_argument_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let tool = FsReadTool::new(dir.path());
        assert!(tool.execute(&json!({})).await.is_err());
    }

    #[test]
    fn spec_and_permission() {
        let dir = tempfile::tempdir().unwrap();
        let tool = FsReadTool::new(dir.path());
        assert_eq!(tool.spec().function.name, "fs_read");
        assert!(!tool.requires_permission());
    }

    #[tokio::test]
    async fn write_creates_file_within_root() {
        let dir = tempfile::tempdir().unwrap();
        let tool = FsWriteTool::new(dir.path());

        tool.execute(&json!({ "path": "out.txt", "content": "hola" }))
            .await
            .unwrap();

        let written = fs::read_to_string(dir.path().join("out.txt")).unwrap();
        assert_eq!(written, "hola");
    }

    #[tokio::test]
    async fn write_creates_missing_subdirectories() {
        let dir = tempfile::tempdir().unwrap();
        let tool = FsWriteTool::new(dir.path());

        tool.execute(&json!({ "path": "a/b/c.txt", "content": "x" }))
            .await
            .unwrap();

        assert!(dir.path().join("a/b/c.txt").exists());
    }

    #[tokio::test]
    async fn write_traversal_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let tool = FsWriteTool::new(dir.path());
        let res = tool
            .execute(&json!({ "path": "../escape.txt", "content": "x" }))
            .await;
        assert!(res.is_err());
        assert!(!dir.path().parent().unwrap().join("escape.txt").exists());
    }

    #[tokio::test]
    async fn write_requires_permission() {
        let dir = tempfile::tempdir().unwrap();
        let tool = FsWriteTool::new(dir.path());
        assert!(tool.requires_permission());
        assert_eq!(tool.spec().function.name, "fs_write");
    }

    #[tokio::test]
    async fn write_missing_args_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let tool = FsWriteTool::new(dir.path());
        assert!(tool.execute(&json!({ "path": "x.txt" })).await.is_err());
        assert!(tool.execute(&json!({ "content": "x" })).await.is_err());
    }
}
