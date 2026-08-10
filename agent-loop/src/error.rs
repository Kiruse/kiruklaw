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
}

impl Error {
  pub async fn status(response: reqwest::Response) -> Error {
    let status = response.status();
    let text = response.text().await.unwrap_or_else(|_| "unknown".to_string());
    Error::Http(format!("{}: {}", status, text))
  }
}
