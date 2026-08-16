use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
  #[error("{0}")]
  Io(#[from] io::Error),
  #[error("{0}")]
  Json(#[from] serde_json::Error),
  #[error("{0} not found")]
  NotFound(String),
  #[error("Invalid command: {0}")]
  Command(String),
}

impl Error {
  pub fn notfound(msg: impl Into<String>) -> Self {
    Self::NotFound(msg.into())
  }

  pub fn command(msg: impl Into<String>) -> Self {
    Self::Command(msg.into())
  }
}
