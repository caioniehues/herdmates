//! Focus pane TUI (D3, issue #86 commit 6): a real plugin pane (split or
//! overlay, NEVER a popup — binding decision) that renders the focus file's
//! task/next-action/decisions. Commit 6 is the static skeleton: read the
//! focus file once, draw it, wait for a quit key. Live file-watch refresh
//! and the attention-queue region are commit 7.
//!
//! Render model is pure and separate from terminal I/O, same split as
//! `board.rs`'s `render`/`run_board`: `draw` only takes a `&mut Frame` and
//! `&FocusFile`, so it can be exercised against `ratatui::backend::TestBackend`
//! in tests without a real terminal — no `HerdrApi` call, no file I/O, no
//! crossterm event loop inside it.

use crate::focusfile::{self, FocusFile, FocusFileError};
use crate::jump;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::{execute, ExecutableCommand};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FocusPaneError {
    #[error(transparent)]
    FocusFile(#[from] FocusFileError),
    #[error("terminal I/O error: {0}")]
    Io(#[from] io::Error),
}

pub fn focus_pane_command(_args: &[String]) -> Result<(), FocusPaneError> {
    let focus = focusfile::read_focus_file(&jump::default_focus_file_path())?;

    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let result = run_until_quit(&mut terminal, &focus);

    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    result
}

fn run_until_quit(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    focus: &FocusFile,
) -> Result<(), FocusPaneError> {
    loop {
        terminal.draw(|frame| draw(frame, focus))?;
        if let Event::Key(key) = event::read()? {
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                return Ok(());
            }
        }
    }
}

/// Pure render: task / next-action / decisions, top to bottom. No I/O.
fn draw(frame: &mut Frame, focus: &FocusFile) {
    let [task_area, next_action_area, decisions_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(3),
    ])
    .areas(frame.area());

    frame.render_widget(section("Task", focus.task.as_deref()), task_area);
    frame.render_widget(
        section("Next Action", focus.next_action.as_deref()),
        next_action_area,
    );
    frame.render_widget(decisions_section(focus), decisions_area);
}

fn section(title: &str, body: Option<&str>) -> Paragraph<'static> {
    let text = body.map_or_else(|| "(none)".to_owned(), str::to_owned);
    Paragraph::new(text).wrap(Wrap { trim: true }).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title.to_owned()),
    )
}

fn decisions_section(focus: &FocusFile) -> Paragraph<'static> {
    let lines = if focus.decisions.is_empty() {
        vec![Line::from("(none)")]
    } else {
        focus
            .decisions
            .iter()
            .map(|decision| {
                let checkbox = if decision.resolved { "[x]" } else { "[ ]" };
                let style = if decision.resolved {
                    Style::default().add_modifier(Modifier::CROSSED_OUT)
                } else {
                    Style::default()
                };
                Line::from(Span::styled(format!("{checkbox} {}", decision.text), style))
            })
            .collect()
    };
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title("Decisions"))
}

/// Render `focus` into an off-screen buffer of the given size — the seam
/// tests use to assert on rendered content without a real terminal.
#[cfg(test)]
fn render_to_buffer(focus: &FocusFile, width: u16, height: u16) -> ratatui::buffer::Buffer {
    use ratatui::backend::TestBackend;

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend never fails to construct");
    terminal
        .draw(|frame| draw(frame, focus))
        .expect("draw into TestBackend never fails");
    terminal.backend().buffer().clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::focusfile::DecisionEntry;

    fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
        buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    #[test]
    fn empty_focus_file_shows_none_placeholders() {
        let buffer = render_to_buffer(&FocusFile::default(), 40, 12);
        let text = buffer_text(&buffer);
        assert!(text.contains("Task"));
        assert!(text.contains("Next Action"));
        assert!(text.contains("Decisions"));
        assert_eq!(text.matches("(none)").count(), 3);
    }

    #[test]
    fn task_and_next_action_text_render() {
        let focus = FocusFile {
            task: Some("Ship #86".to_owned()),
            next_action: Some("Write the TUI skeleton".to_owned()),
            decisions: vec![],
        };
        let text = buffer_text(&render_to_buffer(&focus, 60, 12));
        assert!(text.contains("Ship #86"));
        assert!(text.contains("Write the TUI skeleton"));
    }

    #[test]
    fn unresolved_and_resolved_decisions_get_distinct_checkboxes() {
        let focus = FocusFile {
            task: None,
            next_action: None,
            decisions: vec![
                DecisionEntry {
                    id: "a".to_owned(),
                    text: "Pending call".to_owned(),
                    resolved: false,
                },
                DecisionEntry {
                    id: "b".to_owned(),
                    text: "Done call".to_owned(),
                    resolved: true,
                },
            ],
        };
        let text = buffer_text(&render_to_buffer(&focus, 60, 12));
        assert!(text.contains("[ ] Pending call"));
        assert!(text.contains("[x] Done call"));
    }
}
