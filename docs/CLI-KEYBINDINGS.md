# CLI Key Bindings and Commands

## Key bindings

| Input | Context | Action |
|---|---|---|
| Enter | idle | Submit input text (trailing whitespace trimmed). If empty, no-op. |
| Ctrl+C | any | Quit the application. |
| Alt+r | any | Toggle all reasoning blocks between collapsed and expanded. Default is collapsed. Operates on all messages with non-empty reasoning content. |
| Ctrl+PgUp | idle or generating | Scroll conversation up by 5 lines. Disables auto-scroll. |
| Ctrl+PgDn | idle or generating | Scroll conversation down by 5 lines. |
| PgUp | idle | Scroll up by 5 lines. Disables auto-scroll. |
| PgDn | idle | Scroll down by 5 lines. |
| Mouse wheel up | any | Scroll up by 3 lines. Disables auto-scroll. |
| Mouse wheel down | any | Scroll down by 3 lines. |
| End | idle | Move cursor to end of input and re-enable auto-scroll. |
| Backspace | idle | Delete character before cursor. |
| Delete | idle | Delete character at cursor. |
| Left / Right | idle | Move cursor within input. |
| Home | idle | Move cursor to start of input. |
| Char input | idle | Insert character at cursor position. |

During generation, only Ctrl+C (quit), Ctrl+PgUp/PgDn, mouse wheel, and Alt+r are processed. All other key input is ignored.

## Slash commands

Commands are entered in the input box prefixed with `/`.

| Command | Args | Description |
|---|---|---|
| `/models` | none | Lists all registered model names with their model ID and base URL. Marks the active model with `*`. If no models configured, shows a template for creating the config file. |
| `/model` | `<name>` | Switches the active model to the given name. Prints an error if the name is not found, listing available models. Prints usage if no argument given. |
| `/help` | none | Displays all available commands and key bindings. |

Unknown commands print `Unknown command: /<name>`.

## Auto-scroll

Auto-scroll is enabled by default, keeping the conversation view pinned to the bottom as new content arrives. It is disabled when the user scrolls up manually (PgUp, Ctrl+PgUp, mouse wheel up). It is re-enabled when the user presses End.
