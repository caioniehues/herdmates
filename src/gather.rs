//! Real [`ObservedFacts`] gathering for the signal engine (issue #96 doc
//! comment's promised gather sources; issue #97 stage 2, ADR-0013 §92/§93).
//! `signal_engine::classify` owns no I/O — this module is the caller that
//! reads native team files, herdr agent status, and a session-transcript
//! mtime stat, then hands the result to the engine.
//!
//! All file-system roots are injectable via [`GatherPaths`] so the pure
//! parsing/matching logic here is testable on tempdir fixtures without a
//! live team (no live process, no live herdr, no live Claude Code state).
//!
//! ## Session-id resolution: lead by config, split-pane teammate by pane
//!
//! Live team-config `members[]` entries carry no per-teammate session id
//! field (verified against `docs/research/native-teamfiles-schema-
//! 2026-07-16.md`'s captured 2-teammate sample and re-verified live against
//! every team config on this machine, 2026-07-17) — only the top-level
//! `leadSessionId` resolves a Claude Code session *from the config*. The
//! lead is therefore resolved exactly as before, via `pump::
//! resolve_lead_pane`.
//!
//! What changed 2026-09-03 is that the config is no longer the only
//! source. A SPLIT-PANE teammate's herdr pane carries both halves of the
//! join itself, measured live on four teammates:
//!
//! - `display_agent` is the teammate's NAME — herdmates published it
//!   (`teammux::publish_display_agent`), and no other pane field names a
//!   teammate; and
//! - `agent_session.value` is the teammate's OWN session id, reported by
//!   herdr's official `herdr:claude` integration even though herdr cannot
//!   name the *agent* in that pane (Defect D).
//!
//! Both are still only candidates. [`resolve_teammate_pane`] confirms the
//! pairing against the transcript that session id resolves to, which
//! declares its own `agentName` and `teamName`, and refuses the binding
//! when either disagrees or when two panes claim one name. Anything
//! unverifiable degrades to `None` exactly as before — the never-guess
//! doctrine is unchanged, it simply now has evidence to work with where it
//! previously had none.
//!
//! What has NOT changed: `agent_status` stays `AgentActivity::Unknown` for
//! every non-lead member, because herdr genuinely does not have one for a
//! teammate pane and cannot be given one (see [`resolve_teammate_pane`]'s
//! doc comment for the measured reason). Their liveness comes from the
//! transcript instead, which `signal_engine::classify` grounds the stalled
//! tiers on for exactly that reason. An IN-PROCESS teammate has no pane at
//! all, so it still degrades to `None` on both counts.
//!
//! ## Task ownership matching
//!
//! A task's `owner` field is observed live as either absent, JSON `null`,
//! or an empty string — all three are normalized to the same `None` at
//! parse time (never a naive single-form compare, #89 evidence). A task
//! counts toward [`ObservedFacts::owned_task_blocked_by_incomplete`] for a
//! member only when its normalized owner exactly equals that member's
//! `name` or `agent_id`; unowned tasks (`owner` normalized to `None`)
//! never count for anyone, and a `blockedBy` reference to a task id this
//! gather pass didn't find is never treated as incomplete — under-claiming
//! a block is the honest failure mode here (ADR-0013's "no number to a
//! wrong one"), over-claiming is not.
//!
//! ## Inbox is read-only
//!
//! This module (and the recorder built on it) never writes to an inbox
//! file — no read-flag mutation, no draining. Only [`ObservedFacts::
//! seconds_since_unread_inbox`] is derived, from the oldest entry with
//! `read: false`.

use crate::herdr::{HerdrApi, PaneInfo};
use crate::pump;
use crate::signal_engine::{AgentActivity, ObservedFacts};
use crate::teamfiles;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use thiserror::Error;

/// Injectable roots for every file source this module reads. `teams_root`
/// defaults mirror `pump::default_teams_root` exactly — delegated to it
/// below (issue #115 dedupe) rather than reimplemented; tests always
/// construct this directly against a tempdir.
#[derive(Debug, Clone)]
pub struct GatherPaths {
    pub teams_root: PathBuf,
    pub tasks_root: PathBuf,
    pub projects_root: PathBuf,
}

impl GatherPaths {
    pub fn from_env() -> Option<Self> {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        Some(Self {
            // HOME is confirmed present above, so pump::default_teams_root
            // only fails on a race; fall back to the same default it would
            // have used.
            teams_root: pump::default_teams_root().unwrap_or_else(|_| home.join(".claude/teams")),
            tasks_root: std::env::var_os("HERDMATES_TASKS_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".claude/tasks")),
            projects_root: std::env::var_os("HERDMATES_PROJECTS_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".claude/projects")),
        })
    }
}

/// One teammate's gathered facts, identity-tagged for the recorder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeammateFacts {
    pub name: String,
    pub agent_id: String,
    pub is_lead: bool,
    pub facts: ObservedFacts,
    /// Herdr pane id, when this teammate resolves to one (#99 jump-to-pane
    /// affordance). `None` degrades the affordance to hidden, never a
    /// guess.
    ///
    /// Resolved two ways, per the module doc: the lead through its
    /// config-recorded session id, a split-pane teammate through its own
    /// pane's `display_agent` + `agent_session`, verified against that
    /// session's transcript ([`resolve_teammate_pane`]). An in-process
    /// teammate has no pane and stays `None`.
    pub pane_id: Option<String>,
}

/// Gather every member of `team`'s [`ObservedFacts`] from live sources.
/// Never panics: a missing/malformed team config yields an empty result
/// (nothing to classify), matching every other pass in this crate's
/// degrade-on-missing-file policy (`pump::pump_once`'s callers).
pub fn gather_team<H: HerdrApi>(
    paths: &GatherPaths,
    team: &str,
    herdr: &H,
    now: SystemTime,
) -> Vec<TeammateFacts> {
    let config_path = paths.teams_root.join(team).join("config.json");
    let Ok(config) = teamfiles::read_team_config(&config_path) else {
        return Vec::new();
    };
    let agents = herdr.agent_list().unwrap_or_default();
    // Defect E: fall back to a live teammate's `--parent-session-id` when the
    // config's recorded lead session no longer resolves. Guarded on the team
    // actually having a split-pane teammate — the recovery reads one, and the
    // probe costs a subprocess per candidate pane.
    let lead_pane_id = pump::resolve_lead_pane(&config, &agents).or_else(|| {
        config
            .members
            .iter()
            .any(|member| !member.is_lead && member.backend_type.as_deref() == Some("tmux"))
            .then(|| recover_lead_pane_via_teammates(herdr, team, &agents))
            .flatten()
    });
    let lead_status = lead_pane_id.as_ref().and_then(|pane_id| {
        agents
            .iter()
            .find(|agent| &agent.pane_id == pane_id)
            .and_then(|agent| agent.status.clone())
    });

    let tasks = read_task_files(&paths.tasks_root.join(team));
    let inboxes_dir = paths.teams_root.join(team).join("inboxes");
    // One pane snapshot for the whole pass, like `agent_list` above: the
    // non-lead resolution below is per-member but the source is not.
    let panes = herdr.pane_list(None).unwrap_or_default();

    config
        .members
        .iter()
        .map(|member| {
            let agent_status = if member.is_lead {
                AgentActivity::from_status_str(lead_status.as_deref())
            } else {
                AgentActivity::Unknown
            };

            let owned_task_blocked_by_incomplete =
                any_owned_task_blocked(&tasks, &member.name, &member.agent_id);

            // A non-lead member resolves to a pane only through the
            // verified chain in `resolve_teammate_pane`; its transcript is
            // then the same liveness source the lead uses, read off the
            // path that chain already proved belongs to this teammate.
            let teammate_pane = (!member.is_lead)
                .then(|| resolve_teammate_pane(&panes, &paths.projects_root, team, &member.name))
                .flatten();

            let seconds_since_transcript_activity = if member.is_lead {
                // The lead's recorded `leadSessionId` only names the lead's
                // CURRENT session while it still resolves to the lead's
                // pane. A session id rotates (resume, `/clear`) and the
                // team config keeps the one the team was created with —
                // measured live 2026-09-03 on `session-95cbfad4`, whose
                // config names 95cbfad4… while its lead pane runs
                // 18b67557…. The abandoned session's transcript then sits
                // still while the lead works, so attributing its mtime to
                // the lead claims "stalled" about an agent mid-turn. Same
                // ownership rule `enrich.rs` applies to a payload
                // transcript, and the same one the teammate branch below
                // applies: no verified owner, no fact.
                lead_pane_id
                    .as_ref()
                    .and(config.lead_session_id.as_deref())
                    .and_then(|session_id| {
                        resolve_transcript_mtime(&paths.projects_root, session_id)
                    })
                    .and_then(|mtime| now.duration_since(mtime).ok())
                    .map(|elapsed| elapsed.as_secs())
            } else {
                teammate_pane
                    .as_ref()
                    .and_then(|resolved| std::fs::metadata(&resolved.transcript_path).ok())
                    .and_then(|metadata| metadata.modified().ok())
                    .and_then(|mtime| now.duration_since(mtime).ok())
                    .map(|elapsed| elapsed.as_secs())
            };

            let seconds_since_unread_inbox =
                seconds_since_unread_inbox(&inboxes_dir.join(format!("{}.json", member.name)), now);

            TeammateFacts {
                name: member.name.clone(),
                agent_id: member.agent_id.clone(),
                is_lead: member.is_lead,
                facts: ObservedFacts {
                    agent_status,
                    owned_task_blocked_by_incomplete,
                    seconds_since_transcript_activity,
                    seconds_since_unread_inbox,
                },
                pane_id: if member.is_lead {
                    lead_pane_id.clone()
                } else {
                    teammate_pane.map(|resolved| resolved.pane_id)
                },
            }
        })
        .collect()
}

// ─── Team resolution (#98 board: manifest-launchable single-team default) ─────

#[derive(Debug, Error)]
pub enum ResolveTeamError {
    #[error("no team directories found under {0}")]
    NoTeams(PathBuf),
    #[error("multiple teams found ({0}) — pass --team explicitly")]
    Ambiguous(String),
}

/// Resolve which team a caller should operate on when it wasn't told
/// explicitly (e.g. a `plugin.pane.open` launch, whose argv is fixed at
/// manifest-declaration time and can't carry a per-invocation team name).
///
/// `explicit` always wins when present. Otherwise: exactly one team
/// directory under `paths.teams_root` (identified by containing a
/// `config.json`, not just any subdirectory) resolves silently; zero or
/// more than one is a hard error listing the candidates — same
/// never-guess doctrine as the reason-badge engine (ADR-0013 honesty
/// doctrine): an ambiguous or absent team is never silently picked.
///
/// Candidates are pre-filtered to LIVE teams (#102) before the 0/1/many
/// count decides the outcome — `~/.claude/teams/*/` accumulates one dir
/// per team ever created (dismissal prunes members, not the directory,
/// #102 M1 finding) and never getting filtered would make every stale
/// husk count as ambiguity forever. The filter can only ever narrow the
/// candidate set, so it can only ever turn a would-be wrong/ambiguous
/// pick into a stricter honest error — never produce a wrong pick itself.
pub fn resolve_team(
    paths: &GatherPaths,
    explicit: Option<&str>,
    now: SystemTime,
) -> Result<String, ResolveTeamError> {
    if let Some(team) = explicit {
        return Ok(team.to_owned());
    }
    let mut candidates: Vec<String> = list_team_dirs(&paths.teams_root)
        .into_iter()
        .filter(|team| team_is_live(paths, team, now))
        .collect();
    candidates.sort();
    match candidates.len() {
        1 => Ok(candidates.into_iter().next().expect("len checked as 1")),
        0 => Err(ResolveTeamError::NoTeams(paths.teams_root.clone())),
        _ => Err(ResolveTeamError::Ambiguous(candidates.join(", "))),
    }
}

/// #102: a team dir counts as live only while its lead session has a
/// transcript touched within [`LIVE_TRANSCRIPT_WINDOW`] — `member_count`
/// deliberately isn't a signal (a legit solo-lead team, seconds after
/// `herdr team create` before any teammate joins, would false-negative).
const LIVE_TRANSCRIPT_WINDOW: Duration = Duration::from_secs(60 * 60);

/// Pure half of the liveness check: given an already-resolved transcript
/// mtime (or `None`, meaning no transcript was ever found for the lead
/// session), is it fresh enough. No filesystem access — testable without
/// tempdir fixtures.
fn is_transcript_live(transcript_mtime: Option<SystemTime>, now: SystemTime) -> bool {
    transcript_mtime.is_some_and(|mtime| {
        // A transcript mtime in the future (NTP step-back, clock desync)
        // counts as elapsed-zero — i.e. live. `duration_since` errors on
        // future mtimes; mapping that error to "dead" would silently drop
        // a genuinely live team (2026-07-17 review, finding 8).
        now.duration_since(mtime).unwrap_or(Duration::ZERO) <= LIVE_TRANSCRIPT_WINDOW
    })
}

/// Impure glue: reads `team`'s config to find its lead session, stats
/// that session's transcript, and runs the pure freshness check above.
/// Degrades to `false` (never live) on any missing/malformed input —
/// same never-panic policy as every other pass in this module.
fn team_is_live(paths: &GatherPaths, team: &str, now: SystemTime) -> bool {
    let config_path = paths.teams_root.join(team).join("config.json");
    let Ok(config) = teamfiles::read_team_config(&config_path) else {
        return false;
    };
    let Some(session_id) = config.lead_session_id.filter(|id| !id.is_empty()) else {
        return false;
    };
    let mtime = resolve_transcript_mtime(&paths.projects_root, &session_id);
    is_transcript_live(mtime, now)
}

/// Team directory names under `teams_root` that actually look like a team
/// (a directory containing `config.json`) — filters out stray/incomplete
/// entries rather than counting them as ambiguity candidates. Sorted for
/// a deterministic order.
///
/// This is the CANONICAL team-dir enumeration: `team_hook.rs` (#100 M5),
/// `resolve_team`, and `pump::discover_team_dirs` (and through it
/// `jump.rs`) all route here. The 2026-07-17 review (finding 4) caught a
/// second, already-diverged copy in `pump.rs` — never reintroduce one.
pub(crate) fn list_team_dirs(teams_root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(teams_root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir() && entry.path().join("config.json").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

// ─── Task files (~/.claude/tasks/{team}/{n}.json) ─────────────────────────────

#[derive(Debug, Deserialize)]
struct TaskFileWire {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(rename = "blockedBy", default)]
    blocked_by: Vec<String>,
    #[serde(default)]
    owner: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    /// Any value beyond the three documented-live states (#88/#89) — drift-
    /// tolerant, never errors the whole pass.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskFile {
    id: String,
    /// Task title (#88: always present live, but parsed as optional —
    /// drift-tolerant per this module's policy).
    subject: Option<String>,
    status: TaskStatus,
    blocked_by: Vec<String>,
    /// Normalized: JSON `""` and `null` both collapse to `None` here.
    owner: Option<String>,
}

pub(crate) fn parse_task_file_str(json: &str) -> Result<TaskFile, serde_json::Error> {
    let wire: TaskFileWire = serde_json::from_str(json)?;
    let status = match wire.status.as_deref() {
        Some("pending") => TaskStatus::Pending,
        Some("in_progress") => TaskStatus::InProgress,
        Some("completed") => TaskStatus::Completed,
        _ => TaskStatus::Unknown,
    };
    Ok(TaskFile {
        id: wire.id.unwrap_or_default(),
        subject: wire.subject,
        status,
        blocked_by: wire.blocked_by,
        owner: wire.owner.filter(|owner| !owner.is_empty()),
    })
}

impl TaskStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Unknown => "unknown",
        }
    }
}

/// Minimal per-task snapshot `recorder.rs` needs to detect status/owner
/// changes across polling ticks. Public and string-typed (unlike the
/// private `TaskFile`/`TaskStatus`, which stay internal to this module's
/// `owned_task_blocked_by_incomplete` computation) because the recorder's
/// `task_delta` log line is meant to be human-legible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSnapshot {
    pub id: String,
    pub status: String,
    pub owner: Option<String>,
}

pub fn team_task_snapshots(paths: &GatherPaths, team: &str) -> Vec<TaskSnapshot> {
    read_task_files(&paths.tasks_root.join(team))
        .into_iter()
        .map(|task| TaskSnapshot {
            id: task.id,
            status: task.status.as_str().to_owned(),
            owner: task.owner,
        })
        .collect()
}

fn read_task_files(dir: &Path) -> Vec<TaskFile> {
    read_task_file_entries(dir)
        .into_iter()
        .map(|(_, task)| task)
        .collect()
}

/// Shared base for [`read_task_files`] (recorder's delta-detection path,
/// path-less) and [`team_task_displays`] (board's display path, needs the
/// path to stat mtime) — one directory walk / parse pass, not a forked
/// second reader.
fn read_task_file_entries(dir: &Path) -> Vec<(PathBuf, TaskFile)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .filter_map(|path| {
            let content = std::fs::read_to_string(&path).ok()?;
            let task = parse_task_file_str(&content).ok()?;
            Some((path, task))
        })
        .collect()
}

/// One task as the board displays it: adds `subject` (task title) and
/// mtime-derived `seconds_since_modified` on top of [`TaskFile`]'s fields
/// (private, delta-detection only). No timestamp field exists in the live
/// task-file schema (`docs/research/native-teamfiles-schema-2026-07-16.md`)
/// — file mtime is the only honest "elapsed" proxy, same stat-only
/// doctrine `resolve_transcript_mtime` already applies to session liveness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDisplay {
    pub id: String,
    pub subject: Option<String>,
    pub status: String,
    /// Normalized owner (`""`/`null` collapse to `None` at parse time,
    /// same as `TaskFile::owner`) — used by #99's nudge composition to
    /// find the selected agent's owned in-progress task.
    pub owner: Option<String>,
    pub seconds_since_modified: Option<u64>,
    /// Raw `blockedBy` ids, verbatim (issue #122: `brain::detect_status_lag`
    /// needs the dependency edges to find a stalled task's dependents;
    /// nothing before it needed this field, so it wasn't carried through).
    pub blocked_by: Vec<String>,
}

pub fn team_task_displays(paths: &GatherPaths, team: &str, now: SystemTime) -> Vec<TaskDisplay> {
    read_task_file_entries(&paths.tasks_root.join(team))
        .into_iter()
        .map(|(path, task)| {
            let seconds_since_modified = std::fs::metadata(&path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|mtime| now.duration_since(mtime).ok())
                .map(|elapsed| elapsed.as_secs());
            TaskDisplay {
                id: task.id,
                subject: task.subject,
                status: task.status.as_str().to_owned(),
                owner: task.owner,
                seconds_since_modified,
                blocked_by: task.blocked_by,
            }
        })
        .collect()
}

/// One task's dependency edges, resolved against the rest of the team's
/// task files (#123 hook enrichment, ADR-0015 brain layer). The public,
/// string-typed projection of the private `TaskFile`/`TaskStatus` pair —
/// same reason `TaskSnapshot` exists: consumers outside this module get
/// plain data they can format, never the parser's internals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDependency {
    pub id: String,
    pub subject: Option<String>,
    pub completed: bool,
    /// Normalized owner (`""`/`null` collapse to `None` at parse time).
    pub owner: Option<String>,
    /// `blockedBy` ids that are present in this team's task files AND not
    /// completed. An id no task file backs is dropped, not assumed
    /// incomplete — same under-claim rule as [`any_owned_task_blocked`].
    pub incomplete_blockers: Vec<String>,
}

pub fn team_task_dependencies(paths: &GatherPaths, team: &str) -> Vec<TaskDependency> {
    task_dependencies(&read_task_files(&paths.tasks_root.join(team)))
}

/// Pure core of [`team_task_dependencies`], testable without a tempdir.
fn task_dependencies(tasks: &[TaskFile]) -> Vec<TaskDependency> {
    let status_by_id: HashMap<&str, TaskStatus> = tasks
        .iter()
        .map(|task| (task.id.as_str(), task.status))
        .collect();
    tasks
        .iter()
        .map(|task| TaskDependency {
            id: task.id.clone(),
            subject: task.subject.clone(),
            completed: task.status == TaskStatus::Completed,
            owner: task.owner.clone(),
            incomplete_blockers: task
                .blocked_by
                .iter()
                .filter(|dep_id| {
                    status_by_id
                        .get(dep_id.as_str())
                        .is_some_and(|status| *status != TaskStatus::Completed)
                })
                .cloned()
                .collect(),
        })
        .collect()
}

/// True when at least one task owned by `name`/`agent_id` has a
/// `blockedBy` entry whose referenced task is present and not
/// `completed`. A `blockedBy` id this gather pass never found is never
/// counted incomplete (see module doc: under-claim, don't over-claim).
pub(crate) fn any_owned_task_blocked(tasks: &[TaskFile], name: &str, agent_id: &str) -> bool {
    let status_by_id: HashMap<&str, TaskStatus> = tasks
        .iter()
        .map(|task| (task.id.as_str(), task.status))
        .collect();
    tasks.iter().any(|task| {
        let owned =
            matches!(task.owner.as_deref(), Some(owner) if owner == name || owner == agent_id);
        owned
            && task.blocked_by.iter().any(|dep_id| {
                status_by_id
                    .get(dep_id.as_str())
                    .is_some_and(|status| *status != TaskStatus::Completed)
            })
    })
}

// ─── Inbox entries (~/.claude/teams/{team}/inboxes/{agent}.json) ──────────────
//
// Read-only: this module never writes an inbox file. Live schema per
// `docs/research/teammux-e2e-2026-07-16/attempt-2-results.md`: top-level
// JSON array of `{from, text, timestamp, msgV, msg_id, type, read}`, one
// parser for the whole crate — `teamfiles::InboxMessage` (issue #112
// dedupe; this module used to keep a second, near-identical private copy
// named `InboxEntryWire`).

pub(crate) fn oldest_unread_epoch_from_str(json: &str) -> Option<u64> {
    let entries: Vec<teamfiles::InboxMessage> = serde_json::from_str(json).ok()?;
    entries
        .into_iter()
        .filter(|entry| entry.read == Some(false))
        .filter_map(|entry| entry.timestamp.as_deref().and_then(parse_iso8601_utc))
        .min()
}

fn oldest_unread_epoch(path: &Path) -> Option<u64> {
    let content = std::fs::read_to_string(path).ok()?;
    oldest_unread_epoch_from_str(&content)
}

/// Age of the oldest unread entry in one inbox file, in seconds. `None`
/// when the file is missing/malformed or nothing is unread — i.e. the
/// [`ObservedFacts::seconds_since_unread_inbox`] accelerator has nothing
/// to say. Shared by [`gather_team`] and the hook enrichment path (#123)
/// so neither re-derives the epoch arithmetic.
pub(crate) fn seconds_since_unread_inbox(inbox_path: &Path, now: SystemTime) -> Option<u64> {
    let oldest_epoch = oldest_unread_epoch(inbox_path)?;
    now.duration_since(SystemTime::UNIX_EPOCH + Duration::from_secs(oldest_epoch))
        .ok()
        .map(|elapsed| elapsed.as_secs())
}

/// One rendered mailbox-tail line (#98 board stage 3): a single inbox
/// entry, tagged with which teammate's inbox it came from and how long
/// ago it landed. Read-only — this function (like the rest of the
/// module) never mutates an inbox file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxEntry {
    pub agent: String,
    pub from: Option<String>,
    pub text: Option<String>,
    pub seconds_ago: Option<u64>,
}

fn mailbox_entries_from_str(json: &str, agent: &str, now: SystemTime) -> Vec<MailboxEntry> {
    let Ok(entries) = serde_json::from_str::<Vec<teamfiles::InboxMessage>>(json) else {
        return Vec::new();
    };
    entries
        .into_iter()
        .map(|entry| {
            let epoch = entry.timestamp.as_deref().and_then(parse_iso8601_utc);
            let seconds_ago = epoch.and_then(|epoch| {
                now.duration_since(SystemTime::UNIX_EPOCH + Duration::from_secs(epoch))
                    .ok()
            });
            MailboxEntry {
                agent: agent.to_owned(),
                from: entry.from,
                text: entry.text,
                seconds_ago: seconds_ago.map(|elapsed| elapsed.as_secs()),
            }
        })
        .collect()
}

/// Recent inbox lines across every named member's inbox file, newest
/// first, capped to `limit` total. A missing/malformed inbox file yields
/// no entries for that member (drift-tolerant, matches every other read
/// in this module) rather than failing the whole pass.
pub fn team_mailbox_tail(
    paths: &GatherPaths,
    team: &str,
    member_names: &[String],
    now: SystemTime,
    limit: usize,
) -> Vec<MailboxEntry> {
    let inboxes_dir = paths.teams_root.join(team).join("inboxes");
    let mut entries: Vec<MailboxEntry> = member_names
        .iter()
        .flat_map(|name| {
            let path = inboxes_dir.join(format!("{name}.json"));
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            mailbox_entries_from_str(&content, name, now)
        })
        .collect();
    entries.sort_by_key(|entry| entry.seconds_ago.unwrap_or(u64::MAX));
    entries.truncate(limit);
    entries
}

// ─── Transcript mtime (~/.claude/projects/*/<sessionId>.jsonl) ────────────────
//
// Stat only — the ADR-0013 cut line forbids parsing JSONL transcript
// content in v1; only the file's mtime is read.

/// What a live teammate process's argv reveals about its team (Defect E).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TeammateLaunch {
    pub team_name: String,
    /// The lead session that actually SPAWNED this teammate — which is the
    /// lead's session id *now*, not the one recorded when the team was
    /// created. Those differ whenever the lead's session has rotated.
    pub lead_session_id: String,
}

/// Pure: read a teammate's team and its lead's live session id out of the
/// argv Claude Code launched it with.
///
/// Claude Code spawns a teammate as `<versions-dir>/2.1.x --agent-id
/// <name>@<team> --agent-name <name> --team-name <team> --parent-session-id
/// <lead session> …` (captured verbatim from `pane process-info`). The
/// `--parent-session-id` is the authority for Defect E: a lead's Claude Code
/// session id rotates on resume or `/clear`, and
/// `~/.claude/teams/<team>/config.json` keeps the id the team was created
/// with, so `leadSessionId` can name a session that no longer exists while
/// the team is perfectly alive.
///
/// Returns `None` unless BOTH flags are present with non-empty values — a
/// plain shell, a lead's own `claude`, or a truncated argv all yield nothing
/// rather than a partial guess.
pub(crate) fn parse_teammate_launch(argv: &[String]) -> Option<TeammateLaunch> {
    fn flag_value<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
        let index = argv.iter().position(|part| part == flag)?;
        argv.get(index + 1)
            .map(String::as_str)
            .filter(|value| !value.is_empty() && !value.starts_with("--"))
    }

    Some(TeammateLaunch {
        team_name: flag_value(argv, "--team-name")?.to_owned(),
        lead_session_id: flag_value(argv, "--parent-session-id")?.to_owned(),
    })
}

/// Impure: recover `team`'s lead pane when its recorded `leadSessionId` no
/// longer resolves (Defect E).
///
/// A lead's Claude Code session id rotates on resume or `/clear`, and
/// `~/.claude/teams/<team>/config.json` keeps the id the team was created
/// with. `pump::resolve_lead_pane` then finds nothing, which costs the team
/// its sidebar tokens and costs the lead its own `roster`/`why` verbs —
/// measured 2026-09-06 on 3 of 7 live teams that have teammates.
///
/// A LIVE teammate knows better: it was spawned by the lead's current
/// session and carries `--parent-session-id` in its argv. So this asks each
/// teammate-looking pane what its team and parent are, keeps the ones
/// claiming `team`, and resolves that parent session against `agents`.
///
/// Cost is why this is a fallback and not the primary path: `pane
/// process-info` has no bulk form, so it is one subprocess per candidate
/// pane. Only panes carrying a `display_agent` are probed — a pane herdmates
/// never labelled is not a teammate it spawned.
///
/// Every failure degrades to `None`: no live teammate, a teammate of another
/// team, a parent session that is not running (the team really is dead), or
/// a herdr call that errors. A `None` leaves the caller exactly where it was
/// before, which is a skipped team, never a wrong pane.
pub(crate) fn recover_lead_pane_via_teammates<H: HerdrApi>(
    herdr: &H,
    team: &str,
    agents: &[crate::herdr::AgentInfo],
) -> Option<String> {
    teammate_launches(herdr)
        .into_iter()
        .filter(|launch| launch.team_name == team)
        .find_map(|launch| {
            agents
                .iter()
                .find(|agent| {
                    agent
                        .agent_session
                        .as_ref()
                        .is_some_and(|session| session.value == launch.lead_session_id)
                })
                .map(|lead| lead.pane_id.clone())
        })
}

/// Impure: the team `session_id` leads, discovered from the teammates it
/// spawned rather than from any team config (Defect E, second half).
///
/// `brain::resolve_own_team` matches the calling session against each team's
/// recorded `leadSessionId`, so a lead whose session has rotated is told it
/// leads nothing and loses `roster`/`why` entirely — while its teammates are
/// running in front of it. Those teammates carry `--parent-session-id`, which
/// still names the session asking the question.
///
/// `None` whenever no live teammate names `session_id` as its parent, which
/// keeps the honest "not a team lead" error for a session that genuinely
/// leads nothing. Same cost profile and the same degrade rules as
/// [`recover_lead_pane_via_teammates`]; both share [`teammate_launches`].
pub(crate) fn recover_team_by_lead_session<H: HerdrApi>(
    herdr: &H,
    session_id: &str,
) -> Option<String> {
    teammate_launches(herdr)
        .into_iter()
        .find(|launch| launch.lead_session_id == session_id)
        .map(|launch| launch.team_name)
}

/// Impure: every live teammate's self-declared `(team, lead session)`.
///
/// Only panes carrying a `display_agent` are probed — a pane herdmates never
/// labelled is not a teammate it spawned — because `pane process-info` has no
/// bulk form and costs one subprocess per pane. A herdr failure at either
/// step drops that pane rather than the whole scan.
fn teammate_launches<H: HerdrApi>(herdr: &H) -> Vec<TeammateLaunch> {
    let Ok(panes) = herdr.pane_list(None) else {
        return Vec::new();
    };
    panes
        .iter()
        .filter(|pane| pane.display_agent.is_some())
        .filter_map(|pane| herdr.pane_process_info(&pane.pane_id).ok())
        .filter_map(|info| {
            info.foreground_processes
                .iter()
                .find_map(|process| parse_teammate_launch(&process.argv))
        })
        .collect()
}

/// Pure: the one pane in `panes` whose `display_agent` is exactly `name`.
///
/// `display_agent` is herdmates' own publication (`teammux::
/// publish_display_agent`, called once per teammate right after its pane is
/// split) and the only field on a pane snapshot that names a teammate.
///
/// Returns `None` for zero matches AND for more than one: two teams can
/// each hold a teammate called `reviewer`, and picking either would
/// attribute one team's pane to the other team's member. A caller that gets
/// `None` degrades the fact to absent, which is this module's documented
/// failure mode — an unverifiable binding is never guessed. The surviving
/// single match is still verified against the pane's own transcript before
/// it is used (see [`resolve_teammate_pane`]).
pub(crate) fn match_teammate_pane<'a>(panes: &'a [PaneInfo], name: &str) -> Option<&'a PaneInfo> {
    let mut matches = panes
        .iter()
        .filter(|pane| pane.display_agent.as_deref() == Some(name));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

/// Identity a Claude Code TEAMMATE transcript declares about itself.
///
/// Live-verified 2026-09-03 on four split-pane teammates: every `user` /
/// `assistant` / `attachment` record in a teammate's transcript carries
/// `agentName` and `teamName`, and a LEAD's transcript carries neither
/// (checked against its own transcript over the same window). So this is
/// both an identity and a positive discriminator: `None` means "not a
/// teammate transcript", never "unknown".
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptIdentity {
    pub agent_name: String,
    pub team_name: String,
}

/// How many leading transcript records [`read_transcript_identity`] will
/// read before giving up. The identity lands at record 3 or 4 in every live
/// sample (the first three are `agent-setting` / `mode` /
/// `permission-mode`, which carry no identity), so this is generous by two
/// orders of magnitude — its job is to bound the read on a transcript that
/// turns out not to be a teammate's, since those run to thousands of
/// records.
pub(crate) const IDENTITY_SCAN_RECORDS: usize = 64;

/// Impure: read `path`'s self-declared teammate identity.
///
/// Never errors: a missing file, an unreadable one, malformed lines, or an
/// identity past [`IDENTITY_SCAN_RECORDS`] all degrade to `None` — the same
/// never-guess policy as every other source in this module. A `None` makes
/// the caller refuse the binding rather than accept an unverified one.
fn read_transcript_identity(path: &Path) -> Option<TranscriptIdentity> {
    use std::io::BufRead;

    let file = std::fs::File::open(path).ok()?;
    for line in std::io::BufReader::new(file)
        .lines()
        .take(IDENTITY_SCAN_RECORDS)
    {
        let Ok(line) = line else {
            return None;
        };
        let Ok(record) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let agent_name = record.get("agentName").and_then(|value| value.as_str());
        let team_name = record.get("teamName").and_then(|value| value.as_str());
        if let (Some(agent_name), Some(team_name)) = (agent_name, team_name) {
            return Some(TranscriptIdentity {
                agent_name: agent_name.to_owned(),
                team_name: team_name.to_owned(),
            });
        }
    }
    None
}

/// Impure: the transcript file for `session_id`, whichever per-project
/// directory under `projects_root` holds it. Claude Code names transcripts
/// `<session-id>.jsonl` inside a directory derived from the session's cwd,
/// which a caller holding only a session id cannot reconstruct — hence the
/// scan.
fn resolve_transcript_path(projects_root: &Path, session_id: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(projects_root).ok()?;
    let file_name = format!("{session_id}.jsonl");
    for entry in entries.filter_map(Result::ok) {
        let dir_path = entry.path();
        if !dir_path.is_dir() {
            continue;
        }
        let candidate = dir_path.join(&file_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn resolve_transcript_mtime(projects_root: &Path, session_id: &str) -> Option<SystemTime> {
    let path = resolve_transcript_path(projects_root, session_id)?;
    std::fs::metadata(&path).ok()?.modified().ok()
}

/// One non-lead member's resolved herdr pane, and the transcript whose
/// mtime is its liveness signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedTeammatePane {
    pub pane_id: String,
    pub transcript_path: PathBuf,
}

/// Impure: resolve one non-lead member to its herdr pane, VERIFIED.
///
/// This is the join the module doc used to call impossible, and it is a
/// three-step chain in which every step is a fact rather than a heuristic
/// (measured live 2026-09-03, four teammates):
///
/// 1. [`match_teammate_pane`] finds the single pane whose `display_agent`
///    is this member's name — herdmates' own publication.
/// 2. That pane carries an `agent_session` under herdr's official
///    `herdr:claude` source whose value is the TEAMMATE's own Claude Code
///    session id. (The module doc previously recorded that no per-teammate
///    session id exists anywhere; it exists here, reported by herdr, and
///    that is what makes the rest possible.)
/// 3. That session's transcript declares its own `agentName` and
///    `teamName`, which must both match — so a `beta` pane belonging to
///    another team, or a lead's pane that happens to carry a matching
///    display label, is rejected rather than bound.
///
/// Any break in the chain returns `None`, which the caller degrades to
/// absent facts. Never a guess: an unverifiable pane is not reported.
///
/// What this deliberately does NOT do is ask herdr for the teammate's agent
/// STATE. herdr cannot hold one: a teammate pane's agent-session identity
/// already belongs to the official `herdr:claude` source, and
/// `set_hook_authority_at`'s owner-conflict gate drops any state report
/// from another source unless herdr's own detector confirms the agent —
/// which it cannot, because the teammate runs as argv0
/// `<versions-dir>/2.1.x` and herdr's `lookup_agent` matches on basename
/// (Defect D). Measured live 2026-09-03: the report is accepted on a pane
/// with no session owner and silently dropped on one with it. Hence
/// liveness comes from the transcript, which nobody else owns.
fn resolve_teammate_pane(
    panes: &[PaneInfo],
    projects_root: &Path,
    team: &str,
    member_name: &str,
) -> Option<ResolvedTeammatePane> {
    let pane = match_teammate_pane(panes, member_name)?;
    let session = pane.agent_session.as_ref()?;
    if session.kind != "id" {
        return None;
    }
    let transcript_path = resolve_transcript_path(projects_root, &session.value)?;
    let identity = read_transcript_identity(&transcript_path)?;
    (identity.agent_name == member_name && identity.team_name == team).then(|| {
        ResolvedTeammatePane {
            pane_id: pane.pane_id.clone(),
            transcript_path,
        }
    })
}

// ─── Minimal ISO-8601 UTC parser (no external time crate) ─────────────────────
//
// Inbox `timestamp` is observed live as `"2026-07-17T15:42:00.123Z"`-style
// UTC. Only the subset needed to compute a comparable epoch-seconds value
// is implemented: `YYYY-MM-DDTHH:MM:SS[.fff]Z`. Anything else degrades to
// `None` (never guessed).

pub(crate) fn parse_iso8601_utc(s: &str) -> Option<u64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;

    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() {
        return None;
    }

    let time = time.split('.').next().unwrap_or(time);
    let mut time_parts = time.split(':');
    let hour: u64 = time_parts.next()?.parse().ok()?;
    let minute: u64 = time_parts.next()?.parse().ok()?;
    let second: u64 = time_parts.next()?.parse().ok()?;
    if time_parts.next().is_some() {
        return None;
    }

    let days = crate::dateutil::days_from_civil(year, month, day);
    let days: u64 = days.try_into().ok()?;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herdr::test_support::FakeHerdr;
    use crate::herdr::{AgentInfo, AgentSession, PaneInfo};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "herdmates-gather-tests-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create gather test dir");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    // ── teammate launch argv (Defect E: the lead's session id rotates) ──────

    /// A live teammate's argv, in the order Claude Code emits it (captured
    /// verbatim from `pane process-info` on this machine).
    fn teammate_argv(team: &str, name: &str, parent: Option<&str>) -> Vec<String> {
        let mut argv = vec![
            "/home/zvi/.local/share/claude/versions/2.1.263".to_owned(),
            "--agent-id".to_owned(),
            format!("{name}@{team}"),
            "--agent-name".to_owned(),
            name.to_owned(),
            "--team-name".to_owned(),
            team.to_owned(),
            "--agent-color".to_owned(),
            "blue".to_owned(),
        ];
        if let Some(parent) = parent {
            argv.push("--parent-session-id".to_owned());
            argv.push(parent.to_owned());
        }
        argv.extend(
            [
                "--agent-type",
                "general-purpose",
                "--permission-mode",
                "auto",
            ]
            .iter()
            .map(|part| (*part).to_owned()),
        );
        argv
    }

    #[test]
    fn teammate_argv_yields_its_team_and_the_leads_current_session() {
        let argv = teammate_argv("session-95cbfad4", "mns-stale", Some("18b67557-live"));

        let launch = parse_teammate_launch(&argv).expect("a teammate launch");

        assert_eq!(launch.team_name, "session-95cbfad4");
        assert_eq!(launch.lead_session_id, "18b67557-live");
    }

    #[test]
    fn argv_without_a_parent_session_yields_nothing() {
        let argv = teammate_argv("session-x", "beta", None);

        assert_eq!(parse_teammate_launch(&argv), None);
    }

    #[test]
    fn a_non_teammate_process_argv_yields_nothing() {
        let argv = vec!["/usr/bin/zsh".to_owned()];

        assert_eq!(parse_teammate_launch(&argv), None);
    }

    #[test]
    fn a_trailing_flag_with_no_value_yields_nothing_rather_than_panicking() {
        let argv = vec![
            "claude".to_owned(),
            "--team-name".to_owned(),
            "session-x".to_owned(),
            "--parent-session-id".to_owned(),
        ];

        assert_eq!(parse_teammate_launch(&argv), None);
    }

    #[test]
    fn an_empty_flag_value_is_refused_like_an_absent_one() {
        let argv = teammate_argv("session-x", "beta", Some(""));

        assert_eq!(parse_teammate_launch(&argv), None);
    }

    #[test]
    fn a_resumed_lead_finds_its_own_team_through_its_live_teammates() {
        // The other half of Defect E: `brain::resolve_own_team` matches the
        // CALLING session against each team's recorded `leadSessionId`, so a
        // lead that has resumed cannot invoke its own verbs at all — it is
        // told it is "not registered as any team's lead" while its teammates
        // are running in front of it.
        let herdr = FakeHerdr::default();
        *herdr.panes.borrow_mut() = vec![teammate_pane("w4:p14", Some("beta"), Some("mate-1"))];
        herdr.process_info.borrow_mut().insert(
            "w4:p14".to_owned(),
            process_info(teammate_argv("team-x", "beta", Some("my-live-session"))),
        );

        assert_eq!(
            recover_team_by_lead_session(&herdr, "my-live-session").as_deref(),
            Some("team-x")
        );
    }

    #[test]
    fn a_session_no_teammate_claims_as_parent_recovers_no_team() {
        // A plain session that leads nothing must still get the honest
        // "not a team lead" error, never someone else's team.
        let herdr = FakeHerdr::default();
        *herdr.panes.borrow_mut() = vec![teammate_pane("w4:p14", Some("beta"), Some("mate-1"))];
        herdr.process_info.borrow_mut().insert(
            "w4:p14".to_owned(),
            process_info(teammate_argv(
                "team-x",
                "beta",
                Some("someone-elses-session"),
            )),
        );

        assert_eq!(
            recover_team_by_lead_session(&herdr, "my-live-session"),
            None
        );
    }

    #[test]
    fn a_rotated_leads_pane_is_recovered_from_its_live_teammates_argv() {
        // Defect E end to end: the config names a session that no longer
        // exists, a live teammate names the one that does, and that one
        // resolves to a pane.
        let herdr = FakeHerdr::default();
        *herdr.panes.borrow_mut() = vec![teammate_pane("w4:p14", Some("beta"), Some("mate-1"))];
        herdr.process_info.borrow_mut().insert(
            "w4:p14".to_owned(),
            process_info(teammate_argv("team-x", "beta", Some("live-lead"))),
        );
        let agents = vec![agent_with_session("w4:pT", "live-lead", "idle")];

        let recovered = recover_lead_pane_via_teammates(&herdr, "team-x", &agents);

        assert_eq!(recovered.as_deref(), Some("w4:pT"));
    }

    #[test]
    fn recovery_ignores_teammates_belonging_to_another_team() {
        let herdr = FakeHerdr::default();
        *herdr.panes.borrow_mut() = vec![teammate_pane("w4:p14", Some("beta"), Some("mate-1"))];
        herdr.process_info.borrow_mut().insert(
            "w4:p14".to_owned(),
            process_info(teammate_argv("some-other-team", "beta", Some("live-lead"))),
        );
        let agents = vec![agent_with_session("w4:pT", "live-lead", "idle")];

        assert_eq!(
            recover_lead_pane_via_teammates(&herdr, "team-x", &agents),
            None
        );
    }

    #[test]
    fn recovery_yields_nothing_when_the_named_lead_session_has_no_pane() {
        // The team is genuinely dead: its teammates point at a lead session
        // that is not running anywhere. Skipping is correct.
        let herdr = FakeHerdr::default();
        *herdr.panes.borrow_mut() = vec![teammate_pane("w4:p14", Some("beta"), Some("mate-1"))];
        herdr.process_info.borrow_mut().insert(
            "w4:p14".to_owned(),
            process_info(teammate_argv("team-x", "beta", Some("ghost-lead"))),
        );
        let agents = vec![agent_with_session("w4:pT", "someone-else", "idle")];

        assert_eq!(
            recover_lead_pane_via_teammates(&herdr, "team-x", &agents),
            None
        );
    }

    #[test]
    fn recovery_only_inspects_panes_that_look_like_teammates() {
        // process-info costs one subprocess per pane and there is no bulk
        // form, so a pane with no display_agent is never probed.
        let herdr = FakeHerdr::default();
        *herdr.panes.borrow_mut() = vec![
            teammate_pane("w4:pT", None, Some("live-lead")),
            teammate_pane("w4:p14", Some("beta"), Some("mate-1")),
        ];
        herdr.process_info.borrow_mut().insert(
            "w4:p14".to_owned(),
            process_info(teammate_argv("team-x", "beta", Some("live-lead"))),
        );
        let agents = vec![agent_with_session("w4:pT", "live-lead", "idle")];

        recover_lead_pane_via_teammates(&herdr, "team-x", &agents);

        let probed = herdr
            .calls()
            .into_iter()
            .filter(|call| call.starts_with("pane_process_info"))
            .collect::<Vec<_>>();
        assert_eq!(probed, ["pane_process_info:w4:p14"]);
    }

    #[test]
    fn recovery_degrades_to_none_when_process_info_fails() {
        let herdr = FakeHerdr::default();
        *herdr.panes.borrow_mut() = vec![teammate_pane("w4:p14", Some("beta"), Some("mate-1"))];
        herdr.fail_process_info.set(true);
        let agents = vec![agent_with_session("w4:pT", "live-lead", "idle")];

        assert_eq!(
            recover_lead_pane_via_teammates(&herdr, "team-x", &agents),
            None
        );
    }

    // ── teammate pane matching ──────────────────────────────────────────────

    fn process_info(argv: Vec<String>) -> crate::herdr::PaneProcessInfo {
        crate::herdr::PaneProcessInfo {
            foreground_processes: vec![crate::herdr::ForegroundProcess {
                name: "2.1.263".to_owned(),
                argv,
            }],
        }
    }

    fn teammate_pane(
        pane_id: &str,
        display_agent: Option<&str>,
        session: Option<&str>,
    ) -> PaneInfo {
        PaneInfo {
            pane_id: pane_id.to_owned(),
            workspace_id: "w1".to_owned(),
            tab_id: None,
            agent: None,
            agent_id: None,
            agent_session: session.map(|value| AgentSession {
                source: "herdr:claude".to_owned(),
                agent: "claude".to_owned(),
                kind: "id".to_owned(),
                value: value.to_owned(),
            }),
            agent_status: Some("unknown".to_owned()),
            display_agent: display_agent.map(str::to_owned),
            cwd: None,
        }
    }

    #[test]
    fn matches_the_single_pane_whose_display_agent_is_the_members_name() {
        let panes = vec![
            teammate_pane("w1:p1", None, Some("lead-session")),
            teammate_pane("w1:p2", Some("reviewer"), Some("mate-session")),
        ];

        assert_eq!(
            match_teammate_pane(&panes, "reviewer").map(|pane| pane.pane_id.as_str()),
            Some("w1:p2")
        );
    }

    #[test]
    fn matches_no_pane_when_no_display_agent_carries_the_name() {
        let panes = vec![teammate_pane("w1:p2", Some("other"), Some("s"))];

        assert!(match_teammate_pane(&panes, "reviewer").is_none());
    }

    #[test]
    fn refuses_to_pick_between_two_panes_claiming_the_same_teammate_name() {
        // Two teams can each hold a teammate called `reviewer`. Binding
        // either one would silently attribute one team's pane to another
        // team's member — the exact mis-binding this module's never-guess
        // doctrine forbids.
        let panes = vec![
            teammate_pane("w1:p2", Some("reviewer"), Some("s1")),
            teammate_pane("w4:p9", Some("reviewer"), Some("s2")),
        ];

        assert!(match_teammate_pane(&panes, "reviewer").is_none());
    }

    #[test]
    fn display_agent_matching_is_case_sensitive() {
        // One candidate per case on purpose: with both in the same snapshot
        // a loosened comparison would match BOTH and the ambiguity guard
        // would return `None` anyway, so the test would pass without the
        // exactness it claims to check.
        let panes = vec![teammate_pane("w1:p2", Some("Reviewer"), Some("s1"))];

        assert!(match_teammate_pane(&panes, "reviewer").is_none());
    }

    #[test]
    fn display_agent_matching_is_not_a_prefix_match() {
        let panes = vec![teammate_pane("w1:p3", Some("reviewer-2"), Some("s2"))];

        assert!(match_teammate_pane(&panes, "reviewer").is_none());
    }

    // ── teammate transcript identity ────────────────────────────────────────

    /// Write a transcript whose leading records carry no identity (exactly
    /// how a live one starts: `agent-setting`, `mode`, `permission-mode`)
    /// followed by `body`.
    fn write_transcript(dir: &Path, session_id: &str, body: &[&str]) -> PathBuf {
        fs::create_dir_all(dir).expect("create transcript dir");
        let path = dir.join(format!("{session_id}.jsonl"));
        let mut lines = vec![
            r#"{"type":"agent-setting","sessionId":"s","agentSetting":{}}"#.to_owned(),
            r#"{"type":"mode","sessionId":"s"}"#.to_owned(),
            r#"{"type":"permission-mode","sessionId":"s"}"#.to_owned(),
        ];
        lines.extend(body.iter().map(|line| (*line).to_owned()));
        fs::write(&path, format!("{}\n", lines.join("\n"))).expect("write transcript");
        path
    }

    #[test]
    fn teammate_transcript_declares_its_own_agent_and_team() {
        let temp = TempDir::new();
        let path = write_transcript(
            temp.path(),
            "sess-1",
            &[r#"{"type":"user","agentName":"mns-stale","teamName":"session-95cbfad4"}"#],
        );

        let identity = read_transcript_identity(&path).expect("identity record present");

        assert_eq!(identity.agent_name, "mns-stale");
        assert_eq!(identity.team_name, "session-95cbfad4");
    }

    #[test]
    fn lead_transcript_declares_no_identity_so_it_never_binds_to_a_teammate() {
        let temp = TempDir::new();
        let path = write_transcript(
            temp.path(),
            "sess-lead",
            &[
                r#"{"type":"user","userType":"external"}"#,
                r#"{"type":"assistant","requestId":"r1"}"#,
            ],
        );

        assert_eq!(read_transcript_identity(&path), None);
    }

    #[test]
    fn transcript_identity_scan_is_bounded_and_skips_malformed_lines() {
        let temp = TempDir::new();
        let mut body = vec![r#"not json at all"#.to_owned()];
        body.resize(
            IDENTITY_SCAN_RECORDS + 4,
            r#"{"type":"assistant"}"#.to_owned(),
        );
        body.push(r#"{"type":"user","agentName":"late","teamName":"session-late"}"#.to_owned());
        let borrowed = body.iter().map(String::as_str).collect::<Vec<_>>();
        let path = write_transcript(temp.path(), "sess-late", &borrowed);

        assert_eq!(
            read_transcript_identity(&path),
            None,
            "an identity past the scan bound degrades to None rather than an unbounded read"
        );
    }

    #[test]
    fn missing_transcript_file_degrades_to_no_identity() {
        let temp = TempDir::new();
        assert_eq!(
            read_transcript_identity(&temp.path().join("absent.jsonl")),
            None
        );
    }

    // ── ISO-8601 parsing ────────────────────────────────────────────────────

    #[test]
    fn parses_iso8601_with_millis() {
        assert_eq!(
            parse_iso8601_utc("2026-07-17T15:42:30.123Z"),
            Some(1_784_302_950)
        );
    }

    #[test]
    fn parses_iso8601_without_millis() {
        assert_eq!(
            parse_iso8601_utc("2026-07-17T15:42:30Z"),
            Some(1_784_302_950)
        );
    }

    #[test]
    fn rejects_non_utc_or_malformed_timestamps() {
        assert_eq!(parse_iso8601_utc("2026-07-17T15:42:30+02:00"), None);
        assert_eq!(parse_iso8601_utc("not a timestamp"), None);
        assert_eq!(parse_iso8601_utc(""), None);
    }

    // ── task file parsing / ownership ───────────────────────────────────────

    #[test]
    fn owner_empty_string_and_null_both_normalize_to_none() {
        let empty = parse_task_file_str(r#"{"id":"1","status":"pending","owner":""}"#).unwrap();
        let null = parse_task_file_str(r#"{"id":"2","status":"pending","owner":null}"#).unwrap();
        let absent = parse_task_file_str(r#"{"id":"3","status":"pending"}"#).unwrap();
        assert_eq!(empty.owner, None);
        assert_eq!(null.owner, None);
        assert_eq!(absent.owner, None);
    }

    #[test]
    fn unknown_status_value_is_drift_tolerant() {
        let task = parse_task_file_str(r#"{"id":"1","status":"future-value"}"#).unwrap();
        assert_eq!(task.status, TaskStatus::Unknown);
    }

    #[test]
    fn owned_task_with_incomplete_dependency_is_blocked() {
        let tasks = vec![
            parse_task_file_str(r#"{"id":"1","status":"pending"}"#).unwrap(),
            parse_task_file_str(
                r#"{"id":"2","status":"pending","owner":"alpha","blockedBy":["1"]}"#,
            )
            .unwrap(),
        ];
        assert!(any_owned_task_blocked(&tasks, "alpha", "alpha@team"));
    }

    #[test]
    fn owned_task_with_completed_dependency_is_not_blocked() {
        let tasks = vec![
            parse_task_file_str(r#"{"id":"1","status":"completed"}"#).unwrap(),
            parse_task_file_str(
                r#"{"id":"2","status":"pending","owner":"alpha","blockedBy":["1"]}"#,
            )
            .unwrap(),
        ];
        assert!(!any_owned_task_blocked(&tasks, "alpha", "alpha@team"));
    }

    #[test]
    fn unowned_blocked_task_never_counts_for_anyone() {
        let tasks = vec![
            parse_task_file_str(r#"{"id":"1","status":"pending"}"#).unwrap(),
            parse_task_file_str(r#"{"id":"2","status":"pending","blockedBy":["1"]}"#).unwrap(),
        ];
        assert!(!any_owned_task_blocked(&tasks, "alpha", "alpha@team"));
    }

    #[test]
    fn owner_matches_agent_id_form_too() {
        let tasks = vec![
            parse_task_file_str(r#"{"id":"1","status":"pending"}"#).unwrap(),
            parse_task_file_str(
                r#"{"id":"2","status":"pending","owner":"alpha@team","blockedBy":["1"]}"#,
            )
            .unwrap(),
        ];
        assert!(any_owned_task_blocked(&tasks, "alpha", "alpha@team"));
    }

    #[test]
    fn missing_blocked_by_target_is_never_counted_incomplete() {
        let tasks = vec![parse_task_file_str(
            r#"{"id":"2","status":"pending","owner":"alpha","blockedBy":["does-not-exist"]}"#,
        )
        .unwrap()];
        assert!(!any_owned_task_blocked(&tasks, "alpha", "alpha@team"));
    }

    // ── task_dependencies (#123) ────────────────────────────────────────────

    #[test]
    fn task_dependencies_keep_only_present_and_incomplete_blockers() {
        let tasks = vec![
            parse_task_file_str(r#"{"id":"1","status":"completed"}"#).unwrap(),
            parse_task_file_str(r#"{"id":"2","status":"in_progress"}"#).unwrap(),
            parse_task_file_str(
                r#"{"id":"3","subject":"Ship it","status":"pending","owner":"alpha","blockedBy":["1","2","ghost"]}"#,
            )
            .unwrap(),
        ];
        let deps = task_dependencies(&tasks);
        let third = deps.iter().find(|dep| dep.id == "3").unwrap();
        assert_eq!(third.incomplete_blockers, ["2"]);
        assert_eq!(third.subject.as_deref(), Some("Ship it"));
        assert_eq!(third.owner.as_deref(), Some("alpha"));
        assert!(!third.completed);
        assert!(deps.iter().find(|dep| dep.id == "1").unwrap().completed);
    }

    #[test]
    fn task_dependencies_report_no_blockers_for_an_unblocked_task() {
        let tasks = vec![parse_task_file_str(r#"{"id":"1","status":"pending"}"#).unwrap()];
        assert!(task_dependencies(&tasks)[0].incomplete_blockers.is_empty());
    }

    // ── inbox unread parsing ────────────────────────────────────────────────

    #[test]
    fn oldest_unread_entry_wins_over_newer_unread_and_any_read() {
        let json = r#"[
            {"from":"a","text":"x","timestamp":"2026-07-17T10:00:00Z","read":true},
            {"from":"a","text":"y","timestamp":"2026-07-17T09:00:00Z","read":false},
            {"from":"a","text":"z","timestamp":"2026-07-17T09:30:00Z","read":false}
        ]"#;
        let oldest = oldest_unread_epoch_from_str(json).unwrap();
        assert_eq!(oldest, parse_iso8601_utc("2026-07-17T09:00:00Z").unwrap());
    }

    #[test]
    fn empty_inbox_array_gives_no_unread() {
        assert_eq!(oldest_unread_epoch_from_str("[]"), None);
    }

    #[test]
    fn all_read_entries_give_no_unread() {
        let json = r#"[{"from":"a","text":"x","timestamp":"2026-07-17T09:00:00Z","read":true}]"#;
        assert_eq!(oldest_unread_epoch_from_str(json), None);
    }

    #[test]
    fn malformed_inbox_json_degrades_to_no_unread() {
        assert_eq!(oldest_unread_epoch_from_str("not json"), None);
    }

    // ── gather_team integration on tempdir fixtures ─────────────────────────

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, content).expect("write fixture file");
    }

    fn two_member_config(lead_session_id: &str) -> String {
        format!(
            r#"{{
                "name": "team-x",
                "leadSessionId": "{lead_session_id}",
                "members": [
                    {{"agentId":"team-lead@team-x","name":"team-lead","agentType":"team-lead","tmuxPaneId":"leader","backendType":"in-process"}},
                    {{"agentId":"alpha@team-x","name":"alpha","agentType":"general-purpose","tmuxPaneId":"in-process","backendType":"in-process"}}
                ]
            }}"#
        )
    }

    fn agent_with_session(pane_id: &str, session_value: &str, status: &str) -> AgentInfo {
        AgentInfo {
            pane_id: pane_id.to_owned(),
            workspace_id: "w1".to_owned(),
            agent: Some("claude".to_owned()),
            agent_id: Some(session_value.to_owned()),
            agent_session: Some(AgentSession {
                source: "claude-code".to_owned(),
                agent: "claude".to_owned(),
                kind: "id".to_owned(),
                value: session_value.to_owned(),
            }),
            status: Some(status.to_owned()),
        }
    }

    #[test]
    fn gather_team_missing_config_returns_empty() {
        let dir = TempDir::new();
        let paths = GatherPaths {
            teams_root: dir.path().join("teams"),
            tasks_root: dir.path().join("tasks"),
            projects_root: dir.path().join("projects"),
        };
        let herdr = FakeHerdr::default();
        let result = gather_team(&paths, "team-x", &herdr, SystemTime::now());
        assert!(result.is_empty());
    }

    #[test]
    fn gather_team_resolves_lead_status_and_leaves_non_lead_unknown() {
        let dir = TempDir::new();
        let paths = GatherPaths {
            teams_root: dir.path().join("teams"),
            tasks_root: dir.path().join("tasks"),
            projects_root: dir.path().join("projects"),
        };
        write(
            &paths.teams_root.join("team-x/config.json"),
            &two_member_config("session-lead-1"),
        );

        let herdr = FakeHerdr::default();
        *herdr.agents.borrow_mut() =
            vec![agent_with_session("w1A:p1", "session-lead-1", "blocked")];

        let facts = gather_team(&paths, "team-x", &herdr, SystemTime::now());
        assert_eq!(facts.len(), 2);

        let lead = facts.iter().find(|f| f.is_lead).unwrap();
        assert_eq!(lead.facts.agent_status, AgentActivity::Blocked);

        let alpha = facts.iter().find(|f| f.name == "alpha").unwrap();
        assert_eq!(alpha.facts.agent_status, AgentActivity::Unknown);
        assert_eq!(alpha.facts.seconds_since_transcript_activity, None);
    }

    #[test]
    fn gather_team_computes_transcript_liveness_for_lead_and_not_in_process_members() {
        let dir = TempDir::new();
        let paths = GatherPaths {
            teams_root: dir.path().join("teams"),
            tasks_root: dir.path().join("tasks"),
            projects_root: dir.path().join("projects"),
        };
        write(
            &paths.teams_root.join("team-x/config.json"),
            &two_member_config("session-lead-1"),
        );
        let transcript_path = paths.projects_root.join("proj1/session-lead-1.jsonl");
        write(&transcript_path, "{}");

        // The lead's pane has to resolve for its transcript to count as the
        // lead's — see `lead_transcript_liveness_needs_the_lead_pane_to_
        // resolve_first` for the live case that rule exists for.
        let herdr = FakeHerdr::default();
        *herdr.agents.borrow_mut() = vec![agent_with_session("w1:pLead", "session-lead-1", "idle")];
        let now = SystemTime::now() + Duration::from_secs(120);

        let facts = gather_team(&paths, "team-x", &herdr, now);
        let lead = facts.iter().find(|f| f.is_lead).unwrap();
        let secs = lead
            .facts
            .seconds_since_transcript_activity
            .expect("lead transcript resolved");
        assert!((115..=125).contains(&secs), "secs was {secs}");

        // `alpha` is IN-PROCESS: it has no herdr pane, so there is no
        // pane to carry a session id and nothing to resolve a transcript
        // from. A split-pane teammate does resolve — see
        // `gather_team_resolves_a_split_pane_teammates_pane_and_transcript_liveness`.
        let alpha = facts.iter().find(|f| f.name == "alpha").unwrap();
        assert_eq!(alpha.facts.seconds_since_transcript_activity, None);
        assert_eq!(alpha.pane_id, None);
    }

    #[test]
    fn lead_transcript_liveness_needs_the_lead_pane_to_resolve_first() {
        // Measured live 2026-09-03: team `session-95cbfad4`'s config records
        // `leadSessionId` 95cbfad4… while the lead's pane actually runs
        // session 18b67557… — a Claude Code session id rotates (resume,
        // `/clear`) and the team config keeps the id the team was created
        // with. The recorded session's transcript then goes quiet while the
        // lead works on, and reporting its mtime as the LEAD's liveness
        // reads as "stalled 41m" about an agent that is mid-turn.
        //
        // So the fact needs the same ownership proof the teammate path and
        // `enrich.rs` both require: no verified owner, no fact. An
        // unresolvable lead pane is exactly that case.
        let dir = TempDir::new();
        let paths = GatherPaths {
            teams_root: dir.path().join("teams"),
            tasks_root: dir.path().join("tasks"),
            projects_root: dir.path().join("projects"),
        };
        write(
            &paths.teams_root.join("team-x/config.json"),
            &two_member_config("session-lead-1"),
        );
        write(
            &paths.projects_root.join("proj1/session-lead-1.jsonl"),
            "{}",
        );

        // No agents at all: the lead's recorded session matches no pane.
        let herdr = FakeHerdr::default();
        let facts = gather_team(&paths, "team-x", &herdr, SystemTime::now());

        let lead = facts.iter().find(|f| f.is_lead).unwrap();
        assert_eq!(lead.pane_id, None);
        assert_eq!(
            lead.facts.seconds_since_transcript_activity, None,
            "an unconfirmable lead session must not ground a liveness claim"
        );
    }

    // ── non-lead (split-pane teammate) pane + liveness resolution ───────────

    /// A team whose second member is a SPLIT-PANE teammate: `backendType`
    /// `tmux` and a `tmuxPaneId` of the `%N` shape herdmates' shim mints.
    fn split_pane_team_config(lead_session_id: &str) -> String {
        format!(
            r#"{{
                "name": "team-x",
                "leadSessionId": "{lead_session_id}",
                "members": [
                    {{"agentId":"team-lead@team-x","name":"team-lead","agentType":"team-lead","tmuxPaneId":"leader","backendType":"in-process"}},
                    {{"agentId":"beta@team-x","name":"beta","agentType":"general-purpose","tmuxPaneId":"%1","backendType":"tmux"}}
                ]
            }}"#
        )
    }

    /// The three-file fixture a resolvable split-pane teammate needs: the
    /// team config, the teammate's herdr pane (carrying `display_agent` and
    /// its own session id), and that session's transcript.
    fn teammate_fixture(
        paths: &GatherPaths,
        herdr: &FakeHerdr,
        transcript_team: &str,
        transcript_agent: Option<&str>,
    ) {
        write(
            &paths.teams_root.join("team-x/config.json"),
            &split_pane_team_config("session-lead-1"),
        );
        *herdr.panes.borrow_mut() = vec![teammate_pane(
            "w4:p14",
            Some("beta"),
            Some("mate-session-1"),
        )];
        let identity = match transcript_agent {
            Some(agent) => {
                format!(r#"{{"type":"user","agentName":"{agent}","teamName":"{transcript_team}"}}"#)
            }
            None => r#"{"type":"user","userType":"external"}"#.to_owned(),
        };
        write_transcript(
            &paths.projects_root.join("proj1"),
            "mate-session-1",
            &[identity.as_str()],
        );
    }

    #[test]
    fn gather_team_resolves_a_split_pane_teammates_pane_and_transcript_liveness() {
        let dir = TempDir::new();
        let paths = GatherPaths {
            teams_root: dir.path().join("teams"),
            tasks_root: dir.path().join("tasks"),
            projects_root: dir.path().join("projects"),
        };
        let herdr = FakeHerdr::default();
        teammate_fixture(&paths, &herdr, "team-x", Some("beta"));
        let now = SystemTime::now() + Duration::from_secs(1_200);

        let facts = gather_team(&paths, "team-x", &herdr, now);

        let beta = facts.iter().find(|f| f.name == "beta").unwrap();
        assert_eq!(
            beta.pane_id.as_deref(),
            Some("w4:p14"),
            "a teammate pane verified against its own transcript resolves"
        );
        let secs = beta
            .facts
            .seconds_since_transcript_activity
            .expect("teammate transcript liveness resolved");
        assert!((1_195..=1_205).contains(&secs), "secs was {secs}");
    }

    #[test]
    fn gather_team_refuses_a_pane_whose_transcript_names_another_team() {
        // The display_agent match alone is ambiguous across teams (two teams
        // can each hold a `beta`). The transcript is what settles it, and a
        // mismatch has to degrade to absent rather than bind.
        let dir = TempDir::new();
        let paths = GatherPaths {
            teams_root: dir.path().join("teams"),
            tasks_root: dir.path().join("tasks"),
            projects_root: dir.path().join("projects"),
        };
        let herdr = FakeHerdr::default();
        teammate_fixture(&paths, &herdr, "some-other-team", Some("beta"));

        let facts = gather_team(&paths, "team-x", &herdr, SystemTime::now());

        let beta = facts.iter().find(|f| f.name == "beta").unwrap();
        assert_eq!(beta.pane_id, None);
        assert_eq!(beta.facts.seconds_since_transcript_activity, None);
    }

    #[test]
    fn gather_team_refuses_a_pane_whose_transcript_names_another_teammate() {
        let dir = TempDir::new();
        let paths = GatherPaths {
            teams_root: dir.path().join("teams"),
            tasks_root: dir.path().join("tasks"),
            projects_root: dir.path().join("projects"),
        };
        let herdr = FakeHerdr::default();
        teammate_fixture(&paths, &herdr, "team-x", Some("gamma"));

        let facts = gather_team(&paths, "team-x", &herdr, SystemTime::now());

        let beta = facts.iter().find(|f| f.name == "beta").unwrap();
        assert_eq!(beta.pane_id, None);
        assert_eq!(beta.facts.seconds_since_transcript_activity, None);
    }

    #[test]
    fn gather_team_refuses_a_pane_whose_transcript_declares_no_teammate_identity() {
        // A LEAD's transcript carries no `agentName`/`teamName`, so this is
        // also the guard against binding a member to a lead's pane.
        let dir = TempDir::new();
        let paths = GatherPaths {
            teams_root: dir.path().join("teams"),
            tasks_root: dir.path().join("tasks"),
            projects_root: dir.path().join("projects"),
        };
        let herdr = FakeHerdr::default();
        teammate_fixture(&paths, &herdr, "team-x", None);

        let facts = gather_team(&paths, "team-x", &herdr, SystemTime::now());

        let beta = facts.iter().find(|f| f.name == "beta").unwrap();
        assert_eq!(beta.pane_id, None);
        assert_eq!(beta.facts.seconds_since_transcript_activity, None);
    }

    #[test]
    fn gather_team_refuses_a_teammate_pane_carrying_no_session_id() {
        let dir = TempDir::new();
        let paths = GatherPaths {
            teams_root: dir.path().join("teams"),
            tasks_root: dir.path().join("tasks"),
            projects_root: dir.path().join("projects"),
        };
        let herdr = FakeHerdr::default();
        teammate_fixture(&paths, &herdr, "team-x", Some("beta"));
        *herdr.panes.borrow_mut() = vec![teammate_pane("w4:p14", Some("beta"), None)];

        let facts = gather_team(&paths, "team-x", &herdr, SystemTime::now());

        let beta = facts.iter().find(|f| f.name == "beta").unwrap();
        assert_eq!(beta.pane_id, None);
        assert_eq!(beta.facts.seconds_since_transcript_activity, None);
    }

    #[test]
    fn gather_team_wires_owned_task_blocked_fact() {
        let dir = TempDir::new();
        let paths = GatherPaths {
            teams_root: dir.path().join("teams"),
            tasks_root: dir.path().join("tasks"),
            projects_root: dir.path().join("projects"),
        };
        write(
            &paths.teams_root.join("team-x/config.json"),
            &two_member_config("session-lead-1"),
        );
        write(
            &paths.tasks_root.join("team-x/1.json"),
            r#"{"id":"1","status":"pending"}"#,
        );
        write(
            &paths.tasks_root.join("team-x/2.json"),
            r#"{"id":"2","status":"pending","owner":"alpha","blockedBy":["1"]}"#,
        );

        let herdr = FakeHerdr::default();
        let facts = gather_team(&paths, "team-x", &herdr, SystemTime::now());

        let alpha = facts.iter().find(|f| f.name == "alpha").unwrap();
        assert!(alpha.facts.owned_task_blocked_by_incomplete);

        let lead = facts.iter().find(|f| f.is_lead).unwrap();
        assert!(!lead.facts.owned_task_blocked_by_incomplete);
    }

    #[test]
    fn gather_team_wires_unread_inbox_fact_and_never_writes_it_back() {
        let dir = TempDir::new();
        let paths = GatherPaths {
            teams_root: dir.path().join("teams"),
            tasks_root: dir.path().join("tasks"),
            projects_root: dir.path().join("projects"),
        };
        write(
            &paths.teams_root.join("team-x/config.json"),
            &two_member_config("session-lead-1"),
        );
        let inbox_path = paths.teams_root.join("team-x/inboxes/alpha.json");
        let before =
            r#"[{"from":"team-lead","text":"go","timestamp":"2026-07-17T09:00:00Z","read":false}]"#;
        write(&inbox_path, before);

        let now = parse_iso8601_utc("2026-07-17T09:05:00Z")
            .map(|epoch| SystemTime::UNIX_EPOCH + Duration::from_secs(epoch))
            .unwrap();
        let herdr = FakeHerdr::default();
        let facts = gather_team(&paths, "team-x", &herdr, now);

        let alpha = facts.iter().find(|f| f.name == "alpha").unwrap();
        assert_eq!(alpha.facts.seconds_since_unread_inbox, Some(300));

        let after = fs::read_to_string(&inbox_path).unwrap();
        assert_eq!(after, before, "gather must never mutate the inbox file");
    }

    // ── team_task_displays (#98 board) ──────────────────────────────────────

    #[test]
    fn task_display_parses_subject_and_carries_status() {
        let dir = TempDir::new();
        let paths = gather_paths_for(dir.path());
        write(
            &paths.tasks_root.join("team-x/1.json"),
            r#"{"id":"1","subject":"Ship the board","status":"in_progress"}"#,
        );

        let displays = team_task_displays(&paths, "team-x", SystemTime::now());
        assert_eq!(displays.len(), 1);
        assert_eq!(displays[0].id, "1");
        assert_eq!(displays[0].subject.as_deref(), Some("Ship the board"));
        assert_eq!(displays[0].status, "in_progress");
    }

    #[test]
    fn task_display_missing_subject_is_drift_tolerant() {
        let dir = TempDir::new();
        let paths = gather_paths_for(dir.path());
        write(
            &paths.tasks_root.join("team-x/1.json"),
            r#"{"id":"1","status":"pending"}"#,
        );

        let displays = team_task_displays(&paths, "team-x", SystemTime::now());
        assert_eq!(displays[0].subject, None);
    }

    #[test]
    fn task_display_elapsed_comes_from_file_mtime() {
        let dir = TempDir::new();
        let paths = gather_paths_for(dir.path());
        write(
            &paths.tasks_root.join("team-x/1.json"),
            r#"{"id":"1","status":"pending"}"#,
        );

        let now = SystemTime::now() + Duration::from_secs(90);
        let displays = team_task_displays(&paths, "team-x", now);
        let secs = displays[0].seconds_since_modified.expect("mtime resolved");
        assert!((85..=95).contains(&secs), "secs was {secs}");
    }

    #[test]
    fn task_display_missing_dir_yields_empty() {
        let dir = TempDir::new();
        let paths = gather_paths_for(dir.path());
        assert!(team_task_displays(&paths, "team-x", SystemTime::now()).is_empty());
    }

    // ── team_mailbox_tail (#98 board) ───────────────────────────────────────

    #[test]
    fn mailbox_tail_reads_from_and_text_and_tags_agent() {
        let dir = TempDir::new();
        let paths = gather_paths_for(dir.path());
        write(
            &paths.teams_root.join("team-x/inboxes/alpha.json"),
            r#"[{"from":"team-lead","text":"go","timestamp":"2026-07-17T09:00:00Z","read":false}]"#,
        );

        let now = parse_iso8601_utc("2026-07-17T09:05:00Z")
            .map(|epoch| SystemTime::UNIX_EPOCH + Duration::from_secs(epoch))
            .unwrap();
        let tail = team_mailbox_tail(&paths, "team-x", &["alpha".to_owned()], now, 10);
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].agent, "alpha");
        assert_eq!(tail[0].from.as_deref(), Some("team-lead"));
        assert_eq!(tail[0].text.as_deref(), Some("go"));
        assert_eq!(tail[0].seconds_ago, Some(300));
    }

    #[test]
    fn mailbox_tail_sorts_newest_first_across_members_and_respects_limit() {
        let dir = TempDir::new();
        let paths = gather_paths_for(dir.path());
        write(
            &paths.teams_root.join("team-x/inboxes/alpha.json"),
            r#"[{"from":"lead","text":"older","timestamp":"2026-07-17T08:00:00Z"}]"#,
        );
        write(
            &paths.teams_root.join("team-x/inboxes/beta.json"),
            r#"[{"from":"lead","text":"newer","timestamp":"2026-07-17T09:00:00Z"}]"#,
        );

        let now = parse_iso8601_utc("2026-07-17T09:05:00Z")
            .map(|epoch| SystemTime::UNIX_EPOCH + Duration::from_secs(epoch))
            .unwrap();
        let tail = team_mailbox_tail(
            &paths,
            "team-x",
            &["alpha".to_owned(), "beta".to_owned()],
            now,
            1,
        );
        assert_eq!(tail.len(), 1, "limit must cap the total, not per-inbox");
        assert_eq!(tail[0].text.as_deref(), Some("newer"));
    }

    #[test]
    fn mailbox_tail_missing_or_malformed_inbox_is_skipped_not_fatal() {
        let dir = TempDir::new();
        let paths = gather_paths_for(dir.path());
        write(
            &paths.teams_root.join("team-x/inboxes/beta.json"),
            "not json",
        );

        let tail = team_mailbox_tail(
            &paths,
            "team-x",
            &["alpha".to_owned(), "beta".to_owned()],
            SystemTime::now(),
            10,
        );
        assert!(tail.is_empty());
    }

    // ── resolve_team (#98 board) ────────────────────────────────────────────

    /// #102: `resolve_team`'s auto-pick path now requires a live team
    /// (config with a `leadSessionId` whose transcript exists and is
    /// fresh) — write both fixture pieces together so every candidate
    /// test stays live unless the test is deliberately checking staleness.
    fn write_live_team(paths: &GatherPaths, team: &str, session_id: &str) {
        write(
            &paths.teams_root.join(team).join("config.json"),
            &two_member_config(session_id),
        );
        write(
            &paths
                .projects_root
                .join("proj1")
                .join(format!("{session_id}.jsonl")),
            "{}",
        );
    }

    #[test]
    fn resolve_team_explicit_always_wins_even_with_other_teams_present() {
        let dir = TempDir::new();
        let paths = gather_paths_for(dir.path());
        write_live_team(&paths, "team-x", "session-x");
        write_live_team(&paths, "team-y", "session-y");

        assert_eq!(
            resolve_team(&paths, Some("team-z"), SystemTime::now()).unwrap(),
            "team-z",
            "explicit team name is trusted verbatim, not validated against disk"
        );
    }

    #[test]
    fn resolve_team_auto_picks_the_sole_team_dir() {
        let dir = TempDir::new();
        let paths = gather_paths_for(dir.path());
        write_live_team(&paths, "team-x", "session-x");

        assert_eq!(
            resolve_team(&paths, None, SystemTime::now()).unwrap(),
            "team-x"
        );
    }

    #[test]
    fn resolve_team_errors_on_zero_teams() {
        let dir = TempDir::new();
        let paths = gather_paths_for(dir.path());

        assert!(matches!(
            resolve_team(&paths, None, SystemTime::now()),
            Err(ResolveTeamError::NoTeams(_))
        ));
    }

    #[test]
    fn resolve_team_errors_on_multiple_teams_and_lists_candidates() {
        let dir = TempDir::new();
        let paths = gather_paths_for(dir.path());
        write_live_team(&paths, "team-x", "session-x");
        write_live_team(&paths, "team-y", "session-y");

        match resolve_team(&paths, None, SystemTime::now()) {
            Err(ResolveTeamError::Ambiguous(candidates)) => {
                assert!(candidates.contains("team-x"));
                assert!(candidates.contains("team-y"));
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn resolve_team_ignores_stray_dirs_without_a_config_json() {
        let dir = TempDir::new();
        let paths = gather_paths_for(dir.path());
        write_live_team(&paths, "team-x", "session-x");
        // A leftover/incomplete directory with no config.json must not
        // count as a second candidate.
        fs::create_dir_all(paths.teams_root.join("not-a-team")).unwrap();

        assert_eq!(
            resolve_team(&paths, None, SystemTime::now()).unwrap(),
            "team-x"
        );
    }

    #[test]
    fn resolve_team_ignores_stale_teams_and_degrades_to_no_teams() {
        let dir = TempDir::new();
        let paths = gather_paths_for(dir.path());
        write(
            &paths.teams_root.join("team-x/config.json"),
            &two_member_config("session-x"),
        );
        write(&paths.projects_root.join("proj1/session-x.jsonl"), "{}");
        let long_ago = SystemTime::now() + LIVE_TRANSCRIPT_WINDOW + Duration::from_secs(1);

        assert!(
            matches!(
                resolve_team(&paths, None, long_ago),
                Err(ResolveTeamError::NoTeams(_))
            ),
            "a config whose transcript predates the window must not count as a candidate"
        );
    }

    #[test]
    fn resolve_team_ignores_a_team_with_no_transcript_at_all() {
        let dir = TempDir::new();
        let paths = gather_paths_for(dir.path());
        write(
            &paths.teams_root.join("team-x/config.json"),
            &two_member_config("session-x"),
        );
        // No transcript file written for session-x — the #102 M1
        // `session-b7cc9f6d` case (fresh config, no transcript ever).

        assert!(matches!(
            resolve_team(&paths, None, SystemTime::now()),
            Err(ResolveTeamError::NoTeams(_))
        ));
    }

    // ── is_transcript_live (#102, pure) ─────────────────────────────────────

    #[test]
    fn is_transcript_live_true_within_window() {
        let now = SystemTime::now();
        let mtime = now - Duration::from_secs(60);
        assert!(is_transcript_live(Some(mtime), now));
    }

    #[test]
    fn is_transcript_live_false_past_window() {
        let now = SystemTime::now();
        let mtime = now - LIVE_TRANSCRIPT_WINDOW - Duration::from_secs(1);
        assert!(!is_transcript_live(Some(mtime), now));
    }

    #[test]
    fn is_transcript_live_true_exactly_at_window_boundary() {
        let now = SystemTime::now();
        let mtime = now - LIVE_TRANSCRIPT_WINDOW;
        assert!(is_transcript_live(Some(mtime), now));
    }

    #[test]
    fn is_transcript_live_false_when_transcript_missing() {
        assert!(!is_transcript_live(None, SystemTime::now()));
    }

    #[test]
    fn is_transcript_live_true_for_future_mtime_clock_skew() {
        // 2026-07-17 review, finding 8: a future mtime (NTP step-back)
        // must count as live, never silently drop the team.
        let now = SystemTime::now();
        let mtime = now + Duration::from_secs(120);
        assert!(is_transcript_live(Some(mtime), now));
    }

    fn gather_paths_for(dir: &Path) -> GatherPaths {
        GatherPaths {
            teams_root: dir.join("teams"),
            tasks_root: dir.join("tasks"),
            projects_root: dir.join("projects"),
        }
    }
}
