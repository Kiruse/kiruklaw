// SPDX-License-Identifier: GPL-3.0-or-later
use std::{collections::HashMap, path::Path};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
  pub base_url: String,
  #[serde(default)]
  pub api_key: Option<String>,
  #[serde(default)]
  pub api_key_env: Option<String>,
  pub model: String,
}

impl ModelConfig {
  pub fn resolve_api_key(&self) -> String {
    if let Some(ref env_var) = self.api_key_env {
      std::env::var(env_var).unwrap_or_else(|_| {
        self.api_key.as_deref().unwrap_or_default().to_string()
      })
    } else {
      self.api_key.as_deref().unwrap_or_default().to_string()
    }
  }
}

impl From<ModelConfig> for kiruklaw_agent_loop::ModelConfig {
  fn from(m: ModelConfig) -> Self {
    let api_key = m.resolve_api_key();
    Self::OpenAi {
      base_url: m.base_url,
      api_key,
      model: m.model,
    }
  }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ConfigFile {
  #[serde(default)]
  pub models: HashMap<String, ModelConfig>,
}

impl ConfigFile {
  pub fn default_path() -> PathBuf {
    let config_dir = std::env::var("XDG_CONFIG_HOME")
      .map(PathBuf::from)
      .unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".config")
      });
    config_dir.join("kiruklaw").join("config.json")
  }

  pub fn load(path: &Path) -> Result<ConfigFile, Error> {
    if !path.exists() {
      return Err(Error::notfound("Config file"));
    }

    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
  }
}
