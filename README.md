# kiruklaw

Yet another agent harness for general purpose local automization. Will expand it as I go & add features as I personally need. Agents will NOT receive blanket access to any & all CLI commands, because I'm paranoid.

Specifically, the agent loop is split out into a reusable crate `kiruklaw-agent-loop`, built on `serde`/`serde_json`, `reqwest`, and `tokio`.

In the future, I will likely add a daemon for remote access, and a desktop client built on [gpui](https://gpui.rs/) complementary to the TUI built on [ratatui](https://ratatui.rs/).

## Workspace crates

| Crate | Description |
|---|---|
| `kiruklaw-agent-loop` | Async agent loop: OpenAI-compatible chat completion with SSE streaming, tool dispatch, and multi-step reasoning |
| `kiruklaw-cli` | Terminal UI (TUI) binary that hosts the agent loop as a chat interface |
| `kiruklaw-macros` | Procedural macros (currently the `#[tool]` attribute for defining agent tools) |

## Justfile

This project uses [just](https://just.systems/) for managing common commands for contributors.

## Configuration

The CLI reads model definitions from `~/.config/kiruklaw/config.json` (respects `XDG_CONFIG_HOME`). The file format is:

```json
{
  "models": {
    "My Model": {
      "base_url": "https://api.example.com/v1",
      "model": "model-id",
      "api_key_env": "ENV_VAR_HOLDING_KEY"
    }
  }
}
```

The model key (`My Model` in the example) will be visible wherever models will be selected. It is intended to be a human-readable name for your personal use.

Each model entry requires `base_url` and `model`. Authentication is resolved in order: `api_key_env` (reads the environment variable, falls back to `api_key`), or `api_key` directly. Both `api_key` and `api_key_env` are optional in case no authentication is necessary.

## Key bindings

| Key | Action |
|---|---|
| Enter | Send input |
| Ctrl+C | Quit |
| Ctrl+PgUp / Ctrl+PgDn | Scroll conversation |
| Mouse wheel | Scroll conversation |
| Alt+r | Toggle all reasoning blocks (collapsed by default) |
| End | Jump to bottom and re-enable auto-scroll |

## Slash commands

| Command | Description |
|---|---|
| `/models` | List registered models |
| `/model <name>` | Switch active model |
| `/help` | Show available commands |

## Dependencies

- ratatui 0.30, crossterm 0.29 -- terminal UI
- tokio (rt only) -- async runtime for agent loop thread
- reqwest (agent-loop) -- HTTP client for OpenAI-compatible endpoints
- futures-util -- SSE stream processing
- serde, serde_json -- serialization
- anyhow, thiserror -- error handling

# License

This monorepo uses per-member licensing by path:

**MIT License:**

- `agent-loop/`
- `macros/`

**GPL-3.0 or later License:**

- `cli/`

Files that do not belong to any specific monorepo member, i.e. monorepo root files, are licensed unter MIT License.
