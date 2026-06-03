//! Tool `shell`: ejecuta comandos del sistema. Requiere permiso.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolResult};

/// Tiempo máximo de ejecución antes de abortar el comando.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Ejecuta comandos vía `sh -c` dentro de un directorio de trabajo.
pub struct ShellTool {
    cwd: PathBuf,
}

impl ShellTool {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self { cwd: cwd.into() }
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Ejecuta un comando de shell en el directorio de trabajo y devuelve su salida \
         (stdout + stderr) y el código de salida."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "El comando a ejecutar, p.ej. \"ls -la\"."
                }
            },
            "required": ["command"]
        })
    }

    fn requires_permission(&self) -> bool {
        true
    }

    async fn execute(&self, args: &Value) -> ToolResult {
        let command = args
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| "falta el argumento 'command'".to_string())?;

        let fut = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&self.cwd)
            .output();

        let output = match tokio::time::timeout(TIMEOUT, fut).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return Err(format!("no se pudo ejecutar el comando: {e}")),
            Err(_) => return Err(format!("el comando excedió el tiempo límite ({TIMEOUT:?})")),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().unwrap_or(-1);

        let mut result = String::new();
        if !stdout.is_empty() {
            result.push_str(stdout.trim_end());
        }
        if !stderr.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(stderr.trim_end());
        }
        result.push_str(&format!("\n[exit code: {code}]"));

        if output.status.success() {
            Ok(result)
        } else {
            Err(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell() -> ShellTool {
        ShellTool::new(std::env::temp_dir())
    }

    #[tokio::test]
    async fn runs_command_and_captures_stdout() {
        let out = shell().execute(&json!({ "command": "echo hola" })).await.unwrap();
        assert!(out.contains("hola"), "salida: {out}");
        assert!(out.contains("exit code: 0"));
    }

    #[tokio::test]
    async fn non_zero_exit_is_error_with_code() {
        let res = shell().execute(&json!({ "command": "exit 3" })).await;
        let msg = res.unwrap_err();
        assert!(msg.contains("exit code: 3"), "mensaje: {msg}");
    }

    #[tokio::test]
    async fn captures_stderr() {
        let res = shell()
            .execute(&json!({ "command": "echo problema 1>&2; exit 1" }))
            .await;
        assert!(res.unwrap_err().contains("problema"));
    }

    #[tokio::test]
    async fn runs_in_working_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("marca.txt"), "").unwrap();
        let tool = ShellTool::new(dir.path());
        let out = tool.execute(&json!({ "command": "ls" })).await.unwrap();
        assert!(out.contains("marca.txt"), "salida: {out}");
    }

    #[tokio::test]
    async fn requires_permission_and_has_name() {
        let tool = shell();
        assert!(tool.requires_permission());
        assert_eq!(tool.spec().function.name, "shell");
    }

    #[tokio::test]
    async fn missing_command_argument_is_error() {
        assert!(shell().execute(&json!({})).await.is_err());
    }
}
