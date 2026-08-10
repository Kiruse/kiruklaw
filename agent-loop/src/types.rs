use std::fmt::Display;

pub use log::{debug, trace, info, warn, error};
use serde::Deserialize;
use serde::Serialize;

pub use super::openai::*;
pub use super::tools::*;

pub struct AgentLoopConfig {
  pub max_steps: u8,
  pub model: ModelConfig,
  pub persona: Option<String>,
  pub tools: Vec<Box<dyn AgentTool>>,
  /// Subagents that this agent may invoke. PLACEHOLDER.
  pub subagents: Vec<AgentLoopConfig>,
}

impl Default for AgentLoopConfig {
  fn default() -> Self {
    Self {
      max_steps: 20,
      model: Default::default(),
      persona: None,
      tools: vec![],
      subagents: vec![],
    }
  }
}

#[derive(Debug, Clone)]
pub struct AgentLoopResponse {
  pub steps: u8,
}

#[derive(Debug, Clone, Default)]
pub struct ModelConfig {
  pub base_url: String,
  pub api_key: String,
  pub model: String,
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

  /// Push a system prompt to this conversation.
  pub fn push_system(&mut self, content: String) {
    self.messages.push(ConversationMessage::System { content });
  }

  /// Push a user prompt to this conversation.
  pub fn push_prompt(&mut self, content: String) {
    self.messages.push(ConversationMessage::User { content });
  }

  /// Push an agent response to this conversation.
  pub fn push_response(
    &mut self,
    content: String,
    tool_calls: Vec<(String, String, String)>,
    msgs_cache: &mut Vec<OpenAiMessage>,
  ) {
    let tool_calls: Vec<ToolCall> = tool_calls
      .into_iter()
      .map(|(id, name, args)| ToolCall {
        id,
        name,
        arguments: args,
      })
      .collect();

    let tool_calls_openai = if tool_calls.is_empty() {
      None
    } else {
      Some(
        tool_calls
          .iter()
          .map(|tc| tc.into())
          .collect(),
      )
    };

    self.messages.push(ConversationMessage::Assistant {
      content: content.clone(),
      tool_calls: tool_calls,
    });

    msgs_cache.push(OpenAiMessage {
      role: OpenAiRole::Assistant,
      content,
      tool_calls: tool_calls_openai,
      tool_call_id: None,
    });
  }

  /// Push a tool response to this conversation.
  pub fn push_tool_response(
    &mut self,
    id: String,
    content: String,
  ) {
    self.messages.push(ConversationMessage::Tool { id, content });
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
