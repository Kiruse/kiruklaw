// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::prelude::*;

use kiruklaw_agent_loop::{
  AgentLoop, AgentMessageChunk, Conversation, ConversationMessage,
  FinishReason,
};

use crate::commands::{self, Command, compute_completions};
use crate::error::Error;
use crate::event;
use crate::models::ConfigFile;
use crate::ui::Screen;

#[derive(Debug, Clone)]
pub enum UiMessage {
  User { content: String },
  Assistant {
    content: String,
    reasoning: Option<String>,
    tool_calls: Vec<UiToolCall>,
  },
  Tool { name: String, content: String },
  System { content: String },
}

#[derive(Debug, Clone)]
pub struct UiToolCall {
  pub name: String,
  pub arguments: String,
}

type Term = ratatui::Terminal<CrosstermBackend<std::io::Stdout>>;

#[derive(Debug, Default)]
pub struct App {
  pub(crate) config: ConfigFile,
  pub(crate) screen: Screen,
  pub(crate) current_model: Option<String>,
  pub(crate) config_path: std::path::PathBuf,

  conversation: Conversation,
  pub(crate) messages: Vec<UiMessage>,

  pub(crate) input: String,
  pub(crate) cursor: usize,
  pub(crate) scroll: u16,
  pub(crate) auto_scroll: bool,

  pub(crate) collapsed: HashSet<usize>,

  pub(crate) is_generating: bool,
  stream_rx: Option<mpsc::Receiver<AgentMessageChunk>>,
  done_rx: Option<mpsc::Receiver<Conversation>>,
  pub(crate) pending_content: String,
  pub(crate) pending_reasoning: String,
  pub(crate) pending_tool_calls: HashMap<usize, (String, String, String)>,

  pub(crate) command_completions: Vec<Command>,
  pub(crate) completion_selected: usize,
}

// TODO: Agentic Code - clean up
impl App {
  pub fn new() -> Result<Self, Error> {
    let config_path = ConfigFile::default_path();
    let config = ConfigFile::load(&config_path)?;
    let models = &config.models;
    let current_model = models.keys().next().cloned();

    let mut app = Self {
      config,
      current_model,
      config_path: config_path.clone(),
      auto_scroll: true,
      ..Default::default()
    };

    if app.config.models.is_empty() {
      app.messages.push(UiMessage::System {
        content: format!(
          "Welcome to kiruklaw.\n\
           No models configured. Create {} to get started.\n\
           Type /help for commands.",
          config_path.display()
        ),
      });
    } else {
      app.messages.push(UiMessage::System {
        content: "Welcome to kiruklaw. Type /help for commands.".to_string(),
      });
    }

    Ok(app)
  }

  pub fn run(&mut self, terminal: &mut Term) -> anyhow::Result<()> {
    loop {
      terminal.draw(|frame| crate::ui::draw(frame, self))?;
      self.process_stream();

      if let Some(ev) = event::poll(std::time::Duration::from_millis(16)) {
        if self.handle_event(ev)? {
          break;
        }
      }
    }
    Ok(())
  }

  fn process_stream(&mut self) {
    let mut stream_rx = self.stream_rx.take();
    while let Some(rx) = &mut stream_rx {
      match rx.try_recv() {
        Ok(chunk) => self.handle_chunk(chunk),
        Err(mpsc::error::TryRecvError::Empty) => break,
        Err(mpsc::error::TryRecvError::Disconnected) => {
          stream_rx = None;
          break;
        }
      }
    }
    self.stream_rx = stream_rx;

    let mut done_rx = self.done_rx.take();
    if let Some(rx) = &mut done_rx {
      match rx.try_recv() {
        Ok(conversation) => {
          self.conversation = conversation;
          self.finalize_message();
          self.stream_rx = None;
          done_rx = None;
          self.is_generating = false;
        }
        Err(mpsc::error::TryRecvError::Empty) => {}
        Err(mpsc::error::TryRecvError::Disconnected) => {
          self.stream_rx = None;
          done_rx = None;
          self.is_generating = false;
        }
      }
    }
    self.done_rx = done_rx;
  }

  fn handle_chunk(&mut self, chunk: AgentMessageChunk) {
    match chunk {
      AgentMessageChunk::Content { text } => {
        self.pending_content.push_str(&text);
      }
      AgentMessageChunk::Reasoning { text } => {
        self.pending_reasoning.push_str(&text);
      }
      AgentMessageChunk::ToolCallStart { index, id, name, .. } => {
        self
          .pending_tool_calls
          .insert(index, (name, id, String::new()));
      }
      AgentMessageChunk::ToolCallArgs { index, args, .. } => {
        if let Some((_, _, acc)) = self.pending_tool_calls.get_mut(&index) {
          acc.push_str(&args);
        }
      }
      AgentMessageChunk::Done { reason } => {
        if matches!(reason, FinishReason::ToolCalls) {
          self.pending_content.clear();
          self.pending_reasoning.clear();
          self.pending_tool_calls.clear();
        }
      }
    }
  }

  fn finalize_message(&mut self) {
    self.messages.clear();
    self.collapsed.clear();

    let mut pending_reasoning = if self.pending_reasoning.is_empty() {
      None
    } else {
      Some(std::mem::take(&mut self.pending_reasoning))
    };

    for msg in &self.conversation.messages {
      let idx = self.messages.len();
      let ui_msg = match msg {
        ConversationMessage::User { content } => UiMessage::User {
          content: content.clone(),
        },
        ConversationMessage::Assistant { content, tool_calls } => {
          let reasoning = pending_reasoning.take();
          let has_reasoning = reasoning.as_ref().map_or(false, |r| !r.is_empty());
          if has_reasoning {
            self.collapsed.insert(idx);
          }
          UiMessage::Assistant {
            content: content.clone(),
            reasoning,
            tool_calls: tool_calls
              .iter()
              .map(|tc| UiToolCall {
                name: tc.name.clone(),
                arguments: tc.arguments.clone(),
              })
              .collect(),
          }
        }
        ConversationMessage::Tool { id, content } => UiMessage::Tool {
          name: id.clone(),
          content: content.clone(),
        },
        ConversationMessage::System { content } => UiMessage::System {
          content: content.clone(),
        },
      };
      self.messages.push(ui_msg);
    }

    self.pending_content.clear();
    self.pending_tool_calls.clear();
  }

  fn handle_event(&mut self, event: crossterm::event::Event) -> anyhow::Result<bool> {
    match event {
      crossterm::event::Event::Key(key) => self.handle_key(key),
      crossterm::event::Event::Mouse(mouse) => self.handle_mouse(mouse),
      _ => Ok(false),
    }
  }

  fn handle_key(&mut self, key: KeyEvent) -> anyhow::Result<bool> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
      return Ok(true);
    }
    if key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Char('r') {
      self.toggle_reasoning();
      return Ok(false);
    }

    if self.is_generating {
      if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
          KeyCode::PageUp => {
            self.scroll_up(5);
            self.auto_scroll = false;
          }
          KeyCode::PageDown => {
            self.scroll_down(5);
          }
          _ => {}
        }
      }
      return Ok(false);
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) {
      match key.code {
        KeyCode::PageUp => {
          self.scroll_up(5);
          self.auto_scroll = false;
        }
        KeyCode::PageDown => {
          self.scroll_down(5);
        }
        _ => {}
      }
      return Ok(false);
    }

    if !self.command_completions.is_empty() {
      match key.code {
        KeyCode::Up => {
          if self.completion_selected > 0 {
            self.completion_selected -= 1;
          } else {
            self.completion_selected = self.command_completions.len() - 1;
          }
          return Ok(false);
        }
        KeyCode::Down => {
          if self.completion_selected < self.command_completions.len() - 1 {
            self.completion_selected += 1;
          } else {
            self.completion_selected = 0;
          }
          return Ok(false);
        }
        KeyCode::Tab | KeyCode::Enter => {
          self.accept_completion();
          return Ok(false);
        }
        KeyCode::Esc => {
          self.command_completions.clear();
          self.completion_selected = 0;
          return Ok(false);
        }
        _ => {}
      }
    }

    match key.code {
      KeyCode::Enter => {
        if !self.input.trim().is_empty() {
          self.submit_input();
        }
      }
      KeyCode::Backspace => {
        if self.cursor > 0 {
          self.cursor -= 1;
          self.input.remove(self.cursor);
          self.update_completions();
        }
      }
      KeyCode::Delete => {
        if self.cursor < self.input.len() {
          self.input.remove(self.cursor);
          self.update_completions();
        }
      }
      KeyCode::Left => {
        if self.cursor > 0 {
          self.cursor -= 1;
        }
      }
      KeyCode::Right => {
        if self.cursor < self.input.len() {
          self.cursor += 1;
        }
      }
      KeyCode::Home => self.cursor = 0,
      KeyCode::End => {
        self.cursor = self.input.len();
        self.auto_scroll = true;
      }
      KeyCode::Char(c) => {
        self.input.insert(self.cursor, c);
        self.cursor += 1;
        self.update_completions();
      }
      KeyCode::PageUp => {
        self.scroll_up(5);
        self.auto_scroll = false;
      }
      KeyCode::PageDown => {
        self.scroll_down(5);
      }
      _ => {}
    }

    Ok(false)
  }

  fn handle_mouse(&mut self, mouse: MouseEvent) -> anyhow::Result<bool> {
    match mouse.kind {
      MouseEventKind::ScrollUp => {
        self.scroll_up(3);
        self.auto_scroll = false;
      }
      MouseEventKind::ScrollDown => {
        self.scroll_down(3);
      }
      _ => {}
    }
    Ok(false)
  }

  fn toggle_reasoning(&mut self) {
    let reasoning_indices: Vec<usize> = self
      .messages
      .iter()
      .enumerate()
      .filter(|(_, msg)| {
        matches!(
          msg,
          UiMessage::Assistant {
            reasoning: Some(r),
            ..
          } if !r.is_empty()
        )
      })
      .map(|(i, _)| i)
      .collect();

    let all_collapsed = reasoning_indices.iter().all(|i| self.collapsed.contains(i));

    if all_collapsed {
      for i in &reasoning_indices {
        self.collapsed.remove(i);
      }
    } else {
      for i in &reasoning_indices {
        self.collapsed.insert(*i);
      }
    }
  }

  fn scroll_up(&mut self, amount: u16) {
    self.scroll = self.scroll.saturating_sub(amount);
  }

  fn scroll_down(&mut self, amount: u16) {
    self.scroll = self.scroll.saturating_add(amount);
  }

  fn update_completions(&mut self) {
    if self.input.starts_with('/') {
      let after_slash = &self.input[1..];
      if after_slash.contains(' ') {
        self.command_completions.clear();
        self.completion_selected = 0;
      } else {
        self.command_completions = compute_completions(after_slash);
        self.completion_selected = 0;
      }
    } else {
      self.command_completions.clear();
      self.completion_selected = 0;
    }
  }

  fn accept_completion(&mut self) {
    if let Some(m) = self.command_completions.get(self.completion_selected) {
      self.input = format!("/{} ", m.name());
      self.cursor = self.input.len();
      self.command_completions.clear();
      self.completion_selected = 0;
    }
  }

  fn submit_input(&mut self) {
    let input = self.input.trim().to_string();
    self.input.clear();
    self.cursor = 0;
    self.command_completions.clear();
    self.completion_selected = 0;

    if input.starts_with('/') {
      if let Err(err) = self.handle_command(&input) {
        todo!()
      }
      return;
    }

    if self.current_model.is_none() {
      self.messages.push(UiMessage::User { content: input.clone() });
      self.messages.push(UiMessage::System {
        content: "No model configured. Create a config file to get started.".to_string(),
      });
      return;
    }

    self.conversation.push(ConversationMessage::user(input.clone()));
    self.messages.push(UiMessage::User { content: input });
    self.start_agent_loop();
  }

  fn handle_command(&mut self, cmd: &str) -> Result<(), Error> {
    let result =
      commands::execute(cmd, &self.config.models, &self.current_model, &self.config_path)?;
    if let Some(name) = result.switch_model {
      self.current_model = Some(name);
    }
    self.messages.push(UiMessage::System {
      content: result.message,
    });
    Ok(())
  }

  fn start_agent_loop(&mut self) {
    let model_name = self.current_model.as_ref().unwrap().clone();
    let model_config: kiruklaw_agent_loop::ModelConfig = self
      .config
      .models
      .get(&model_name)
      .map(|m| m.clone().into())
      .unwrap_or_default();

    let (stream_tx, stream_rx) = mpsc::channel(256);
    let (done_tx, done_rx) = mpsc::channel(1);
    let mut conversation = self.conversation.clone();

    self.is_generating = true;
    self.stream_rx = Some(stream_rx);
    self.done_rx = Some(done_rx);
    self.pending_content.clear();
    self.pending_reasoning.clear();
    self.pending_tool_calls.clear();

    std::thread::spawn(move || {
      let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
      match rt {
        Ok(rt) => rt.block_on(async move {
          let mut agent_loop = AgentLoop::new(model_config);
          let _ = agent_loop.run(&mut conversation, stream_tx).await;
          let _ = done_tx.send(conversation).await;
        }),
        Err(_) => {
          let _ = done_tx.blocking_send(conversation);
        }
      }
    });
  }
}
