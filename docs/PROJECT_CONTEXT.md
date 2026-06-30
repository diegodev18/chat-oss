# Project Context — NativeDesk

> **Product name:** NativeDesk  
> **Crates:** `nativedesk-core`, `nativedesk-ui` · **Binary:** `nativedesk`  
> **Version:** 0.1.0 · **License:** MIT · **Status:** early development

This document captures the product vision, design intent, and technical context for
contributors and AI agents working on the codebase. It complements the technical
[`README.md`](../README.md) with the *why* behind the project.

See also: [`ROADMAP.md`](ROADMAP.md) for phased delivery plan.

---

## 1. Vision

NativeDesk aims to be a **native desktop alternative to Claude Desktop** for users who
want to chat with **open-source models**, **bring their own API keys**, and stay off
closed, vendor-locked services.

Core principles:

| Principle | Description |
|-----------|-------------|
| **Native first** | Rust + egui/eframe — no webview, no Electron. Optimized for performance and a small footprint. |
| **Local control** | Ollama today; API-key providers planned. Users own their models and data. |
| **Agentic** | Tool calling loop (function calling) so the model can act, not just talk. |
| **Open source** | Personal project, open to community contributions. |

---

## 2. Target audience

Anyone who wants a **desktop chat application** without depending on a closed service:

- Privacy-conscious users running models locally (Ollama).
- Developers who prefer OSS tooling and native performance.
- Users who want to plug in remote APIs with their own keys (planned).

---

## 3. Product surface (planned)

The long-term goal is **feature parity with Claude Desktop** across three modes:

| Mode | Intent | Status |
|------|--------|--------|
| **Chat** | General conversational assistant | **Partial** — core chat + agent loop exists |
| **Cowork** | Collaborative / task-oriented workflows | **Not started** |
| **Code** | Coding assistant with filesystem & shell tools | **Partial** — `fs_*` and `shell` tools exist |

Supporting capabilities tied to Cowork/Code (not yet implemented):

- `web_search` tool (interface exists, **no default provider** — planned for Cowork/Code phase)
- Configurable working directory for `fs_read`, `fs_write`, and `shell` (**decision pending**)

---

## 4. Non-goals

Explicit boundaries to keep scope clear:

| Non-goal | Notes |
|----------|-------|
| **Web UI** | Desktop-native only. No browser-based interface. |

Future goals that are **in scope** but **not near-term**:

- **Cloud sync** — desired eventually.
- **Plugin marketplace** — hosted in this same repo; plugins stored locally on disk.

---

## 5. Architecture (current)

Rust workspace with two crates:

```
┌─────────────────────────────────────────────────────────┐
│  ui (nativedesk-ui)                                     │
│  egui/eframe · markdown rendering · permission dialogs  │
│         │ Command                    UiEvent ▲          │
│         ▼                                  │            │
│  Dedicated thread + tokio runtime ("engine")            │
└─────────────────────────┬───────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────┐
│  core (nativedesk-core) — UI-agnostic, testable         │
│  ┌──────────┐  ┌──────────┐  ┌─────────┐  ┌─────────┐  │
│  │  Agent   │  │  Ollama  │  │  Tools  │  │ SQLite  │  │
│  │  loop    │  │  client  │  │registry │  │  Store  │  │
│  └──────────┘  └──────────┘  └─────────┘  └─────────┘  │
└─────────────────────────────────────────────────────────┘
                          │
                    Ollama HTTP API
              (default http://localhost:11434)
```

### Agent loop

1. User sends a message → appended to conversation history.
2. Agent calls Ollama with tool specs (streaming enabled).
3. If the model requests tools → execute via `ToolRegistry`, respecting `PermissionGate`.
4. Tool results are fed back into history; loop repeats (max **10** iterations).
5. Final text response is streamed to the UI token by token.

### UI ↔ engine bridge

The UI thread is synchronous (egui). A dedicated background thread runs a tokio
runtime and communicates via channels:

- **`Command`** — UI → engine (send message, select conversation, permission decision, …)
- **`UiEvent`** — engine → UI (streaming tokens, tool status, errors, …)

### Persistence

- SQLite database at `~/.nativedesk/nativedesk.db`
- Legacy path `~/.chatoss/chatoss.db` is migrated automatically on first launch
- Tables: `conversations`, `messages` (with serialized tool calls)
- Conversation titles are auto-generated via a separate LLM call

### Tools (shipped)

| Tool | Permission required | Scope |
|------|:-------------------:|-------|
| `calc` | No | Arithmetic expressions |
| `datetime` | No | Current date/time |
| `fs_read` | No | Read files within working directory |
| `fs_write` | **Yes** | Write files within working directory |
| `shell` | **Yes** | Execute shell commands |
| `web_search` | No | Pluggable; **no provider wired yet** |

Path traversal is blocked for filesystem tools.

---

## 6. Backends & configuration

### Today

| Setting | Mechanism | Default |
|---------|-----------|---------|
| Ollama host | `OLLAMA_HOST` env var | `http://localhost:11434` |
| Data directory | Hardcoded | `~/.nativedesk/` |
| Model selection | Per conversation, from Ollama `/api/tags` | User picks in UI |
| Default test model | `NATIVEDESK_MODEL` env var | `llama3.1:8b` |

### Planned

| Feature | Notes |
|---------|-------|
| **Config file** (TOML/YAML) | Replace / extend env-only configuration |
| **API-key providers** | OpenRouter, OpenAI-compatible APIs, etc. — see [Phase 2](ROADMAP.md#phase-2--api-key-providers) |
| **Working directory** | Per-app or per-conversation — **undecided** |

---

## 7. Security & permissions

### Current behavior

Tools marked as requiring permission (`fs_write`, `shell`) prompt the user **on every
invocation** via a UI dialog. The user can allow or deny; denial is reported back to
the model as a tool error.

### Desired evolution

The maintainer wants richer permission models:

- **Trust for this session** — approve once, auto-allow subsequent calls in the same session
- **Allowlist** — pre-approved commands/paths/tools
- **Auto mode per session** — optional hands-off mode with guardrails

Hard security boundaries (e.g. never run shell without any confirmation, encryption at
rest for API keys) are **not yet defined**.

---

## 8. Models & known issues

- Documented example model: `llama3.1:8b` (via Ollama).
- Maintainer reports **tool calling failures** with `llama3.1:8b` — may be due to
  incomplete project state rather than the model itself. Live integration tests exist
  (`cargo test -- --ignored`) for validation against a real Ollama instance.

---

## 9. Development conventions

| Topic | Rule |
|-------|------|
| **Code language** | **English** — identifiers, comments, error messages in source |
| **Contributions** | Welcome — project is open to external contributors |
| **Quality gate** | `make check` → `fmt --check` + `clippy -D warnings` + tests |
| **Rust edition** | 2021, stable 1.85+ |

### Useful commands

```sh
make setup      # verify deps, pull Ollama model
make run        # release build
make dev        # debug + auto-restart on change
make test       # unit/integration (no Ollama)
make test-live  # integration against real Ollama
make reset-db   # wipe ~/.nativedesk/nativedesk.db
```

---

## 10. Open questions log

Track decisions as they get made:

- [ ] Config file format and location (`~/.config/nativedesk/config.toml`?)
- [ ] Working directory: global vs per-conversation?
- [ ] Permission model: session trust, allowlist, auto mode — design & UX
- [ ] API-key provider abstraction and first supported backends
- [ ] `web_search` provider choice (SearXNG, Brave, Tavily, …)
- [ ] Cloud sync architecture
- [ ] Plugin marketplace format and loading mechanism
- [ ] Security & privacy policy (encryption, key storage, data retention)
- [ ] Distribution channels (GitHub Releases, Homebrew, …)
- [ ] Rename GitHub repo from `chat-oss` to `nativedesk` (optional)

---

## 11. For AI agents

When working on this codebase:

1. Read this file first for product intent and boundaries.
2. Read [`README.md`](../README.md) for build/run instructions.
3. Read [`ROADMAP.md`](ROADMAP.md) before picking up feature work.
4. Keep **`core` UI-agnostic** — business logic and tests belong there.
5. UI changes stay in `ui/`; cross-boundary communication uses `Command` / `UiEvent`.
6. New tools implement the registry pattern in `core/src/tools/`.
7. Dangerous tools must go through `PermissionGate`.
8. All new **source code** must be in **English**.
9. Do not add a web UI — desktop-native only.
