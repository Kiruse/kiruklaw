// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

use crate::error::Error;
use crate::models::ModelConfig;

#[derive(Debug, Clone)]
pub(crate) enum Command {
  Help,
  Model,
  Models,
}

impl Command {
  pub fn name(&self) -> &'static str {
    match self {
      Self::Help => "help",
      Self::Model => "model",
      Self::Models => "models",
    }
  }

  pub fn desc(&self) -> &'static str {
    match self {
      Self::Help => "List all available commands in alphanumeric order.",
      Self::Model => "Select a model by name.",
      Self::Models => "List all registered models and their configurations.",
    }
  }

  pub fn execute(&self, args: &str) -> Result<CommandResult, Error> {
    match self {
      Self::Help => cmd_help(),
      Self::Model => cmd_model(args),
      Self::Models => cmd_models(args),
    }
  }

  pub fn all() -> Vec<Command> {
    vec![
      Self::Help,
      Self::Model,
      Self::Models,
    ]
  }
}

impl FromStr for Command {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let s = if s.chars().nth(0) == Some('/') { &s[1..] } else { &s[0..] };
    match s {
      "help" => Ok(Self::Help),
      "model" => Ok(Self::Model),
      "models" => Ok(Self::Models),
      _ => Err(Error::command(format!("unknown command {s}"))),
    }
  }
}

pub(crate) fn compute_completions(query: &str) -> Vec<Command> {
  if query.is_empty() {
    let mut all: Vec<Command> = Command::all();
    all.sort_by_key(|m| m.name());
    all.truncate(5);
    return all;
  }

  let mut exact = Vec::new();
  let mut subword = Vec::new();
  let mut fuzzy = Vec::new();

  for cmd in Command::all() {
    if query == cmd.name() {
      exact.push(cmd);
    } else if is_subword_match(query, cmd.name()) {
      subword.push(cmd);
    } else if is_fuzzy_match(query, cmd.name()) {
      fuzzy.push(cmd);
    }
  }

  exact.sort_by_key(|m| m.name());
  subword.sort_by_key(|m| m.name());
  fuzzy.sort_by_key(|m| m.name());

  let mut result = exact;
  result.extend(subword);
  result.extend(fuzzy);
  result.truncate(5);
  result
}

pub(crate) struct CommandResult {
  pub(crate) message: String,
  pub(crate) switch_model: Option<String>,
}

pub(crate) fn execute(
  cmd: &str,
  models: &HashMap<String, ModelConfig>,
  current_model: &Option<String>,
  config_path: &Path,
) -> Result<CommandResult, Error> {
  let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
  let command = &parts[0];
  let command: Command = command.parse()?;
  command.execute(parts[1])
}

fn cmd_help() -> Result<CommandResult, Error> {
  let mut text = String::from("Commands:\n");
  for cmd in Command::all() {
    text.push_str(&format!("  /{} - {}\n", cmd.name(), cmd.desc()));
  }
  text.push_str(
    "  Ctrl+C - Quit\n  Ctrl+PgUp/PgDn or mouse wheel - Scroll\n  Alt+r - Toggle reasoning\n  End - Jump to bottom",
  );
  Ok(CommandResult {
    message: text,
    switch_model: None,
  })
}

fn cmd_model(args: &str) -> Result<CommandResult, Error> {
  todo!()
  // if let Some(model_name) = parts.get(1).map(|s| s.trim()) {
  //   if models.contains_key(model_name) {
  //     CommandResult {
  //       message: format!("Switched to model: {}", model_name),
  //       switch_model: Some(model_name.to_string()),
  //     }
  //   } else {
  //     let available: Vec<&str> = models.keys().map(|s| s.as_str()).collect();
  //     CommandResult {
  //       message: format!(
  //         "Unknown model: {}. Available: {}",
  //         model_name,
  //         available.join(", ")
  //       ),
  //       switch_model: None,
  //     }
  //   }
  // } else {
  //   CommandResult {
  //     message: "Usage: /model <name>".to_string(),
  //     switch_model: None,
  //   }
  // }
}

fn cmd_models(args: &str) -> Result<CommandResult, Error> {
  todo!();
  // if models.is_empty() {
  //   CommandResult {
  //     message: format!(
  //       "No models configured. Create {}:\n\
  //        {{\n  \"models\": {{\n    \"model-name\": {{\n      \"base_url\": \"https://api.example.com/v1\",\n      \"model\": \"model-id\",\n      \"api_key_env\": \"API_KEY_VAR\"\n    }}\n  }}\n}}",
  //       config_path.display()
  //     ),
  //     switch_model: None,
  //   }
  // } else {
  //   let mut output = String::from("Registered models:\n");
  //   for (name, config) in models {
  //     let current = if current_model.as_ref() == Some(name) {
  //       " *"
  //     } else {
  //       ""
  //     };
  //     output.push_str(&format!(
  //       "  {}{} ({}) : {}\n",
  //       name, current, config.model, config.base_url
  //     ));
  //   }
  //   CommandResult {
  //     message: output,
  //     switch_model: None,
  //   }
  // }
}

fn is_subword_match(query: &str, name: &str) -> bool {
  let mut search_from = 0;
  while search_from < name.len() {
    if name[search_from..].starts_with(query) {
      return true;
    }
    match name[search_from..].find('-') {
      Some(pos) => search_from += pos + 1,
      None => break,
    }
  }
  false
}

/// Simple fuzzy search: all characters must be present in-order
fn is_fuzzy_match(query: &str, name: &str) -> bool {
  let mut qi = 0;
  let qchars: Vec<char> = query.chars().collect();
  for ch in name.chars() {
    if qi < qchars.len() && ch == qchars[qi] {
      qi += 1;
    }
  }
  qi == qchars.len()
}
