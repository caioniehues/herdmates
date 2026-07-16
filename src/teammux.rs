//! `teammux` dispatcher core (issue #85 commit 3, extended commit 4).
//!
//! One reusable dispatch entry point, per cmux's `__tmux-compat` pattern
//! (`docs/research/cmux-comparative-2026-07-16/REPORT.md`, correction c):
//! [`dispatch`] takes a parsed call plus its dependencies (a [`HerdrApi`]
//! implementor and a loaded [`IdMap`]) and returns a [`DispatchOutcome`],
//! with no process-boundary I/O of its own — real herdr calls and idmap
//! file reads happen through the injected dependencies, so tests can supply
//! `crate::herdr::test_support::FakeHerdr` (the same recording-fake process
//! trait pattern the rest of the codebase already uses) and a temp-file-
//! backed `IdMap`. [`run`] is the only code that touches real stdio, a real
//! `HerdrClient`, and the real `TEAMMUX_STATE_PATH` file.
//!
//! Commit 3 wired the three static probes from
//! `docs/research/spike-tmux-verbs-2026-07-16/REPORT.md` §3 (`show -Av
//! mouse`, `show -gv focus-events`, `display-message -p #{client_termtype}`).
//! Commit 4 adds the structural reads: `list-panes -F #{pane_id}` (via
//! `herdr pane list` filtered client-side by tab id — live-verified: `herdr
//! tab get` returns tab metadata, not a pane roster, so `pane list` is the
//! only way to enumerate a tab's panes) and `display-message -p
//! #{window_id}` (via `herdr pane get`'s `tab_id` field). Both translate a
//! herdr id back to its tmux `%N`/`@N` id through [`IdMap::reverse_lookup`]
//! before printing — output must always speak in tmux's id space, never
//! herdr's. A herdr pane found in the target tab with no idmap registration
//! is a loud error (inconsistent state), not a silently-dropped line: by the
//! time teammux runs, the launcher has already registered every pane it
//! created, so an orphan means something is wrong, not that it's optional.
//!
//! Every other successfully-*parsed* verb (split-window, lifecycle, styling
//! — commits 5-7) is still a deliberate, labeled "not yet implemented"
//! placeholder, not a translate-don't-emulate failure: it is recognized by
//! `tmuxargs`, just not yet handled here.

use crate::herdr::HerdrApi;
use crate::idmap::IdMap;
use crate::tmuxargs::{self, DisplayField, ParseError, TmuxId, Verb};
use std::process::ExitCode;

/// The result of dispatching one parsed tmux call, before any process I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// Print `stdout` to the real stdout and exit 0.
    Ok { stdout: String },
    /// Print `message` to stderr and exit nonzero.
    Error { message: String },
}

/// Dispatch one parsed call against real (or faked) herdr + idmap state.
pub fn dispatch<H: HerdrApi>(
    herdr: &H,
    idmap: &IdMap,
    call: tmuxargs::ParsedCall,
) -> DispatchOutcome {
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
        Verb::DisplayMessage {
            target: Some(pane),
            field: DisplayField::WindowId,
        } => display_window_id(herdr, idmap, &pane),
        Verb::ListPaneIds { window } => list_pane_ids(herdr, idmap, &window),
        other => DispatchOutcome::Error {
            message: format!(
                "teammux: {other:?} not yet implemented (issue #85 commits 5-7 pending)"
            ),
        },
    }
}

/// `display-message -t %N -p #{window_id}`: resolve the tmux window id that
/// owns `pane`, via herdr's own `tab_id` field on the pane.
fn display_window_id<H: HerdrApi>(herdr: &H, idmap: &IdMap, pane: &TmuxId) -> DispatchOutcome {
    let herdr_pane_id = match idmap.lookup(pane.as_str()) {
        Some(id) => id.to_owned(),
        None => return unknown_tmux_id("display-message", pane.as_str()),
    };
    let info = match herdr.pane_get(&herdr_pane_id) {
        Ok(info) => info,
        Err(error) => {
            return DispatchOutcome::Error {
                message: format!("teammux: display-message: herdr pane get failed: {error}"),
            }
        }
    };
    let Some(herdr_tab_id) = info.tab_id else {
        return DispatchOutcome::Error {
            message: format!(
                "teammux: display-message: herdr pane `{herdr_pane_id}` has no tab_id"
            ),
        };
    };
    match idmap.reverse_lookup(&herdr_tab_id) {
        Some(tmux_window_id) => DispatchOutcome::Ok {
            stdout: format!("{tmux_window_id}\n"),
        },
        None => DispatchOutcome::Error {
            message: format!(
                "teammux: display-message: herdr tab `{herdr_tab_id}` has no tmux window id registered in idmap"
            ),
        },
    }
}

/// `list-panes -t @N -F #{pane_id}`: enumerate the panes herdr reports for
/// the tab `window` maps to, translating each back to its tmux `%N` id.
fn list_pane_ids<H: HerdrApi>(herdr: &H, idmap: &IdMap, window: &TmuxId) -> DispatchOutcome {
    let herdr_tab_id = match idmap.lookup(window.as_str()) {
        Some(id) => id.to_owned(),
        None => return unknown_tmux_id("list-panes", window.as_str()),
    };
    let panes = match herdr.pane_list(None) {
        Ok(panes) => panes,
        Err(error) => {
            return DispatchOutcome::Error {
                message: format!("teammux: list-panes: herdr pane list failed: {error}"),
            }
        }
    };

    let mut tmux_ids = Vec::new();
    for pane in panes {
        if pane.tab_id.as_deref() != Some(herdr_tab_id.as_str()) {
            continue;
        }
        match idmap.reverse_lookup(&pane.pane_id) {
            Some(tmux_id) => tmux_ids.push(tmux_id.to_owned()),
            None => {
                return DispatchOutcome::Error {
                    message: format!(
                        "teammux: list-panes: herdr pane `{}` in tab `{herdr_tab_id}` has no tmux id registered in idmap",
                        pane.pane_id
                    ),
                }
            }
        }
    }
    tmux_ids.sort_by_key(|id| pane_sort_key(id));

    let mut stdout = String::new();
    for id in tmux_ids {
        stdout.push_str(&id);
        stdout.push('\n');
    }
    DispatchOutcome::Ok { stdout }
}

/// Sort key for tmux `%N`/`@N` ids: numeric by `N`, falling back to the raw
/// string for anything that doesn't parse (never observed, but sorting must
/// not panic on it).
fn pane_sort_key(tmux_id: &str) -> i64 {
    tmux_id
        .trim_start_matches(['%', '@'])
        .parse()
        .unwrap_or(i64::MAX)
}

fn unknown_tmux_id(verb: &str, tmux_id: &str) -> DispatchOutcome {
    DispatchOutcome::Error {
        message: format!("teammux: {verb}: unknown tmux id `{tmux_id}` (not in idmap)"),
    }
}

/// Parse `argv` and dispatch it against real (or faked) herdr + idmap state.
pub fn execute<H: HerdrApi>(herdr: &H, idmap: &IdMap, argv: &[String]) -> DispatchOutcome {
    match tmuxargs::parse(argv) {
        Ok(call) => dispatch(herdr, idmap, call),
        Err(error) => DispatchOutcome::Error {
            message: format_parse_error(&error),
        },
    }
}

fn format_parse_error(error: &ParseError) -> String {
    format!("teammux: {error}")
}

/// The process-boundary entry point `src/bin/teammux.rs` calls: loads the
/// real idmap and herdr client from the environment, dispatches, prints the
/// outcome, and returns a faithful exit code.
pub fn run(argv: &[String]) -> ExitCode {
    let idmap = match IdMap::load_from_env() {
        Ok(idmap) => idmap,
        Err(error) => {
            eprintln!("teammux: {error}");
            return ExitCode::FAILURE;
        }
    };
    let herdr = crate::herdr::HerdrClient::from_env();
    match execute(&herdr, &idmap, argv) {
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
    use crate::herdr::test_support::FakeHerdr;
    use crate::herdr::PaneInfo;
    use crate::tmuxargs::GlobalFlags;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn temp_idmap(entries: &[(&str, &str)]) -> IdMap {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock should follow Unix epoch")
            .as_nanos();
        let dir = env::temp_dir().join(format!(
            "teammux-dispatch-tests-{}-{nanos}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp idmap test dir");
        let path: PathBuf = dir.join("state.json");
        for (tmux_id, herdr_id) in entries {
            IdMap::insert(&path, *tmux_id, *herdr_id).expect("seed idmap fixture");
        }
        IdMap::load(&path).expect("load seeded idmap fixture")
    }

    fn call(verb: Verb) -> tmuxargs::ParsedCall {
        tmuxargs::ParsedCall {
            globals: GlobalFlags::default(),
            verb,
        }
    }

    fn pane(pane_id: &str, tab_id: Option<&str>) -> PaneInfo {
        PaneInfo {
            pane_id: pane_id.to_owned(),
            workspace_id: "w1A".to_owned(),
            tab_id: tab_id.map(str::to_owned),
            agent: None,
            agent_id: None,
            agent_session: None,
            agent_status: None,
            cwd: None,
        }
    }

    #[test]
    fn show_mouse_probe_returns_static_off() {
        let idmap = temp_idmap(&[]);
        assert_eq!(
            dispatch(&FakeHerdr::default(), &idmap, call(Verb::ShowMouse)),
            DispatchOutcome::Ok {
                stdout: "off\n".to_owned()
            }
        );
    }

    #[test]
    fn show_focus_events_probe_returns_static_zero() {
        let idmap = temp_idmap(&[]);
        assert_eq!(
            dispatch(&FakeHerdr::default(), &idmap, call(Verb::ShowFocusEvents)),
            DispatchOutcome::Ok {
                stdout: "0\n".to_owned()
            }
        );
    }

    #[test]
    fn client_termtype_probe_returns_static_terminal_type() {
        let idmap = temp_idmap(&[]);
        assert_eq!(
            dispatch(
                &FakeHerdr::default(),
                &idmap,
                call(Verb::DisplayMessage {
                    target: None,
                    field: DisplayField::ClientTermtype,
                })
            ),
            DispatchOutcome::Ok {
                stdout: "xterm-256color\n".to_owned()
            }
        );
    }

    #[test]
    fn recognized_but_unhandled_verbs_are_a_labeled_placeholder_not_a_silent_success() {
        let idmap = temp_idmap(&[]);
        let outcome = dispatch(
            &FakeHerdr::default(),
            &idmap,
            call(Verb::KillPane {
                pane: TmuxId::parse("%0").unwrap(),
            }),
        );
        match outcome {
            DispatchOutcome::Error { message } => {
                assert!(message.contains("not yet implemented"));
            }
            other => panic!("expected a placeholder Error outcome, got {other:?}"),
        }
    }

    #[test]
    fn execute_surfaces_unrecognized_verbs_as_a_loud_error_not_silent_success() {
        let idmap = temp_idmap(&[]);
        let outcome = execute(
            &FakeHerdr::default(),
            &idmap,
            &["frobnicate-pane".to_owned()],
        );
        match outcome {
            DispatchOutcome::Error { message } => {
                assert!(message.starts_with("teammux: unrecognized verb"));
            }
            other => panic!("expected an Error outcome, got {other:?}"),
        }
    }

    #[test]
    fn display_message_window_id_resolves_the_owning_tmux_window() {
        let idmap = temp_idmap(&[("%1", "w1A:p6"), ("@0", "w1A:t1")]);
        let fake = FakeHerdr::default();
        *fake.pane.borrow_mut() = Some(pane("w1A:p6", Some("w1A:t1")));

        let outcome = dispatch(
            &fake,
            &idmap,
            call(Verb::DisplayMessage {
                target: Some(TmuxId::parse("%1").unwrap()),
                field: DisplayField::WindowId,
            }),
        );
        assert_eq!(
            outcome,
            DispatchOutcome::Ok {
                stdout: "@0\n".to_owned()
            }
        );
    }

    #[test]
    fn display_message_window_id_fails_loudly_for_an_unregistered_pane() {
        let idmap = temp_idmap(&[]);
        let outcome = dispatch(
            &FakeHerdr::default(),
            &idmap,
            call(Verb::DisplayMessage {
                target: Some(TmuxId::parse("%9").unwrap()),
                field: DisplayField::WindowId,
            }),
        );
        match outcome {
            DispatchOutcome::Error { message } => assert!(message.contains("unknown tmux id")),
            other => panic!("expected an Error outcome, got {other:?}"),
        }
    }

    #[test]
    fn display_message_window_id_fails_loudly_when_the_tab_has_no_idmap_entry() {
        let idmap = temp_idmap(&[("%1", "w1A:p6")]);
        let fake = FakeHerdr::default();
        *fake.pane.borrow_mut() = Some(pane("w1A:p6", Some("w1A:t1")));

        let outcome = dispatch(
            &fake,
            &idmap,
            call(Verb::DisplayMessage {
                target: Some(TmuxId::parse("%1").unwrap()),
                field: DisplayField::WindowId,
            }),
        );
        match outcome {
            DispatchOutcome::Error { message } => {
                assert!(message.contains("no tmux window id registered"));
            }
            other => panic!("expected an Error outcome, got {other:?}"),
        }
    }

    #[test]
    fn list_pane_ids_emits_tmux_shaped_ids_sorted_numerically() {
        let idmap = temp_idmap(&[("@0", "w1A:t1"), ("%2", "w1A:p8"), ("%1", "w1A:p6")]);
        let fake = FakeHerdr::default();
        *fake.panes.borrow_mut() = vec![
            pane("w1A:p8", Some("w1A:t1")),
            pane("w1A:p6", Some("w1A:t1")),
            pane("w1A:pOther", Some("w1A:t2")),
        ];

        let outcome = dispatch(
            &fake,
            &idmap,
            call(Verb::ListPaneIds {
                window: TmuxId::parse("@0").unwrap(),
            }),
        );
        assert_eq!(
            outcome,
            DispatchOutcome::Ok {
                stdout: "%1\n%2\n".to_owned()
            }
        );
    }

    #[test]
    fn list_pane_ids_fails_loudly_for_an_unregistered_window() {
        let idmap = temp_idmap(&[]);
        let outcome = dispatch(
            &FakeHerdr::default(),
            &idmap,
            call(Verb::ListPaneIds {
                window: TmuxId::parse("@9").unwrap(),
            }),
        );
        match outcome {
            DispatchOutcome::Error { message } => assert!(message.contains("unknown tmux id")),
            other => panic!("expected an Error outcome, got {other:?}"),
        }
    }

    #[test]
    fn list_pane_ids_fails_loudly_on_an_orphan_pane_missing_from_idmap() {
        let idmap = temp_idmap(&[("@0", "w1A:t1")]);
        let fake = FakeHerdr::default();
        *fake.panes.borrow_mut() = vec![pane("w1A:pOrphan", Some("w1A:t1"))];

        let outcome = dispatch(
            &fake,
            &idmap,
            call(Verb::ListPaneIds {
                window: TmuxId::parse("@0").unwrap(),
            }),
        );
        match outcome {
            DispatchOutcome::Error { message } => {
                assert!(message.contains("no tmux id registered"));
            }
            other => panic!("expected an Error outcome, got {other:?}"),
        }
    }

    #[test]
    fn list_pane_ids_returns_empty_output_when_the_tab_has_no_matching_panes() {
        let idmap = temp_idmap(&[("@0", "w1A:t1")]);
        let fake = FakeHerdr::default();
        *fake.panes.borrow_mut() = vec![pane("w1A:pOther", Some("w1A:t2"))];

        let outcome = dispatch(
            &fake,
            &idmap,
            call(Verb::ListPaneIds {
                window: TmuxId::parse("@0").unwrap(),
            }),
        );
        assert_eq!(
            outcome,
            DispatchOutcome::Ok {
                stdout: String::new()
            }
        );
    }

    // `run()` itself (env var + real HerdrClient wiring) is intentionally not
    // unit-tested here: TEAMMUX_STATE_PATH is process-global state, and
    // `cargo test` runs tests in parallel threads within one process —
    // setting/unsetting it from a test would race against
    // `idmap::tests::load_from_env_reports_a_clear_error_when_unset`, which
    // legitimately unsets it. `run()`'s glue is exercised by building and
    // invoking the real binary by hand (see PROGRESS.md) instead.
}
