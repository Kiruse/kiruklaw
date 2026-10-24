// SPDX-License-Identifier: MIT
use std::collections::HashMap;
use std::iter::Iterator;

use futures_util::StreamExt;

use log::{info, trace, warn};
use tokio::sync::mpsc::Sender;
use tokio::time::Instant;

use crate::{AgentMessageChunk, AgentToolDescriptor, AgentUsage, Conversation, ConversationMessage, FinishReason, Loggable, ModelConfig, OpenAiChatCompletionChunk, OpenAiChatCompletionRequest, OpenAiMessage, OpenAiReasoningConfig, OpenAiRole, OpenAiStreamOptions, ToolCall, Toolset};
use crate::error::Error;

/// Agentic loop configuration.
///
/// [AgentLoop::run] receives a `ctx: C` argument that must be cloneable.
/// Every immutable borrow is cloneable. Mutable borrows are not, and are
/// deliberately not supported in order to enable parallel tool execution
/// in a future update. If you must share state between tools, prefer to
/// use [tokio::sync::RwLock].
///
/// **Note** that [AgentLoop::subagents] is currently an unused placeholder.
pub struct AgentLoop<C: Clone = ()> {
  pub max_steps: u8,
  pub model: ModelConfig,
  pub persona: Option<String>,
  /// Hide tool calls & their results from earlier rounds in the prompt,
  /// so each request only carries the current round's tool traffic.
  /// The full history is still kept in the [Conversation].
  pub hide_tool_history: bool,
  /// Reasoning request configuration, forwarded to the LLM endpoint as the
  /// unified (OpenRouter-style) `reasoning` parameter. `None` disables it.
  pub reasoning: Option<OpenAiReasoningConfig>,
  pub tools: HashMap<String, Toolset<C>>,
  /// Subagents that this agent may invoke. PLACEHOLDER.
  pub subagents: HashMap<String, AgentLoop<C>>,
}

impl<C: Clone> AgentLoop<C> {
  pub fn new(model: ModelConfig) -> Self {
    Self {
      model,
      ..Default::default()
    }
  }

  pub fn with_max_steps(self, max_steps: u8) -> Self {
    Self {
      max_steps,
      ..self
    }
  }

  pub fn with_persona(self, persona: String) -> Self {
    Self {
      persona: Some(persona),
      ..self
    }
  }

  pub fn with_hide_tool_history(self, hide_tool_history: bool) -> Self {
    Self {
      hide_tool_history,
      ..self
    }
  }

  pub fn with_reasoning(self, reasoning: Option<OpenAiReasoningConfig>) -> Self {
    Self {
      reasoning,
      ..self
    }
  }

  pub fn with_toolsets(self, toolsets: impl Iterator<Item = Toolset<C>>) -> Self {
    Self {
      tools: toolsets
        .map(|ts| (ts.name().to_string(), ts))
        .collect(),
      ..self
    }
  }

  pub fn with_subagents(self, subagents: impl Iterator<Item = (String, AgentLoop<C>)>) -> Self {
    Self {
      subagents: subagents.collect(),
      ..self
    }
  }

  pub async fn run(
    &mut self,
    ctx: ContextProvider<'_, C>,
    conversation: &mut Conversation,
    sender: Sender<AgentMessageChunk>,
  ) -> Result<AgentLoopResponse, Error> {
    let msgs = conversation.as_openai_msgs();
    let mut msgs = if self.hide_tool_history {
      msgs.into_iter()
        .filter(|msg| !matches!(msg.role, OpenAiRole::Tool))
        .map(|mut msg| {
          if matches!(msg.role, OpenAiRole::Assistant) {
            msg.tool_calls = None;
          }
          msg
        })
        .collect::<Vec<_>>()
    } else {
      msgs
    };

    let descriptors: Vec<AgentToolDescriptor> = self.tools
      .values()
      .flat_map(|ts| ts.tools())
      .collect();

    let mut last_step = self.max_steps;
    let mut total_usage = AgentUsage::default();
    let mut last_step_usage = AgentUsage::default();
    for step in 0..self.max_steps {
      let (response, usage) = prompt(
        &self.model,
        &descriptors,
        msgs.clone(),
        self.reasoning.clone(),
        sender.clone(),
      ).await?;
      total_usage += usage.clone();
      last_step_usage = usage;

      let tool_calls = response.tool_calls();

      conversation.push(response.clone());
      msgs.push(response.into());

      if tool_calls.is_empty() {
        last_step = step;
        break;
      }

      for tc in tool_calls {
        // Tool names are namespaced `<toolset>::<tool>`; the toolset's own
        // `handle` expects the bare tool name (see the trait contract).
        let (ts_name, bare_name) = match tc.name.split_once("::") {
          Some((ts_name, bare_name)) => (ts_name, bare_name),
          None => (tc.name.as_str(), tc.name.as_str()),
        };
        let response = match self.tools.get_mut(ts_name) {
          Some(ts) => {
            let res = ts.handle(
              ctx.ctx(ts, &tc.name).map_err(|e| Error::External(format!("{e}")))?,
              bare_name,
              tc.arguments,
            ).await;
            trace!("Agent called tool {} with result: {}", tc.name, res);
            res
          }
          None => {
            info!("Agent called unknown tool {}", tc.name);
            format!("Error: Unknown tool {}", tc.name)
          }
        };

        let response = ConversationMessage::tool(tc.id, response);
        conversation.push(response.clone());
        msgs.push(response.into());
      }
    }

    Ok(AgentLoopResponse {
      steps: last_step,
      usage: total_usage,
      last_step: last_step_usage,
    })
  }
}

impl<C: Clone> Default for AgentLoop<C> {
  fn default() -> Self {
    Self {
      max_steps: 20,
      model: Default::default(),
      persona: None,
      hide_tool_history: false,
      reasoning: None,
      tools: Default::default(),
      subagents: Default::default(),
    }
  }
}

#[derive(Debug, Clone)]
pub struct AgentLoopResponse {
  pub steps: u8,
  pub usage: AgentUsage,
  /// Usage of the final LLM call alone. Its `input_tokens` reflect the
  /// full prompt of the last request — i.e. how much of the model's
  /// context window the conversation was actually filling.
  pub last_step: AgentUsage,
}

/// Request a single prompt from the LLM endpoint. Does not handle
/// or resolve tool calls, and uses MPSC for SSE events.
pub async fn prompt(
  llm: &ModelConfig,
  tools: &[AgentToolDescriptor],
  messages: Vec<OpenAiMessage>,
  reasoning: Option<OpenAiReasoningConfig>,
  sender: Sender<AgentMessageChunk>,
) -> Result<(ConversationMessage, AgentUsage), Error> {
  let ModelConfig::OpenAi { base_url, api_key, model } = llm;

  let request = OpenAiChatCompletionRequest {
    model: model.clone(),
    messages,
    stream: true,
    stream_options: Some(OpenAiStreamOptions { include_usage: true }),
    tools: if tools.is_empty() {
      None
    } else {
      Some(tools.iter().map(|t| t.to_schema()).collect())
    },
    reasoning,
  };

  let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

  // Native reqwest streams the body via hyper. On wasm, reqwest's `stream`
  // feature is a no-op (`bytes_stream` does not exist), so the request is
  // issued as a raw browser `fetch` and the body's `ReadableStream` is
  // unwrapped manually — see `fetch_stream`.
  #[cfg(not(target_arch = "wasm32"))]
  let mut stream = {
    let client = reqwest::Client::new();
    let mut req = client.post(&url).json(&request);
    if !api_key.is_empty() {
      req = req.header("Authorization", format!("Bearer {}", api_key));
    }
    let response = req
      .send()
      .await?;

    if !response.status().is_success() {
      return Err(Error::status(response).await);
    }

    response
      .bytes_stream()
      .map(|chunk| chunk.map(|bytes| bytes.to_vec()).map_err(Error::from))
  };
  #[cfg(target_arch = "wasm32")]
  let mut stream = {
    let body = serde_json::to_string(&request)?;
    fetch_stream(&url, api_key, &body).await?
  };
  consume_sse(&mut stream, &sender).await
}

/// Consume an SSE byte stream of OpenAI chat-completion chunks, forwarding
/// content, reasoning, tool-call and finish events to `sender`, and return
/// the accumulated assistant message plus usage.
///
/// Target-independent: the transport (native reqwest vs browser fetch) is
/// resolved by the caller, so this runs — and is tested — on all targets.
async fn consume_sse<S>(
  stream: &mut S,
  sender: &Sender<AgentMessageChunk>,
) -> Result<(ConversationMessage, AgentUsage), Error>
where
  S: futures_util::Stream<Item = Result<Vec<u8>, Error>> + Unpin,
{
  let start = Instant::now();
  let mut buffer = String::new();
  let mut done_sent = false;
  let mut content_acc = String::new();
  let mut tool_calls_acc: Vec<ToolCall> = Vec::new();
  let mut usage = AgentUsage::default();

  // stupid algorithms get written & maintained by AI
  // cus i get a headache if i try to write good code here
  // SSE loop
  'outer: while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    buffer.push_str(&String::from_utf8_lossy(&chunk));

    while let Some(pos) = buffer.find('\n') {
      let line = buffer[..pos].trim_end_matches('\r').to_string();
      buffer = buffer[pos + 1..].to_string();

      if !line.starts_with("data: ") {
        continue;
      }

      let data = &line[6..];
      if data == "[DONE]" {
        if !done_sent {
          sender.send(AgentMessageChunk::Done {
            reason: FinishReason::Stop,
          }).await.log_warn(None);
        }
        break 'outer;
      }

      let parsed: OpenAiChatCompletionChunk = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(e) => {
          warn!("Failed to parse completion chunk: {e}");
          continue;
        },
      };

      // The terminal usage chunk (`include_usage: true`) arrives with an
      // empty `choices` array — only look at a choice if there is one.
      let choice = parsed.choices.first();

      if let Some(delta) = choice.map(|choice| &choice.delta) {
        if let Some(content) = &delta.content {
          content_acc.push_str(content);
          sender.send(AgentMessageChunk::Content {
            text: content.clone(),
          }).await.log_warn(None);
        }

        if let Some(text) = delta.reasoning() {
          if !text.is_empty() {
            sender.send(AgentMessageChunk::Reasoning {
              text: text.clone(),
            }).await.log_warn(None);
          }
        }

        for tc in &delta.tool_calls {
          if let Some(id) = &tc.id {
            let name = tc.function.name.clone().unwrap_or_default();
            sender.send(AgentMessageChunk::ToolCallStart {
              index: tc.index,
              id: id.clone(),
              name: name.clone(),
            }).await.log_warn(None);
            while tool_calls_acc.len() <= tc.index {
              tool_calls_acc.push(Default::default());
            }
            tool_calls_acc[tc.index] = ToolCall {
              id: id.clone(),
              name: name.clone(),
              arguments: "".to_string(),
            };
          }

          if let Some(args) = &tc.function.arguments {
            if tool_calls_acc.len() > tc.index {
              tool_calls_acc[tc.index].arguments.push_str(args);
            }
            if !args.is_empty() {
              sender.send(AgentMessageChunk::ToolCallArgs {
                index: tc.index,
                args: args.clone(),
              }).await.log_warn(None);
            }
          }
        }
      }

      if let Some(reason) = choice.and_then(|choice| choice.finish_reason.as_ref()) {
        if !done_sent {
          done_sent = true;
          let finish = match reason.as_str() {
            "stop" => FinishReason::Stop,
            "length" => FinishReason::Length,
            "tool_calls" => FinishReason::ToolCalls,
            "content_filter" => FinishReason::ContentFilter,
            _ => FinishReason::Stop,
          };
          sender
            .send(AgentMessageChunk::Done { reason: finish })
            .await
            .log_warn(None);
        }
      }

      if let Some(u) = &parsed.usage {
        usage.input_tokens = u.prompt_tokens;
        usage.output_tokens = u.completion_tokens;
      }
    }
  }

  usage.total_duration_ms = start.elapsed().as_millis() as u64;

  Ok((ConversationMessage::assistant(content_acc, tool_calls_acc), usage))
}

/// Provides context to [AgentLoop::run] invokations.
pub enum ContextProvider<'a, C: Clone> {
  Owned(C),
  Factory(Box<dyn Fn(&Toolset<C>, &str) -> Result<C, anyhow::Error> + 'a>),
}

impl<'a, C: Clone> ContextProvider<'a, C> {
  pub fn owned(ctx: C) -> Self {
    Self::Owned(ctx)
  }

  pub fn factory(cb: impl Fn(&Toolset<C>, &str) -> Result<C, anyhow::Error> + 'a) -> Self {
    Self::Factory(Box::new(cb))
  }

  fn ctx(&self, toolset: &Toolset<C>, tool: &str) -> Result<C, anyhow::Error> {
    match self {
      Self::Owned(ctx) => Ok(ctx.clone()),
      Self::Factory(cb) => cb(toolset, tool),
    }
  }
}

/// Issue the chat-completion request as a raw browser `fetch` and expose
/// the response body's `ReadableStream` as a byte-chunk stream. reqwest's
/// `stream` feature does not exist on wasm, so this bypasses it entirely.
#[cfg(target_arch = "wasm32")]
async fn fetch_stream(
  url: &str,
  api_key: &str,
  body: &str,
) -> Result<std::pin::Pin<Box<dyn futures_util::Stream<Item = Result<Vec<u8>, Error>>>>, Error> {
  use futures_util::stream;
  use wasm_bindgen::JsCast;

  let headers = web_sys::Headers::new()
    .map_err(|_| Error::external("failed to create fetch headers"))?;
  headers
    .set("Content-Type", "application/json")
    .map_err(|_| Error::external("failed to set Content-Type"))?;
  if !api_key.is_empty() {
    headers
      .set("Authorization", &format!("Bearer {}", api_key))
      .map_err(|_| Error::external("failed to set Authorization"))?;
  }

  let init = web_sys::RequestInit::new();
  init.set_method("POST");
  init.set_headers(&headers.into());
  init.set_body(&wasm_bindgen::JsValue::from_str(body));

  let request = web_sys::Request::new_with_str_and_init(url, &init)
    .map_err(|_| Error::external("failed to create fetch request"))?;

  let window = web_sys::window()
    .ok_or_else(|| Error::external("no global window object to fetch from"))?;
  let response = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
    .await
    .map_err(|err| Error::external(format!("fetch failed: {err:?}")))?;
  let response: web_sys::Response = response
    .dyn_into()
    .map_err(|_| Error::external("fetch did not resolve to a Response"))?;

  if !response.ok() {
    let status = response.status();
    let text = match response.text() {
      Ok(promise) => wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_else(|| "unknown".to_string()),
      Err(_) => "unknown".to_string(),
    };
    return Err(Error::http(format!("{}: {}", status, text)));
  }

  let body_stream = response
    .body()
    .ok_or_else(|| Error::external("response has no body stream"))?;
  let reader: web_sys::ReadableStreamDefaultReader = body_stream
    .get_reader()
    .dyn_into()
    .map_err(|_| Error::external("failed to acquire the body stream reader"))?;

  Ok(Box::pin(stream::unfold(reader, |reader| async move {
    let chunk = wasm_bindgen_futures::JsFuture::from(reader.read()).await.ok()?;
    let done = js_sys::Reflect::get(&chunk, &wasm_bindgen::JsValue::from_str("done"))
      .ok()?
      .as_bool()?;
    if done {
      return None;
    }
    let value = js_sys::Reflect::get(&chunk, &wasm_bindgen::JsValue::from_str("value")).ok()?;
    let array = value.dyn_into::<js_sys::Uint8Array>().ok()?;
    Some((Ok::<Vec<u8>, Error>(array.to_vec()), reader))
  })))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[cfg(target_arch = "wasm32")]
  use wasm_bindgen_test::wasm_bindgen_test;

  /// Define a test on every target: `#[tokio::test]` on native platforms
  /// and `#[wasm_bindgen_test]` under wasm32 (execute the suite with
  /// `wasm-bindgen-test-runner` against Node or a browser).
  macro_rules! cross_test {
    ($(#[$meta:meta])* $name:ident $body:block) => {
      #[cfg(not(target_arch = "wasm32"))]
      $(#[$meta])*
      #[tokio::test]
      async fn $name() $body

      #[cfg(target_arch = "wasm32")]
      $(#[$meta])*
      #[wasm_bindgen_test]
      async fn $name() $body
    };
  }

  type SseChunk = Result<Vec<u8>, Error>;

  /// Feed raw byte chunks through the target-independent SSE consumer and
  /// collect every event it emitted alongside its final result.
  async fn consume(
    chunks: Vec<SseChunk>,
  ) -> (Vec<AgentMessageChunk>, Result<(ConversationMessage, AgentUsage), Error>) {
    let mut stream = futures_util::stream::iter(chunks);
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let result = consume_sse(&mut stream, &tx).await;
    drop(tx);
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
      events.push(event);
    }
    (events, result)
  }

  /// One SSE completion chunk with the given delta JSON and finish reason.
  fn sse_chunk(delta: &str, finish: Option<&str>) -> String {
    let finish = finish
      .map(|reason| format!("\"{reason}\""))
      .unwrap_or_else(|| "null".to_string());
    format!(
      "data: {{\"choices\":[{{\"index\":0,\"delta\":{delta},\"finish_reason\":{finish}}}]}}\n\n",
    )
  }

  /// The terminal usage-only chunk that `include_usage: true` asks for.
  fn sse_usage_chunk(prompt_tokens: u64, completion_tokens: u64) -> String {
    format!(
      "data: {{\"choices\":[],\"usage\":{{\"prompt_tokens\":{prompt_tokens},\"completion_tokens\":{completion_tokens},\"total_tokens\":{}}}}}\n\n",
      prompt_tokens + completion_tokens,
    )
  }

  fn reason_label(reason: &FinishReason) -> &'static str {
    match reason {
      FinishReason::Stop => "stop",
      FinishReason::Length => "length",
      FinishReason::ToolCalls => "tool_calls",
      FinishReason::ContentFilter => "content_filter",
    }
  }

  /// Render events as comparable labels.
  fn event_labels(events: &[AgentMessageChunk]) -> Vec<String> {
    events
      .iter()
      .map(|event| match event {
        AgentMessageChunk::Content { text } => format!("content:{text}"),
        AgentMessageChunk::Reasoning { text } => format!("reasoning:{text}"),
        AgentMessageChunk::ToolCallStart { index, id, name } => {
          format!("start:{index}:{id}:{name}")
        },
        AgentMessageChunk::ToolCallArgs { index, args } => format!("args:{index}:{args}"),
        AgentMessageChunk::Done { reason } => format!("done:{}", reason_label(reason)),
      })
      .collect()
  }

  cross_test!(
    /// Content, reasoning and tool-call chunks stream through as events and
    /// accumulate onto the assistant message; the terminal usage-only chunk
    /// (empty `choices` — regression: it used to panic on the `choices[0]`
    /// index) is folded into the usage instead.
    sse_streams_content_reasoning_and_usage {
      let chunks = vec![
        Ok(sse_chunk(r#"{"role":"assistant","content":"Hel"}"#, None).into_bytes()),
        Ok(sse_chunk(r#"{"content":"lo"}"#, None).into_bytes()),
        Ok(sse_chunk(r#"{"reasoning_content":"hmm"}"#, None).into_bytes()),
        Ok(sse_usage_chunk(11, 7).into_bytes()),
        Ok(b"data: [DONE]\n\n".to_vec()),
      ];

      let (events, result) = consume(chunks).await;
      let (message, usage) = result.unwrap();
      let ConversationMessage::Assistant { content, tool_calls } = message else {
        panic!("expected an assistant message");
      };

      assert_eq!(
        event_labels(&events),
        ["content:Hel", "content:lo", "reasoning:hmm", "done:stop"],
      );
      assert_eq!(content, "Hello");
      assert!(tool_calls.is_empty());
      assert_eq!(usage.input_tokens, 11);
      assert_eq!(usage.output_tokens, 7);
    }
  );

  cross_test!(
    /// Tool-call deltas accumulate per index across argument fragments;
    /// empty argument strings emit no event.
    sse_accumulates_tool_calls_by_index {
      let chunks = vec![
        Ok(sse_chunk(r#"{"role":"assistant","content":null,"tool_calls":[{"index":0,"id":"t0","type":"function","function":{"name":"probe::record","arguments":"{\"va"}}]}"#, None).into_bytes()),
        Ok(sse_chunk(r#"{"tool_calls":[{"index":1,"id":"t1","type":"function","function":{"name":"other::echo","arguments":""}}]}"#, None).into_bytes()),
        Ok(sse_chunk(r#"{"tool_calls":[{"index":0,"function":{"arguments":"lue\":\"hi\"}"}}]}"#, None).into_bytes()),
        Ok(sse_chunk(r#"{"tool_calls":[{"index":1,"function":{"arguments":"{\"x\":1}"}}]}"#, None).into_bytes()),
        Ok(sse_chunk("{}", Some("tool_calls")).into_bytes()),
        Ok(b"data: [DONE]\n\n".to_vec()),
      ];

      let (events, result) = consume(chunks).await;
      let (message, _) = result.unwrap();
      let ConversationMessage::Assistant { tool_calls, .. } = message else {
        panic!("expected an assistant message");
      };

      assert_eq!(
        event_labels(&events),
        [
          "start:0:t0:probe::record",
          "args:0:{\"va",
          "start:1:t1:other::echo",
          "args:0:lue\":\"hi\"}",
          "args:1:{\"x\":1}",
          "done:tool_calls",
        ],
      );

      assert_eq!(tool_calls.len(), 2);
      assert_eq!(tool_calls[0].id, "t0");
      assert_eq!(tool_calls[0].name, "probe::record");
      assert_eq!(tool_calls[0].arguments, r#"{"value":"hi"}"#);
      assert_eq!(tool_calls[1].id, "t1");
      assert_eq!(tool_calls[1].name, "other::echo");
      assert_eq!(tool_calls[1].arguments, r#"{"x":1}"#);
    }
  );

  cross_test!(
    /// Every documented finish reason maps onto its [FinishReason], unknown
    /// strings fall back to Stop, and a [DONE] sentinel following a finish
    /// reason does not duplicate the Done event.
    sse_maps_finish_reasons {
      for (reason, expected) in [
        ("stop", "stop"),
        ("length", "length"),
        ("tool_calls", "tool_calls"),
        ("content_filter", "content_filter"),
        ("something_new", "stop"),
      ] {
        let chunks = vec![
          Ok(sse_chunk("{}", Some(reason)).into_bytes()),
          Ok(b"data: [DONE]\n\n".to_vec()),
        ];

        let (events, result) = consume(chunks).await;
        result.unwrap();

        assert_eq!(event_labels(&events), [format!("done:{expected}")]);
      }
    }
  );

  cross_test!(
    /// Comment lines, other SSE fields and chunks with invalid JSON are
    /// skipped without killing the stream.
    sse_skips_noise_and_invalid_chunks {
      let chunks = vec![Ok(concat!(
        ": keepalive\n",
        "event: ping\n",
        "garbage\n",
        "data: {not json}\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\"finish_reason\":null}]}\n",
        "data: [DONE]\n",
      ).as_bytes().to_vec())];

      let (events, result) = consume(chunks).await;
      result.unwrap();

      assert_eq!(event_labels(&events), ["content:ok", "done:stop"]);
    }
  );

  cross_test!(
    /// Transport errors mid-stream surface to the caller.
    sse_surfaces_transport_errors {
      let chunks = vec![
        Ok(sse_chunk(r#"{"content":"partial"}"#, None).into_bytes()),
        Err(Error::external("connection reset")),
      ];

      let (events, result) = consume(chunks).await;

      assert_eq!(event_labels(&events), ["content:partial"]);
      assert!(matches!(result, Err(Error::External(msg)) if msg.contains("connection reset")));
    }
  );

  cross_test!(
    /// SSE lines split across multiple byte reads are reassembled.
    sse_reassembles_lines_split_across_reads {
      let body = format!(
        "{}data: [DONE]\n\n",
        sse_chunk(r#"{"content":"hey"}"#, None),
      );
      let bytes = body.into_bytes();
      let (head, rest) = bytes.split_at(24);
      let (middle, tail) = rest.split_at(rest.len() - 5);

      let (events, result) = consume(vec![
        Ok(head.to_vec()),
        Ok(middle.to_vec()),
        Ok(tail.to_vec()),
      ]).await;
      result.unwrap();

      assert_eq!(event_labels(&events), ["content:hey", "done:stop"]);
    }
  );
}

/// Networking-backed tests: a throwaway TCP server stands in for the
/// OpenAI endpoint, exercising the full native reqwest path. tokio's net
/// stack does not exist on wasm.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod network_tests {
  use super::*;
  use crate::{toolset, AgentToolset};
  use std::sync::{Arc, Mutex};

  #[derive(Default)]
  struct Probe {
    recorded: Arc<Mutex<Vec<String>>>,
  }

  #[toolset]
  impl AgentToolset<()> for Probe {
    /// Records the given value.
    /// @value Any value to record.
    async fn record(&self, _ctx: (), value: String) -> Result<String, anyhow::Error> {
      self.recorded.lock().unwrap().push(value);
      Ok("recorded".to_string())
    }
  }

  /// Serve the given SSE bodies, one per incoming connection, in order.
  async fn serve_sse(bodies: Vec<String>) -> String {
    serve_sse_recording(bodies, None).await
  }

  /// Serve the given SSE bodies, one per incoming connection, in order.
  /// When `recorded` is given, each request's JSON body is appended to it.
  async fn serve_sse_recording(
    bodies: Vec<String>,
    recorded: Option<Arc<Mutex<Vec<String>>>>,
  ) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let bodies = Arc::new(Mutex::new(bodies.into_iter()));
    tokio::spawn(async move {
      loop {
        let Some(body) = bodies.lock().unwrap().next() else { break };
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut data: Vec<u8> = Vec::new();
        let mut buf = vec![0u8; 8192];
        let body_start = loop {
          let n = sock.read(&mut buf).await.unwrap();
          assert!(n > 0, "client closed before sending a request");
          data.extend_from_slice(&buf[..n]);
          let Some(head_end) = data.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
          };
          let content_length: usize = String::from_utf8_lossy(&data[..head_end])
            .lines()
            .find_map(|l| {
              l.to_ascii_lowercase()
                .strip_prefix("content-length:")?
                .trim()
                .parse()
                .ok()
            })
            .unwrap_or(0);
          if data.len() >= head_end + 4 + content_length {
            break head_end + 4;
          }
        };
        if let Some(recorded) = &recorded {
          recorded
            .lock()
            .unwrap()
            .push(String::from_utf8_lossy(&data[body_start..]).to_string());
        }
        let response = format!(
          "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
          body.len(),
          body,
        );
        sock.write_all(response.as_bytes()).await.unwrap();
      }
    });
    format!("http://127.0.0.1:{port}/v1")
  }

  /// Regression: `AgentLoop::run` dispatched the full namespaced tool name
  /// (`probe::record`) into the toolset's `handle`, whose arms match bare
  /// tool names only — every tool call came back "Error: unknown tool".
  #[tokio::test]
  async fn run_dispatches_namespaced_tool_calls() {
    let tool_call_body = concat!(
      "data: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null,",
      "\"tool_calls\":[{\"index\":0,\"id\":\"t1\",\"type\":\"function\",\"function\":",
      "{\"name\":\"probe::record\",\"arguments\":\"{\\\"value\\\":\\\"hello\\\"}\"}}]},",
      "\"finish_reason\":null}]}\n\n",
      "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
      "data: [DONE]\n\n",
    );
    let final_body = concat!(
      "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"done\"},\"finish_reason\":null}]}\n\n",
      "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
      "data: [DONE]\n\n",
    );
    let base_url = serve_sse(vec![tool_call_body.to_string(), final_body.to_string()]).await;

    let probe = Probe::default();
    let recorded = probe.recorded.clone();
    let mut agent_loop = AgentLoop::new(ModelConfig::OpenAi {
      base_url,
      api_key: String::new(),
      model: "test".to_string(),
    })
    .with_toolsets([Toolset::Immutable(Box::new(probe))].into_iter());

    let mut conversation = Conversation::default();
    conversation.push(ConversationMessage::user("record hello".to_string()));
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let drainer = tokio::spawn(async move { while rx.recv().await.is_some() {} });

    agent_loop
      .run(ContextProvider::owned(()), &mut conversation, tx)
      .await
      .unwrap();
    drainer.abort();

    assert_eq!(recorded.lock().unwrap().as_slice(), ["hello".to_string()]);
    // user → assistant(tool call) → tool result → assistant(final)
    assert_eq!(conversation.messages.len(), 4);
  }

  /// With `hide_tool_history`, tool calls & results from before the loop
  /// run are stripped from the prompt (assistant history keeps its text),
  /// while all rounds of the current run remain visible.
  #[tokio::test]
  async fn hide_tool_history_strips_pre_run_tool_traffic() {
    let tool_call_body = |id: &str, value: &str| {
      let args = serde_json::json!({ "value": value }).to_string();
      let chunk = serde_json::json!({
        "choices": [{
          "index": 0,
          "delta": {
            "role": "assistant",
            "content": null,
            "tool_calls": [{
              "index": 0,
              "id": id,
              "type": "function",
              "function": { "name": "probe::record", "arguments": args },
            }],
          },
          "finish_reason": null,
        }]
      }).to_string();
      let done = serde_json::json!({
        "choices": [{ "index": 0, "delta": {}, "finish_reason": "tool_calls" }]
      }).to_string();
      format!("data: {chunk}\n\ndata: {done}\n\ndata: [DONE]\n\n")
    };
    let final_body = concat!(
      "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"done\"},\"finish_reason\":null}]}\n\n",
      "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
      "data: [DONE]\n\n",
    );

    let recorded_requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let base_url = serve_sse_recording(
      vec![
        tool_call_body("t1", "one"),
        tool_call_body("t2", "two"),
        final_body.to_string(),
      ],
      Some(recorded_requests.clone()),
    ).await;

    let probe = Probe::default();
    let recorded = probe.recorded.clone();
    let mut agent_loop = AgentLoop::new(ModelConfig::OpenAi {
      base_url,
      api_key: String::new(),
      model: "test".to_string(),
    })
    .with_toolsets([Toolset::Immutable(Box::new(probe))].into_iter())
    .with_hide_tool_history(true);

    // Seed the conversation with a previous round of tool traffic that
    // must never reach the prompt.
    let mut conversation = Conversation::default();
    conversation.push(ConversationMessage::user("earlier question".to_string()));
    conversation.push(ConversationMessage::assistant(
      "I used a tool".to_string(),
      vec![ToolCall {
        id: "t0".to_string(),
        name: "probe::record".to_string(),
        arguments: r#"{"value":"before"}"#.to_string(),
      }],
    ));
    conversation.push(ConversationMessage::tool(
      "t0".to_string(),
      "before-run result".to_string(),
    ));
    conversation.push(ConversationMessage::assistant("all done earlier".to_string(), vec![]));
    conversation.push(ConversationMessage::user("record things".to_string()));

    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let drainer = tokio::spawn(async move { while rx.recv().await.is_some() {} });

    agent_loop
      .run(ContextProvider::owned(()), &mut conversation, tx)
      .await
      .unwrap();
    drainer.abort();

    // All tool calls still execute despite the hidden history.
    assert_eq!(
      recorded.lock().unwrap().as_slice(),
      ["one".to_string(), "two".to_string()],
    );
    // The conversation keeps the full history, including the seeded round.
    // user → asst(t0) → tool(t0) → asst → user → asst(t1) → tool(t1)
    //   → asst(t2) → tool(t2) → assistant(final)
    assert_eq!(conversation.messages.len(), 10);

    let requests = recorded_requests.lock().unwrap();
    let request = |idx: usize| -> String {
      serde_json::from_str::<serde_json::Value>(&requests[idx])
        .unwrap()
        .to_string()
    };

    // Pre-run tool traffic is stripped from every request.
    for (idx, raw) in requests.iter().enumerate() {
      let req = serde_json::from_str::<serde_json::Value>(raw)
        .unwrap()
        .to_string();
      assert!(!req.contains("t0"), "request {idx} leaked pre-run tool call");
      assert!(
        !req.contains("before-run result"),
        "request {idx} leaked pre-run tool result",
      );
    }

    // Round 0: the seeded assistant text history survives the strip.
    let req0 = request(0);
    assert!(req0.contains("earlier question"));
    assert!(req0.contains("I used a tool"));
    assert!(req0.contains("all done earlier"));
    assert!(req0.contains("record things"));

    // Round 1: the run's first tool call & its result appear.
    let req1 = request(1);
    assert!(req1.contains("t1"));
    assert!(req1.contains("one"));
    assert!(!req1.contains("t2"));

    // Round 2: rounds within the run accumulate.
    let req2 = request(2);
    assert!(req2.contains("t1"));
    assert!(req2.contains("one"));
    assert!(req2.contains("t2"));
    assert!(req2.contains("two"));
  }
}

/// Wasm-only tests: drive the browser-fetch transport directly. Run with
/// `wasm-bindgen-test-runner` (Node or a browser).
#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
  use super::*;
  use wasm_bindgen_test::wasm_bindgen_test;

  /// The wasm cfg path issues the request through browser `fetch`; an
  /// unreachable endpoint must surface as a transport error, not panic.
  #[wasm_bindgen_test]
  async fn fetch_stream_errors_on_unreachable_endpoint() {
    let result = fetch_stream("http://127.0.0.1:1/v1/chat/completions", "", "{}").await;
    assert!(result.is_err());
  }
}
