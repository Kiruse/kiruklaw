# AGENTS.md

## Architecture

Rust workspace with three crates under `src/` directories within `agent-loop/`, `cli/`, and `macros/`.

```
kiruklaw/
  agent-loop/    async agent loop library (kiruklaw-agent-loop)
  cli/           TUI binary (kiruklaw-cli, binary name: kiruklaw)
  macros/        proc-macro library (kiruklaw-macros)
  Cargo.toml     workspace root
```

### agent-loop (kiruklaw-agent-loop)

Core library. Sends chat completion requests to OpenAI-compatible endpoints, processes SSE streams, dispatches tool calls in a loop up to `max_steps`, and streams chunks back to the caller via `tokio::sync::mpsc::Sender<AgentMessageChunk>`.

Modules:
- `lib.rs` -- re-exports from `prompt.rs` and `types.rs`.
- `prompt.rs` -- `AgentLoop` struct with `run()` method (the multi-step loop). `AgentLoop.tools` is `HashMap<String, Box<dyn AgentToolMut>>`. `AgentLoop::with_tools()` accepts `impl Iterator<Item = Box<dyn AgentToolMut>>`. Also contains `prompt()` which returns `Result<(ConversationMessage, AgentUsage), Error>`; tracks wall-clock duration via `std::time::Instant` and parses `usage` from SSE chunks. Requests send `stream_options: { include_usage: true }`. `AgentLoop::run()` accumulates `AgentUsage` across steps and includes it in `AgentLoopResponse`.
- `types.rs` -- `Conversation`, `ConversationMessage`, `AgentMessageChunk`, `FinishReason`, `AgentUsage` (token counts + wall-clock duration, implements `Default` and `AddAssign`), `AgentLoopResponse` (contains `steps: u8` and `usage: AgentUsage`), `ModelConfig` (serde-tagged enum, `#[serde(tag = "type", rename_all = "snake_case")]`, single variant `OpenAi { base_url, api_key, model }`, manual `Default` impl returning `OpenAi` with empty strings)
- `openai.rs` -- OpenAI wire types (requests, responses, chunks), including `OpenAiStreamOptions` (`include_usage: bool`), `OpenAiUsage` (prompt/completion/total tokens), and `OpenAiChatCompletionChunk` (optional `usage: Option<OpenAiUsage>` field)
- `tools.rs` -- `AgentTool` trait (readonly, `handle(&self, ...)`), `AgentToolMut` trait (mutable, `handle(&mut self, ...)`), `AgentToolSet` trait (returns `Vec<Box<dyn AgentTool>>`), `AgentToolSetMut` trait (returns `Vec<Box<dyn AgentToolMut>>`), and `AgentToolDescriptor`
- `error.rs` -- error types, including `Message(String)` variant for unsupported model config types

### cli (kiruklaw-cli)

Terminal UI binary. Spawns the agent loop on a background std thread with its own single-threaded tokio runtime. Main thread runs a ratatui render loop polling crossterm events at ~60fps. Communication from agent thread to UI thread via `tokio::sync::mpsc` channels (buffer size 256 for stream, 1 for done).

Modules:
- `main.rs` -- terminal init/restore (raw mode, alternate screen)
- `app.rs` -- application state, agent loop lifecycle, event dispatch
- `commands.rs` -- `CommandDef`, `COMMANDS`, `CommandMatch`, `CommandResult`, `execute()` dispatch, three-tier completion matching (`is_subword_match`, `is_fuzzy_match`, `compute_completions`)
- `ui.rs` -- ratatui frame rendering (conversation view, status bar, input box, completion popup)
- `event.rs` -- crossterm event polling wrapper
- `models.rs` -- config file loading and `ModelConfigFile` to `kiruklaw_agent_loop::ModelConfig::OpenAi { ... }` conversion

### macros (kiruklaw-macros)

Proc-macro crate providing the `#[tool]` and `#[toolset]` attributes used by `kiruklaw-agent-loop`.

Modules:
- `casing.rs` -- `Casing` enum (Camel, Kebab, Pascal, Snake) with `recase(&self, src: &str) -> String`. Uses `split_into_words()` which splits on both `_` boundaries and uppercase transitions (e.g., `HTTPRequest` -> `["HTTP", "Request"]`), correctly handling acronym boundaries where the next character is lowercase. Snake and Kebab variants explicitly lowercase the output. Implements `syn::parse::Parse` for use as a proc-macro attribute (`casing = "snake"`). Also provides `FromStr`.
- `tool.rs` -- `#[tool]` attribute proc macro. Generates `impl AgentToolMut` for the annotated item.
- `toolset.rs` -- `#[toolset]` attribute proc macro. Applied to `impl StructName` blocks. Two modes:
  - Default (mutable): transforms `&self` and `&mut self` methods into `AgentToolMut` implementations. Generates an `impl AgentToolSetMut for StructName` with a `tools() -> Vec<Box<dyn AgentToolMut>>` method.
  - `#[toolset(readonly)]`: transforms only `&self` methods into `AgentTool` implementations (rejects `&mut self` methods at compile time). Generates an `impl AgentToolSet for StructName` with a `tools() -> Vec<Box<dyn AgentTool>>` method.
  Both modes: each tool's descriptor name uses the namespace format `struct_name::tool_name` (struct name in snake_case via `Casing::Snake.recase`, which handles PascalCase/acronym struct names, tool name following the optional casing attribute). Accepts an optional `casing = "snake|camel|kebab|pascal"` attribute (defaults to snake). Each generated wrapper struct holds a clone of the parent struct (requires `StructName: Clone`). Doc comments on methods become tool descriptions; `@arg_name` lines become arg descriptions. Reuses shared helpers from `tool.rs` (`extract_doc_lines`, `parse_arg_descriptions`, `parse_casing_attr`, `get_arg_type`). Does not support generic impl blocks.

## Concepts

**Conversation** is the central data structure, a serializable list of `ConversationMessage` variants (System, User, Assistant, Tool). The agent loop consumes and mutates this in-place.

**Streaming** uses `tokio::sync::mpsc` channels. The agent loop is async internally and communicates with the sync UI thread via `try_recv` calls in the render loop. Because tokio's `Receiver::try_recv()` requires `&mut self` (unlike `std::sync::mpsc`), `process_stream()` in `app.rs` uses a take/put-back pattern: receivers are moved out of app state via `.take()`, used mutably, then returned via replacement fields. The done channel sender uses `.await` in the async path and `.blocking_send()` in sync error paths. Error matching uses `mpsc::error::TryRecvError`.

**UI messages** (`UiMessage` in `app.rs`) are a simplified rendering-oriented view over `ConversationMessage`, with additional fields for collapsed reasoning state and pending streaming content.

**Tools** are defined via two traits: `AgentTool` (readonly, `handle(&self, ...)`) and `AgentToolMut` (mutable, `handle(&mut self, ...)`). Collection traits `AgentToolSet` and `AgentToolSetMut` return `Vec<Box<dyn AgentTool>>` and `Vec<Box<dyn AgentToolMut>>` respectively. The `AgentLoop` stores tools as `HashMap<String, Box<dyn AgentToolMut>>`. The CLI passes an empty tools map.

**Command completion** activates when the input starts with `/`. `compute_completions()` matches against registered `COMMANDS` using three priority tiers (exact, subword at word boundaries, fuzzy subsequence) and returns up to 5 results. The popup is navigated with Up/Down, accepted with Tab/Enter, and dismissed with Esc or when a space is typed.

## Stack

- Language: Rust (edition 2024)
- Terminal UI: ratatui 0.30 + crossterm 0.29
- Async runtime: tokio (single-threaded in CLI background thread, multi-thread in agent-loop)
- HTTP: reqwest with SSE streaming
- Serialization: serde + serde_json

## Licensing

All source code files must include an SPDX license identifier comment at the very first line:

- `cli/` uses `GPL-3.0-or-later`: `// SPDX-License-Identifier: GPL-3.0-or-later`
- All other crates (`agent-loop/`, `macros/`) and workspace-level files use `MIT`: `// SPDX-License-Identifier: MIT`

## Conventions

- 2-space indentation (enforced by `.rustfmt.toml`)
- Workspace dependencies are declared in root `Cargo.toml` under `[workspace.dependencies]`
- Error handling: `anyhow::Result` in the CLI crate, custom `thiserror` types in agent-loop
- Config path: `~/.config/kiruklaw/config.json`, respecting `XDG_CONFIG_HOME`
- No tools are exposed to the agent from the CLI (security model)
