// SPDX-License-Identifier: GPL-3.0-or-later
use ratatui::{
  prelude::*,
  widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use crate::{app::{App, UiMessage}, error::Error};

const MAX_PREVIEW_LEN: usize = 80;

pub fn draw(frame: &mut Frame, app: &mut App) {
  let screen = app.screen.clone();
  if let Err(err) = screen.render(frame, app) {
    todo!("Handle render errors")
  }
}

fn build_lines(app: &App) -> Vec<Line<'static>> {
  let mut lines = Vec::new();

  for (i, msg) in app.messages.iter().enumerate() {
    match msg {
      UiMessage::User { content } => {
        lines.push(Line::from(Span::styled(
          format!("> {}", content),
          Style::default().fg(Color::Green),
        )));
        lines.push(Line::from(""));
      }
      UiMessage::Assistant {
        content,
        reasoning,
        tool_calls,
      } => {
        if let Some(reasoning_text) = reasoning {
          if !reasoning_text.is_empty() {
            let is_collapsed = app.collapsed.contains(&i);
            if is_collapsed {
              let line_count = reasoning_text.lines().count();
              lines.push(Line::from(Span::styled(
                format!("  \u{25b6} thinking ({line_count} lines)..."),
                Style::default().fg(Color::DarkGray),
              )));
            } else {
              lines.push(Line::from(Span::styled(
                "  \u{25bc} reasoning",
                Style::default().fg(Color::DarkGray),
              )));
              for line in reasoning_text.lines() {
                lines.push(Line::from(Span::styled(
                  format!("  {}", line),
                  Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
                )));
              }
            }
            lines.push(Line::from(""));
          }
        }
        for line in content.lines() {
          lines.push(Line::from(line.to_string()));
        }
        if !content.is_empty() {
          lines.push(Line::from(""));
        }
        for tc in tool_calls {
          let preview = truncate(&tc.arguments, MAX_PREVIEW_LEN);
          lines.push(Line::from(Span::styled(
            format!("  \u{25b6} {}({})", tc.name, preview),
            Style::default().fg(Color::Yellow),
          )));
        }
        if !tool_calls.is_empty() {
          lines.push(Line::from(""));
        }
      }
      UiMessage::Tool { name, content } => {
        let preview = truncate(content.as_str(), MAX_PREVIEW_LEN);
        lines.push(Line::from(Span::styled(
          format!("  \u{2190} {} | {}", name, preview),
          Style::default().fg(Color::DarkGray),
        )));
      }
      UiMessage::System { content } => {
        for line in content.lines() {
          lines.push(Line::from(Span::styled(
            format!("  {}", line),
            Style::default().fg(Color::DarkGray),
          )));
        }
      }
    }
  }

  if app.is_generating {
    if !app.pending_reasoning.is_empty() {
      lines.push(Line::from(Span::styled(
        "  \u{25b6} thinking...",
        Style::default().fg(Color::DarkGray),
      )));
    }
    if !app.pending_content.is_empty() {
      for line in app.pending_content.lines() {
        lines.push(Line::from(Span::from(line.to_string())));
      }
    }
    if !app.pending_tool_calls.is_empty() {
      let mut sorted: Vec<_> = app.pending_tool_calls.iter().collect();
      sorted.sort_by_key(|(idx, _)| *idx);
      for (_, (name, _, args)) in sorted {
        let preview = truncate(args, MAX_PREVIEW_LEN);
        lines.push(Line::from(Span::styled(
          format!("  \u{25b6} {}({})", name, preview),
          Style::default().fg(Color::Yellow),
        )));
      }
    }
  }

  lines
}

fn truncate(s: &str, max_len: usize) -> String {
  if s.len() <= max_len {
    s.to_string()
  } else {
    format!("{}...", &s[..max_len.saturating_sub(3)])
  }
}

#[derive(Debug, Clone, Default)]
pub(crate) enum Screen {
  #[default]
  Chat,
}

impl Screen {
  pub fn render(&self, frame: &mut Frame, app: &mut App) -> Result<(), Error> {
    match self {
      Self::Chat => render_chat(frame, app),
    }
  }
}

fn render_chat(frame: &mut Frame, app: &mut App) -> Result<(), Error> {
  let area = frame.area();
  let chunks = Layout::vertical([
    Constraint::Min(0),
    Constraint::Length(1),
    Constraint::Length(3),
  ])
  .split(area);

  let lines = build_lines(app);
  let content_height = lines.len() as u16;
  let viewport_height = chunks[0].height;
  let max_scroll = content_height.saturating_sub(viewport_height);

  if app.auto_scroll {
    app.scroll = max_scroll;
  } else {
    app.scroll = app.scroll.min(max_scroll);
  }

  let conversation = Paragraph::new(lines)
    .wrap(Wrap { trim: false })
    .scroll((app.scroll, 0));
  frame.render_widget(conversation, chunks[0]);

  let model_name = app.current_model.as_deref().unwrap_or("no model");
  let status = if app.is_generating {
    format!(" {} | generating...", model_name)
  } else {
    format!(" {}", model_name)
  };
  let status_bar = Paragraph::new(Span::styled(
    status,
    Style::default().fg(Color::DarkGray),
  ));
  frame.render_widget(status_bar, chunks[1]);

  let input = Paragraph::new(app.input.as_str())
    .block(Block::default().borders(Borders::ALL).title("Input"));
  frame.render_widget(input, chunks[2]);

  let cursor_x = chunks[2].x
    + 1
    + (app.cursor as u16).min(chunks[2].width.saturating_sub(2));
  frame.set_cursor_position(Position::new(cursor_x, chunks[2].y + 1));

  if !app.command_completions.is_empty() {
    let count = app.command_completions.len() as u16;
    let popup_height = count + 2;
    let popup_area = Rect::new(
      chunks[2].x,
      chunks[2].y.saturating_sub(popup_height),
      chunks[2].width,
      popup_height,
    );
    let items: Vec<ListItem> = app
      .command_completions
      .iter()
      .enumerate()
      .map(|(i, m)| {
        let style = if i == app.completion_selected {
          Style::default().bg(Color::DarkGray).fg(Color::White)
        } else {
          Style::default()
        };
        ListItem::new(Line::from(Span::styled(
          format!(" /{}  {}", m.name(), m.desc()),
          style,
        )))
      })
      .collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL));
    frame.render_widget(Clear, popup_area);
    frame.render_widget(list, popup_area);
  }

  Ok(())
}
