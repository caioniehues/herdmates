//! herdr-agent-team — Herdr plugin binary.
//!
//! Subcommand surface is fixed by docs/spec.md §1: the CLI half (`spawn`,
//! `status`, `kill`) is invoked by the god session or via manifest actions;
//! the event half (`on-agent-status`) is invoked by Herdr's event hook with
//! HERDR_PLUGIN_EVENT_JSON in the environment.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    match cmd.as_str() {
        "spawn" => todo("spawn: read herdr-team.toml / --agents shorthand, create workspaces, launch workers, generate AGENTS.md (spec §4)"),
        "status" => todo("status: run.toml + live `herdr agent list` join (spec §6)"),
        "kill" => todo("kill: close team workspaces, guard dirty worktrees (spec §6)"),
        "on-agent-status" => todo("event hook: match pane to run, append inbox/events.jsonl, inject pointer line into god pane (spec §5)"),
        "" | "help" | "--help" | "-h" => {
            eprintln!("herdr-agent-team <spawn|status|kill|on-agent-status>");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown subcommand: {other}");
            ExitCode::FAILURE
        }
    }
}

fn todo(what: &str) -> ExitCode {
    eprintln!("not implemented yet — {what}");
    ExitCode::FAILURE
}
