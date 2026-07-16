//! `teammux` dispatcher core (issue #85 commit 3).
//!
//! One reusable dispatch entry point, per cmux's `__tmux-compat` pattern
//! (`docs/research/cmux-comparative-2026-07-16/REPORT.md`, correction c):
//! [`execute`] takes raw argv and returns a [`DispatchOutcome`], with no
//! process-boundary I/O of its own, so it can be unit-tested directly and,
//! per the same correction, reused if herdmates ever shims another agent
//! CLI's tmux calls the same way. [`run`] is the thin process-boundary
//! wrapper `src/bin/teammux.rs` calls: it prints stdout/stderr and maps the
//! outcome to a faithful exit code (0 for success, nonzero for anything
//! else — translate-don't-emulate, issue #85 decision doc: never silent
//! success).
//!
//! This commit only wires up the three static probes inventoried in
//! `docs/research/spike-tmux-verbs-2026-07-16/REPORT.md` §3 (`show -Av
//! mouse`, `show -gv focus-events`, `display-message -p #{client_termtype}`).
//! Every other successfully-*parsed* verb (structural reads, split-window,
//! lifecycle, styling — commits 4-7) is a deliberate, labeled "not yet
//! implemented" placeholder, not a translate-don't-emulate failure: it is
//! recognized by `tmuxargs`, just not yet handled here.

use crate::tmuxargs::{self, DisplayField, ParseError, Verb};
use std::process::ExitCode;

/// The result of dispatching one parsed tmux call, before any process I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// Print `stdout` to the real stdout and exit 0.
    Ok { stdout: String },
    /// Print `message` to stderr and exit nonzero.
    Error { message: String },
}

/// Dispatch one parsed call. The single match arm every verb handler will
/// eventually replace its placeholder branch in, commit by commit.
pub fn dispatch(call: tmuxargs::ParsedCall) -> DispatchOutcome {
    match call.verb {
        Verb::ShowMouse => DispatchOutcome::Ok {
            stdout: "off\n".to_owned(),
        },
        Verb::ShowFocusEvents => DispatchOutcome::Ok {
            stdout: "0\n".to_owned(),
        },
        Verb::DisplayMessage {
            field: DisplayField::ClientTermtype,
            ..
        } => DispatchOutcome::Ok {
            stdout: "xterm-256color\n".to_owned(),
        },
        other => DispatchOutcome::Error {
            message: format!(
                "teammux: {other:?} not yet implemented (issue #85 commits 4-7 pending)"
            ),
        },
    }
}

/// Parse `argv` and dispatch it, without any process I/O — the pure core
/// [`run`] wraps for the real binary.
pub fn execute(argv: &[String]) -> DispatchOutcome {
    match tmuxargs::parse(argv) {
        Ok(call) => dispatch(call),
        Err(error) => DispatchOutcome::Error {
            message: format_parse_error(&error),
        },
    }
}

fn format_parse_error(error: &ParseError) -> String {
    format!("teammux: {error}")
}

/// The process-boundary entry point `src/bin/teammux.rs` calls: prints the
/// outcome and returns a faithful exit code.
pub fn run(argv: &[String]) -> ExitCode {
    match execute(argv) {
        DispatchOutcome::Ok { stdout } => {
            print!("{stdout}");
            ExitCode::SUCCESS
        }
        DispatchOutcome::Error { message } => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmuxargs::GlobalFlags;

    fn call(verb: Verb) -> tmuxargs::ParsedCall {
        tmuxargs::ParsedCall {
            globals: GlobalFlags::default(),
            verb,
        }
    }

    #[test]
    fn show_mouse_probe_returns_static_off() {
        assert_eq!(
            dispatch(call(Verb::ShowMouse)),
            DispatchOutcome::Ok {
                stdout: "off\n".to_owned()
            }
        );
    }

    #[test]
    fn show_focus_events_probe_returns_static_zero() {
        assert_eq!(
            dispatch(call(Verb::ShowFocusEvents)),
            DispatchOutcome::Ok {
                stdout: "0\n".to_owned()
            }
        );
    }

    #[test]
    fn client_termtype_probe_returns_static_terminal_type() {
        assert_eq!(
            dispatch(call(Verb::DisplayMessage {
                target: None,
                field: DisplayField::ClientTermtype,
            })),
            DispatchOutcome::Ok {
                stdout: "xterm-256color\n".to_owned()
            }
        );
    }

    #[test]
    fn recognized_but_unhandled_verbs_are_a_labeled_placeholder_not_a_silent_success() {
        let outcome = dispatch(call(Verb::KillPane {
            pane: tmuxargs::TmuxId::parse("%0").unwrap(),
        }));
        match outcome {
            DispatchOutcome::Error { message } => {
                assert!(message.contains("not yet implemented"));
            }
            other => panic!("expected a placeholder Error outcome, got {other:?}"),
        }
    }

    #[test]
    fn execute_surfaces_unrecognized_verbs_as_a_loud_error_not_silent_success() {
        let outcome = execute(&["frobnicate-pane".to_owned()]);
        match outcome {
            DispatchOutcome::Error { message } => {
                assert!(message.starts_with("teammux: unrecognized verb"));
            }
            other => panic!("expected an Error outcome, got {other:?}"),
        }
    }

    #[test]
    fn run_exits_success_for_a_recognized_probe() {
        assert!(
            run(&["show".to_owned(), "-Av".to_owned(), "mouse".to_owned()]) == ExitCode::SUCCESS
        );
    }

    #[test]
    fn run_exits_failure_for_an_unrecognized_verb() {
        assert!(run(&["nonsense".to_owned()]) == ExitCode::FAILURE);
    }
}
