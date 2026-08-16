# Agent Loop

The agent loop is implemented in `kiruklaw-agent-loop`. It sends requests to an OpenAI-compatible chat completion endpoint, processes the SSE response stream, dispatches tool calls, and repeats for up to `max_steps` iterations.

## Entry points

### `run_agent_loop`

```rust
pub async fn run_agent_loop(
    cfg: &AgentLoopConfig,
    conversation: &mut Conversation,
    sender: Sender<AgentMessageChunk>,
) -> Result<AgentLoopResponse, Error>
```

Runs the full multi-step loop. On each step, calls `prompt()` to get an LLM response. If the response contains tool calls, dispatches them (looked up by name from `cfg.tools`) and pushes the results into the conversation before the next step. If no tool calls, the loop terminates. Streams all chunks (content, reasoning, tool call fragments, done) to the caller via the `sender` channel.

### `prompt`

```rust
pub async fn prompt(
    llm: &ModelConfig,
    tools: &[AgentToolDescriptor],
    messages: Vec<OpenAiMessage>,
    sender: Sender<AgentMessageChunk>,
) -> Result<ConversationMessage, Error>
```

Sends a single chat completion request with `stream: true`. Parses the SSE stream, extracting content deltas, reasoning tokens, tool call fragments, and finish reasons. Sends each extracted piece as an `AgentMessageChunk` through the sender. Returns the full assembled `ConversationMessage::Assistant` once the stream ends.

## Types

### `AgentLoopConfig`

```rust
pub struct AgentLoopConfig {
    pub max_steps: u8,
    pub model: ModelConfig,
    pub persona: Option<String>,
    pub tools: Vec<Box<dyn AgentTool>>,
    pub subagents: Vec<AgentLoopConfig>,
}
```

### `ModelConfig`

```rust
pub struct ModelConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}
```

### `Conversation`

```rust
pub struct Conversation {
    pub messages: Vec<ConversationMessage>,
}
```

Serializable. Methods: `as_openai_msgs()`, `push_system()`, `push_prompt()`, `push_response()`, `push_tool_response()`.

### `ConversationMessage`

```rust
pub enum ConversationMessage {
    System { content: String },
    User { content: String },
    Assistant { content: String, tool_calls: Vec<ToolCall> },
    Tool { id: String, content: String },
}
```

### `AgentMessageChunk`

Sent via `mpsc::Sender` during streaming. Variants: `Content { text }`, `Reasoning { text }`, `ToolCallStart { index, id, name }`, `ToolCallArgs { index, args }`, `Done { reason }`.

### `FinishReason`

Variants: `Stop`, `Length`, `ToolCalls`, `ContentFilter`.

### `AgentLoopResponse`

```rust
pub struct AgentLoopResponse {
    pub steps: u8,
}
```

Number of steps actually taken (the index of the final step, not a count).

## Tool dispatch

Tool calls in the LLM response are looked up by name in the `tools` HashMap. If a tool is found, `tool.handle(arguments)` is called and the result is pushed as a `Tool` message. If not found, an error string is returned. Tool calls are executed sequentially within a step.

## Wire protocol

The library speaks the OpenAI chat completion API. The request POSTs to `{base_url}/chat/completions` with JSON body containing `model`, `messages`, `stream: true`, and optionally `tools` (omitted when the tools vec is empty). The response is an SSE stream of `data: {...}` lines terminated by `data: [DONE]`.
