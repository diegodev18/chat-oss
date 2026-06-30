# Roadmap — NativeDesk

Phased delivery plan derived from maintainer intent and current codebase state.
Phases are ordered; items within a phase can be parallelized.

**Legend:** ✅ done · 🚧 in progress · ⬜ planned

---

## Phase 0 — Foundation *(current)*

**Goal:** A usable native chat app with Ollama, agent loop, and basic tools.

| Item | Status | Notes |
|------|:------:|-------|
| Rust + egui desktop shell | ✅ | No webview |
| Ollama client with streaming | ✅ | `OLLAMA_HOST` configurable |
| Agent tool-calling loop | ✅ | Max 10 iterations |
| SQLite conversation persistence | ✅ | `~/.nativedesk/nativedesk.db` |
| Tools: calc, datetime, fs_*, shell | ✅ | Permission gate for dangerous tools |
| Legacy `chatoss` data migration | ✅ | Auto on first launch |
| Rename to NativeDesk (crates/binary) | ✅ | Repo name `chat-oss` unchanged for now |
| Reliable tool calling with default models | 🚧 | `llama3.1:8b` reported broken — needs investigation |
| Project context & roadmap docs | ✅ | This file |

**Exit criteria:** Stable Chat mode; tool calling works with at least one recommended model.

---

## Phase 1 — Chat polish

**Goal:** Make Chat mode dependable and pleasant for daily use.

| Item | Status | Notes |
|------|:------:|-------|
| Config file (TOML) | ⬜ | Replace env-only settings; path TBD |
| Permission: trust this session | ⬜ | Approve once per session |
| Permission: allowlist | ⬜ | Pre-approved tools/commands/paths |
| Permission: auto mode per session | ⬜ | Optional hands-off with guardrails |
| Model/tool calling diagnostics | ⬜ | Better errors when model lacks tool support |
| UI polish | ⬜ | Themes, keyboard shortcuts, export chats |
| Document recommended models | ⬜ | Test matrix for tool-capable models |

**Exit criteria:** Config file shipped; permission model beyond per-call confirm; Chat stable.

**Open decisions:**

- Security hard limits (TBD)
- Data privacy / backup policy (TBD)

---

## Phase 2 — API-key providers

**Goal:** Support remote models alongside local Ollama (BYOK).

| Item | Status | Notes |
|------|:------:|-------|
| Provider abstraction in `core` | ⬜ | Unified chat interface over Ollama + HTTP APIs |
| OpenAI-compatible API backend | ⬜ | OpenRouter, Groq, local proxies, etc. |
| Secure API key storage | ⬜ | Keychain / OS secret store — format TBD |
| Per-conversation provider + model | ⬜ | Extend conversation metadata |
| Provider selection UI | ⬜ | Settings panel |

**Exit criteria:** User can chat via Ollama **or** at least one API-key provider.

---

## Phase 3 — Code mode

**Goal:** Coding assistant surface comparable to Claude Desktop Code.

| Item | Status | Notes |
|------|:------:|-------|
| Code mode UI shell | ⬜ | Distinct layout/workflow from Chat |
| Working directory selection | ⬜ | Global vs per-conversation — **decision pending** |
| Enhanced filesystem tools | ⬜ | Directory listing, search, diff-aware edits |
| Shell improvements | ⬜ | Cwd scoping, output limits, timeout |
| Code-specific system prompts | ⬜ | Per-mode prompt templates |

**Exit criteria:** Developer can use Code mode for file edits and shell tasks with clear scope.

---

## Phase 4 — Cowork mode

**Goal:** Task-oriented workflows with web access.

| Item | Status | Notes |
|------|:------:|-------|
| Cowork mode UI shell | ⬜ | Task/plan oriented UX |
| `web_search` provider integration | ⬜ | SearXNG, Brave, Tavily, etc. — **provider TBD** |
| Multi-step task tracking | ⬜ | Visible plan/progress in UI |
| Document/artifact handling | ⬜ | Attachments, exports |

**Exit criteria:** Cowork mode can search the web and complete multi-step tasks.

---

## Phase 5 — Ecosystem

**Goal:** Extensibility and sync without a web UI.

| Item | Status | Notes |
|------|:------:|-------|
| Plugin API & loader | ⬜ | Local plugins only |
| In-repo plugin marketplace | ⬜ | Curated index; plugins stored on disk |
| Cloud sync | ⬜ | Architecture TBD; opt-in |
| MCP integration (optional) | ⬜ | Evaluate after plugin API exists |

**Exit criteria:** Third-party tools installable locally; sync prototype if pursued.

---

## Phase 6 — Distribution

**Goal:** Easy install for non-Rust users.

| Item | Status | Notes |
|------|:------:|-------|
| GitHub Releases with binaries | ⬜ | Linux, macOS, Windows |
| Homebrew cask / package managers | ⬜ | After stable releases |
| Rename GitHub repo to `nativedesk` | ⬜ | Optional branding step |
| Install/update docs | ⬜ | Replace clone-and-cargo-run as primary path |

**Exit criteria:** Prebuilt binary install documented for at least one platform.

---

## How to pick work

1. Prefer finishing **Phase 0 exit criteria** before starting Phase 1 features.
2. Match PR scope to a single roadmap item when possible.
3. Update this file when an item ships (change status to ✅).
4. Record new open decisions in [`PROJECT_CONTEXT.md`](PROJECT_CONTEXT.md#10-open-questions-log).
