# Agent Loop

The agent loop is implemented in `kiruklaw-agent-loop`. It sends requests to an OpenAI-compatible chat completion endpoint, processes the SSE response stream, dispatches tool calls, and repeats for up to `max_steps` iterations.

## Entry points

### `AgentLoop::run`

```rust
pub async fn run(
    &mut self,
    conversation: &mut Conversation,
    sender: Sender<AgentMessageChunk>,
) -> Result<AgentLoopResponse, Error>
```

Runs the full multi-step loop on the `AgentLoop` struct. On each step, calls `prompt()` to get an LLM response. If the response contains tool calls, dispatches them (looked up by name from `self.tools`) and pushes the results into the conversation before the next step. If no tool calls, the loop terminates. Streams all chunks (content, reasoning, tool call fragments, done) to the caller via the `sender` channel. Accumulates `AgentUsage` across steps and includes it in the returned `AgentLoopResponse`.

Toolsets are stored as `HashMap<String, Toolset>` keyed by toolset name and can be set via `AgentLoop::with_toolsets()` which accepts `impl Iterator<Item = Toolset>`. Descriptors are collected via `ts.tools()` from each toolset. Tool calls are dispatched by splitting `tc.name` on `::` to find the target toolset, then calling `ts.handle(&tc.name, tc.arguments)`.

### `prompt`

```rust
pub async fn prompt(
    llm: &ModelConfig,
    tools: &[AgentToolDescriptor],
    messages: Vec<OpenAiMessage>,
    sender: Sender<AgentMessageChunk>,
) -> Result<(ConversationMessage, AgentUsage), Error>
```

Sends a single chat completion request with `stream: true` and `stream_options: { include_usage: true }`. Parses the SSE stream, extracting content deltas, reasoning tokens, tool call fragments, and finish reasons. Sends each extracted piece as an `AgentMessageChunk` through the sender. Returns the full assembled `ConversationMessage::Assistant` together with parsed `AgentUsage` once the stream ends.

## Types

### `AgentLoop`

```rust
pub struct AgentLoop {
    pub max_steps: u8,
    pub model: ModelConfig,
    pub persona: Option<String>,
    pub tools: HashMap<String, Toolset>,
}
```

### `ModelConfig`

```rust
pub enum ModelConfig {
    OpenAi { base_url: String, api_key: String, model: String },
}
```

Serde-tagged enum (`#[serde(tag = "type", rename_all = "snake_case")]`). Has a manual `Default` impl returning `OpenAi` with empty strings.

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

### `AgentUsage`

Token counts (prompt, completion, total) plus wall-clock duration. Implements `Default` and `AddAssign`.

### `AgentLoopResponse`

```rust
pub struct AgentLoopResponse {
    pub steps: u8,
    pub usage: AgentUsage,
}
```

Number of steps actually taken (the index of the final step, not a count), together with accumulated usage across all steps.

## Toolset traits

Two toolset traits are defined in `tools.rs`:

- **`AgentToolset`** -- readonly trait (`Send + Sync`). Methods: `name(&self) -> &'static str`, `tools(&self) -> Vec<AgentToolDescriptor>`, `handle(&self, tool_name: &str, args: String) -> Pin<Box<dyn Future<Output = String> + Send + '_>>`.
- **`AgentToolsetMut`** -- mutable trait. Same signatures as `AgentToolset` but `handle` takes `&mut self`.

The **`Toolset`** enum unifies both:

```rust
pub enum Toolset {
    Immutable(Box<dyn AgentToolset>),
    Mutable(Box<dyn AgentToolsetMut>),
}
```

Exposes unified `name()`, `tools()`, and `handle(&mut self, ...)` methods.

The `AgentLoop` stores toolsets as `HashMap<String, Toolset>` keyed by toolset name and dispatches via the `Toolset` enum's unified interface.

## Tool dispatch

Tool calls in the LLM response are dispatched by splitting `tc.name` on `::` to locate the target toolset in the `tools` HashMap (the part before `::` is the toolset name). If the toolset is found, `toolset.handle(&tc.name, tc.arguments)` is called and the result is pushed as a `Tool` message. If not found, an error string is returned. Tool calls are executed sequentially within a step.

## Wire protocol

The library speaks the OpenAI chat completion API. The request POSTs to `{base_url}/chat/completions` with JSON body containing `model`, `messages`, `stream: true`, `stream_options: { include_usage: true }`, and optionally `tools` (omitted when the toolsets map is empty). The response is an SSE stream of `data: {...}` lines terminated by `data: [DONE]`.
