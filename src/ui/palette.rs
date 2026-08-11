//! The interface's colour tokens, and what they become on a terminal without truecolor.
//!
//! One decision point rather than a `Color::Rgb` scattered through the renderer: the
//! design spec names nine roles, and a terminal that cannot render them needs every one
//! of them mapped, not just the ones someone remembered.

use std::{env, sync::OnceLock};

use ratatui::style::Color;

/// The nine roles from `design/tui/handoff.md`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Token {
  /// Ordinary text.
  Text,
  /// Labels, units, anything the eye should skip over first.
  Muted,
  /// Borders, rules, and cells with nothing in them yet.
  Empty,
  /// Something is happening.
  Accent,
  Success,
  /// Waiting on the user, or about to be retried.
  Warn,
  Error,
  /// Background of a selected row.
  SelectedBg,
  /// Same, in the yellow of the "to identify" view.
  WaitingBg,
}

/// Whether `COLORTERM` announces 24-bit colour.
///
/// Nothing else is a reliable signal: `TERM` says `xterm-256color` on emulators that do
/// support truecolor and on plenty that do not. Absent the variable we assume the
/// narrower terminal, because the failure is asymmetric — the 16-colour palette is
/// legible everywhere, while `Color::Rgb` on a 16-colour terminal is approximated into
/// whatever is nearest, and `#2b323c` and `#5b6673` both land on black.
pub(crate) fn wants_truecolor(colorterm: Option<&str>) -> bool {
  colorterm.is_some_and(|v| v.contains("truecolor") || v.contains("24bit"))
}

/// The colour for a role, on a terminal of the given capability.
pub(crate) fn resolve(token: Token, truecolor: bool) -> Color {
  if !truecolor {
    return match token {
      Token::Text => Color::Reset,
      Token::Muted | Token::Empty => Color::DarkGray,
      Token::Accent => Color::Cyan,
      Token::Success => Color::Green,
      Token::Warn => Color::Yellow,
      Token::Error => Color::Red,
      // No highlight rather than a wrong one: sixteen colours have nothing dark enough
      // to sit behind text without swallowing it. The `▌` cursor and the bold name are
      // what mark the selection there.
      Token::SelectedBg | Token::WaitingBg => Color::Reset,
    };
  }
  match token {
    Token::Text => Color::Rgb(0xc9, 0xd1, 0xd9),
    Token::Muted => Color::Rgb(0x5b, 0x66, 0x73),
    Token::Empty => Color::Rgb(0x2b, 0x32, 0x3c),
    Token::Accent => Color::Rgb(0x5e, 0xc8, 0xd8),
    Token::Success => Color::Rgb(0x78, 0xd1, 0x8b),
    Token::Warn => Color::Rgb(0xe2, 0xb3, 0x5c),
    Token::Error => Color::Rgb(0xe8, 0x6a, 0x76),
    Token::SelectedBg => Color::Rgb(0x16, 0x1c, 0x24),
    Token::WaitingBg => Color::Rgb(0x1c, 0x1a, 0x14),
  }
}

/// Read once: the environment does not change under a running process, and this is
/// called several times per row per frame.
fn truecolor() -> bool {
  static TRUECOLOR: OnceLock<bool> = OnceLock::new();
  *TRUECOLOR.get_or_init(|| wants_truecolor(env::var("COLORTERM").ok().as_deref()))
}

/// The colour for a role on this terminal.
pub(crate) fn color(token: Token) -> Color {
  resolve(token, truecolor())
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Only an explicit announcement counts. `TERM=xterm-256color` says nothing either
  /// way, and guessing wrong flattens two distinct greys onto black.
  #[test]
  fn only_an_explicit_colorterm_buys_truecolor() {
    assert!(wants_truecolor(Some("truecolor")));
    assert!(wants_truecolor(Some("24bit")));
    assert!(!wants_truecolor(Some("")));
    assert!(!wants_truecolor(None));
  }

  /// Every role has a 16-colour answer. A role that fell through to `Color::Rgb` would
  /// be approximated by the terminal, which is exactly what the fallback exists to
  /// avoid.
  #[test]
  fn every_role_resolves_without_truecolor() {
    for token in [
      Token::Text,
      Token::Muted,
      Token::Empty,
      Token::Accent,
      Token::Success,
      Token::Warn,
      Token::Error,
      Token::SelectedBg,
      Token::WaitingBg,
    ] {
      assert!(
        !matches!(resolve(token, false), Color::Rgb(..)),
        "{:?} still asks for truecolor on a terminal that has none",
        token
      );
      assert!(
        matches!(resolve(token, true), Color::Rgb(..)),
        "{:?} does not use the spec's palette when it could",
        token
      );
    }
  }

  /// The three states a row is read by have to stay apart in both modes; two roles
  /// landing on the same colour is a row that cannot be read at a glance.
  #[test]
  fn the_outcome_colours_stay_distinct_in_both_modes() {
    for truecolor in [false, true] {
      let success = resolve(Token::Success, truecolor);
      let warn = resolve(Token::Warn, truecolor);
      let error = resolve(Token::Error, truecolor);
      assert_ne!(success, warn);
      assert_ne!(warn, error);
      assert_ne!(success, error);
    }
  }
}
