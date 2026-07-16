//! Pure-logic parsing of Claude Code native-team files (`~/.claude/teams/`).
//!
//! Unknown JSON fields are silently ignored; all optional member fields default
//! to `None`/`false` so future Claude Code schema additions cannot break the
//! board pump (ADR-0012). The pump provides paths; this module only reads them.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use thiserror::Error;

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum TeamFilesError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
}

// ─── Wire types (JSON shape; serde ignores unknown fields by default) ─────────

#[derive(Deserialize)]
struct TeamConfigWire {
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "leadSessionId", default)]
    lead_session_id: Option<String>,
    #[serde(default)]
    members: Vec<MemberWire>,
}

#[derive(Deserialize)]
struct MemberWire {
    #[serde(rename = "agentId", default)]
    agent_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "agentType", default)]
    agent_type: Option<String>,
    #[serde(rename = "tmuxPaneId", default)]
    tmux_pane_id: Option<String>,
    #[serde(rename = "backendType", default)]
    backend_type: Option<String>,
    #[serde(rename = "isActive", default)]
    is_active: Option<bool>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
}

// ─── Domain model ─────────────────────────────────────────────────────────────

/// Parsed form of `~/.claude/teams/{team}/config.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamConfig {
    pub name: String,
    pub lead_session_id: Option<String>,
    pub members: Vec<Member>,
}

/// One entry from the `members` array of a team config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub agent_id: String,
    pub name: String,
    pub is_lead: bool,
    pub tmux_pane_id: Option<String>,
    pub backend_type: Option<String>,
    pub is_active: bool,
    pub model: Option<String>,
    /// Raw initial prompt / task description supplied at spawn.
    pub prompt: Option<String>,
}

/// One message from an `inboxes/{name}.json` array.
/// All fields optional; unknown fields silently ignored.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct InboxMessage {
    #[serde(rename = "fromAgentId", default)]
    pub from_agent_id: Option<String>,
    #[serde(rename = "toAgentId", default)]
    pub to_agent_id: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
}

/// Aggregated view of one team member: config fields + inbox messages.
///
/// `tmux_pane_id` carries the Claude Code–assigned tmux pane reference.
/// Before the shim exists it will not map to a herdr pane; the pump degrades
/// to a workspace-level fallback or skip rather than erroring (ADR-0012).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Teammate {
    pub name: String,
    pub agent_id: String,
    pub is_lead: bool,
    pub tmux_pane_id: Option<String>,
    pub backend_type: Option<String>,
    pub is_active: bool,
    pub model: Option<String>,
    /// Raw task description from the spawn prompt; truncation is the tokens module's job.
    pub task: Option<String>,
    pub inbox: Vec<InboxMessage>,
}

// ─── I/O (accept caller-provided paths; no path discovery here) ───────────────

pub fn read_team_config(path: &Path) -> Result<TeamConfig, TeamFilesError> {
    let json = std::fs::read_to_string(path).map_err(|source| TeamFilesError::Read {
        path: path.display().to_string(),
        source,
    })?;
    parse_team_config_str(&json, &path.display().to_string())
}

pub fn read_inbox(path: &Path) -> Result<Vec<InboxMessage>, TeamFilesError> {
    let json = std::fs::read_to_string(path).map_err(|source| TeamFilesError::Read {
        path: path.display().to_string(),
        source,
    })?;
    parse_inbox_str(&json, &path.display().to_string())
}

// ─── Pure parsing (pub(crate) so unit tests can drive them without disk I/O) ──

pub(crate) fn parse_team_config_str(json: &str, label: &str) -> Result<TeamConfig, TeamFilesError> {
    let wire: TeamConfigWire =
        serde_json::from_str(json).map_err(|source| TeamFilesError::Parse {
            path: label.to_owned(),
            source,
        })?;
    let members = wire
        .members
        .into_iter()
        .filter_map(member_from_wire)
        .collect();
    Ok(TeamConfig {
        name: wire.name.unwrap_or_default(),
        lead_session_id: wire.lead_session_id,
        members,
    })
}

pub(crate) fn parse_inbox_str(
    json: &str,
    label: &str,
) -> Result<Vec<InboxMessage>, TeamFilesError> {
    serde_json::from_str(json).map_err(|source| TeamFilesError::Parse {
        path: label.to_owned(),
        source,
    })
}

fn member_from_wire(wire: MemberWire) -> Option<Member> {
    let name = wire.name?;
    let agent_id = wire.agent_id.unwrap_or_else(|| name.clone());
    Some(Member {
        agent_id,
        name,
        is_lead: wire.agent_type.as_deref() == Some("team-lead"),
        tmux_pane_id: wire.tmux_pane_id,
        backend_type: wire.backend_type,
        is_active: wire.is_active.unwrap_or(false),
        model: wire.model,
        prompt: wire.prompt,
    })
}

// ─── Pure aggregation ─────────────────────────────────────────────────────────

/// Join member configs with available inboxes.
/// Members without a matching inbox entry receive an empty inbox.
pub fn build_teammates(
    config: &TeamConfig,
    inboxes: &BTreeMap<String, Vec<InboxMessage>>,
) -> Vec<Teammate> {
    config
        .members
        .iter()
        .map(|member| Teammate {
            name: member.name.clone(),
            agent_id: member.agent_id.clone(),
            is_lead: member.is_lead,
            tmux_pane_id: member.tmux_pane_id.clone(),
            backend_type: member.backend_type.clone(),
            is_active: member.is_active,
            model: member.model.clone(),
            task: member.prompt.clone(),
            inbox: inboxes.get(&member.name).cloned().unwrap_or_default(),
        })
        .collect()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(rel: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/teamfiles")
            .join(rel)
    }

    // ── config parsing ────────────────────────────────────────────────────────

    #[test]
    fn lead_only_config_parses_name_and_single_member() {
        let config =
            read_team_config(&fixture("lead-only/config.json")).expect("lead-only fixture");
        assert_eq!(config.name, "session-abc123");
        assert_eq!(
            config.lead_session_id.as_deref(),
            Some("abc123-0000-0000-0000-000000000000")
        );
        assert_eq!(config.members.len(), 1);
        let lead = &config.members[0];
        assert_eq!(lead.name, "team-lead");
        assert!(lead.is_lead);
        assert_eq!(lead.tmux_pane_id.as_deref(), Some("leader"));
        assert_eq!(lead.backend_type.as_deref(), Some("in-process"));
        assert!(!lead.is_active);
    }

    #[test]
    fn two_member_config_captures_teammate_fields() {
        let config =
            read_team_config(&fixture("two-member/config.json")).expect("two-member fixture");
        assert_eq!(config.members.len(), 2);

        let lead = config
            .members
            .iter()
            .find(|m| m.name == "team-lead")
            .expect("lead member");
        assert!(lead.is_lead);
        assert_eq!(lead.backend_type.as_deref(), Some("in-process"));

        let alpha = config
            .members
            .iter()
            .find(|m| m.name == "alpha")
            .expect("alpha member");
        assert!(!alpha.is_lead);
        assert_eq!(alpha.model.as_deref(), Some("claude-opus-4-8"));
        assert_eq!(alpha.tmux_pane_id.as_deref(), Some("%1"));
        assert_eq!(alpha.backend_type.as_deref(), Some("tmux"));
        assert!(alpha.is_active);
        assert!(alpha.prompt.as_deref().is_some_and(|p| p.contains("haiku")));
    }

    #[test]
    fn config_tolerates_unknown_json_fields() {
        let config =
            read_team_config(&fixture("unknown-fields/config.json")).expect("unknown fields");
        assert_eq!(config.name, "session-future");
        assert_eq!(config.members.len(), 1);
        assert_eq!(config.members[0].name, "alpha");
        assert_eq!(config.members[0].tmux_pane_id.as_deref(), Some("%1"));
    }

    #[test]
    fn sparse_config_gives_defaults_without_error() {
        let config = read_team_config(&fixture("sparse/config.json")).expect("sparse/empty config");
        assert!(config.name.is_empty());
        assert!(config.lead_session_id.is_none());
        assert!(config.members.is_empty());
    }

    #[test]
    fn member_without_name_field_is_silently_skipped() {
        let json = r#"{"members":[{"agentId":"x@y"},{"name":"kept","agentId":"kept@y"}]}"#;
        let config = parse_team_config_str(json, "test").expect("parse");
        assert_eq!(config.members.len(), 1);
        assert_eq!(config.members[0].name, "kept");
    }

    #[test]
    fn member_without_agent_id_falls_back_to_name() {
        let json = r#"{"members":[{"name":"alpha"}]}"#;
        let config = parse_team_config_str(json, "test").expect("parse");
        assert_eq!(config.members[0].agent_id, "alpha");
    }

    // ── inbox parsing ─────────────────────────────────────────────────────────

    #[test]
    fn empty_inbox_parses_to_empty_vec() {
        let msgs = read_inbox(&fixture("two-member/inboxes/team-lead.json")).expect("empty inbox");
        assert!(msgs.is_empty());
    }

    #[test]
    fn populated_inbox_parses_known_fields_and_ignores_unknown() {
        let msgs = read_inbox(&fixture("two-member/inboxes/alpha.json")).expect("populated inbox");
        assert_eq!(msgs.len(), 1);
        assert_eq!(
            msgs[0].from_agent_id.as_deref(),
            Some("alpha@session-fixture")
        );
        assert_eq!(
            msgs[0].to_agent_id.as_deref(),
            Some("team-lead@session-fixture")
        );
        assert_eq!(msgs[0].content.as_deref(), Some("Task complete"));
    }

    // ── build_teammates ───────────────────────────────────────────────────────

    fn fixture_config() -> TeamConfig {
        TeamConfig {
            name: "test-team".to_owned(),
            lead_session_id: None,
            members: vec![
                Member {
                    agent_id: "team-lead@test".to_owned(),
                    name: "team-lead".to_owned(),
                    is_lead: true,
                    tmux_pane_id: Some("leader".to_owned()),
                    backend_type: Some("in-process".to_owned()),
                    is_active: false,
                    model: None,
                    prompt: None,
                },
                Member {
                    agent_id: "alpha@test".to_owned(),
                    name: "alpha".to_owned(),
                    is_lead: false,
                    tmux_pane_id: Some("%1".to_owned()),
                    backend_type: Some("tmux".to_owned()),
                    is_active: true,
                    model: Some("claude-opus-4-8".to_owned()),
                    prompt: Some("Do the task".to_owned()),
                },
            ],
        }
    }

    #[test]
    fn build_teammates_maps_all_members() {
        let config = fixture_config();
        let teammates = build_teammates(&config, &BTreeMap::new());
        assert_eq!(teammates.len(), 2);
    }

    #[test]
    fn build_teammates_sets_lead_flag_and_task() {
        let config = fixture_config();
        let teammates = build_teammates(&config, &BTreeMap::new());

        let lead = teammates.iter().find(|t| t.name == "team-lead").unwrap();
        assert!(lead.is_lead);
        assert!(lead.task.is_none());

        let alpha = teammates.iter().find(|t| t.name == "alpha").unwrap();
        assert!(!alpha.is_lead);
        assert_eq!(alpha.task.as_deref(), Some("Do the task"));
    }

    #[test]
    fn build_teammates_joins_inbox_by_name() {
        let config = fixture_config();
        let msg = InboxMessage {
            from_agent_id: Some("alpha@test".to_owned()),
            to_agent_id: Some("team-lead@test".to_owned()),
            content: Some("done".to_owned()),
        };
        let inboxes = BTreeMap::from([("alpha".to_owned(), vec![msg.clone()])]);

        let teammates = build_teammates(&config, &inboxes);
        let lead = teammates.iter().find(|t| t.name == "team-lead").unwrap();
        let alpha = teammates.iter().find(|t| t.name == "alpha").unwrap();

        assert!(lead.inbox.is_empty(), "lead has no inbox entry → empty");
        assert_eq!(alpha.inbox, [msg]);
    }

    #[test]
    fn build_teammates_gives_empty_inbox_when_absent() {
        let config = TeamConfig {
            name: "t".to_owned(),
            lead_session_id: None,
            members: vec![Member {
                agent_id: "a@t".to_owned(),
                name: "a".to_owned(),
                is_lead: false,
                tmux_pane_id: None,
                backend_type: None,
                is_active: false,
                model: None,
                prompt: None,
            }],
        };
        let teammates = build_teammates(&config, &BTreeMap::new());
        assert!(teammates[0].inbox.is_empty());
    }
}
