//! Persistencia en SQLite de conversaciones y mensajes.

use std::path::Path;

use rusqlite::{params, Connection};

use crate::ollama::{ChatMessage, Role, ToolCall};

/// Errores de la capa de persistencia.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error("error de (de)serialización: {0}")]
    Serde(#[from] serde_json::Error),
}

type Result<T> = std::result::Result<T, StorageError>;

/// Metadatos de una conversación guardada.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversation {
    pub id: i64,
    pub title: String,
    pub model: String,
    pub created_at: String,
}

/// Acceso a la base de datos SQLite.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Abre (o crea) la base de datos en disco.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// Base de datos en memoria (para tests).
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS conversations (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                title      TEXT NOT NULL,
                model      TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS messages (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                role            TEXT NOT NULL,
                content         TEXT NOT NULL,
                tool_calls      TEXT NOT NULL DEFAULT '[]',
                created_at      TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_messages_conv ON messages(conversation_id);
            "#,
        )?;
        Ok(Self { conn })
    }

    /// Crea una conversación y devuelve su id.
    pub fn create_conversation(&self, title: &str, model: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO conversations (title, model) VALUES (?1, ?2)",
            params![title, model],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Lista las conversaciones, de la más reciente a la más antigua.
    pub fn list_conversations(&self) -> Result<Vec<Conversation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, model, created_at FROM conversations ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                model: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Cambia el título de una conversación.
    pub fn rename_conversation(&self, id: i64, title: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE conversations SET title = ?1 WHERE id = ?2",
            params![title, id],
        )?;
        Ok(())
    }

    /// Borra una conversación y sus mensajes (cascade).
    pub fn delete_conversation(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Añade un mensaje a una conversación y devuelve su id.
    pub fn append_message(&self, conversation_id: i64, msg: &ChatMessage) -> Result<i64> {
        let tool_calls = serde_json::to_string(&msg.tool_calls)?;
        self.conn.execute(
            "INSERT INTO messages (conversation_id, role, content, tool_calls)
             VALUES (?1, ?2, ?3, ?4)",
            params![conversation_id, role_str(msg.role), msg.content, tool_calls],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Carga los mensajes de una conversación, en orden cronológico.
    pub fn load_messages(&self, conversation_id: i64) -> Result<Vec<ChatMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT role, content, tool_calls FROM messages
             WHERE conversation_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![conversation_id], |row| {
            let role: String = row.get(0)?;
            let content: String = row.get(1)?;
            let tool_calls: String = row.get(2)?;
            Ok((role, content, tool_calls))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (role, content, tool_calls) = row?;
            out.push(ChatMessage {
                role: role_from_str(&role),
                content,
                tool_calls: serde_json::from_str::<Vec<ToolCall>>(&tool_calls)?,
            });
        }
        Ok(out)
    }
}

fn role_str(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

fn role_from_str(s: &str) -> Role {
    match s {
        "system" => Role::System,
        "user" => Role::User,
        "tool" => Role::Tool,
        _ => Role::Assistant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ollama::{ChatMessage, FunctionCall, Role, ToolCall};
    use serde_json::json;

    fn store() -> Store {
        Store::open_in_memory().unwrap()
    }

    #[test]
    fn create_and_list_conversation() {
        let s = store();
        let id = s.create_conversation("Mi chat", "llama3.1:8b").unwrap();

        let convs = s.list_conversations().unwrap();
        assert_eq!(convs.len(), 1);
        assert_eq!(convs[0].id, id);
        assert_eq!(convs[0].title, "Mi chat");
        assert_eq!(convs[0].model, "llama3.1:8b");
    }

    #[test]
    fn append_and_load_messages_in_order() {
        let s = store();
        let id = s.create_conversation("c", "m").unwrap();
        s.append_message(id, &ChatMessage::user("hola")).unwrap();
        s.append_message(id, &ChatMessage::assistant("qué tal")).unwrap();

        let msgs = s.load_messages(id).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[0].content, "hola");
        assert_eq!(msgs[1].role, Role::Assistant);
        assert_eq!(msgs[1].content, "qué tal");
    }

    #[test]
    fn tool_calls_round_trip() {
        let s = store();
        let id = s.create_conversation("c", "m").unwrap();
        let assistant = ChatMessage {
            role: Role::Assistant,
            content: String::new(),
            tool_calls: vec![ToolCall {
                function: FunctionCall { name: "calc".into(), arguments: json!({"expr": "1+1"}) },
            }],
        };
        s.append_message(id, &assistant).unwrap();

        let loaded = s.load_messages(id).unwrap();
        assert_eq!(loaded[0].tool_calls.len(), 1);
        assert_eq!(loaded[0].tool_calls[0].function.name, "calc");
        assert_eq!(loaded[0].tool_calls[0].function.arguments["expr"], "1+1");
    }

    #[test]
    fn messages_are_isolated_per_conversation() {
        let s = store();
        let a = s.create_conversation("a", "m").unwrap();
        let b = s.create_conversation("b", "m").unwrap();
        s.append_message(a, &ChatMessage::user("en a")).unwrap();

        assert_eq!(s.load_messages(a).unwrap().len(), 1);
        assert_eq!(s.load_messages(b).unwrap().len(), 0);
    }

    #[test]
    fn delete_removes_conversation_and_messages() {
        let s = store();
        let id = s.create_conversation("c", "m").unwrap();
        s.append_message(id, &ChatMessage::user("hola")).unwrap();

        s.delete_conversation(id).unwrap();
        assert!(s.list_conversations().unwrap().is_empty());
        assert!(s.load_messages(id).unwrap().is_empty());
    }

    #[test]
    fn rename_conversation_updates_title() {
        let s = store();
        let id = s.create_conversation("sin título", "m").unwrap();
        s.rename_conversation(id, "Título nuevo").unwrap();
        assert_eq!(s.list_conversations().unwrap()[0].title, "Título nuevo");
    }
}
