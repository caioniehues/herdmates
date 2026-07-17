//! Append-only JSONL recorder of the signal engine's classified output
//! (issue #97 stage 2, ADR-0013 §93 stage 2, `docs/spec.md` §4). Log
//! schema = engine output schema: `signal_engine::WaitingReason` +
//! `ObservedFacts` plus team/agent/timestamp/badge/record-type framing —
//! no second format.
//!
//! Append policy is deltas-only, never every-tick spam: a `"baseline"`
//! line the first time an agent is seen in this run, a `"transition"`
//! line only when an already-seen agent's classified reason or badge
//! changes, and a `"task_delta"` line only when an already-seen task's
//! status or owner changes (a task's first sighting is recorded silently,
//! establishing the comparison baseline without emitting a line — task
//! files have no per-agent "start of run" moment the way agents do).
//!
//! File semantics: open-append, one JSON object per line, never rewritten.
//! This module has no read-back/replay path — delta state lives only in
//! [`RecorderState`], in memory, for the lifetime of one `record` run.

use crate::gather::{self, GatherPaths, TeammateFacts};
use crate::herdr::HerdrApi;
use crate::signal_engine::{self, ObservedFacts, StalledThresholds, WaitingReason};
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RecorderError {
    #[error("cannot resolve the recorder log directory: set XDG_STATE_HOME or HOME")]
    UnresolvedLogDir,
    #[error("cannot resolve team-file paths: set HOME")]
    UnresolvedGatherPaths,
    #[error("failed to create recorder log directory {path}: {source}")]
    CreateDir {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to open recorder log {path}: {source}")]
    Open {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write recorder log {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Error)]
pub enum RecordArgsError {
    #[error("--team is required")]
    MissingTeam,
    #[error("--interval-secs requires a value")]
    MissingIntervalValue,
    #[error("invalid --interval-secs value: {0}")]
    InvalidInterval(String),
    #[error("--log-path requires a value")]
    MissingLogPathValue,
}

/// Default log path: `${XDG_STATE_HOME:-~/.local/state}/herdmates/
/// recorder/{team}.jsonl` (Caio decision, 2026-07-17). Distinct from
/// `paths::state_dir` (that one is the plugin's own herdr-scoped state
/// dir; the recorder's log is a plain XDG state path so it survives
/// independent of the herdr plugin install location).
pub fn default_log_path(team: &str) -> Result<PathBuf, RecorderError> {
    default_log_path_from(
        std::env::var_os("XDG_STATE_HOME"),
        std::env::var_os("HOME"),
        team,
    )
}

/// Pure core of [`default_log_path`], parameterized on the two env vars
/// it consults — tested directly rather than mutating real process env
/// vars (which would race other tests running in parallel; matches
/// `paths::resolve_dir_values`'s existing precedent in this crate).
fn default_log_path_from(
    xdg_state_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
    team: &str,
) -> Result<PathBuf, RecorderError> {
    let base = xdg_state_home
        .map(PathBuf::from)
        .or_else(|| home.map(|home| PathBuf::from(home).join(".local/state")))
        .ok_or(RecorderError::UnresolvedLogDir)?;
    Ok(base
        .join("herdmates/recorder")
        .join(format!("{team}.jsonl")))
}

/// One appended log line. `record_type` is the serde-tag discriminator
/// (`"baseline"` / `"transition"` / `"task_delta"`); `Baseline` and
/// `Transition` carry the full engine output for one agent, `TaskDelta`
/// carries a task's before/after status and owner.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "record_type", rename_all = "snake_case")]
pub enum Record {
    Baseline {
        team: String,
        agent: String,
        timestamp: u64,
        reason: WaitingReason,
        badge: Option<String>,
        facts: ObservedFacts,
    },
    Transition {
        team: String,
        agent: String,
        timestamp: u64,
        reason: WaitingReason,
        badge: Option<String>,
        facts: ObservedFacts,
    },
    TaskDelta {
        team: String,
        task_id: String,
        timestamp: u64,
        old_status: String,
        new_status: String,
        old_owner: Option<String>,
        new_owner: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct AgentSnapshot {
    reason: WaitingReason,
    badge: Option<String>,
}

/// Cross-tick delta state for one `record` run. Never persisted or read
/// back — a fresh `record` invocation always re-baselines every agent it
/// sees (no attempt to resume from a previous run's log).
#[derive(Debug, Default)]
pub struct RecorderState {
    agents: HashMap<String, AgentSnapshot>,
    tasks: HashMap<String, (String, Option<String>)>,
}

impl RecorderState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// One gather+classify+diff pass: returns the delta records this tick
/// produced (often empty — most ticks change nothing). Pure with respect
/// to `state`, which callers own and thread through successive ticks;
/// all other I/O (team files, herdr) happens inside [`gather::gather_team`].
pub fn tick<H: HerdrApi>(
    state: &mut RecorderState,
    paths: &GatherPaths,
    team: &str,
    herdr: &H,
    thresholds: &StalledThresholds,
    now: SystemTime,
) -> Vec<Record> {
    let timestamp = epoch_secs(now);
    let mut records = Vec::new();

    for teammate in gather_and_classify(paths, team, herdr, thresholds, now) {
        let TeammateClassification {
            name,
            reason,
            badge,
            facts,
        } = teammate;
        let snapshot = AgentSnapshot {
            reason,
            badge: badge.clone(),
        };

        match state.agents.get(&name) {
            None => records.push(Record::Baseline {
                team: team.to_owned(),
                agent: name.clone(),
                timestamp,
                reason,
                badge: badge.clone(),
                facts,
            }),
            Some(previous) if *previous != snapshot => records.push(Record::Transition {
                team: team.to_owned(),
                agent: name.clone(),
                timestamp,
                reason,
                badge: badge.clone(),
                facts,
            }),
            Some(_) => {}
        }
        state.agents.insert(name, snapshot);
    }

    for task in gather::team_task_snapshots(paths, team) {
        let previous = state
            .tasks
            .insert(task.id.clone(), (task.status.clone(), task.owner.clone()));
        if let Some((old_status, old_owner)) = previous {
            if old_status != task.status || old_owner != task.owner {
                records.push(Record::TaskDelta {
                    team: team.to_owned(),
                    task_id: task.id,
                    timestamp,
                    old_status,
                    new_status: task.status,
                    old_owner,
                    new_owner: task.owner,
                });
            }
        }
    }

    records
}

struct TeammateClassification {
    name: String,
    reason: WaitingReason,
    badge: Option<String>,
    facts: ObservedFacts,
}

fn gather_and_classify<H: HerdrApi>(
    paths: &GatherPaths,
    team: &str,
    herdr: &H,
    thresholds: &StalledThresholds,
    now: SystemTime,
) -> Vec<TeammateClassification> {
    gather::gather_team(paths, team, herdr, now)
        .into_iter()
        .map(|TeammateFacts { name, facts, .. }| {
            let reason = signal_engine::classify(&facts, thresholds);
            let badge = signal_engine::reason_badge(reason, None);
            TeammateClassification {
                name,
                reason,
                badge,
                facts,
            }
        })
        .collect()
}

fn epoch_secs(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Append `records` to `log_path` as one JSON object per line. Creates
/// parent directories if needed. Opens in append mode and never
/// truncates/rewrites — a crash mid-write leaves whatever was already
/// flushed intact, never a corrupted rewrite of prior lines. No-op (no
/// file touched at all) when `records` is empty.
pub fn append_records(log_path: &Path, records: &[Record]) -> Result<(), RecorderError> {
    if records.is_empty() {
        return Ok(());
    }
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| RecorderError::CreateDir {
            path: parent.display().to_string(),
            source,
        })?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|source| RecorderError::Open {
            path: log_path.display().to_string(),
            source,
        })?;
    for record in records {
        let line = serde_json::to_string(record).expect("Record always serializes");
        writeln!(file, "{line}").map_err(|source| RecorderError::Write {
            path: log_path.display().to_string(),
            source,
        })?;
    }
    Ok(())
}

// ─── `herdmates record` subcommand ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecordArgs {
    pub team: String,
    pub interval_secs: u64,
    pub log_path: Option<PathBuf>,
}

const DEFAULT_INTERVAL_SECS: u64 = 2;

pub(crate) fn parse_record_args(args: &[String]) -> Result<RecordArgs, RecordArgsError> {
    let mut team = None;
    let mut interval_secs = DEFAULT_INTERVAL_SECS;
    let mut log_path = None;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--team" => team = iter.next().cloned(),
            "--interval-secs" => {
                let value = iter.next().ok_or(RecordArgsError::MissingIntervalValue)?;
                interval_secs = value
                    .parse()
                    .map_err(|_| RecordArgsError::InvalidInterval(value.clone()))?;
            }
            "--log-path" => {
                log_path = Some(PathBuf::from(
                    iter.next().ok_or(RecordArgsError::MissingLogPathValue)?,
                ));
            }
            _ => {}
        }
    }

    Ok(RecordArgs {
        team: team.ok_or(RecordArgsError::MissingTeam)?,
        interval_secs,
        log_path,
    })
}

#[derive(Debug, Error)]
pub enum RecordCommandError {
    #[error("{0}")]
    Args(#[from] RecordArgsError),
    #[error(transparent)]
    Recorder(#[from] RecorderError),
}

/// `herdmates record --team <name> [--interval-secs N] [--log-path P]`
/// (issue #97 stage 2, ADR-0013 §93 stage 2, `docs/spec.md` §4): poll
/// [`gather::gather_team`] + `signal_engine::classify` on a fixed
/// interval (default 2s), appending only the delta records to an
/// append-only JSONL log. Runs until killed — the recorder is a live
/// dogfood tap, not a one-shot report.
pub fn record_command(args: &[String]) -> Result<(), RecordCommandError> {
    let parsed = parse_record_args(args)?;
    let paths = GatherPaths::from_env().ok_or(RecorderError::UnresolvedGatherPaths)?;
    let log_path = match parsed.log_path {
        Some(path) => path,
        None => default_log_path(&parsed.team)?,
    };
    let herdr = crate::herdr::HerdrClient::from_env();
    let thresholds = StalledThresholds::default();
    let interval = Duration::from_secs(parsed.interval_secs.max(1));
    let mut state = RecorderState::new();

    loop {
        let records = tick(
            &mut state,
            &paths,
            &parsed.team,
            &herdr,
            &thresholds,
            SystemTime::now(),
        );
        append_records(&log_path, &records)?;
        std::thread::sleep(interval);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herdr::test_support::FakeHerdr;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "herdmates-recorder-tests-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create recorder test dir");
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

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, content).expect("write fixture file");
    }

    fn lead_only_config() -> &'static str {
        r#"{
            "name": "team-x",
            "leadSessionId": "session-lead-1",
            "members": [
                {"agentId":"team-lead@team-x","name":"team-lead","agentType":"team-lead","tmuxPaneId":"leader","backendType":"in-process"}
            ]
        }"#
    }

    fn gather_paths(dir: &Path) -> GatherPaths {
        GatherPaths {
            teams_root: dir.join("teams"),
            tasks_root: dir.join("tasks"),
            projects_root: dir.join("projects"),
        }
    }

    // ── arg parsing ─────────────────────────────────────────────────────────

    #[test]
    fn parses_team_with_defaults() {
        let args = parse_record_args(&["--team".to_owned(), "team-x".to_owned()]).unwrap();
        assert_eq!(args.team, "team-x");
        assert_eq!(args.interval_secs, DEFAULT_INTERVAL_SECS);
        assert_eq!(args.log_path, None);
    }

    #[test]
    fn parses_interval_and_log_path_overrides() {
        let args = parse_record_args(&[
            "--team".to_owned(),
            "team-x".to_owned(),
            "--interval-secs".to_owned(),
            "5".to_owned(),
            "--log-path".to_owned(),
            "/tmp/custom.jsonl".to_owned(),
        ])
        .unwrap();
        assert_eq!(args.interval_secs, 5);
        assert_eq!(args.log_path, Some(PathBuf::from("/tmp/custom.jsonl")));
    }

    #[test]
    fn missing_team_is_an_error() {
        assert!(matches!(
            parse_record_args(&[]),
            Err(RecordArgsError::MissingTeam)
        ));
    }

    #[test]
    fn invalid_interval_is_an_error() {
        assert!(matches!(
            parse_record_args(&[
                "--team".to_owned(),
                "t".to_owned(),
                "--interval-secs".to_owned(),
                "not-a-number".to_owned(),
            ]),
            Err(RecordArgsError::InvalidInterval(_))
        ));
    }

    // ── default_log_path ────────────────────────────────────────────────────

    #[test]
    fn default_log_path_uses_xdg_state_home_when_set() {
        let path =
            default_log_path_from(Some("/xdg-state".into()), Some("/home/x".into()), "team-x")
                .unwrap();
        assert_eq!(
            path,
            PathBuf::from("/xdg-state/herdmates/recorder/team-x.jsonl")
        );
    }

    #[test]
    fn default_log_path_falls_back_to_home_local_state() {
        let path = default_log_path_from(None, Some("/home/x".into()), "team-x").unwrap();
        assert_eq!(
            path,
            PathBuf::from("/home/x/.local/state/herdmates/recorder/team-x.jsonl")
        );
    }

    #[test]
    fn default_log_path_errors_when_neither_env_var_is_set() {
        assert!(matches!(
            default_log_path_from(None, None, "team-x"),
            Err(RecorderError::UnresolvedLogDir)
        ));
    }

    // ── tick: baseline / transition / no-op ─────────────────────────────────

    #[test]
    fn first_tick_emits_one_baseline_per_agent() {
        let dir = TempDir::new();
        let paths = gather_paths(dir.path());
        write(
            &paths.teams_root.join("team-x/config.json"),
            lead_only_config(),
        );
        let herdr = FakeHerdr::default();
        let mut state = RecorderState::new();

        let records = tick(
            &mut state,
            &paths,
            "team-x",
            &herdr,
            &StalledThresholds::default(),
            SystemTime::now(),
        );
        assert_eq!(records.len(), 1);
        assert!(matches!(records[0], Record::Baseline { .. }));
    }

    #[test]
    fn unchanged_second_tick_emits_nothing() {
        let dir = TempDir::new();
        let paths = gather_paths(dir.path());
        write(
            &paths.teams_root.join("team-x/config.json"),
            lead_only_config(),
        );
        let herdr = FakeHerdr::default();
        let mut state = RecorderState::new();
        let thresholds = StalledThresholds::default();
        let now = SystemTime::now();

        tick(&mut state, &paths, "team-x", &herdr, &thresholds, now);
        let second = tick(&mut state, &paths, "team-x", &herdr, &thresholds, now);
        assert!(second.is_empty(), "no-op tick must emit nothing");
    }

    #[test]
    fn reason_change_emits_a_transition_not_another_baseline() {
        let dir = TempDir::new();
        let paths = gather_paths(dir.path());
        write(
            &paths.teams_root.join("team-x/config.json"),
            lead_only_config(),
        );
        let herdr = FakeHerdr::default();
        let mut state = RecorderState::new();
        let thresholds = StalledThresholds::default();
        let now = SystemTime::now();

        tick(&mut state, &paths, "team-x", &herdr, &thresholds, now);

        *herdr.agents.borrow_mut() = vec![crate::herdr::AgentInfo {
            pane_id: "w1A:p1".to_owned(),
            workspace_id: "w1".to_owned(),
            agent: Some("claude".to_owned()),
            agent_id: Some("session-lead-1".to_owned()),
            agent_session: Some(crate::herdr::AgentSession {
                source: "claude-code".to_owned(),
                agent: "claude".to_owned(),
                kind: "id".to_owned(),
                value: "session-lead-1".to_owned(),
            }),
            status: Some("blocked".to_owned()),
        }];

        let second = tick(&mut state, &paths, "team-x", &herdr, &thresholds, now);
        assert_eq!(second.len(), 1);
        assert!(matches!(second[0], Record::Transition { .. }));
    }

    // ── tick: task deltas ────────────────────────────────────────────────────

    #[test]
    fn first_sighting_of_a_task_emits_no_task_delta() {
        let dir = TempDir::new();
        let paths = gather_paths(dir.path());
        write(
            &paths.teams_root.join("team-x/config.json"),
            lead_only_config(),
        );
        write(
            &paths.tasks_root.join("team-x/1.json"),
            r#"{"id":"1","status":"pending"}"#,
        );
        let herdr = FakeHerdr::default();
        let mut state = RecorderState::new();

        let records = tick(
            &mut state,
            &paths,
            "team-x",
            &herdr,
            &StalledThresholds::default(),
            SystemTime::now(),
        );
        assert!(
            records
                .iter()
                .all(|r| !matches!(r, Record::TaskDelta { .. })),
            "first sighting establishes the baseline silently"
        );
    }

    #[test]
    fn task_status_change_emits_a_task_delta() {
        let dir = TempDir::new();
        let paths = gather_paths(dir.path());
        write(
            &paths.teams_root.join("team-x/config.json"),
            lead_only_config(),
        );
        let task_path = paths.tasks_root.join("team-x/1.json");
        write(&task_path, r#"{"id":"1","status":"pending"}"#);
        let herdr = FakeHerdr::default();
        let mut state = RecorderState::new();
        let thresholds = StalledThresholds::default();
        let now = SystemTime::now();

        tick(&mut state, &paths, "team-x", &herdr, &thresholds, now);
        write(&task_path, r#"{"id":"1","status":"completed"}"#);
        let second = tick(&mut state, &paths, "team-x", &herdr, &thresholds, now);

        let deltas: Vec<_> = second
            .iter()
            .filter(|r| matches!(r, Record::TaskDelta { .. }))
            .collect();
        assert_eq!(deltas.len(), 1);
        match deltas[0] {
            Record::TaskDelta {
                old_status,
                new_status,
                ..
            } => {
                assert_eq!(old_status, "pending");
                assert_eq!(new_status, "completed");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn task_owner_change_alone_also_emits_a_task_delta() {
        let dir = TempDir::new();
        let paths = gather_paths(dir.path());
        write(
            &paths.teams_root.join("team-x/config.json"),
            lead_only_config(),
        );
        let task_path = paths.tasks_root.join("team-x/1.json");
        write(&task_path, r#"{"id":"1","status":"pending"}"#);
        let herdr = FakeHerdr::default();
        let mut state = RecorderState::new();
        let thresholds = StalledThresholds::default();
        let now = SystemTime::now();

        tick(&mut state, &paths, "team-x", &herdr, &thresholds, now);
        write(
            &task_path,
            r#"{"id":"1","status":"pending","owner":"alpha"}"#,
        );
        let second = tick(&mut state, &paths, "team-x", &herdr, &thresholds, now);

        assert!(second.iter().any(|r| matches!(r, Record::TaskDelta { .. })));
    }

    // ── append_records: file semantics ──────────────────────────────────────

    #[test]
    fn append_records_is_a_noop_on_empty_slice() {
        let dir = TempDir::new();
        let log_path = dir.path().join("nested/team-x.jsonl");
        append_records(&log_path, &[]).unwrap();
        assert!(!log_path.exists(), "empty tick must not touch the file");
    }

    #[test]
    fn append_records_creates_parent_dirs_and_appends_one_line_per_record() {
        let dir = TempDir::new();
        let log_path = dir.path().join("nested/team-x.jsonl");
        let mut state = RecorderState::new();
        let paths = gather_paths(dir.path());
        write(
            &paths.teams_root.join("team-x/config.json"),
            lead_only_config(),
        );
        let herdr = FakeHerdr::default();
        let records = tick(
            &mut state,
            &paths,
            "team-x",
            &herdr,
            &StalledThresholds::default(),
            SystemTime::now(),
        );

        append_records(&log_path, &records).unwrap();
        let contents = fs::read_to_string(&log_path).unwrap();
        assert_eq!(contents.lines().count(), 1);
        assert!(contents.contains("\"record_type\":\"baseline\""));
    }

    #[test]
    fn append_records_never_truncates_prior_lines() {
        let dir = TempDir::new();
        let log_path = dir.path().join("team-x.jsonl");
        let mut state = RecorderState::new();
        let paths = gather_paths(dir.path());
        write(
            &paths.teams_root.join("team-x/config.json"),
            lead_only_config(),
        );
        let herdr = FakeHerdr::default();
        let thresholds = StalledThresholds::default();
        let now = SystemTime::now();

        let first = tick(&mut state, &paths, "team-x", &herdr, &thresholds, now);
        append_records(&log_path, &first).unwrap();

        *herdr.agents.borrow_mut() = vec![crate::herdr::AgentInfo {
            pane_id: "w1A:p1".to_owned(),
            workspace_id: "w1".to_owned(),
            agent: Some("claude".to_owned()),
            agent_id: Some("session-lead-1".to_owned()),
            agent_session: Some(crate::herdr::AgentSession {
                source: "claude-code".to_owned(),
                agent: "claude".to_owned(),
                kind: "id".to_owned(),
                value: "session-lead-1".to_owned(),
            }),
            status: Some("blocked".to_owned()),
        }];
        let second = tick(&mut state, &paths, "team-x", &herdr, &thresholds, now);
        append_records(&log_path, &second).unwrap();

        let contents = fs::read_to_string(&log_path).unwrap();
        assert_eq!(contents.lines().count(), 2);
        assert!(contents.lines().next().unwrap().contains("\"baseline\""));
        assert!(contents.lines().nth(1).unwrap().contains("\"transition\""));
    }
}
