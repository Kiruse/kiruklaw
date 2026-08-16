// SPDX-License-Identifier: GPL-3.0-or-later
mod app;
mod commands;
mod error;
mod event;
mod models;
mod ui;

use std::io;

use crossterm::{
  execute,
  terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use app::App;

fn main() -> anyhow::Result<()> {
  enable_raw_mode()?;
  execute!(io::stdout(), EnterAlternateScreen)?;
  let backend = CrosstermBackend::new(io::stdout());
  let mut terminal = Terminal::new(backend)?;

  let mut app = App::new()?;
  let result = app.run(&mut terminal);

  disable_raw_mode()?;
  execute!(io::stdout(), LeaveAlternateScreen)?;
  terminal.show_cursor()?;

  result
}
