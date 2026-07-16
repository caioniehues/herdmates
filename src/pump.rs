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

use crate::herdr::{AgentInfo, HerdrApi};
use crate::teamfiles::{self, InboxMessage, TeamConfig};
use crate::tokens;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PumpError {
    #[error("cannot resolve the Claude Code team files directory: set HOME")]
    UnresolvedTeamsRoot,
}

pub fn pump_board_command(_args: &[String]) -> Result<(), PumpError> {
    let teams_root = default_teams_root()?;
    let herdr = crate::herdr::HerdrClient::from_env();
    pump_once(&teams_root, &herdr);
    Ok(())
}

fn default_teams_root() -> Result<PathBuf, PumpError> {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".claude/teams"))
        .ok_or(PumpError::UnresolvedTeamsRoot)
}

/// One pump pass: discover every team under `teams_root`, resolve each
/// team's lead to a herdr pane, and publish that lead's sidebar tokens.
/// Never errors — any per-team or per-teammate failure (missing/malformed
/// file, unresolvable pane, herdr call failure) is skipped silently, per
/// ADR-0012's degrade policy.
pub fn pump_once<H: HerdrApi>(teams_root: &Path, herdr: &H) {
    let team_dirs = discover_team_dirs(teams_root);
    if team_dirs.is_empty() {
        return;
    }
    let Ok(agents) = herdr.agent_list() else {
        return;
    };

    for team_dir in team_dirs {
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

        let token_set = tokens::teammate_tokens(lead);
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
/// sorted by directory name for a deterministic pass order.
pub fn discover_team_dirs(teams_root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(teams_root) else {
        return Vec::new();
    };
    let mut dirs = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join("config.json").is_file())
        .collect::<Vec<_>>();
    dirs.sort();
    dirs
}

fn read_inboxes(inboxes_dir: &Path) -> BTreeMap<String, Vec<InboxMessage>> {
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
    use crate::herdr::{test_support::FakeHerdr, AgentSession};
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

        pump_once(temp.path(), &fake);

        let calls = fake.calls();
        assert_eq!(
            calls,
            [
                "agent_list",
                "pane_report_tokens:w1:pLead:herdmates-board:task=Coordinate the team,status=idle"
            ]
        );
    }

    #[test]
    fn pump_once_skips_teams_with_no_resolvable_lead_session() {
        let temp = TempDir::new();
        write_team(temp.path(), "session-a", "session-a-id", "");
        let fake = FakeHerdr::default();
        // Realistic degrade path: herdr reachable, but no pane's detected
        // session matches this team's lead — never an error, just a skip.
        *fake.agents.borrow_mut() = vec![agent_with_session("w1:p1", "unrelated-session")];

        pump_once(temp.path(), &fake);

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

        pump_once(temp.path(), &fake);

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

        let token_set = tokens::teammate_tokens(lead);

        assert_eq!(
            token_set
                .tokens()
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["task", "status", "model"]
        );
    }
}
