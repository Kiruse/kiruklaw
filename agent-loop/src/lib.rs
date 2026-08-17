pub mod error;
pub mod openai;
pub mod tools;
pub mod types;

use std::{collections::HashMap, sync::mpsc::Sender, time::Instant};

use futures_util::StreamExt;

pub use kiruklaw_macros::tool;

pub use error::Error as AgentLoopError;
pub use types::*;

use error::Error;

/// Run an agent loop with the given config & conversation.
pub async fn run_agent_loop(
  cfg: &AgentLoopConfig,
  conversation: &mut Conversation,
  sender: Sender<AgentMessageChunk>,
) -> Result<AgentLoopResponse, Error> {
  let mut msgs = conversation.as_openai_msgs();

  let tools: HashMap<String, &Box<dyn AgentTool>> = cfg.tools
    .iter()
    .map(|tool| (tool.descriptor().name.clone(), tool))
    .collect();

  let mut last_step = cfg.max_steps;
  let mut total_usage = AgentUsage::default();
  for step in 0..cfg.max_steps {
    let (response, usage) = prompt(
      &cfg.model,
      &cfg.tools
        .iter()
        .map(|tool| tool.descriptor())
        .collect::<Vec<_>>(),
      msgs.clone(),
      sender.clone(),
    ).await?;
    total_usage += usage;

    let tool_calls = response.tool_calls();

    conversation.push(response.clone());
    msgs.push(response.into());

    if tool_calls.is_empty() {
      last_step = step;
      break;
    }

    for tc in tool_calls {
      let response = tools.get(&tc.name)
        .map(|tool| {
          let res = tool.handle(tc.arguments);
          trace!("Agent called tool {} with result: {}", tc.name, res);
          res
        })
        .unwrap_or_else(|| {
          info!("Agent called unknown tool {}", tc.name);
          format!("Error: Unknown tool {}", tc.name)
        });
      let response = ConversationMessage::tool(tc.id, response);
      conversation.push(response.clone());
      msgs.push(response.into());
    }
  }

  Ok(AgentLoopResponse {
    steps: last_step,
    usage: total_usage,
  })
}

/// Request a single prompt from the LLM endpoint. Does not handle
/// or resolve tool calls, and uses MPSC for SSE events.
pub async fn prompt(
  llm: &ModelConfig,
  tools: &[AgentToolDescriptor],
  messages: Vec<OpenAiMessage>,
  sender: Sender<AgentMessageChunk>,
) -> Result<(ConversationMessage, AgentUsage), Error> {
  let ModelConfig::OpenAi { base_url, api_key, model } = llm;
  let start = Instant::now();
  let client = reqwest::Client::new();

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
  };

  let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
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

  let mut stream = response.bytes_stream();
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
        sender.send(AgentMessageChunk::Done {
          reason: FinishReason::Stop,
        }).log_warn(None);
        break 'outer;
      }

      let parsed: OpenAiChatCompletionChunk = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(e) => {
          warn!("Failed to parse completion chunk: {e}");
          continue;
        },
      };

      let choice = &parsed.choices[0];
      let delta = &choice.delta;

      if let Some(content) = &delta.content {
        content_acc.push_str(content);
        sender.send(AgentMessageChunk::Content {
          text: content.clone(),
        }).log_warn(None);
      }

      if let Some(text) = delta.reasoning() {
        if !text.is_empty() {
          sender.send(AgentMessageChunk::Reasoning {
            text: text.clone(),
          }).log_warn(None);
        }
      }

      for tc in &delta.tool_calls {
        if let Some(id) = &tc.id {
          let name = tc.function.name.clone().unwrap_or_default();
          sender.send(AgentMessageChunk::ToolCallStart {
            index: tc.index,
            id: id.clone(),
            name: name.clone(),
          }).log_warn(None);
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
            }).log_warn(None);
          }
        }
      }

      if let Some(reason) = &choice.finish_reason {
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
