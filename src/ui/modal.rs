use std::{
  io,
  sync::{Arc, Mutex},
  time::Duration,
};

use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{backend::CrosstermBackend, Terminal};

use super::render::render;
use super::{AppState, ModalDisplayState, ModalMode, ModalRequest, ModalResponse};

// ── Modal interaction loop ─────────────────────────────────────────────────

/// Blocking modal interaction loop. Runs inside the render thread.
///
/// - Drains any buffered key events before opening the modal.
/// - Redraws at ~50 ms intervals while waiting for user input.
/// - Sends exactly one `ModalResponse` before returning.
pub(super) fn show_modal(
  req: ModalRequest,
  terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
  state: &Arc<Mutex<AppState>>,
) {
  // Drain buffered events so stray keypresses don't close the modal immediately.
  while crossterm::event::poll(Duration::ZERO).unwrap_or(false) {
    let _ = crossterm::event::read();
  }

  let mut cursor: usize = 0;
  let mut input = String::new();
  let mut mode = ModalMode::List;
  let mut input_status: Option<String> = None;

  loop {
    // Update modal display state before drawing.
    {
      let mut s = state.lock().unwrap();
      s.tick += 1;
      s.modal = Some(ModalDisplayState {
        filename: req.filename.clone(),
        sha1: req.sha1.clone(),
        candidates: req.candidates.clone(),
        cursor,
        input: input.clone(),
        mode: mode.clone(),
        input_status: input_status.clone(),
      });
    }

    terminal
      .draw(|frame| {
        let s = state.lock().unwrap();
        render(frame, &s);
      })
      .unwrap();

    if crossterm::event::poll(Duration::from_millis(50)).unwrap_or(false) {
      if let Ok(Event::Key(key)) = crossterm::event::read() {
        if key.kind != KeyEventKind::Press {
          continue;
        }
        match mode.clone() {
          ModalMode::List => match key.code {
            KeyCode::Up => cursor = cursor.saturating_sub(1),
            KeyCode::Down => {
              if cursor + 1 < req.candidates.len() {
                cursor += 1;
              }
            }
            KeyCode::Enter => {
              let resp = if req.candidates.is_empty() {
                ModalResponse::Cancelled
              } else {
                ModalResponse::SelectedId(req.candidates[cursor].game_id.clone())
              };
              let _ = req.response.send(resp);
              state.lock().unwrap().modal = None;
              return;
            }
            KeyCode::Char('i') => {
              mode = ModalMode::Input;
              input_status = None;
            }
            KeyCode::Esc => {
              let _ = req.response.send(ModalResponse::Cancelled);
              state.lock().unwrap().modal = None;
              return;
            }
            _ => {}
          },

          ModalMode::Input => match key.code {
            KeyCode::Enter => {
              if let Ok(game_id) = input.parse::<u32>() {
                // Show "looking up…" before the blocking API call.
                {
                  let mut s = state.lock().unwrap();
                  if let Some(ref mut m) = s.modal {
                    m.input_status = Some("Looking up…".to_string());
                  }
                }
                terminal
                  .draw(|frame| {
                    let s = state.lock().unwrap();
                    render(frame, &s);
                  })
                  .unwrap();

                match (req.fetch_by_id)(game_id) {
                  Some(name) => {
                    mode = ModalMode::Confirming {
                      game_id: input.clone(),
                      game_name: name,
                    };
                    input_status = None;
                  }
                  None => {
                    input_status = Some("ID not found on ScreenScraper".to_string());
                  }
                }
              }
            }
            KeyCode::Esc => {
              mode = ModalMode::List;
              input.clear();
              input_status = None;
            }
            KeyCode::Backspace => {
              input.pop();
              input_status = None;
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
              input.push(c);
              input_status = None;
            }
            _ => {}
          },

          ModalMode::Confirming { game_id, .. } => match key.code {
            KeyCode::Enter => {
              let _ = req.response.send(ModalResponse::ManualId(game_id));
              state.lock().unwrap().modal = None;
              return;
            }
            KeyCode::Esc => {
              // Go back to input mode keeping the typed ID.
              mode = ModalMode::Input;
              input_status = None;
            }
            _ => {}
          },
        }
      }
    }
  }
}
