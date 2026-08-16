// SPDX-License-Identifier: GPL-3.0-or-later
use crossterm::event::{self, Event};
use std::time::Duration;

pub fn poll(timeout: Duration) -> Option<Event> {
  if event::poll(timeout).unwrap_or(false) {
    event::read().ok()
  } else {
    None
  }
}
