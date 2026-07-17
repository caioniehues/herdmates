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
//! ## Session-id resolution is lead-only (documented gap, not a guess)
//!
//! Live team-config `members[]` entries carry no per-teammate session id
//! field (verified against `docs/research/native-teamfiles-schema-
//! 2026-07-16.md`'s captured 2-teammate sample and re-verified live against
//! every team config on this machine, 2026-07-17) — only the top-level
//! `leadSessionId` resolves to a Claude Code session. `src/pump.rs`
//! already encodes this exact doctrine (`resolve_lead_pane`, reused here):
//! non-lead teammates have no herdr-resolvable pane pre-shim, so they are
//! always skipped rather than paired via an unreliable heuristic (e.g.
//! cwd-matching, which risks silently binding the wrong agent). Concretely:
//! only the team lead ever gets a non-`Unknown` `agent_status` or a
//! `Some` transcript-liveness fact from this module; every other member
//! degrades to `AgentActivity::Unknown` / `None`, which is exactly the
//! engine's never-wrong-reason doctrine (a `None` here drives the
//! reason-less `Waiting` degrade, never a guessed class).
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

use crate::herdr::HerdrApi;
use crate::pump;
use crate::signal_engine::{AgentActivity, ObservedFacts};
use crate::teamfiles;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Injectable roots for every file source this module reads. Defaults
/// mirror Claude Code's documented on-disk layout and `pump::
/// default_teams_root`'s existing `HERDMATES_TEAMS_ROOT` override
/// convention; tests always construct this directly against a tempdir.
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
            teams_root: std::env::var_os("HERDMATES_TEAMS_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".claude/teams")),
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
}

/// Gather every member of `team`'s [`ObservedFacts`] from live sources.
/// Never panics: a missing/malformed team config yields an empty result
/// (nothing to classify), matching every other pass in this crate's
/// degrade-on-missing-file policy (`pump::pump_once`, `attention::
/// build_attention_queue`'s callers).
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
    let lead_status = pump::resolve_lead_pane(&config, &agents).and_then(|pane_id| {
        agents
            .iter()
            .find(|agent| agent.pane_id == pane_id)
            .and_then(|agent| agent.status.clone())
    });

    let tasks = read_task_files(&paths.tasks_root.join(team));
    let inboxes_dir = paths.teams_root.join(team).join("inboxes");

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

            let seconds_since_transcript_activity = if member.is_lead {
                config
                    .lead_session_id
                    .as_deref()
                    .and_then(|session_id| {
                        resolve_transcript_mtime(&paths.projects_root, session_id)
                    })
                    .and_then(|mtime| now.duration_since(mtime).ok())
                    .map(|elapsed| elapsed.as_secs())
            } else {
                None
            };

            let inbox_path = inboxes_dir.join(format!("{}.json", member.name));
            let seconds_since_unread_inbox = oldest_unread_epoch(&inbox_path)
                .and_then(|oldest_epoch| {
                    now.duration_since(SystemTime::UNIX_EPOCH + Duration::from_secs(oldest_epoch))
                        .ok()
                })
                .map(|elapsed| elapsed.as_secs());

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
            }
        })
        .collect()
}

// ─── Task files (~/.claude/tasks/{team}/{n}.json) ─────────────────────────────

#[derive(Debug, Deserialize)]
struct TaskFileWire {
    #[serde(default)]
    id: Option<String>,
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
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .filter_map(|path| {
            let content = std::fs::read_to_string(&path).ok()?;
            parse_task_file_str(&content).ok()
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
// JSON array of `{from, text, timestamp, msgV, msg_id, type, read}`. This
// is a distinct, more complete shape than `teamfiles::InboxMessage`
// (camelCase `fromAgentId`/`toAgentId`/`content`, no `read` flag) — that
// struct predates the live capture and is used elsewhere for the board
// pump's display text, not touched here (unrelated-code rule; flagged as
// a future cleanup, not fixed in this pass).

#[derive(Debug, Deserialize)]
struct InboxEntryWire {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    read: Option<bool>,
}

pub(crate) fn oldest_unread_epoch_from_str(json: &str) -> Option<u64> {
    let entries: Vec<InboxEntryWire> = serde_json::from_str(json).ok()?;
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

// ─── Transcript mtime (~/.claude/projects/*/<sessionId>.jsonl) ────────────────
//
// Stat only — the ADR-0013 cut line forbids parsing JSONL transcript
// content in v1; only the file's mtime is read.

fn resolve_transcript_mtime(projects_root: &Path, session_id: &str) -> Option<SystemTime> {
    let entries = std::fs::read_dir(projects_root).ok()?;
    let file_name = format!("{session_id}.jsonl");
    for entry in entries.filter_map(Result::ok) {
        let dir_path = entry.path();
        if !dir_path.is_dir() {
            continue;
        }
        let candidate = dir_path.join(&file_name);
        if let Ok(metadata) = std::fs::metadata(&candidate) {
            if let Ok(modified) = metadata.modified() {
                return Some(modified);
            }
        }
    }
    None
}

// ─── Minimal ISO-8601 UTC parser (no external time crate) ─────────────────────
//
// Inbox `timestamp` is observed live as `"2026-07-17T15:42:00.123Z"`-style
// UTC. Only the subset needed to compute a comparable epoch-seconds value
// is implemented: `YYYY-MM-DDTHH:MM:SS[.fff]Z`. Anything else degrades to
// `None` (never guessed).

fn parse_iso8601_utc(s: &str) -> Option<u64> {
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

    let days = days_from_civil(year, month, day);
    let days: u64 = days.try_into().ok()?;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Howard Hinnant's `days_from_civil`: days since the Unix epoch
/// (1970-01-01) for a proleptic-Gregorian civil date. Standard,
/// well-tested algorithm; reimplemented here rather than pulling in a
/// time crate for one conversion.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herdr::test_support::FakeHerdr;
    use crate::herdr::{AgentInfo, AgentSession};
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
    fn gather_team_computes_transcript_liveness_for_lead_only() {
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

        let herdr = FakeHerdr::default();
        let now = SystemTime::now() + Duration::from_secs(120);

        let facts = gather_team(&paths, "team-x", &herdr, now);
        let lead = facts.iter().find(|f| f.is_lead).unwrap();
        let secs = lead
            .facts
            .seconds_since_transcript_activity
            .expect("lead transcript resolved");
        assert!((115..=125).contains(&secs), "secs was {secs}");

        let alpha = facts.iter().find(|f| f.name == "alpha").unwrap();
        assert_eq!(alpha.facts.seconds_since_transcript_activity, None);
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
}
