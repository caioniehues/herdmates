//! `pump-board` subcommand (D1 agent board, ADR-0012): one pass that reads
//! native Claude Code team files and publishes sidebar tokens via
//! `pane report-metadata --token`.
//!
//! Pane resolution is scoped to each team's lead: the lead's Claude Code
//! session id is recorded in `config.json` (`leadSessionId`) and can be
//! matched against `herdr agent list`'s `agent_session.value` to find the
//! herdr pane that hosts it (live-verified 2026-07-16, see findings.md).
//! Non-lead teammates carry only a Claude-Code-internal tmux pane
//! reference, meaningless to herdr before the teammux shim exists — they
//! are always skipped, never erroring the pass (ADR-0012's degrade
//! policy).

use crate::brain;
use crate::gather::{self, GatherPaths};
use crate::herdr::{AgentInfo, HerdrApi};
use crate::signal_engine::{self, StalledThresholds};
use crate::teamfiles::{self, InboxMessage, TeamConfig};
use crate::tokens;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PumpError {
    #[error("cannot resolve the Claude Code team files directory: set HOME")]
    UnresolvedTeamsRoot,
}

pub fn pump_board_command(_args: &[String]) -> Result<(), PumpError> {
    let paths = GatherPaths::from_env().ok_or(PumpError::UnresolvedTeamsRoot)?;
    let herdr = crate::herdr::HerdrClient::from_env();
    pump_once(&paths, &herdr);
    Ok(())
}

pub(crate) fn default_teams_root() -> Result<PathBuf, PumpError> {
    if let Some(root) = std::env::var_os("HERDMATES_TEAMS_ROOT") {
        return Ok(PathBuf::from(root));
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".claude/teams"))
        .ok_or(PumpError::UnresolvedTeamsRoot)
}

/// Debounce window between board-pump passes triggered by hook events
/// (issue #125, formerly manifest events, issue #84 step 6): events can
/// fire in quick succession, and each pass walks the whole
/// `~/.claude/teams` tree.
const PUMP_DEBOUNCE_MS: u64 = 2_000;

const DEBOUNCE_MARKER_FILE: &str = "pump-board-last-run";

/// Entry point wired into `herdmates hook` (issue #125): every Claude
/// Code hook event inside a herdr session ticks the board, replacing the
/// manifest `on-agent-status` wiring deleted in #119/#121. The herdr
/// work runs in a DETACHED `herdmates pump-board` child — the hook
/// critical path must never block on a subprocess (review finding F2:
/// `HerdrClient::invoke` has no timeout, and Claude Code waits on hook
/// processes). Only the marker-file debounce runs in-process; never
/// errors — any failure degrades to a skipped pass.
pub fn auto_pump(state_dir: &Path) {
    if !debounce_admit(state_dir, now_ms(), PUMP_DEBOUNCE_MS) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    // ponytail: a wedged herdr leaves detached children instead of a
    // blocked hook — at most one per 2s window per non-racing hook;
    // concurrent hooks can each admit (see `write_marker`'s lost-race
    // note). Add a child-side timeout in HerdrClient::invoke if
    // accumulation is ever observed.
    let _ = std::process::Command::new(exe)
        .arg("pump-board")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Marker-file debounce: returns whether a pass is admitted, writing the
/// marker on admit. Shared by [`auto_pump`] (detached spawn) and
/// [`maybe_pump_at`] (in-process, the tested core).
pub(crate) fn debounce_admit(state_dir: &Path, now_ms: u64, debounce_ms: u64) -> bool {
    debounce_admit_marker(&state_dir.join(DEBOUNCE_MARKER_FILE), now_ms, debounce_ms)
}

/// Core of [`debounce_admit`], on an explicit marker path so other
/// debounced paths (issue #131: the enrichment post keys per
/// team/event/teammate) can reuse it without inheriting the board marker.
pub(crate) fn debounce_admit_marker(marker: &Path, now_ms: u64, debounce_ms: u64) -> bool {
    if let Some(last_ms) = read_marker(marker) {
        if now_ms.saturating_sub(last_ms) < debounce_ms {
            return false;
        }
    }
    write_marker(marker, now_ms);
    true
}

/// In-process debounced pass: [`debounce_admit`] + [`pump_once`], the
/// composition `auto_pump` runs across two processes. Test-only — it
/// exists so the debounce boundary is assertable without spawning the
/// detached child.
#[cfg(test)]
pub(crate) fn maybe_pump_at<H: HerdrApi>(
    state_dir: &Path,
    paths: &GatherPaths,
    herdr: &H,
    now_ms: u64,
    debounce_ms: u64,
) -> bool {
    if !debounce_admit(state_dir, now_ms, debounce_ms) {
        return false;
    }
    pump_once(paths, herdr);
    true
}

fn read_marker(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn write_marker(path: &Path, now_ms: u64) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Best-effort: a lost race between concurrent hook invocations just
    // means one extra pump pass, never a stuck or duplicated publish
    // (pump_once's report-metadata calls are idempotent overwrites).
    let _ = std::fs::write(path, now_ms.to_string());
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

/// One pump pass: discover every team under `paths.teams_root`, resolve
/// each team's lead to a herdr pane, and publish that lead's
/// priority-differentiated sidebar tokens (issue #124, ADR-0015 ambient
/// layer). Never errors — any per-team or per-teammate failure
/// (missing/malformed file, unresolvable pane, herdr call failure) is
/// skipped silently, per ADR-0012's degrade policy.
pub fn pump_once<H: HerdrApi>(paths: &GatherPaths, herdr: &H) {
    let team_dirs = discover_team_dirs(&paths.teams_root);
    if team_dirs.is_empty() {
        return;
    }
    let Ok(agents) = herdr.agent_list() else {
        return;
    };
    let now = SystemTime::now();
    let thresholds = StalledThresholds::default();

    for team_dir in team_dirs {
        let Some(team_name) = team_dir.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Ok(config) = teamfiles::read_team_config(&team_dir.join("config.json")) else {
            continue;
        };
        let Some(pane_id) = resolve_lead_pane(&config, &agents) else {
            continue;
        };
        let inboxes = read_inboxes(&team_dir.join("inboxes"));
        let teammates = teamfiles::build_teammates(&config, &inboxes);
        let Some(lead) = teammates.iter().find(|teammate| teammate.is_lead) else {
            continue;
        };

        // Priority-differentiated ambient state (issue #124): reuse
        // brain::five_state's exact WaitingReason -> FiveState mapping
        // (#122) rather than re-deriving it here (single-source rule
        // #90). `gather_team` re-reads this team's config to gather full
        // `ObservedFacts` (task-blocked, transcript/inbox liveness) —
        // a second, small read, not worth threading the parsed config
        // through just to save it.
        let facts = gather::gather_team(paths, team_name, herdr, now);
        let Some(lead_facts) = facts.iter().find(|member| member.is_lead) else {
            continue;
        };
        let reason = signal_engine::classify(&lead_facts.facts, &thresholds);
        let state = brain::five_state(lead_facts.facts.agent_status, reason);

        // The reap count is the team's, not the lead's own state: how many
        // of its teammates finished and are still running (brain::
        // reapable_count over the same `facts` the state above came from,
        // so the sidebar and `roster` cannot disagree — rule #90).
        let token_set =
            tokens::teammate_tokens(lead, state, brain::reapable_count(&facts, &thresholds));
        let pairs = token_set
            .into_iter()
            .map(|token| (token.name, token.value))
            .collect::<Vec<_>>();
        if pairs.is_empty() {
            continue;
        }
        let _ = herdr.pane_report_tokens(&pane_id, tokens::SOURCE, &pairs);
    }
}

/// Team directories directly under `teams_root` that contain a `config.json`,
/// sorted by directory name for a deterministic pass order. Thin path-mapping
/// wrapper over `gather::list_team_dirs`, the canonical enumeration
/// (2026-07-17 review, finding 4: the previous inline copy had already
/// diverged from it).
pub fn discover_team_dirs(teams_root: &Path) -> Vec<PathBuf> {
    crate::gather::list_team_dirs(teams_root)
        .into_iter()
        .map(|name| teams_root.join(name))
        .collect()
}

pub(crate) fn read_inboxes(inboxes_dir: &Path) -> BTreeMap<String, Vec<InboxMessage>> {
    let Ok(entries) = std::fs::read_dir(inboxes_dir) else {
        return BTreeMap::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .filter_map(|path| {
            let name = path.file_stem()?.to_str()?.to_owned();
            let messages = teamfiles::read_inbox(&path).ok()?;
            Some((name, messages))
        })
        .collect()
}

/// Match the team's recorded lead session id against `herdr agent list`'s
/// detected agent sessions to find the herdr pane hosting the lead.
pub fn resolve_lead_pane(config: &TeamConfig, agents: &[AgentInfo]) -> Option<String> {
    let lead_session_id = config.lead_session_id.as_deref()?;
    agents
        .iter()
        .find(|agent| {
            agent
                .agent_session
                .as_ref()
                .is_some_and(|session| session.value == lead_session_id)
        })
        .map(|agent| agent.pane_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::FiveState;
    use crate::herdr::{test_support::FakeHerdr, AgentSession, PaneInfo};
    use crate::teamfiles::{Member, Teammate};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "herdmates-pump-tests-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create pump test directory");
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

    /// `tasks_root`/`projects_root` point at siblings that are never
    /// written in these pump-focused tests — `gather::gather_team`
    /// degrades a missing dir to empty facts, never an error.
    fn paths_for(teams_root: &Path) -> GatherPaths {
        GatherPaths {
            teams_root: teams_root.to_path_buf(),
            tasks_root: teams_root.join("__tasks_unused"),
            projects_root: teams_root.join("__projects_unused"),
        }
    }

    /// A teammate's herdr pane as `pane list` reports one: no detected
    /// agent (Defect D), a `display_agent` herdmates published, and the
    /// teammate's own session id under herdr's official claude source.
    fn teammate_pane(pane_id: &str, display_agent: &str, session_value: &str) -> PaneInfo {
        PaneInfo {
            pane_id: pane_id.to_owned(),
            workspace_id: "w4".to_owned(),
            tab_id: None,
            agent: None,
            agent_id: None,
            agent_session: Some(AgentSession {
                source: "herdr:claude".to_owned(),
                agent: "claude".to_owned(),
                kind: "id".to_owned(),
                value: session_value.to_owned(),
            }),
            agent_status: Some("unknown".to_owned()),
            display_agent: Some(display_agent.to_owned()),
            cwd: None,
        }
    }

    fn write_team(
        teams_root: &Path,
        team_name: &str,
        lead_session_id: &str,
        extra_json: &str,
    ) -> PathBuf {
        let team_dir = teams_root.join(team_name);
        fs::create_dir_all(&team_dir).expect("create team dir");
        fs::write(
            team_dir.join("config.json"),
            format!(
                r#"{{
                    "name": "{team_name}",
                    "leadSessionId": "{lead_session_id}",
                    "members": [
                        {{
                            "agentId": "team-lead@{team_name}",
                            "name": "team-lead",
                            "agentType": "team-lead",
                            "tmuxPaneId": "leader",
                            "backendType": "in-process",
                            "prompt": "Coordinate the team"
                        }}
                        {extra_json}
                    ]
                }}"#
            ),
        )
        .expect("write team config");
        team_dir
    }

    fn agent_with_session(pane_id: &str, session_value: &str) -> AgentInfo {
        AgentInfo {
            pane_id: pane_id.to_owned(),
            workspace_id: "workspace".to_owned(),
            agent: Some("claude".to_owned()),
            agent_id: Some(session_value.to_owned()),
            agent_session: Some(AgentSession {
                source: "herdr:claude".to_owned(),
                agent: "claude".to_owned(),
                kind: "id".to_owned(),
                value: session_value.to_owned(),
            }),
            status: Some("working".to_owned()),
        }
    }

    // ── read_inboxes ──────────────────────────────────────────────────────────

    /// Regression for issue #112: the real, live-verified on-disk inbox
    /// shape (`from`/`text`/`timestamp`/`msgV`/`msg_id`/`type`/`read`) must
    /// parse to non-`None` `from`/`text` through this exact pump path
    /// (`read_inboxes` → `teamfiles::read_inbox` → `teamfiles::InboxMessage`),
    /// not just through `teamfiles`'s own direct tests.
    #[test]
    fn read_inboxes_parses_the_real_live_inbox_shape() {
        let temp = TempDir::new();
        let inboxes_dir = temp.path().join("inboxes");
        fs::create_dir_all(&inboxes_dir).expect("create inboxes dir");
        fs::write(
            inboxes_dir.join("alpha.json"),
            r#"[{"from":"alpha","text":"Task complete","timestamp":"2026-07-16T09:00:00.000Z","msgV":1,"msg_id":"m1","type":"message","read":false}]"#,
        )
        .expect("write real-shape inbox fixture");

        let inboxes = read_inboxes(&inboxes_dir);

        let alpha = inboxes.get("alpha").expect("alpha inbox present");
        assert_eq!(alpha.len(), 1);
        assert_eq!(alpha[0].from.as_deref(), Some("alpha"));
        assert_eq!(alpha[0].text.as_deref(), Some("Task complete"));
    }

    // ── discover_team_dirs ──────────────────────────────────────────────────

    #[test]
    fn discover_team_dirs_finds_only_dirs_with_config_json_sorted() {
        let temp = TempDir::new();
        write_team(temp.path(), "session-b", "b-session", "");
        write_team(temp.path(), "session-a", "a-session", "");
        fs::create_dir_all(temp.path().join("not-a-team")).expect("create non-team dir");

        let dirs = discover_team_dirs(temp.path());

        assert_eq!(
            dirs,
            [temp.path().join("session-a"), temp.path().join("session-b"),]
        );
    }

    #[test]
    fn discover_team_dirs_on_missing_root_returns_empty() {
        let dirs = discover_team_dirs(Path::new("/nonexistent/teams/root"));
        assert!(dirs.is_empty());
    }

    // ── resolve_lead_pane ────────────────────────────────────────────────────

    #[test]
    fn resolve_lead_pane_matches_session_id() {
        let config = TeamConfig {
            name: "t".to_owned(),
            lead_session_id: Some("lead-session-1".to_owned()),
            members: vec![],
        };
        let agents = vec![
            agent_with_session("w1:p1", "other-session"),
            agent_with_session("w1:p2", "lead-session-1"),
        ];

        assert_eq!(
            resolve_lead_pane(&config, &agents).as_deref(),
            Some("w1:p2")
        );
    }

    #[test]
    fn resolve_lead_pane_returns_none_when_unmatched() {
        let config = TeamConfig {
            name: "t".to_owned(),
            lead_session_id: Some("lead-session-1".to_owned()),
            members: vec![],
        };
        let agents = vec![agent_with_session("w1:p1", "other-session")];

        assert_eq!(resolve_lead_pane(&config, &agents), None);
    }

    #[test]
    fn resolve_lead_pane_returns_none_without_lead_session_id() {
        let config = TeamConfig {
            name: "t".to_owned(),
            lead_session_id: None,
            members: vec![],
        };
        let agents = vec![agent_with_session("w1:p1", "any-session")];

        assert_eq!(resolve_lead_pane(&config, &agents), None);
    }

    // ── pump_once (integration smoke) ───────────────────────────────────────

    #[test]
    fn pump_once_publishes_a_reap_token_when_a_teammate_finished_and_was_never_stopped() {
        // End-to-end over the real pipeline: a team config naming a
        // split-pane teammate, that teammate's herdr pane carrying the
        // display_agent herdmates publishes plus its own session id, and
        // that session's transcript declaring the matching agentName /
        // teamName with an mtime well past the hard stalled threshold.
        let temp = TempDir::new();
        let paths = GatherPaths {
            teams_root: temp.path().join("teams"),
            tasks_root: temp.path().join("tasks"),
            projects_root: temp.path().join("projects"),
        };
        write_team(
            &paths.teams_root,
            "session-t",
            "lead-session",
            r#", {"agentId":"beta@session-t","name":"beta","agentType":"general-purpose","tmuxPaneId":"%1","backendType":"tmux"}"#,
        );
        let transcript_dir = paths.projects_root.join("proj1");
        fs::create_dir_all(&transcript_dir).expect("create transcript dir");
        fs::write(
            transcript_dir.join("mate-session.jsonl"),
            "{\"type\":\"user\",\"agentName\":\"beta\",\"teamName\":\"session-t\"}\n",
        )
        .expect("write transcript");
        let stale = SystemTime::now() - std::time::Duration::from_secs(3 * 3_600);
        fs::File::options()
            .write(true)
            .open(transcript_dir.join("mate-session.jsonl"))
            .expect("reopen transcript")
            .set_modified(stale)
            .expect("age the transcript");

        let fake = FakeHerdr::default();
        *fake.agents.borrow_mut() = vec![agent_with_session("w1:pLead", "lead-session")];
        *fake.panes.borrow_mut() = vec![teammate_pane("w4:p14", "beta", "mate-session")];

        pump_once(&paths, &fake);

        let published = fake
            .calls()
            .into_iter()
            .find(|call| call.starts_with("pane_report_tokens"))
            .expect("the lead's tokens were published");
        assert!(
            published.contains("reap=1 to stop"),
            "published tokens were: {published}"
        );
    }

    #[test]
    fn pump_once_publishes_tokens_for_resolvable_lead_and_skips_unresolvable_team() {
        let temp = TempDir::new();
        write_team(temp.path(), "session-resolvable", "resolvable-session", "");
        write_team(
            temp.path(),
            "session-unresolvable",
            "unresolvable-session",
            "",
        );
        let fake = FakeHerdr::default();
        *fake.agents.borrow_mut() = vec![agent_with_session("w1:pLead", "resolvable-session")];

        pump_once(&paths_for(temp.path()), &fake);

        let calls = fake.calls();
        assert_eq!(
            calls,
            [
                "agent_list",
                "agent_list",
                "pane_list",
                "pane_report_tokens:w1:pLead:herdmates-board:task=Coordinate the team,status=online"
            ],
            "two agent_list calls: pump_once's own lead-pane resolution, then \
             gather::gather_team's inside the state-classification path; the \
             pane_list is gather_team's non-lead teammate resolution, one \
             snapshot per pass rather than one per member"
        );
    }

    // #124 review fix: pump-level guards for the two states the ticket is
    // about — a hard-coded FiveState in pump_once would pass the wiring
    // test above (working → online) but fail these.

    #[test]
    fn pump_once_renders_a_blocked_lead_as_needs_you_with_the_marker() {
        let temp = TempDir::new();
        write_team(temp.path(), "session-blocked", "blocked-session", "");
        let fake = FakeHerdr::default();
        let mut agent = agent_with_session("w1:pLead", "blocked-session");
        // herdr "blocked" = permission prompt (signal_engine::classify).
        agent.status = Some("blocked".to_owned());
        *fake.agents.borrow_mut() = vec![agent];

        pump_once(&paths_for(temp.path()), &fake);

        let report = fake
            .calls()
            .into_iter()
            .find(|call| call.starts_with("pane_report_tokens"))
            .expect("a report call");
        assert!(report.ends_with("status=!! needs-you"), "{report}");
    }

    #[test]
    fn pump_once_renders_a_statusless_lead_as_waiting_never_online() {
        let temp = TempDir::new();
        write_team(temp.path(), "session-nostatus", "nostatus-session", "");
        let fake = FakeHerdr::default();
        let mut agent = agent_with_session("w1:pLead", "nostatus-session");
        agent.status = None;
        *fake.agents.borrow_mut() = vec![agent];

        pump_once(&paths_for(temp.path()), &fake);

        let report = fake
            .calls()
            .into_iter()
            .find(|call| call.starts_with("pane_report_tokens"))
            .expect("a report call");
        assert!(report.ends_with("status=waiting"), "{report}");
    }

    #[test]
    fn pump_once_skips_teams_with_no_resolvable_lead_session() {
        let temp = TempDir::new();
        write_team(temp.path(), "session-a", "session-a-id", "");
        let fake = FakeHerdr::default();
        // Realistic degrade path: herdr reachable, but no pane's detected
        // session matches this team's lead — never an error, just a skip.
        *fake.agents.borrow_mut() = vec![agent_with_session("w1:p1", "unrelated-session")];

        pump_once(&paths_for(temp.path()), &fake);

        assert_eq!(
            fake.calls(),
            ["agent_list"],
            "no resolvable lead means no report-metadata call, never an error"
        );
    }

    #[test]
    fn pump_once_on_empty_teams_root_makes_no_herdr_calls() {
        let temp = TempDir::new();
        let fake = FakeHerdr::default();

        pump_once(&paths_for(temp.path()), &fake);

        assert!(
            fake.calls().is_empty(),
            "empty discovery must short-circuit before any herdr call"
        );
    }

    #[test]
    fn build_teammates_and_tokens_integrate_end_to_end_for_a_populated_lead() {
        let config = TeamConfig {
            name: "t".to_owned(),
            lead_session_id: Some("lead-1".to_owned()),
            members: vec![Member {
                agent_id: "team-lead@t".to_owned(),
                name: "team-lead".to_owned(),
                is_lead: true,
                tmux_pane_id: Some("leader".to_owned()),
                backend_type: Some("in-process".to_owned()),
                is_active: true,
                model: Some("claude-opus-4-8".to_owned()),
                prompt: Some("Coordinate the pivot work".to_owned()),
            }],
        };
        let teammates = teamfiles::build_teammates(&config, &BTreeMap::new());
        let lead: &Teammate = teammates.iter().find(|t| t.is_lead).unwrap();

        let token_set = tokens::teammate_tokens(lead, FiveState::Online, 0);

        assert_eq!(
            token_set
                .tokens()
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["task", "status", "model"]
        );
    }

    // ── maybe_pump_at (debounce) ────────────────────────────────────────────

    #[test]
    fn first_call_with_no_marker_runs_and_writes_marker() {
        let state_dir = TempDir::new();
        let teams_dir = TempDir::new();
        let fake = FakeHerdr::default();

        let ran = maybe_pump_at(
            state_dir.path(),
            &paths_for(teams_dir.path()),
            &fake,
            1_000,
            2_000,
        );

        assert!(ran, "no prior marker means the pass must run");
        assert_eq!(
            read_marker(&state_dir.path().join(DEBOUNCE_MARKER_FILE)),
            Some(1_000)
        );
    }

    #[test]
    fn call_inside_debounce_window_is_skipped_and_marker_unchanged() {
        let state_dir = TempDir::new();
        let teams_dir = TempDir::new();
        let fake = FakeHerdr::default();
        let paths = paths_for(teams_dir.path());
        assert!(maybe_pump_at(state_dir.path(), &paths, &fake, 1_000, 2_000));

        let ran_again = maybe_pump_at(state_dir.path(), &paths, &fake, 2_500, 2_000);

        assert!(
            !ran_again,
            "1500ms after the first pass is inside a 2000ms debounce window"
        );
        assert_eq!(
            read_marker(&state_dir.path().join(DEBOUNCE_MARKER_FILE)),
            Some(1_000),
            "marker must not move on a skipped pass"
        );
    }

    #[test]
    fn call_after_debounce_window_elapses_runs_again() {
        let state_dir = TempDir::new();
        let teams_dir = TempDir::new();
        let fake = FakeHerdr::default();
        let paths = paths_for(teams_dir.path());
        assert!(maybe_pump_at(state_dir.path(), &paths, &fake, 1_000, 2_000));

        let ran_again = maybe_pump_at(state_dir.path(), &paths, &fake, 3_100, 2_000);

        assert!(
            ran_again,
            "2100ms after the first pass is outside a 2000ms debounce window"
        );
        assert_eq!(
            read_marker(&state_dir.path().join(DEBOUNCE_MARKER_FILE)),
            Some(3_100)
        );
    }

    #[test]
    fn debounced_pass_makes_no_herdr_calls_at_all() {
        let state_dir = TempDir::new();
        let teams_dir = TempDir::new();
        write_team(teams_dir.path(), "session-a", "session-a-id", "");
        let fake = FakeHerdr::default();
        *fake.agents.borrow_mut() = vec![agent_with_session("w1:p1", "session-a-id")];
        let paths = paths_for(teams_dir.path());
        assert!(maybe_pump_at(state_dir.path(), &paths, &fake, 1_000, 2_000));
        let calls_after_first_pass = fake.calls().len();

        maybe_pump_at(state_dir.path(), &paths, &fake, 1_500, 2_000);

        assert_eq!(
            fake.calls().len(),
            calls_after_first_pass,
            "a debounced call must not touch herdr at all"
        );
    }
}
