//! `herdmates why|deadlocks|roster` — v3.1 lead-facing skill verbs (issue
//! #122, ADR-0015 "brain layer" §Decision point 2: "project-level skill
//! verbs — `why <agent>`, `deadlocks`, `roster` — resolving the team
//! from the lead's own session id"). This is the pull half of the brain
//! layer; the push half (hook enrichment) is a separate, unbuilt spike
//! (ADR-0015 §Rollout, v3.1+).
//!
//! ## Team resolution — never a `--team` flag, never a guess
//!
//! Every verb resolves its team from the CALLING process's own
//! `CLAUDE_CODE_SESSION_ID` (live-verified present in every Claude Code
//! subprocess, including a Bash-tool child, 2026-08-20 — `env | grep
//! CLAUDE` inside a running session), matched against every team's
//! `leadSessionId` the exact way [`team_hook::resolve_team_bucket`]
//! matches a hook payload's `session_id` — same field, same semantics,
//! reused enumeration ([`team_hook::discover_teams_at`]). A session that
//! isn't registered as any team's lead (a teammate session, or no team
//! at all) gets an honest "not a team lead" error, never a silent guess
//! — same never-guess doctrine as [`gather::resolve_team`]'s ambiguity
//! error and the signal engine's reason-less `Waiting` degrade
//! (ADR-0013).
//!
//! ## Output shape
//!
//! Plain dense text, meant for a model to read (not a human TUI): no
//! tables, no color, no box-drawing. Every fact traces back to
//! `gather.rs`/`signal_engine.rs`/`teamfiles.rs` — this module computes
//! nothing about team-file shape or waiting-reason precedence itself.
//!
//! ## 5-state vocabulary (amq-noc pattern, per issue #122)
//!
//! [`FiveState`] is a coarser display layer over the signal engine's
//! four-class [`WaitingReason`] taxonomy, not a replacement for it —
//! `why`'s output always shows both. See [`five_state`] for the mapping.

use crate::gather::{self, GatherPaths, TaskDisplay, TeammateFacts};
use crate::signal_engine::{self, AgentActivity, StalledThresholds, StalledTier, WaitingReason};
use crate::team_hook;
use std::time::SystemTime;
use thiserror::Error;

// ─── team resolution ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TeamResolveError {
    #[error(
        "CLAUDE_CODE_SESSION_ID is not set — brain verbs only work invoked from inside a running Claude Code session"
    )]
    NoSessionId,
    #[error(
        "session {0} is not registered as any team's lead — brain verbs resolve strictly from the calling session's own id (ADR-0015), never a guess; run this from the team lead, not a teammate"
    )]
    NotATeamLead(String),
}

/// Pure: match `session_id` against `teams`' `(team_directory_name,
/// lead_session_id)` pairs (same shape [`team_hook::discover_teams_at`]
/// produces). No I/O, no env — the impure shell below owns both.
pub(crate) fn resolve_team_by_session(
    session_id: Option<&str>,
    teams: &[(String, Option<String>)],
) -> Result<String, TeamResolveError> {
    let session_id = session_id
        .filter(|id| !id.is_empty())
        .ok_or(TeamResolveError::NoSessionId)?;
    teams
        .iter()
        .find(|(_, lead)| lead.as_deref() == Some(session_id))
        .map(|(team, _)| team.clone())
        .ok_or_else(|| TeamResolveError::NotATeamLead(session_id.to_owned()))
}

fn own_session_id() -> Option<String> {
    std::env::var("CLAUDE_CODE_SESSION_ID")
        .ok()
        .filter(|id| !id.is_empty())
}

/// Which team the CALLING session leads.
///
/// The recorded `leadSessionId` is tried first and answers for a lead that
/// has not resumed. When it does not (Defect E: a Claude Code session id
/// rotates on resume or `/clear` while the team config keeps the original),
/// the lead's own teammates are asked — they carry `--parent-session-id`,
/// which still names this session. Without that fallback a resumed lead is
/// told it leads nothing and loses `why`/`roster` outright, with its
/// teammates visibly running.
///
/// The fallback is attempted only for the `NotATeamLead` case: a missing
/// `CLAUDE_CODE_SESSION_ID` is a different error and there is nothing to
/// match on. It costs one `pane process-info` per teammate-looking pane, and
/// only on the path that was previously a hard error.
fn resolve_own_team<H: crate::herdr::HerdrApi>(
    paths: &GatherPaths,
    herdr: &H,
) -> Result<String, TeamResolveError> {
    let teams = team_hook::discover_teams_at(Some(&paths.teams_root));
    let session_id = own_session_id();
    match resolve_team_by_session(session_id.as_deref(), &teams) {
        Ok(team) => Ok(team),
        Err(TeamResolveError::NotATeamLead(id)) => gather::recover_team_by_lead_session(herdr, &id)
            .ok_or(TeamResolveError::NotATeamLead(id)),
        Err(other) => Err(other),
    }
}

// ─── 5-state vocabulary ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FiveState {
    Online,
    NeedsYou,
    Blocked,
    Waiting,
    Stale,
}

impl FiveState {
    /// `pub(crate)`, not private: issue #124 (ADR-0015 ambient layer)
    /// reuses this exact vocabulary for sidebar tokens — single-source
    /// rule #90, sidebar and brain verbs must never disagree.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::NeedsYou => "needs-you",
            Self::Blocked => "blocked",
            Self::Waiting => "waiting",
            Self::Stale => "stale",
        }
    }
}

/// Pure: coarsen the engine's four-class [`WaitingReason`] (plus the raw
/// `agent_status`, needed to tell "actively working" apart from
/// "finished a turn, nothing else to report") into amq-noc's five-state
/// vocabulary. `PermissionPrompt` → `needs-you` (a human decision is the
/// blocker); `BlockedOnDependency` → `blocked` (blocked on other work,
/// not on a human); hard-tier `Stalled` → `stale`; soft-tier `Stalled`
/// and the unbadged/reason-less classes → `waiting`, unless the raw
/// status says the agent is actively `Working` right now, which reads
/// as `online`. Total — every `WaitingReason` variant maps somewhere,
/// no panic branch.
pub(crate) fn five_state(status: AgentActivity, reason: WaitingReason) -> FiveState {
    match reason {
        WaitingReason::PermissionPrompt => FiveState::NeedsYou,
        WaitingReason::BlockedOnDependency => FiveState::Blocked,
        WaitingReason::Stalled {
            tier: StalledTier::Stalled,
            ..
        } => FiveState::Stale,
        WaitingReason::Stalled {
            tier: StalledTier::Quiet,
            ..
        } => FiveState::Waiting,
        WaitingReason::TurnComplete | WaitingReason::Waiting => {
            if status == AgentActivity::Working {
                FiveState::Online
            } else {
                FiveState::Waiting
            }
        }
    }
}

// ─── why <agent> ────────────────────────────────────────────────────────────

/// Pure: exact match on `name` or `agent_id` — the same two identifiers
/// [`teamfiles::Member`] carries. No fuzzy matching: an honest miss beats
/// a wrong pick (same doctrine as team resolution above).
pub(crate) fn find_teammate<'a>(
    members: &'a [TeammateFacts],
    needle: &str,
) -> Option<&'a TeammateFacts> {
    members
        .iter()
        .find(|member| member.name == needle || member.agent_id == needle)
}

/// Pure: dense plain-text explanation of one teammate's waiting reason —
/// the state-vocabulary label, the engine's precise class, and every raw
/// fact that could have grounded it (never just the winning one — a
/// model asking "why" wants the evidence, not only the verdict).
pub(crate) fn render_why(
    team: &str,
    member: &TeammateFacts,
    thresholds: &StalledThresholds,
) -> String {
    let reason = signal_engine::classify(&member.facts, thresholds);
    let state = five_state(member.facts.agent_status, reason);
    let mut out = format!(
        "{} ({}) in team {} — {}: {}\n",
        member.name,
        if member.is_lead { "lead" } else { "member" },
        team,
        state.label(),
        describe_reason(reason),
    );
    out.push_str(&format!(
        "  agent_status: {:?}\n",
        member.facts.agent_status
    ));
    out.push_str(&format!(
        "  owned task blocked-by-incomplete: {}\n",
        member.facts.owned_task_blocked_by_incomplete
    ));
    out.push_str(&format!(
        "  transcript activity: {}\n",
        match member.facts.seconds_since_transcript_activity {
            Some(secs) => format!("{secs}s ago"),
            None => "unknown (no transcript whose ownership this member could be proved for: an in-process teammate has no pane, a split-pane teammate's pane failed the transcript identity check, or the lead's recorded session no longer resolves to its pane — gather.rs module doc)".to_owned(),
        }
    ));
    out.push_str(&format!(
        "  unread inbox: {}\n",
        match member.facts.seconds_since_unread_inbox {
            Some(secs) => format!("oldest unread {secs}s ago"),
            None => "none".to_owned(),
        }
    ));
    out.push_str(&format!(
        "  pane: {}",
        member.pane_id.as_deref().unwrap_or("unresolved")
    ));
    out
}

fn describe_reason(reason: WaitingReason) -> String {
    match reason {
        WaitingReason::PermissionPrompt => "permission-prompt (pane-backed, herdr agent_status Blocked)".to_owned(),
        WaitingReason::BlockedOnDependency => "blocked-on-dependency (owns a task with an incomplete blockedBy entry)".to_owned(),
        WaitingReason::Stalled { tier: StalledTier::Quiet, secs } => format!("quiet {}m (soft stalled tier)", secs / 60),
        WaitingReason::Stalled { tier: StalledTier::Stalled, secs } => format!("stalled {}m (hard stalled tier)", secs / 60),
        WaitingReason::TurnComplete => "turn-complete (not waiting on anything the engine can name)".to_owned(),
        WaitingReason::Waiting => "reason unresolved — no pane-backed status and no liveness signal grounds a class (never-wrong-reason degrade, ADR-0013)".to_owned(),
    }
}

fn render_unknown_teammate(team: &str, needle: &str, members: &[TeammateFacts]) -> String {
    let known: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
    format!(
        "no teammate named or id-matching \"{needle}\" in team {team}\nknown members: {}",
        if known.is_empty() {
            "(none)".to_owned()
        } else {
            known.join(", ")
        }
    )
}

// ─── deadlocks ──────────────────────────────────────────────────────────────

/// A task suspected — never asserted — of the documented native
/// status-lag bug: work that's actually done but whose file never flips
/// to `completed`, wedging every dependent behind it (ADR-0015 §Decision
/// point 6, CONTEXT.md's "Status-lag deadlock"). This is a heuristic,
/// not a fact the engine can ground: nothing in the task-file schema
/// (#88) records semantic completion, only the `status` field itself —
/// the same field this bug leaves stale. [`render_deadlocks`] always
/// labels these as suspicions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeadlockSuspect {
    pub task_id: String,
    pub subject: Option<String>,
    pub owner: Option<String>,
    pub seconds_stale: u64,
    pub blocked_dependents: Vec<String>,
}

/// Pure: an `in_progress` task whose file hasn't changed in at least
/// `stalled_secs` (reuses the signal engine's own hard-stalled threshold
/// — the same bar that grounds a teammate's `Stalled{Stalled}` class, so
/// "this task's owner would also read as stale" is the honest bar for
/// "this task looks abandoned too") AND has at least one other
/// non-completed task naming it in `blockedBy` is a suspect. A stalled
/// task with no dependent isn't reported — nothing downstream is wedged
/// by it, so it isn't a *deadlock* suspect even if it's stale (that's
/// what the per-teammate `Stalled` badge already covers).
pub(crate) fn detect_status_lag(tasks: &[TaskDisplay], stalled_secs: u64) -> Vec<DeadlockSuspect> {
    tasks
        .iter()
        .filter(|task| task.status == "in_progress")
        .filter_map(|task| {
            let seconds_stale = task
                .seconds_since_modified
                .filter(|secs| *secs >= stalled_secs)?;
            let blocked_dependents: Vec<String> = tasks
                .iter()
                .filter(|other| {
                    other.status != "completed" && other.blocked_by.iter().any(|id| id == &task.id)
                })
                .map(|other| other.id.clone())
                .collect();
            if blocked_dependents.is_empty() {
                return None;
            }
            Some(DeadlockSuspect {
                task_id: task.id.clone(),
                subject: task.subject.clone(),
                owner: task.owner.clone(),
                seconds_stale,
                blocked_dependents,
            })
        })
        .collect()
}

/// Pure: dense plain-text rendering of [`detect_status_lag`]'s output.
pub(crate) fn render_deadlocks(team: &str, suspects: &[DeadlockSuspect]) -> String {
    if suspects.is_empty() {
        return format!("no suspected status-lag deadlocks in team {team}");
    }
    let mut out = format!(
        "{} suspected status-lag deadlock(s) in team {team}:\n",
        suspects.len()
    );
    for suspect in suspects {
        out.push_str(&format!(
            "  task {} \"{}\" (owner: {}) — in_progress, unmodified {}m, blocking {} dependent(s): {}\n",
            suspect.task_id,
            suspect.subject.as_deref().unwrap_or("(no subject)"),
            suspect.owner.as_deref().unwrap_or("(unowned)"),
            suspect.seconds_stale / 60,
            suspect.blocked_dependents.len(),
            suspect.blocked_dependents.join(", "),
        ));
    }
    out.push_str("  suspicion only: the task file may be stuck at in_progress after the work actually finished (documented native bug, ADR-0015); verify before assuming a true deadlock.");
    out
}

// ─── reaping ────────────────────────────────────────────────────────────────

/// Pure: is `member` a finished-but-unreaped teammate — one the lead
/// should either give more work or stop?
///
/// A Claude Code teammate is a persistent interactive session: finishing
/// its task does not end it. Two were measured alive at 3h58m and 3h14m
/// after reporting back (findings §2), and `TaskStop` is the only thing
/// that reaps one — pane closed, idmap entry dropped, registry row deleted,
/// all verified live. Nothing invoked it because nothing told the lead a
/// teammate was done. This predicate is that signal.
///
/// Three conditions, and each one is doing work:
///
/// - **Not the lead.** Stopping the lead is not what a reap hint means.
/// - **`FiveState::Stale`**, i.e. `signal_engine`'s HARD stalled tier: the
///   teammate's own transcript has not moved for `stalled_secs`. The
///   classification is not re-derived here — single-source rule #90, the
///   engine owns it and the sidebar sees the same verdict.
/// - **A resolved pane.** Without one nothing has confirmed this member is
///   still running: the row may be a leftover whose pane was closed by
///   hand, or herdr may just not have answered. Claiming a live session
///   either way would be a guess.
///
/// Deliberately NOT part of this: rewriting or purging the member's
/// registry row. The teammate is genuinely alive and the lead can still
/// message it — the record is honest and falsifying it would be worse than
/// the symptom (findings §3). This only surfaces; a human or the lead acts.
pub(crate) fn reapable(member: &TeammateFacts, state: FiveState) -> bool {
    !member.is_lead && member.pane_id.is_some() && matches!(state, FiveState::Stale)
}

/// Pure: how many of `members` are finished-but-unreaped teammates.
pub(crate) fn reapable_count(members: &[TeammateFacts], thresholds: &StalledThresholds) -> usize {
    members
        .iter()
        .filter(|member| {
            let reason = signal_engine::classify(&member.facts, thresholds);
            reapable(member, five_state(member.facts.agent_status, reason))
        })
        .count()
}

// ─── roster ─────────────────────────────────────────────────────────────────

/// Pure: dense plain-text roster — one line per member, state + engine
/// reason + pane, plus a reap hint for every teammate that has finished
/// and not been stopped.
///
/// Two notes can follow the list and each is conditional on what the
/// roster actually contains, so neither is boilerplate:
///
/// - a member with no resolved pane really is degraded — nothing grounded
///   its state (see `gather::gather_team`'s ownership rules); and
/// - a non-lead member that DID resolve still carries `Unknown`
///   `agent_status`, because herdr cannot hold a teammate's agent state at
///   all (`gather::resolve_teammate_pane`'s doc comment has the measured
///   reason). Its liveness is a real read off its own transcript, but the
///   pane-backed classes — `needs-you` (permission prompt), `blocked` —
///   can never fire for it. Saying so is what keeps the difference between
///   "quiet" and "not waiting on a human" visible.
pub(crate) fn render_roster(
    team: &str,
    members: &[TeammateFacts],
    thresholds: &StalledThresholds,
) -> String {
    if members.is_empty() {
        return format!("team {team} has no members");
    }
    let mut out = format!("team {team} — {} member(s):\n", members.len());
    let mut any_unresolved_pane = false;
    let mut any_resolved_non_lead = false;
    let mut reap_count = 0usize;
    for member in members {
        let reason = signal_engine::classify(&member.facts, thresholds);
        let state = five_state(member.facts.agent_status, reason);
        match (member.is_lead, member.pane_id.is_some()) {
            (_, false) => any_unresolved_pane = true,
            (false, true) => any_resolved_non_lead = true,
            (true, true) => {}
        }
        let reap_hint = if reapable(member, state) {
            reap_count += 1;
            format!(" — finished? reap it: TaskStop {}", member.name)
        } else {
            String::new()
        };
        out.push_str(&format!(
            "  {} [{}] {} — {}, pane {}{reap_hint}\n",
            member.name,
            if member.is_lead { "lead" } else { "member" },
            state.label(),
            describe_reason(reason),
            member.pane_id.as_deref().unwrap_or("unresolved"),
        ));
    }
    if reap_count > 0 {
        out.push_str(&format!(
            "  {reap_count} teammate(s) finished and still running — a teammate is a persistent session, so nothing ends it but TaskStop. Give it more work or stop it; do not just close the pane, which kills a live session the lead can still message.\n"
        ));
    }
    if any_resolved_non_lead {
        out.push_str("  note: a non-lead member's liveness is read off its own transcript, but herdr holds no agent_status for a teammate pane (gather.rs: resolve_teammate_pane) — so needs-you/blocked can never show for one, however true they might be.\n");
    }
    if any_unresolved_pane {
        out.push_str("  note: a member shown as pane `unresolved` grounded nothing — no pane means no confirmed live session, so its state/reason is an honest degrade rather than a real read.\n");
    }
    out.trim_end().to_owned()
}

// ─── command wiring (impure shell) ─────────────────────────────────────────

#[derive(Debug, Error)]
pub enum BrainError {
    #[error(transparent)]
    TeamResolve(#[from] TeamResolveError),
    #[error("could not resolve herdmates state paths (HOME unset?)")]
    UnresolvedGatherPaths,
    #[error("usage: herdmates why <agent-name-or-id>")]
    MissingAgentArg,
    #[error("{0}")]
    UnknownTeammate(String),
}

fn gather_paths() -> Result<GatherPaths, BrainError> {
    GatherPaths::from_env().ok_or(BrainError::UnresolvedGatherPaths)
}

/// `herdmates why <agent>` (issue #122, ADR-0015 brain layer pull path).
pub fn why_command(args: &[String]) -> Result<(), BrainError> {
    let agent = args.first().ok_or(BrainError::MissingAgentArg)?;
    let paths = gather_paths()?;
    let herdr = crate::herdr::HerdrClient::from_env();
    let team = resolve_own_team(&paths, &herdr)?;
    let thresholds = StalledThresholds::default();
    let members = gather::gather_team(&paths, &team, &herdr, SystemTime::now());
    match find_teammate(&members, agent) {
        Some(member) => {
            println!("{}", render_why(&team, member, &thresholds));
            Ok(())
        }
        None => Err(BrainError::UnknownTeammate(render_unknown_teammate(
            &team, agent, &members,
        ))),
    }
}

/// `herdmates deadlocks` (issue #122, ADR-0015 §Decision point 6).
pub fn deadlocks_command(_args: &[String]) -> Result<(), BrainError> {
    let paths = gather_paths()?;
    let herdr = crate::herdr::HerdrClient::from_env();
    let team = resolve_own_team(&paths, &herdr)?;
    let tasks = gather::team_task_displays(&paths, &team, SystemTime::now());
    let suspects = detect_status_lag(&tasks, StalledThresholds::default().stalled_secs);
    println!("{}", render_deadlocks(&team, &suspects));
    Ok(())
}

/// `herdmates roster` (issue #122, ADR-0015 brain layer pull path).
pub fn roster_command(_args: &[String]) -> Result<(), BrainError> {
    let paths = gather_paths()?;
    let herdr = crate::herdr::HerdrClient::from_env();
    let team = resolve_own_team(&paths, &herdr)?;
    let thresholds = StalledThresholds::default();
    let members = gather::gather_team(&paths, &team, &herdr, SystemTime::now());
    println!("{}", render_roster(&team, &members, &thresholds));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal_engine::ObservedFacts;

    // ── resolve_team_by_session ─────────────────────────────────────────────

    #[test]
    fn no_session_id_is_an_explicit_error() {
        let teams = [("team-a".to_owned(), Some("s1".to_owned()))];
        assert_eq!(
            resolve_team_by_session(None, &teams),
            Err(TeamResolveError::NoSessionId)
        );
    }

    #[test]
    fn empty_session_id_is_treated_as_absent() {
        let teams = [("team-a".to_owned(), Some("s1".to_owned()))];
        assert_eq!(
            resolve_team_by_session(Some(""), &teams),
            Err(TeamResolveError::NoSessionId)
        );
    }

    #[test]
    fn matching_lead_session_id_resolves_the_team() {
        let teams = [
            ("team-a".to_owned(), Some("s1".to_owned())),
            ("team-b".to_owned(), Some("s2".to_owned())),
        ];
        assert_eq!(
            resolve_team_by_session(Some("s2"), &teams),
            Ok("team-b".to_owned())
        );
    }

    #[test]
    fn unmatched_session_id_is_not_a_team_lead_error() {
        let teams = [("team-a".to_owned(), Some("s1".to_owned()))];
        assert_eq!(
            resolve_team_by_session(Some("teammate-session"), &teams),
            Err(TeamResolveError::NotATeamLead(
                "teammate-session".to_owned()
            ))
        );
    }

    #[test]
    fn team_with_no_lead_session_id_never_matches() {
        let teams = [("team-a".to_owned(), None)];
        assert_eq!(
            resolve_team_by_session(Some("s1"), &teams),
            Err(TeamResolveError::NotATeamLead("s1".to_owned()))
        );
    }

    // ── five_state ───────────────────────────────────────────────────────────

    #[test]
    fn permission_prompt_is_needs_you() {
        assert_eq!(
            five_state(AgentActivity::Blocked, WaitingReason::PermissionPrompt),
            FiveState::NeedsYou
        );
    }

    #[test]
    fn blocked_on_dependency_is_blocked() {
        assert_eq!(
            five_state(AgentActivity::Idle, WaitingReason::BlockedOnDependency),
            FiveState::Blocked
        );
    }

    #[test]
    fn hard_stalled_tier_is_stale() {
        assert_eq!(
            five_state(
                AgentActivity::Idle,
                WaitingReason::Stalled {
                    tier: StalledTier::Stalled,
                    secs: 600
                }
            ),
            FiveState::Stale
        );
    }

    #[test]
    fn soft_stalled_tier_is_waiting() {
        assert_eq!(
            five_state(
                AgentActivity::Idle,
                WaitingReason::Stalled {
                    tier: StalledTier::Quiet,
                    secs: 300
                }
            ),
            FiveState::Waiting
        );
    }

    #[test]
    fn working_with_turn_complete_is_online() {
        assert_eq!(
            five_state(AgentActivity::Working, WaitingReason::TurnComplete),
            FiveState::Online
        );
    }

    #[test]
    fn idle_with_turn_complete_is_waiting_not_online() {
        assert_eq!(
            five_state(AgentActivity::Idle, WaitingReason::TurnComplete),
            FiveState::Waiting
        );
    }

    #[test]
    fn reason_less_waiting_never_reads_as_online() {
        assert_eq!(
            five_state(AgentActivity::Unknown, WaitingReason::Waiting),
            FiveState::Waiting
        );
    }

    // ── find_teammate ────────────────────────────────────────────────────────

    fn member(name: &str, agent_id: &str, is_lead: bool) -> TeammateFacts {
        TeammateFacts {
            name: name.to_owned(),
            agent_id: agent_id.to_owned(),
            is_lead,
            facts: ObservedFacts::default(),
            pane_id: None,
        }
    }

    #[test]
    fn find_teammate_matches_by_name() {
        let members = [member("alpha", "alpha@t", false)];
        assert_eq!(
            find_teammate(&members, "alpha").map(|m| &m.name),
            Some(&"alpha".to_owned())
        );
    }

    #[test]
    fn find_teammate_matches_by_agent_id() {
        let members = [member("alpha", "alpha@t", false)];
        assert!(find_teammate(&members, "alpha@t").is_some());
    }

    #[test]
    fn find_teammate_is_none_on_a_miss() {
        let members = [member("alpha", "alpha@t", false)];
        assert!(find_teammate(&members, "bob").is_none());
    }

    // ── detect_status_lag ────────────────────────────────────────────────────

    fn task(
        id: &str,
        status: &str,
        seconds_since_modified: Option<u64>,
        blocked_by: &[&str],
    ) -> TaskDisplay {
        TaskDisplay {
            id: id.to_owned(),
            subject: Some(format!("task {id}")),
            status: status.to_owned(),
            owner: Some("someone".to_owned()),
            seconds_since_modified,
            blocked_by: blocked_by.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn stale_in_progress_task_with_blocked_dependent_is_a_suspect() {
        let tasks = [
            task("1", "in_progress", Some(700), &[]),
            task("2", "pending", Some(10), &["1"]),
        ];
        let suspects = detect_status_lag(&tasks, 600);
        assert_eq!(suspects.len(), 1);
        assert_eq!(suspects[0].task_id, "1");
        assert_eq!(suspects[0].blocked_dependents, vec!["2".to_owned()]);
    }

    #[test]
    fn stale_task_with_no_dependent_is_not_a_suspect() {
        let tasks = [task("1", "in_progress", Some(700), &[])];
        assert!(detect_status_lag(&tasks, 600).is_empty());
    }

    #[test]
    fn not_yet_stale_task_is_not_a_suspect_even_with_a_dependent() {
        let tasks = [
            task("1", "in_progress", Some(300), &[]),
            task("2", "pending", Some(10), &["1"]),
        ];
        assert!(detect_status_lag(&tasks, 600).is_empty());
    }

    #[test]
    fn pending_task_is_never_a_suspect_regardless_of_staleness() {
        let tasks = [
            task("1", "pending", Some(700), &[]),
            task("2", "pending", Some(10), &["1"]),
        ];
        assert!(detect_status_lag(&tasks, 600).is_empty());
    }

    #[test]
    fn already_completed_dependent_does_not_count_as_blocked() {
        let tasks = [
            task("1", "in_progress", Some(700), &[]),
            task("2", "completed", Some(10), &["1"]),
        ];
        assert!(detect_status_lag(&tasks, 600).is_empty());
    }

    #[test]
    fn task_with_no_mtime_fact_is_never_a_suspect() {
        let tasks = [
            task("1", "in_progress", None, &[]),
            task("2", "pending", Some(10), &["1"]),
        ];
        assert!(detect_status_lag(&tasks, 600).is_empty());
    }

    #[test]
    fn multiple_dependents_are_all_named() {
        let tasks = [
            task("1", "in_progress", Some(700), &[]),
            task("2", "pending", Some(10), &["1"]),
            task("3", "in_progress", Some(10), &["1"]),
        ];
        let suspects = detect_status_lag(&tasks, 600);
        assert_eq!(suspects.len(), 1);
        assert_eq!(suspects[0].blocked_dependents.len(), 2);
    }

    // ── render_* smoke tests (contract: contains the load-bearing facts) ────

    #[test]
    fn render_why_names_state_and_raw_facts() {
        let facts = ObservedFacts {
            agent_status: AgentActivity::Blocked,
            ..Default::default()
        };
        let m = TeammateFacts {
            name: "alpha".to_owned(),
            agent_id: "alpha@t".to_owned(),
            is_lead: false,
            facts,
            pane_id: Some("%3".to_owned()),
        };
        let rendered = render_why("team-x", &m, &StalledThresholds::default());
        assert!(rendered.contains("alpha"));
        assert!(rendered.contains("needs-you"));
        assert!(rendered.contains("permission-prompt"));
        assert!(rendered.contains("%3"));
    }

    #[test]
    fn render_deadlocks_reports_none_honestly() {
        assert_eq!(
            render_deadlocks("team-x", &[]),
            "no suspected status-lag deadlocks in team team-x"
        );
    }

    #[test]
    fn render_deadlocks_names_task_owner_and_dependents() {
        let suspects = [DeadlockSuspect {
            task_id: "3".to_owned(),
            subject: Some("wire spool wake".to_owned()),
            owner: Some("builder-98".to_owned()),
            seconds_stale: 840,
            blocked_dependents: vec!["4".to_owned()],
        }];
        let rendered = render_deadlocks("team-x", &suspects);
        assert!(rendered.contains("wire spool wake"));
        assert!(rendered.contains("builder-98"));
        assert!(rendered.contains("14m"));
        assert!(rendered.contains("suspicion only"));
    }

    // ── reapability (issue: nothing reaps a finished teammate) ──────────────

    /// A member with a resolved pane and a transcript quiet past the hard
    /// stalled threshold — the shape a finished-but-unstopped split-pane
    /// teammate has.
    fn quiet_teammate(
        name: &str,
        is_lead: bool,
        pane: Option<&str>,
        quiet_secs: u64,
    ) -> TeammateFacts {
        TeammateFacts {
            name: name.to_owned(),
            agent_id: format!("{name}@t"),
            is_lead,
            facts: ObservedFacts {
                seconds_since_transcript_activity: Some(quiet_secs),
                ..ObservedFacts::default()
            },
            pane_id: pane.map(str::to_owned),
        }
    }

    #[test]
    fn a_stale_non_lead_member_with_a_live_pane_is_reapable() {
        let member = quiet_teammate("fix3-imp3093", false, Some("w3:p22"), 3 * 3_600);
        let state = five_state(
            member.facts.agent_status,
            signal_engine::classify(&member.facts, &StalledThresholds::default()),
        );

        assert_eq!(state, FiveState::Stale);
        assert!(reapable(&member, state));
    }

    #[test]
    fn a_lead_is_never_reapable_however_quiet_it_is() {
        let lead = quiet_teammate("team-lead", true, Some("w3:p21"), 3 * 3_600);
        let state = five_state(
            lead.facts.agent_status,
            signal_engine::classify(&lead.facts, &StalledThresholds::default()),
        );

        assert_eq!(state, FiveState::Stale);
        assert!(
            !reapable(&lead, state),
            "stopping the lead is not what a reap hint means"
        );
    }

    #[test]
    fn a_member_whose_pane_never_resolved_is_not_called_reapable() {
        // No pane means nothing confirmed this member is still running —
        // the row may be a leftover, or herdr may simply not have answered.
        // Either way there is no live session to claim, so no hint.
        let member = quiet_teammate("ghost", false, None, 3 * 3_600);
        let state = five_state(
            member.facts.agent_status,
            signal_engine::classify(&member.facts, &StalledThresholds::default()),
        );

        assert!(!reapable(&member, state));
    }

    #[test]
    fn a_member_still_inside_the_stalled_threshold_is_not_reapable() {
        let member = quiet_teammate("busy", false, Some("w3:p22"), 60);
        let state = five_state(
            member.facts.agent_status,
            signal_engine::classify(&member.facts, &StalledThresholds::default()),
        );

        assert_ne!(state, FiveState::Stale);
        assert!(!reapable(&member, state));
    }

    #[test]
    fn render_roster_prints_the_reap_hint_only_for_reapable_members() {
        let members = [
            quiet_teammate("team-lead", true, Some("w3:p21"), 3 * 3_600),
            quiet_teammate("fix3-imp3093", false, Some("w3:p22"), 3 * 3_600),
            quiet_teammate("busy", false, Some("w3:p23"), 30),
        ];

        let rendered = render_roster("team-x", &members, &StalledThresholds::default());

        assert!(
            rendered.contains("TaskStop fix3-imp3093"),
            "the finished teammate gets the reap hint; rendered:\n{rendered}"
        );
        assert!(
            !rendered.contains("TaskStop busy"),
            "a teammate that is still producing output does not; rendered:\n{rendered}"
        );
        assert!(
            !rendered.contains("TaskStop team-lead"),
            "the lead is never a reap target; rendered:\n{rendered}"
        );
    }

    #[test]
    fn render_roster_says_how_many_teammates_are_waiting_to_be_reaped() {
        let members = [
            quiet_teammate("team-lead", true, Some("w3:p21"), 10),
            quiet_teammate("a", false, Some("w3:p22"), 3 * 3_600),
            quiet_teammate("b", false, Some("w3:p23"), 3 * 3_600),
        ];

        let rendered = render_roster("team-x", &members, &StalledThresholds::default());

        assert!(
            rendered.contains("2 teammate(s) finished"),
            "rendered:\n{rendered}"
        );
    }

    #[test]
    fn render_roster_footnotes_members_whose_pane_never_resolved() {
        // Both members here have `pane_id: None`, which after the ownership
        // rules in gather.rs means nothing grounded their state — the note
        // has to say that rather than the old blanket claim that non-lead
        // members can never resolve, which they now can.
        let members = [
            member("team-lead", "lead@t", true),
            member("alpha", "alpha@t", false),
        ];
        let rendered = render_roster("team-x", &members, &StalledThresholds::default());
        assert!(rendered.contains("team-lead"));
        assert!(rendered.contains("alpha"));
        assert!(rendered.contains("no confirmed live session"));
    }

    #[test]
    fn render_roster_footnotes_the_teammate_agent_status_gap_only_when_one_resolved() {
        let unresolved = [member("alpha", "alpha@t", false)];
        assert!(
            !render_roster("team-x", &unresolved, &StalledThresholds::default())
                .contains("herdr holds no agent_status"),
            "the note is about resolved teammates; an unresolved one is a different degrade"
        );

        let resolved = [quiet_teammate("alpha", false, Some("w1:p2"), 30)];
        assert!(
            render_roster("team-x", &resolved, &StalledThresholds::default())
                .contains("herdr holds no agent_status")
        );
    }

    #[test]
    fn render_roster_empty_team_is_honest_not_blank() {
        assert_eq!(
            render_roster("team-x", &[], &StalledThresholds::default()),
            "team team-x has no members"
        );
    }
}
