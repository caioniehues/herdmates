//! Push-based worker status report hook from `docs/spec.md` section 5.

use crate::herdr::HerdrClient;
use crate::run;
use serde::Deserialize;
use serde_json::Value;
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
    use serde_json::json;
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
                    "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nprintf '%s\\n' '{{\"result\":{{\"type\":\"ok\"}}}}'\n",
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
