use std::fmt::Display;

pub use log::{debug, trace, info, warn, error};
use serde::Deserialize;
use serde::Serialize;

pub use super::openai::*;
pub use super::tools::*;

#[derive(Debug, Clone, Default)]
pub struct AgentUsage {
  pub input_tokens: u64,
  pub output_tokens: u64,
  pub total_duration_ms: u64,
}

impl std::ops::AddAssign for AgentUsage {
  fn add_assign(&mut self, rhs: Self) {
    self.input_tokens += rhs.input_tokens;
    self.output_tokens += rhs.output_tokens;
    self.total_duration_ms += rhs.total_duration_ms;
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelConfig {
  OpenAi {
    base_url: String,
    api_key: String,
    model: String,
  },
}

impl Default for ModelConfig {
  fn default() -> Self {
    Self::OpenAi {
      base_url: String::new(),
      api_key: String::new(),
      model: String::new(),
    }
  }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Conversation {
  pub messages: Vec<ConversationMessage>,
}

impl Conversation {
  /// Produce an OpenAI spec standard vector of messages.
  /// Used in the [crate::run_agent_loop] function for
  /// optimization.
  pub fn as_openai_msgs(&self) -> Vec<OpenAiMessage> {
    self.messages
      .iter()
      .map(|msg| msg.into())
      .collect()
  }

  /// Push a new [ConversationMessage] to this conversation.
  pub fn push(&mut self, msg: impl Into<ConversationMessage>) {
    self.messages.push(msg.into());
  }

  /// Add the messages from the `other` conversation to this one.
  pub fn extend(&mut self, other: Conversation) {
    self.messages.extend(other.messages);
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationMessage {
  System {
    content: String,
  },
  User {
    content: String,
  },
  Assistant {
    content: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<ToolCall>,
  },
  Tool {
    id: String,
    content: String,
  },
}

impl ConversationMessage {
  pub fn system(content: String) -> Self {
    Self::System { content }
  }

  pub fn user(content: String) -> Self {
    Self::User { content }
  }

  pub fn assistant(content: String, tool_calls: Vec<ToolCall>) -> Self {
    Self::Assistant { content, tool_calls }
  }

  pub fn tool(id: String, content: String) -> Self {
    Self::Tool { id, content }
  }

  /// Get the tool calls of this message, if any.
  /// Only has an effect on [Self::Assistant] instances.
  pub fn tool_calls(&self) -> Vec<ToolCall> {
    match self {
      Self::Assistant { tool_calls, .. } => tool_calls.clone(),
      _ => vec![],
    }
  }
}

impl Into<ConversationMessage> for OpenAiMessage {
  fn into(self) -> ConversationMessage {
    match self.role {
      OpenAiRole::System => {
        ConversationMessage::System { content: self.content }
      }
      OpenAiRole::User => {
        ConversationMessage::User { content: self.content }
      }
      OpenAiRole::Assistant => {
        ConversationMessage::Assistant {
          content: self.content,
          tool_calls: self.tool_calls
            .map(|tcs| tcs.into_iter().map(|tc| tc.into()).collect())
            .unwrap_or_default(),
        }
      }
      OpenAiRole::Tool => {
        ConversationMessage::Tool {
          id: self.tool_call_id.expect("Missing tool call ID on tool message"),
          content: self.content,
        }
      }
    }
  }
}

impl Into<OpenAiMessage> for ConversationMessage {
  fn into(self) -> OpenAiMessage {
    match self {
      ConversationMessage::System { content } => {
        OpenAiMessage {
          role: OpenAiRole::System,
          content,
          ..Default::default()
        }
      }
      ConversationMessage::User { content } => {
        OpenAiMessage {
          role: OpenAiRole::User,
          content,
          ..Default::default()
        }
      }
      ConversationMessage::Assistant { content, tool_calls } => {
        OpenAiMessage {
          role: OpenAiRole::Assistant,
          content,
          tool_calls: if tool_calls.is_empty() {
            None
          } else {
            Some(tool_calls
              .iter()
              .map(|tc| tc.into())
              .collect())
          },
          ..Default::default()
        }
      }
      ConversationMessage::Tool { id, content } => {
        OpenAiMessage {
          role: OpenAiRole::Tool,
          tool_call_id: Some(id),
          content,
          ..Default::default()
        }
      }
    }
  }
}

impl Into<OpenAiMessage> for &ConversationMessage {
  fn into(self) -> OpenAiMessage {
    match self {
      ConversationMessage::System { content } => {
        OpenAiMessage {
          role: OpenAiRole::System,
          content: content.clone(),
          ..Default::default()
        }
      }
      ConversationMessage::User { content } => {
        OpenAiMessage {
          role: OpenAiRole::User,
          content: content.clone(),
          ..Default::default()
        }
      }
      ConversationMessage::Assistant { content, tool_calls } => {
        OpenAiMessage {
          role: OpenAiRole::Assistant,
          content: content.clone(),
          tool_calls: if tool_calls.is_empty() {
            None
          } else {
            Some(tool_calls
              .iter()
              .map(|tc| tc.into())
              .collect())
          },
          ..Default::default()
        }
      }
      ConversationMessage::Tool { id, content } => {
        OpenAiMessage {
          role: OpenAiRole::Tool,
          tool_call_id: Some(id.clone()),
          content: content.clone(),
          ..Default::default()
        }
      }
    }
  }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCall {
  pub id: String,
  pub name: String,
  /// JSON string of arguments (regrettably)
  pub arguments: String,
}

impl Into<OpenAiToolCall> for ToolCall {
  fn into(self) -> OpenAiToolCall {
    OpenAiToolCall {
      id: self.id,
      ty: "function".to_string(),
      function: OpenAiToolCallFunction {
        name: self.name,
        arguments: self.arguments,
      },
    }
  }
}

impl Into<OpenAiToolCall> for &ToolCall {
  fn into(self) -> OpenAiToolCall {
    OpenAiToolCall {
      id: self.id.clone(),
      ty: "function".to_string(),
      function: OpenAiToolCallFunction {
        name: self.name.clone(),
        arguments: self.arguments.clone(),
      },
    }
  }
}

impl Into<ToolCall> for OpenAiToolCall {
  fn into(self) -> ToolCall {
    ToolCall {
      id: self.id,
      name: self.function.name,
      arguments: self.function.arguments,
    }
  }
}

#[derive(Debug, Clone)]
pub enum AgentMessageChunk {
  Content {
    text: String,
  },
  Reasoning {
    text: String,
  },
  ToolCallStart {
    index: usize,
    id: String,
    name: String,
  },
  ToolCallArgs {
    index: usize,
    args: String,
  },
  Done {
    reason: FinishReason,
  },
}

#[derive(Debug, Clone)]
pub enum FinishReason {
  Stop,
  Length,
  ToolCalls,
  ContentFilter,
}

#[allow(unused)]
pub(crate) trait Loggable {
  fn log_debug(&self, msg: Option<&str>);
  fn log_trace(&self, msg: Option<&str>);
  fn log_info(&self, msg: Option<&str>);
  fn log_warn(&self, msg: Option<&str>);
  fn log_error(&self, msg: Option<&str>);
}

impl<E: Display> Loggable for Result<(), E> {
  fn log_debug(&self, msg: Option<&str>) {
    match (self, msg) {
      (Err(e), None) => debug!("{e}"),
      (Err(e), Some(msg)) => debug!("{msg}: {e}"),
      _ => {}
    }
  }

  fn log_trace(&self, msg: Option<&str>) {
    match (self, msg) {
      (Err(e), None) => trace!("{e}"),
      (Err(e), Some(msg)) => trace!("{msg}: {e}"),
      _ => {}
    }
  }

  fn log_info(&self, msg: Option<&str>) {
    match (self, msg) {
      (Err(e), None) => info!("{e}"),
      (Err(e), Some(msg)) => info!("{msg}: {e}"),
      _ => {}
    }
  }

  fn log_warn(&self, msg: Option<&str>) {
    match (self, msg) {
      (Err(e), None) => warn!("{e}"),
      (Err(e), Some(msg)) => warn!("{msg}: {e}"),
      _ => {}
    }
  }

  fn log_error(&self, msg: Option<&str>) {
    match (self, msg) {
      (Err(e), None) => error!("{e}"),
      (Err(e), Some(msg)) => error!("{msg}: {e}"),
      _ => {}
    }
  }
}
