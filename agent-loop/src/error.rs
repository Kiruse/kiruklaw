use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
  #[error("Reqwest error: {0}")]
  Reqwest(#[from] reqwest::Error),
  #[error("HTTP error: {0}")]
  Http(String),
  #[error("JSON error: {0}")]
  Json(#[from] serde_json::Error),
  #[error("SSE parse error: {0}")]
  Sse(String),
  #[error("External error: {0}")]
  External(String),
}

impl Error {
  pub fn http(msg: impl Into<String>) -> Self {
    Self::Http(msg.into())
  }

  pub async fn status(response: reqwest::Response) -> Self {
    let status = response.status();
    let text = response.text().await.unwrap_or_else(|_| "unknown".to_string());
    Self::Http(format!("{}: {}", status, text))
  }

  pub fn sse(msg: impl Into<String>) -> Self {
    Self::Sse(msg.into())
  }

  pub fn external(msg: impl Into<String>) -> Self {
    Self::External(msg.into())
  }
}
