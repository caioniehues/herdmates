//! Pure-logic merge of attention sources into one ordered queue (D3, issue
//! #86 commit 3). The focus pane (commit 7) renders this; this module owns
//! no I/O — callers pass in already-parsed data from `teamfiles`/`focusfile`
//! and already-fetched `herdr::AgentInfo`.
//!
//! v1 sources (BRIEF binding decision — no permission-prompt interception,
//! that's v2):
//! - unresolved focus-file decisions (a human explicitly needs to decide)
//! - workers reporting `blocked` agent status via herdr (a worker is stuck)
//! - inbox messages addressed to the team lead (worker reports a human, in
//!   the lead role, would otherwise have to go dig up by hand)
//!
//! Only the lead's inbox is surfaced, not every member's: in this project's
//! usage a human sits in the lead role and reads the lead's inbox to see
//! worker reports (`docs/agents/...`, ADR-0012's "we host + observe" stance).
//! A non-lead teammate's inbox is peer-to-peer traffic the human doesn't
//! need surfaced.
//!
//! Ordering is priority-then-source-order, not true chronological
//! "newest first": none of the three source shapes carries a wall-clock
//! timestamp today (`teamfiles::InboxMessage` has none; herdr's agent
//! status is a snapshot, not a history). Blocked workers sort first (a
//! stuck worker blocks progress until a human looks), then unresolved
//! decisions, then inbox messages. Within a kind, source order is
//! preserved as given by the caller. This is a documented assumption, not
//! a contract from issue #86 — revisit if/when a source gains real
//! timestamps.

use crate::focusfile::{stable_id, FocusFile};
use crate::herdr::AgentInfo;
use crate::teamfiles::Teammate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttentionKind {
    Blocked,
    Decision,
    InboxMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionItem {
    /// Stable across rebuilds; unique within one queue. Prefixed by kind so
    /// ids from different sources never collide even if their content hashes
    /// happened to match.
    pub id: String,
    pub kind: AttentionKind,
    pub summary: String,
    /// Best-effort; `None` when the source has no pane to jump to, or the
    /// pane isn't resolvable pre-shim (ADR-0012 degrade policy, matching
    /// `pump::resolve_lead_pane`).
    pub pane_id: Option<String>,
}

/// Merge blocked-worker status, unresolved focus-file decisions, and the
/// lead's inbox messages into one ordered queue. `lead_pane_id` is the
/// resolved pane for the team's lead (see `pump::resolve_lead_pane`), used
/// to attach a jump target to inbox items; pass `None` when unresolved.
pub fn build_attention_queue(
    agents: &[AgentInfo],
    focus: &FocusFile,
    lead: Option<&Teammate>,
    lead_pane_id: Option<&str>,
) -> Vec<AttentionItem> {
    let mut items = Vec::new();

    for agent in agents {
        if agent.status.as_deref() == Some("blocked") {
            items.push(AttentionItem {
                id: format!("blocked:{}", agent.pane_id),
                kind: AttentionKind::Blocked,
                summary: format!("{} is blocked", agent.pane_id),
                pane_id: Some(agent.pane_id.clone()),
            });
        }
    }

    for decision in &focus.decisions {
        if !decision.resolved {
            items.push(AttentionItem {
                id: format!("decision:{}", decision.id),
                kind: AttentionKind::Decision,
                summary: decision.text.clone(),
                pane_id: None,
            });
        }
    }

    if let Some(lead) = lead {
        for message in &lead.inbox {
            let content = message.content.as_deref().unwrap_or_default();
            let from = message.from_agent_id.as_deref().unwrap_or("unknown");
            let id = stable_id(&format!("{from}|{content}"));
            items.push(AttentionItem {
                id: format!("inbox:{id}"),
                kind: AttentionKind::InboxMessage,
                summary: format!("{from}: {content}"),
                pane_id: lead_pane_id.map(str::to_owned),
            });
        }
    }

    items.sort_by_key(|item| item.kind);
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::focusfile::DecisionEntry;
    use crate::herdr::AgentSession;
    use crate::teamfiles::InboxMessage;

    fn agent(pane_id: &str, status: Option<&str>) -> AgentInfo {
        AgentInfo {
            pane_id: pane_id.to_owned(),
            workspace_id: "w1".to_owned(),
            agent: Some("claude".to_owned()),
            agent_id: None,
            agent_session: Some(AgentSession {
                source: "claude-code".to_owned(),
                agent: "claude".to_owned(),
                kind: "session".to_owned(),
                value: "session-1".to_owned(),
            }),
            status: status.map(str::to_owned),
        }
    }

    fn lead_with_inbox(messages: Vec<InboxMessage>) -> Teammate {
        Teammate {
            name: "team-lead".to_owned(),
            agent_id: "team-lead@t".to_owned(),
            is_lead: true,
            tmux_pane_id: None,
            backend_type: None,
            is_active: true,
            model: None,
            task: None,
            inbox: messages,
        }
    }

    #[test]
    fn empty_sources_give_empty_queue() {
        let queue = build_attention_queue(&[], &FocusFile::default(), None, None);
        assert!(queue.is_empty());
    }

    #[test]
    fn blocked_agent_becomes_attention_item() {
        let agents = [
            agent("w1A:p1", Some("blocked")),
            agent("w1A:p2", Some("working")),
        ];
        let queue = build_attention_queue(&agents, &FocusFile::default(), None, None);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].kind, AttentionKind::Blocked);
        assert_eq!(queue[0].pane_id.as_deref(), Some("w1A:p1"));
        assert_eq!(queue[0].id, "blocked:w1A:p1");
    }

    #[test]
    fn resolved_decisions_are_excluded_unresolved_are_included() {
        let focus = FocusFile {
            task: None,
            next_action: None,
            decisions: vec![
                DecisionEntry {
                    id: "abc".to_owned(),
                    text: "Ship it?".to_owned(),
                    resolved: false,
                },
                DecisionEntry {
                    id: "def".to_owned(),
                    text: "Already decided".to_owned(),
                    resolved: true,
                },
            ],
        };
        let queue = build_attention_queue(&[], &focus, None, None);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].kind, AttentionKind::Decision);
        assert_eq!(queue[0].summary, "Ship it?");
        assert_eq!(queue[0].id, "decision:abc");
    }

    #[test]
    fn lead_inbox_messages_become_attention_items_with_jump_target() {
        let lead = lead_with_inbox(vec![InboxMessage {
            from_agent_id: Some("alpha@t".to_owned()),
            to_agent_id: Some("team-lead@t".to_owned()),
            content: Some("STEP 3 READY".to_owned()),
        }]);
        let queue = build_attention_queue(&[], &FocusFile::default(), Some(&lead), Some("w1A:p1"));
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].kind, AttentionKind::InboxMessage);
        assert_eq!(queue[0].summary, "alpha@t: STEP 3 READY");
        assert_eq!(queue[0].pane_id.as_deref(), Some("w1A:p1"));
    }

    #[test]
    fn non_lead_inbox_is_never_passed_in_so_never_surfaced() {
        // Documents the design boundary: callers only ever pass the lead's
        // Teammate. A peer teammate's inbox simply isn't a valid `lead` arg.
        let peer = Teammate {
            is_lead: false,
            ..lead_with_inbox(vec![InboxMessage {
                from_agent_id: Some("beta@t".to_owned()),
                to_agent_id: Some("alpha@t".to_owned()),
                content: Some("peer chatter".to_owned()),
            }])
        };
        let queue = build_attention_queue(&[], &FocusFile::default(), Some(&peer), None);
        assert_eq!(
            queue.len(),
            1,
            "the function itself doesn't filter on is_lead — callers must pass the right Teammate"
        );
        assert_eq!(queue[0].summary, "beta@t: peer chatter");
    }

    #[test]
    fn ordering_is_blocked_then_decisions_then_inbox() {
        let agents = [agent("w1A:p1", Some("blocked"))];
        let focus = FocusFile {
            task: None,
            next_action: None,
            decisions: vec![DecisionEntry {
                id: "abc".to_owned(),
                text: "Pending".to_owned(),
                resolved: false,
            }],
        };
        let lead = lead_with_inbox(vec![InboxMessage {
            from_agent_id: Some("alpha@t".to_owned()),
            to_agent_id: None,
            content: Some("report".to_owned()),
        }]);
        let queue = build_attention_queue(&agents, &focus, Some(&lead), None);
        assert_eq!(queue.len(), 3);
        assert_eq!(queue[0].kind, AttentionKind::Blocked);
        assert_eq!(queue[1].kind, AttentionKind::Decision);
        assert_eq!(queue[2].kind, AttentionKind::InboxMessage);
    }

    #[test]
    fn ids_are_stable_across_separate_builds() {
        let lead = lead_with_inbox(vec![InboxMessage {
            from_agent_id: Some("alpha@t".to_owned()),
            to_agent_id: None,
            content: Some("same message".to_owned()),
        }]);
        let first = build_attention_queue(&[], &FocusFile::default(), Some(&lead), None);
        let second = build_attention_queue(&[], &FocusFile::default(), Some(&lead), None);
        assert_eq!(first[0].id, second[0].id);
    }

    #[test]
    fn missing_status_is_not_treated_as_blocked() {
        let agents = [agent("w1A:p1", None)];
        let queue = build_attention_queue(&agents, &FocusFile::default(), None, None);
        assert!(queue.is_empty());
    }
}
