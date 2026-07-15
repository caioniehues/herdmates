//! Push-based worker status report and outbox hook from `docs/spec.md` sections 5 and 11.

use crate::herdr::HerdrClient;
use crate::msg;
use crate::run;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fmt::Display;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HookError {
    #[error("required environment variable {0} is not set or is not valid Unicode")]
    MissingEnvironment(&'static str),

    #[error("invalid agent-status event JSON: {0}")]
    InvalidEvent(#[from] serde_json::Error),

    #[error("expected {field} to be `pane_agent_status_changed`, received `{actual}`")]
    UnexpectedEvent { field: &'static str, actual: String },

    #[error("failed to resolve an absolute report path: {0}")]
    CurrentDirectory(#[from] std::io::Error),

    #[error("failed to {action} `{path}`: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Run(#[from] run::RunError),

    #[error(transparent)]
    Herdr(#[from] crate::herdr::HerdrError),
}

#[derive(Debug, Deserialize)]
struct EventEnvelope {
    event: String,
    data: AgentStatusEvent,
}

#[derive(Debug, Deserialize)]
struct AgentStatusEvent {
    #[serde(rename = "type")]
    kind: String,
    pane_id: String,
    agent_status: AgentStatus,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum AgentStatus {
    Idle,
    Working,
    Blocked,
    Done,
    Unknown,
}

impl AgentStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Unknown => "unknown",
        }
    }

    fn sends_pointer(self) -> bool {
        matches!(self, Self::Blocked | Self::Done)
    }

    fn drains_outbox(self) -> bool {
        matches!(self, Self::Idle | Self::Done)
    }
}

pub fn hook_command() -> Result<(), HookError> {
    let event_json = std::env::var("HERDR_PLUGIN_EVENT_JSON")
        .map_err(|_| HookError::MissingEnvironment("HERDR_PLUGIN_EVENT_JSON"))?;
    let state_dir = std::env::var("HERDR_PLUGIN_STATE_DIR")
        .map(PathBuf::from)
        .map_err(|_| HookError::MissingEnvironment("HERDR_PLUGIN_STATE_DIR"))?;
    on_agent_status(&event_json, &state_dir, &HerdrClient::from_env())
}

pub fn on_agent_status(
    event_json: &str,
    state_dir: &Path,
    herdr: &HerdrClient,
) -> Result<(), HookError> {
    let raw_event: Value = serde_json::from_str(event_json)?;
    let event: EventEnvelope = serde_json::from_value(raw_event.clone())?;
    require_status_event("event", &event.event)?;
    require_status_event("data.type", &event.data.kind)?;

    let Some(matched) = run::match_pane(state_dir, &event.data.pane_id)? else {
        return Ok(());
    };

    run::append_event(&matched.run.dir, &raw_event)?;
    if event.data.agent_status.drains_outbox() {
        drain_outbox(&matched.run.dir, &matched.worker_name, |text| {
            msg::deliver_queued_message(&matched.run, &matched.worker_name, text, herdr)
        })?;
    }
    if !event.data.agent_status.sends_pointer() {
        return Ok(());
    }

    let report_path = absolute_path(
        &matched
            .run
            .dir
            .join("inbox")
            .join(format!("{}.md", matched.worker_name)),
    )?;
    let pointer = format!(
        "[team {}] {} is {} — report: {}",
        matched.run.state.spec.name,
        matched.worker_name,
        event.data.agent_status.as_str(),
        report_path.display()
    );
    herdr.pane_run(&matched.run.state.god_pane_id, &pointer)?;
    Ok(())
}

fn drain_outbox<E, F>(run_dir: &Path, target: &str, mut deliver: F) -> Result<(), HookError>
where
    E: Display,
    F: FnMut(&str) -> Result<(), E>,
{
    for path in queued_message_paths(run_dir, target)? {
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                append_delivery_event(
                    run_dir,
                    "delivery_failed",
                    target,
                    &path,
                    Some(&error.to_string()),
                )?;
                break;
            }
        };

        if let Err(error) = deliver(&text) {
            append_delivery_event(
                run_dir,
                "delivery_failed",
                target,
                &path,
                Some(&error.to_string()),
            )?;
            break;
        }

        if let Err(error) = std::fs::remove_file(&path) {
            append_delivery_event(
                run_dir,
                "delivery_failed",
                target,
                &path,
                Some(&error.to_string()),
            )?;
            break;
        }
        append_delivery_event(run_dir, "delivered", target, &path, None)?;
    }
    Ok(())
}

fn queued_message_paths(run_dir: &Path, target: &str) -> Result<Vec<PathBuf>, HookError> {
    let outbox_dir = run_dir.join("outbox").join(target);
    let entries = match std::fs::read_dir(&outbox_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(HookError::Io {
                action: "read message outbox",
                path: outbox_dir,
                source,
            })
        }
    };

    let mut messages = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| HookError::Io {
            action: "read message outbox entry",
            path: outbox_dir.clone(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| HookError::Io {
            action: "inspect queued message",
            path: path.clone(),
            source,
        })?;
        if !file_type.is_file() {
            continue;
        }
        let Some(sequence) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.strip_suffix(".msg"))
            .and_then(|sequence| sequence.parse::<u64>().ok())
        else {
            continue;
        };
        messages.push((sequence, path));
    }
    messages.sort_unstable();
    Ok(messages.into_iter().map(|(_, path)| path).collect())
}

fn append_delivery_event(
    run_dir: &Path,
    kind: &str,
    target: &str,
    path: &Path,
    error: Option<&str>,
) -> Result<(), HookError> {
    let mut event = json!({
        "event": kind,
        "target": target,
        "path": path.display().to_string(),
    });
    if let Some(error) = error {
        event["error"] = Value::String(error.to_owned());
    }
    run::append_event(run_dir, &event)?;
    Ok(())
}

fn require_status_event(field: &'static str, actual: &str) -> Result<(), HookError> {
    if actual == "pane_agent_status_changed" {
        Ok(())
    } else {
        Err(HookError::UnexpectedEvent {
            field,
            actual: actual.to_owned(),
        })
    }
}

fn absolute_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{create_run, RunBoard};
    use crate::types::{
        GodSpec, RunLifecycle, RunState, TeamSpec, Topology, WorkerLifecycle, WorkerRunState,
        WorkerSpec,
    };
    use std::collections::BTreeMap;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    const CAPTURED_EVENT: &str = r#"{"event":"pane_agent_status_changed","data":{"type":"pane_agent_status_changed","pane_id":"wG:p2","workspace_id":"wG","agent_status":"idle","agent":"claude"}}"#;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test clock should be after Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "herdr-hook-tests-{}-{nanos}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create hook test directory");
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

    struct FakeHerdr {
        client: HerdrClient,
        log: PathBuf,
    }

    impl FakeHerdr {
        fn new(temp: &TempDir) -> Self {
            let binary = temp.path().join("fake-herdr");
            let log = temp.path().join("herdr-argv.log");
            fs::write(
                &binary,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nif [ \"$1\" = 'agent' ] && [ \"$2\" = 'wait' ]; then\n  printf '{{\"event\":\"pane.agent_status_changed\",\"data\":{{\"pane_id\":\"%s\",\"agent_status\":\"working\"}}}}\\n' \"$3\"\nfi\n",
                    log.display()
                ),
            )
            .expect("write fake Herdr CLI");
            let mut permissions = fs::metadata(&binary).expect("stat fake CLI").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&binary, permissions).expect("make fake CLI executable");
            Self {
                client: HerdrClient { binary },
                log,
            }
        }

        fn argv(&self) -> Vec<String> {
            fs::read_to_string(&self.log)
                .expect("read fake CLI log")
                .lines()
                .map(str::to_owned)
                .collect()
        }
    }

    fn fixture_run(state_dir: &Path, worker_pane: &str) -> RunBoard {
        let worker_name = "builder".to_owned();
        create_run(
            state_dir,
            RunState {
                spec: TeamSpec {
                    name: "alpha".to_owned(),
                    topology: Topology::Star,
                    cwd: PathBuf::from("/tmp/project"),
                    setup: Vec::new(),
                    god: GodSpec {
                        target: "self".to_owned(),
                    },
                    workers: vec![WorkerSpec {
                        name: worker_name.clone(),
                        agent: "codex".to_owned(),
                        role: "builder".to_owned(),
                        worktree: false,
                        branch: None,
                        brief: PathBuf::from("brief.md"),
                    }],
                },
                god_pane_id: "god-pane".to_owned(),
                workers: BTreeMap::from([(
                    worker_name,
                    WorkerRunState {
                        workspace_id: Some("worker-workspace".to_owned()),
                        pane_id: Some(worker_pane.to_owned()),
                        agent_id: Some("agent-1".to_owned()),
                        worktree_path: None,
                        adopted: false,
                        lifecycle: WorkerLifecycle::Running,
                    },
                )]),
                lifecycle: RunLifecycle::Active,
            },
        )
        .expect("create hook fixture run")
    }

    fn event(pane_id: &str, status: &str) -> Value {
        json!({
            "event": "pane_agent_status_changed",
            "data": {
                "type": "pane_agent_status_changed",
                "pane_id": pane_id,
                "workspace_id": "worker-workspace",
                "agent_status": status,
                "agent": "codex",
                "custom_status": null,
                "display_agent": null,
                "title": null,
                "state_labels": {"phase": "verification"}
            }
        })
    }

    fn queue_message(run: &RunBoard, sequence: u64, text: &str) -> PathBuf {
        let outbox = run.dir.join("outbox/builder");
        fs::create_dir_all(&outbox).expect("create fixture outbox");
        let path = outbox.join(format!("{sequence:020}.msg"));
        fs::write(&path, text).expect("write queued fixture message");
        path
    }

    fn read_events(run: &RunBoard) -> Vec<Value> {
        fs::read_to_string(run.dir.join("inbox/events.jsonl"))
            .expect("read durable event log")
            .lines()
            .map(|line| serde_json::from_str(line).expect("parse durable event"))
            .collect()
    }

    #[test]
    fn only_idle_and_done_statuses_drain_outboxes() {
        assert!(AgentStatus::Idle.drains_outbox());
        assert!(AgentStatus::Done.drains_outbox());
        assert!(!AgentStatus::Working.drains_outbox());
        assert!(!AgentStatus::Blocked.drains_outbox());
        assert!(!AgentStatus::Unknown.drains_outbox());
    }

    #[test]
    fn drain_delivers_exact_content_in_sequence_order_then_removes_and_audits() {
        let temp = TempDir::new();
        let run = fixture_run(temp.path(), "worker-pane");
        let later = queue_message(&run, 10, "second\nline");
        let earlier = queue_message(&run, 2, "first");
        let mut delivered = Vec::new();

        drain_outbox(&run.dir, "builder", |text| {
            delivered.push(text.to_owned());
            Ok::<(), std::convert::Infallible>(())
        })
        .expect("drain queued messages");

        assert_eq!(delivered, ["first", "second\nline"]);
        assert!(!earlier.exists());
        assert!(!later.exists());
        let events = read_events(&run);
        assert_eq!(events.len(), 2);
        assert!(events
            .iter()
            .all(|event| event["event"] == "delivered" && event["target"] == "builder"));
        assert!(events[0]["path"]
            .as_str()
            .unwrap()
            .ends_with("00000000000000000002.msg"));
        assert!(events[1]["path"]
            .as_str()
            .unwrap()
            .ends_with("00000000000000000010.msg"));
    }

    #[test]
    fn failed_delivery_keeps_queue_logs_failure_and_stops_before_later_messages() {
        let temp = TempDir::new();
        let run = fixture_run(temp.path(), "worker-pane");
        let first = queue_message(&run, 1, "first");
        let second = queue_message(&run, 2, "second");
        let mut attempts = Vec::new();

        drain_outbox(&run.dir, "builder", |text| {
            attempts.push(text.to_owned());
            Err::<(), _>("delivery refused")
        })
        .expect("record failed drain without aborting the hook");

        assert_eq!(attempts, ["first"]);
        assert!(first.exists());
        assert!(second.exists());
        let events = read_events(&run);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["event"], "delivery_failed");
        assert_eq!(events[0]["target"], "builder");
        assert_eq!(events[0]["error"], "delivery refused");
        assert!(events[0]["path"]
            .as_str()
            .unwrap()
            .ends_with("00000000000000000001.msg"));
    }

    #[test]
    fn done_drains_through_verified_delivery_before_injecting_report_pointer() {
        let temp = TempDir::new();
        let run = fixture_run(temp.path(), "worker-pane");
        let first = queue_message(&run, 1, "first");
        let second = queue_message(&run, 2, "second");
        let fake = FakeHerdr::new(&temp);

        on_agent_status(
            &event("worker-pane", "done").to_string(),
            temp.path(),
            &fake.client,
        )
        .expect("drain done worker outbox");

        assert!(!first.exists());
        assert!(!second.exists());
        let argv = fake.argv();
        let call_position = |expected: &[&str]| {
            argv.windows(expected.len())
                .position(|window| {
                    window
                        .iter()
                        .map(String::as_str)
                        .eq(expected.iter().copied())
                })
                .expect("expected Herdr call")
        };
        let report_path = run.dir.join("inbox/builder.md");
        let pointer = format!(
            "[team alpha] builder is done — report: {}",
            report_path.display()
        );
        let first_delivery = call_position(&["pane", "run", "worker-pane", "first"]);
        let second_delivery = call_position(&["pane", "run", "worker-pane", "second"]);
        let pointer_delivery = call_position(&["pane", "run", "god-pane", &pointer]);
        assert!(first_delivery < second_delivery);
        assert!(second_delivery < pointer_delivery);
        assert_eq!(
            argv.windows(2)
                .filter(|window| window[0] == "agent" && window[1] == "wait")
                .count(),
            2
        );
        let events = read_events(&run);
        assert_eq!(
            events
                .iter()
                .map(|event| event["event"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["pane_agent_status_changed", "delivered", "delivered"]
        );
    }

    #[test]
    fn working_blocked_and_unknown_flips_leave_queued_messages_untouched() {
        for status in ["working", "blocked", "unknown"] {
            let temp = TempDir::new();
            let run = fixture_run(temp.path(), "worker-pane");
            let queued = queue_message(&run, 1, "queued message");
            let fake = FakeHerdr::new(&temp);

            on_agent_status(
                &event("worker-pane", status).to_string(),
                temp.path(),
                &fake.client,
            )
            .expect("process non-draining status");

            assert!(queued.exists(), "{status} must not drain the outbox");
            if fake.log.exists() {
                assert!(!fs::read_to_string(&fake.log)
                    .expect("read fake Herdr log")
                    .lines()
                    .any(|argument| argument == "queued message"));
            }
        }
    }

    #[test]
    fn captured_payload_and_optional_fields_are_preserved_for_non_terminal_statuses() {
        let temp = TempDir::new();
        let run = fixture_run(temp.path(), "wG:p2");
        let fake = FakeHerdr::new(&temp);
        let captured_event = serde_json::from_str::<Value>(CAPTURED_EVENT).unwrap();
        let event_with_optional_fields = event("wG:p2", "working");

        on_agent_status(CAPTURED_EVENT, temp.path(), &fake.client).expect("process captured event");
        on_agent_status(
            &event_with_optional_fields.to_string(),
            temp.path(),
            &fake.client,
        )
        .expect("process event with optional fields");

        let events =
            fs::read_to_string(run.dir.join("inbox/events.jsonl")).expect("read durable event log");
        let events = events
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(events, [captured_event, event_with_optional_fields]);
        assert!(!fake.log.exists(), "non-terminal statuses must not inject");
    }

    #[test]
    fn unrelated_pane_exits_without_writing_or_invoking_herdr() {
        let temp = TempDir::new();
        let run = fixture_run(temp.path(), "worker-pane");
        let fake = FakeHerdr::new(&temp);

        on_agent_status(
            &event("not-a-team-pane", "done").to_string(),
            temp.path(),
            &fake.client,
        )
        .expect("ignore unrelated pane");

        assert!(!run.dir.join("inbox/events.jsonl").exists());
        assert!(!fake.log.exists());
    }

    #[test]
    fn blocked_and_done_append_events_and_inject_exact_absolute_pointers() {
        let temp = TempDir::new();
        let run = fixture_run(temp.path(), "worker-pane");
        let fake = FakeHerdr::new(&temp);

        for status in ["blocked", "done"] {
            on_agent_status(
                &event("worker-pane", status).to_string(),
                temp.path(),
                &fake.client,
            )
            .expect("process terminal status");
        }

        let events =
            fs::read_to_string(run.dir.join("inbox/events.jsonl")).expect("read durable event log");
        assert_eq!(events.lines().count(), 2);

        let report_path = run.dir.join("inbox/builder.md");
        assert!(report_path.is_absolute());
        let argv = fake.argv();
        assert_eq!(
            argv,
            [
                "pane",
                "run",
                "god-pane",
                &format!(
                    "[team alpha] builder is blocked — report: {}",
                    report_path.display()
                ),
                "pane",
                "run",
                "god-pane",
                &format!(
                    "[team alpha] builder is done — report: {}",
                    report_path.display()
                ),
            ]
        );
    }

    #[test]
    fn dot_form_event_types_are_rejected() {
        let temp = TempDir::new();
        let fake = FakeHerdr::new(&temp);
        let mut raw_event = event("worker-pane", "done");
        raw_event["event"] = json!("pane.agent_status_changed");

        let error = on_agent_status(&raw_event.to_string(), temp.path(), &fake.client)
            .expect_err("dot-form JSON event must fail");

        assert!(matches!(
            error,
            HookError::UnexpectedEvent { field: "event", .. }
        ));
    }
}
