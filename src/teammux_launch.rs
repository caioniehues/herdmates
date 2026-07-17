//! `herdmates teammux-launch` (issue #85 commit 8; takeover default
//! #103): stand up a lead `claude` session whose native tmux teammate
//! mode is translated to herdr panes by the `teammux` shim.
//!
//! DEFAULT flow (#103, takeover): the CALLING pane becomes the lead —
//! install a `tmux` symlink to the `teammux` binary in a shim bin dir,
//! seed the idmap (`%0` → the calling pane, `@0` → its tab, tab mapping
//! skipped if `pane get` can't resolve it), then **exec** `claude` in
//! place with `PATH` prepended, a fake `TMUX`/`TMUX_PANE` environment
//! (cmux correction: no real tmux anywhere), scoped settings
//! (`teammateMode: tmux`), and `TEAMMUX_STATE_PATH` pointing at the
//! per-lead idmap file. All trailing args pass through to claude.
//!
//! `--split` (recognized as the FIRST arg only, so claude's own flags
//! are never swallowed) keeps the original behavior: split a new herdr
//! pane off the caller and launch the lead there via `pane run`.
//!
//! Command construction is pure and unit-tested; `launch` takes the
//! state root as a parameter (never reads env internally — same
//! testability rule as `pump.rs`/`audit.rs`), so tests use a tempdir and
//! never touch process-global environment.

use crate::herdr::{HerdrApi, HerdrClient, HerdrError};
use crate::idmap::{IdMap, STATE_PATH_ENV};
use crate::paths::{self, PathError};
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Fake `$TMUX` value: `socket,pid,session`. Claude Code only checks
/// presence, but the shape matches real tmux for any naive parser.
pub const FAKE_TMUX: &str = "teammux,0,0";
/// The lead pane's fake tmux pane id, pre-registered in the idmap.
pub const LEAD_TMUX_PANE: &str = "%0";
/// The lead tab's fake tmux window id, pre-registered in the idmap.
pub const LEAD_TMUX_WINDOW: &str = "@0";
/// Scoped Claude Code settings enabling native split-pane teammate mode
/// (no `--teammate-mode` CLI flag exists as of claude 2.1.211).
pub const LEAD_SETTINGS: &str = r#"{"teammateMode":"tmux"}"#;

#[derive(Debug, Error)]
pub enum TeammuxLaunchError {
    #[error("teammux-launch must run inside a herdr pane (HERDR_PANE_ID unset)")]
    NoCallerPane,
    #[error("teammux binary not found next to herdmates at {0}")]
    ShimBinaryMissing(PathBuf),
    #[error(transparent)]
    Path(#[from] PathError),
    #[error(transparent)]
    Herdr(#[from] HerdrError),
    #[error(transparent)]
    IdMap(#[from] crate::idmap::IdMapError),
    #[error("shim install failed: {0}")]
    Io(#[from] io::Error),
}

/// The environment pairs the lead `claude` process must see.
pub fn lead_env(
    shim_bin_dir: &Path,
    state_path: &Path,
    current_path: &str,
) -> Vec<(String, String)> {
    vec![
        (
            "PATH".to_owned(),
            format!("{}:{current_path}", shim_bin_dir.display()),
        ),
        ("TMUX".to_owned(), FAKE_TMUX.to_owned()),
        ("TMUX_PANE".to_owned(), LEAD_TMUX_PANE.to_owned()),
        (STATE_PATH_ENV.to_owned(), state_path.display().to_string()),
    ]
}

/// Compose the single shell command line submitted to the lead pane via
/// `pane run`: `env 'K=V'... claude --settings '<json>' [extra args]`.
/// (`pane run` cannot set environment, so `env` carries it.)
pub fn lead_command_line(env_pairs: &[(String, String)], claude_args: &[String]) -> String {
    let mut parts = vec!["env".to_owned()];
    for (key, value) in env_pairs {
        parts.push(shell_quote(&format!("{key}={value}")));
    }
    parts.push("claude".to_owned());
    parts.push("--settings".to_owned());
    parts.push(shell_quote(LEAD_SETTINGS));
    for arg in claude_args {
        parts.push(shell_quote(arg));
    }
    parts.join(" ")
}

/// Install (or refresh) the `tmux` symlink in `shim_bin_dir`, pointing at
/// the real `teammux` binary. Idempotent.
#[cfg(unix)]
pub fn install_shim(shim_bin_dir: &Path, teammux_binary: &Path) -> io::Result<PathBuf> {
    std::fs::create_dir_all(shim_bin_dir)?;
    let link = shim_bin_dir.join("tmux");
    match std::fs::remove_file(&link) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    std::os::unix::fs::symlink(teammux_binary, &link)?;
    Ok(link)
}

/// Locate the `teammux` binary: a sibling of the running `herdmates`
/// executable (both are workspace bin targets, installed side by side).
pub fn sibling_teammux(current_exe: &Path) -> Result<PathBuf, TeammuxLaunchError> {
    let candidate = current_exe
        .parent()
        .map(|dir| dir.join("teammux"))
        .unwrap_or_default();
    if candidate.is_file() {
        Ok(candidate)
    } else {
        Err(TeammuxLaunchError::ShimBinaryMissing(candidate))
    }
}

/// Pure: split `--split` (first arg only) off the passthrough claude args.
pub(crate) fn parse_launch_args(args: &[String]) -> (bool, &[String]) {
    match args.first().map(String::as_str) {
        Some("--split") => (true, &args[1..]),
        _ => (false, args),
    }
}

/// Pure: the argv tail claude is exec'd with in takeover mode.
pub(crate) fn lead_exec_args(claude_args: &[String]) -> Vec<String> {
    let mut argv = vec!["--settings".to_owned(), LEAD_SETTINGS.to_owned()];
    argv.extend(claude_args.iter().cloned());
    argv
}

/// `herdmates teammux-launch [--split] [claude args...]`
pub fn teammux_launch_command(args: &[String]) -> Result<(), TeammuxLaunchError> {
    let caller_pane =
        std::env::var("HERDR_PANE_ID").map_err(|_| TeammuxLaunchError::NoCallerPane)?;
    let herdr = HerdrClient::from_env();
    let state_root = paths::state_dir()?.join("teammux");
    let current_exe = std::env::current_exe()?;
    let teammux_binary = sibling_teammux(&current_exe)?;
    let current_path = std::env::var("PATH").unwrap_or_default();
    let (split, claude_args) = parse_launch_args(args);
    if split {
        launch(
            &herdr,
            &caller_pane,
            &state_root,
            &teammux_binary,
            &current_path,
            claude_args,
            &mut std::io::stdout(),
        )
    } else {
        // Takeover (#103 default): this pane becomes the lead. The tab
        // mapping degrades to pane-only when unresolvable — window-level
        // shim verbs then miss `@0`, pane-level ones still work.
        let lead_tab_id = herdr
            .pane_get(&caller_pane)
            .ok()
            .and_then(|pane| pane.tab_id);
        let env_pairs = seed_lead_state(
            &state_root,
            &teammux_binary,
            &current_path,
            &caller_pane,
            lead_tab_id.as_deref(),
        )?;
        // exec only returns on failure.
        Err(exec_lead(&env_pairs, claude_args))
    }
}

/// Shared lead-state seeding for both modes: idmap (`%0`/`@0`), shim
/// symlink, and the composed lead environment.
fn seed_lead_state(
    state_root: &Path,
    teammux_binary: &Path,
    current_path: &str,
    lead_pane_id: &str,
    lead_tab_id: Option<&str>,
) -> Result<Vec<(String, String)>, TeammuxLaunchError> {
    std::fs::create_dir_all(state_root)?;
    let state_path = state_root.join(format!("{}.json", lead_pane_id.replace(':', "_")));
    IdMap::insert(&state_path, LEAD_TMUX_PANE, lead_pane_id)?;
    if let Some(tab_id) = lead_tab_id {
        IdMap::insert(&state_path, LEAD_TMUX_WINDOW, tab_id)?;
    }
    let shim_bin_dir = state_root.join("bin");
    #[cfg(unix)]
    install_shim(&shim_bin_dir, teammux_binary)?;
    Ok(lead_env(&shim_bin_dir, &state_path, current_path))
}

/// Replace this process with the lead `claude` (takeover mode). Returns
/// only when exec itself fails.
#[cfg(unix)]
fn exec_lead(env_pairs: &[(String, String)], claude_args: &[String]) -> TeammuxLaunchError {
    use std::os::unix::process::CommandExt;
    let mut command = std::process::Command::new("claude");
    command.args(lead_exec_args(claude_args));
    for (key, value) in env_pairs {
        command.env(key, value);
    }
    TeammuxLaunchError::Io(command.exec())
}

#[cfg(not(unix))]
fn exec_lead(_env_pairs: &[(String, String)], _claude_args: &[String]) -> TeammuxLaunchError {
    TeammuxLaunchError::Io(io::Error::other(
        "takeover mode requires unix exec; use --split",
    ))
}

fn launch(
    herdr: &impl HerdrApi,
    caller_pane: &str,
    state_root: &Path,
    teammux_binary: &Path,
    current_path: &str,
    claude_args: &[String],
    out: &mut impl io::Write,
) -> Result<(), TeammuxLaunchError> {
    let lead = herdr.pane_split_pane(caller_pane, "right", None)?;

    let env_pairs = seed_lead_state(
        state_root,
        teammux_binary,
        current_path,
        &lead.pane_id,
        lead.tab_id.as_deref(),
    )?;
    let command_line = lead_command_line(&env_pairs, claude_args);
    herdr.pane_run(&lead.pane_id, &command_line)?;

    writeln!(
        out,
        "lead pane {} launched (state {})",
        lead.pane_id,
        state_root
            .join(format!("{}.json", lead.pane_id.replace(':', "_")))
            .display()
    )?;
    Ok(())
}

fn shell_quote(argument: &str) -> String {
    format!("'{}'", argument.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herdr::test_support::FakeHerdr;
    use crate::herdr::PaneInfo;

    fn lead_pane() -> PaneInfo {
        PaneInfo {
            pane_id: "w1A:p9".to_owned(),
            workspace_id: "w1A".to_owned(),
            tab_id: Some("w1A:t3".to_owned()),
            agent: None,
            agent_id: None,
            agent_session: None,
            agent_status: None,
            cwd: None,
        }
    }

    #[test]
    fn lead_env_prepends_shim_dir_and_sets_fake_tmux() {
        let pairs = lead_env(
            Path::new("/state/bin"),
            Path::new("/state/lead.json"),
            "/usr/bin",
        );
        assert_eq!(
            pairs[0],
            ("PATH".to_owned(), "/state/bin:/usr/bin".to_owned())
        );
        assert!(pairs.contains(&("TMUX".to_owned(), FAKE_TMUX.to_owned())));
        assert!(pairs.contains(&("TMUX_PANE".to_owned(), "%0".to_owned())));
        assert!(pairs.contains(&(STATE_PATH_ENV.to_owned(), "/state/lead.json".to_owned())));
    }

    #[test]
    fn parse_launch_args_strips_leading_split_flag_only() {
        let args = vec!["--split".to_owned(), "--resume".to_owned()];
        let (split, rest) = parse_launch_args(&args);
        assert!(split);
        assert_eq!(rest, ["--resume".to_owned()]);

        // `--split` NOT in first position belongs to claude, never to us.
        let args = vec!["--resume".to_owned(), "--split".to_owned()];
        let (split, rest) = parse_launch_args(&args);
        assert!(!split);
        assert_eq!(rest, args.as_slice());

        let (split, rest) = parse_launch_args(&[]);
        assert!(!split);
        assert!(rest.is_empty());
    }

    #[test]
    fn lead_exec_args_prepend_settings_then_pass_through() {
        let argv = lead_exec_args(&["--resume".to_owned(), "abc".to_owned()]);
        assert_eq!(
            argv,
            [
                "--settings".to_owned(),
                LEAD_SETTINGS.to_owned(),
                "--resume".to_owned(),
                "abc".to_owned(),
            ]
        );
    }

    #[test]
    fn lead_command_line_quotes_env_and_settings() {
        let pairs = vec![("TMUX".to_owned(), FAKE_TMUX.to_owned())];
        let line = lead_command_line(&pairs, &["--model".to_owned(), "sonnet".to_owned()]);
        assert_eq!(
            line,
            "env 'TMUX=teammux,0,0' claude --settings '{\"teammateMode\":\"tmux\"}' '--model' 'sonnet'"
        );
    }

    #[cfg(unix)]
    #[test]
    fn launch_splits_seeds_idmap_installs_shim_and_runs_lead() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("test clock should follow Unix epoch")
            .as_nanos();
        let temp = std::env::temp_dir().join(format!(
            "teammux-launch-tests-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&temp).expect("create temp dir");
        let state_root = temp.join("teammux");
        let fake_teammux = temp.join("teammux-bin");
        std::fs::write(&fake_teammux, b"#!/bin/sh\n").expect("fake teammux");
        let herdr = FakeHerdr::default();
        *herdr.split_result.borrow_mut() = Some(lead_pane());
        let mut out = Vec::new();

        launch(
            &herdr,
            "w1A:p1",
            &state_root,
            &fake_teammux,
            "/usr/bin",
            &[],
            &mut out,
        )
        .expect("launch");

        let calls = herdr.calls();
        assert!(calls
            .iter()
            .any(|call| call.starts_with("pane_split_pane:w1A:p1:right")));
        let run_call = calls
            .iter()
            .find(|call| call.starts_with("pane_run:w1A:p9:"))
            .expect("lead pane_run");
        assert!(run_call.contains("teammateMode"));
        assert!(run_call.contains("TMUX=teammux,0,0"));

        let state_path = state_root.join("w1A_p9.json");
        let map = IdMap::load(&state_path).expect("idmap");
        assert_eq!(map.lookup("%0"), Some("w1A:p9"));
        assert_eq!(map.lookup("@0"), Some("w1A:t3"));
        assert!(state_root.join("bin/tmux").exists());

        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("w1A:p9"));
    }

    #[test]
    fn sibling_teammux_missing_is_loud() {
        let error = sibling_teammux(Path::new("/nonexistent/herdmates")).unwrap_err();
        assert!(matches!(error, TeammuxLaunchError::ShimBinaryMissing(_)));
    }
}
