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

Core library. Sends chat completion requests to OpenAI-compatible endpoints, processes SSE streams, dispatches tool calls in a loop up to `max_steps`, and streams chunks back to the caller via `std::sync::mpsc::Sender<AgentMessageChunk>`.

Modules:
- `lib.rs` -- `run_agent_loop()` and `prompt()` entry points
- `types.rs` -- `Conversation`, `ConversationMessage`, `AgentLoopConfig`, `AgentMessageChunk`, `FinishReason`
- `openai.rs` -- OpenAI wire types (requests, responses, chunks)
- `tools.rs` -- `AgentTool` trait and `AgentToolDescriptor`
- `error.rs` -- error types

### cli (kiruklaw-cli)

Terminal UI binary. Spawns the agent loop on a background std thread with its own single-threaded tokio runtime. Main thread runs a ratatui render loop polling crossterm events at ~60fps. Communication from agent thread to UI thread via `mpsc` channels (stream chunks + done signal).

Modules:
- `main.rs` -- terminal init/restore (raw mode, alternate screen)
- `app.rs` -- application state, agent loop lifecycle, event dispatch
- `commands.rs` -- `CommandDef`, `COMMANDS`, `CommandMatch`, `CommandResult`, `execute()` dispatch, three-tier completion matching (`is_subword_match`, `is_fuzzy_match`, `compute_completions`)
- `ui.rs` -- ratatui frame rendering (conversation view, status bar, input box, completion popup)
- `event.rs` -- crossterm event polling wrapper
- `models.rs` -- config file loading and `ModelConfigFile` to `ModelConfig` conversion

### macros (kiruklaw-macros)

Proc-macro crate providing the `#[tool]` attribute used by `kiruklaw-agent-loop`.

## Concepts

**Conversation** is the central data structure, a serializable list of `ConversationMessage` variants (System, User, Assistant, Tool). The agent loop consumes and mutates this in-place.

**Streaming** uses `std::sync::mpsc` channels, not tokio channels. The agent loop is async internally but communicates with the sync UI thread via blocking `try_recv` calls in the render loop.

**UI messages** (`UiMessage` in `app.rs`) are a simplified rendering-oriented view over `ConversationMessage`, with additional fields for collapsed reasoning state and pending streaming content.

**Tools** are defined via the `AgentTool` trait and registered through `AgentLoopConfig.tools`. The CLI passes an empty tools vec.

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
