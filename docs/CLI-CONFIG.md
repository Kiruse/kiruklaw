# CLI Configuration

Config file location: `~/.config/kiruklaw/config.json`. Respects `XDG_CONFIG_HOME`; if set, uses `$XDG_CONFIG_HOME/kiruklaw/config.json` instead.

## File format

```json
{
  "models": {
    "<name>": {
      "base_url": "<openai-compatible base url>",
      "model": "<model identifier>",
      "api_key": "<literal key, optional>",
      "api_key_env": "<env var name, optional>"
    }
  }
}
```

## Fields

| Field | Type | Required | Description |
|---|---|---|---|
| `models` | `HashMap<String, ModelConfigFile>` | no | Map of named model configurations. If absent or empty, the CLI starts with no models available. |
| `base_url` | `String` | yes | Base URL of the OpenAI-compatible API. `/chat/completions` is appended automatically. Trailing slashes are stripped. |
| `model` | `String` | yes | Model identifier sent in the request body (e.g. `gpt-4o`, `claude-3-5-sonnet`). |
| `api_key` | `Option<String>` | no | Literal API key. Used as fallback if `api_key_env` is set but the variable is not found. |
| `api_key_env` | `Option<String>` | no | Name of an environment variable containing the API key. Takes priority over `api_key` if the variable exists. |

## API key resolution order

1. If `api_key_env` is set and the environment variable exists and is non-empty, use that value.
2. Otherwise, fall back to `api_key` if set.
3. Otherwise, the key is an empty string (unauthenticated request).

## Default behavior

If the config file does not exist, the CLI starts with an empty model set and displays a message telling the user to create the file. The first key in the `models` map is selected as the default model on startup.
