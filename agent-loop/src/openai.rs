use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenAiMessage {
  pub role: OpenAiRole,
  pub content: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tool_calls: Option<Vec<OpenAiToolCall>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenAiRole {
  System,
  #[default]
  User,
  Assistant,
  Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiToolCall {
  pub id: String,
  #[serde(rename = "type")]
  pub ty: String,
  pub function: OpenAiToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiToolCallFunction {
  pub name: String,
  pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiChatCompletionRequest {
  pub model: String,
  pub messages: Vec<OpenAiMessage>,
  pub stream: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub stream_options: Option<OpenAiStreamOptions>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tools: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiStreamOptions {
  pub include_usage: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OpenAiUsage {
  #[serde(default)]
  pub prompt_tokens: u64,
  #[serde(default)]
  pub completion_tokens: u64,
  #[serde(default)]
  pub total_tokens: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiChatCompletionChunk {
  #[serde(default)]
  pub choices: Vec<OpenAiChunkChoice>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub usage: Option<OpenAiUsage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiChunkChoice {
  pub delta: OpenAiChunkDelta,
  #[serde(default, deserialize_with = "deserialize_finish_reason")]
  pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiChunkDelta {
  #[serde(default, deserialize_with = "deserialize_empty_string")]
  pub content: Option<String>,
  #[serde(default, deserialize_with = "deserialize_empty_string")]
  pub reasoning_content: Option<String>,
  #[serde(default, deserialize_with = "deserialize_empty_string")]
  pub reasoning: Option<String>,
  #[serde(default)]
  pub tool_calls: Vec<OpenAiDeltaToolCall>,
}

impl OpenAiChunkDelta {
  pub fn reasoning(&self) -> Option<&String> {
    self.reasoning_content
      .as_ref()
      .or(self.reasoning.as_ref())
  }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiDeltaToolCall {
  pub index: usize,
  #[serde(default, deserialize_with = "deserialize_empty_string")]
  pub id: Option<String>,
  #[serde(default)]
  pub function: OpenAiDeltaToolCallFunction,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OpenAiDeltaToolCallFunction {
  #[serde(default, deserialize_with = "deserialize_empty_string")]
  pub name: Option<String>,
  #[serde(default, deserialize_with = "deserialize_empty_string")]
  pub arguments: Option<String>,
}

fn deserialize_empty_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
  D: serde::Deserializer<'de>,
{
  let s: String = Deserialize::deserialize(deserializer)?;
  if s.is_empty() {
    Ok(None)
  } else {
    Ok(Some(s))
  }
}

fn deserialize_finish_reason<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
  D: serde::Deserializer<'de>,
{
  let s: String = Deserialize::deserialize(deserializer)?;
  if s == "null" || s.is_empty() {
    Ok(None)
  } else {
    Ok(Some(s))
  }
}
