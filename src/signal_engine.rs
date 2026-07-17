//! Waiting-reason classification engine (#92/#93, issue #96 stage 1 tracer
//! bullet). Single source of blocked/stalled facts: the focus pane's
//! attention queue (`attention.rs`) and the sidebar's reason badges both
//! consume [`classify`] instead of deriving their own signals, so they
//! cannot disagree (#90).
//!
//! Pure logic only — this module owns no I/O. Callers gather
//! [`ObservedFacts`] from `herdr::AgentInfo`, native task files, and a
//! session-transcript mtime stat, then call [`classify`].
//!
//! Four-class taxonomy, precedence top-down (#92 resolution):
//! permission-prompt > blocked-on-dependency > stalled > turn-complete
//! (unbadged default). Stalled is two-tier ("quiet" soft at `quiet_secs`,
//! "stalled" hard at `stalled_secs`, both configurable via
//! [`StalledThresholds`]), grounded in transcript-mtime liveness (mode-
//! independent, immune to the idle-vs-done attention-state trap) with an
//! unread-inbox accelerator that can pull an idle agent into the "quiet"
//! tier early. Degradation rule: never display a wrong reason — when the
//! facts can't ground any classification (no pane-backed status, no
//! transcript liveness), [`classify`] returns [`WaitingReason::Waiting`]
//! (reason-less) rather than guessing.

/// Facts an engine caller has already gathered from live sources. All
/// `Option`s are `None` when the source has no answer, not when it wasn't
/// consulted — the caller is expected to always consult every source it can
/// reach; a `None` here is what drives the degradation rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct ObservedFacts {
    /// Herdr `agent_status`, verbatim. Pane-backed ground truth for the
    /// permission-prompt class — never inferred from any other signal.
    pub agent_status: AgentActivity,
    /// True when the agent owns a native task (`owner` matches "" or null
    /// per #89 evidence — never a naive string-match) whose `blockedBy` list
    /// has at least one incomplete entry.
    pub owned_task_blocked_by_incomplete: bool,
    /// Seconds since the agent's session-transcript file last changed.
    /// `None` when the transcript path couldn't be resolved or stat'd.
    pub seconds_since_transcript_activity: Option<u64>,
    /// Seconds since the oldest unread message in the agent's inbox.
    /// `None` when there is no unread inbox message.
    pub seconds_since_unread_inbox: Option<u64>,
}

/// Coarse activity read off `herdr::AgentInfo::status`. `Unknown` covers a
/// missing field and any status string this engine doesn't model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub enum AgentActivity {
    Working,
    Idle,
    Done,
    /// Pane-backed permission-prompt signal (herdr `agent_status: "blocked"`).
    Blocked,
    #[default]
    Unknown,
}

impl AgentActivity {
    /// Parse herdr's wire status string. Unrecognized/absent values degrade
    /// to `Unknown` rather than guessing a class (matches `teamfiles`'
    /// tolerant-parsing precedent).
    pub fn from_status_str(status: Option<&str>) -> Self {
        match status {
            Some("working") => Self::Working,
            Some("idle") => Self::Idle,
            Some("done") => Self::Done,
            Some("blocked") => Self::Blocked,
            _ => Self::Unknown,
        }
    }

    fn is_quiescent(self) -> bool {
        matches!(self, Self::Idle | Self::Done)
    }
}

/// Configurable stalled-tier boundaries (#92: "T configurable"). Defaults
/// match the resolution comment: quiet at 5 minutes, stalled at 10 minutes,
/// inbox accelerator at 2 minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StalledThresholds {
    pub quiet_secs: u64,
    pub stalled_secs: u64,
    pub inbox_accelerator_secs: u64,
}

impl Default for StalledThresholds {
    fn default() -> Self {
        Self {
            quiet_secs: 5 * 60,
            stalled_secs: 10 * 60,
            inbox_accelerator_secs: 2 * 60,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum StalledTier {
    Quiet,
    Stalled,
}

/// One reason class, precedence order top-to-bottom matches variant order
/// (`PermissionPrompt` outranks everything below it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum WaitingReason {
    PermissionPrompt,
    BlockedOnDependency,
    Stalled {
        tier: StalledTier,
        secs: u64,
    },
    /// Unbadged default: not waiting on anything the engine can name.
    TurnComplete,
    /// Reason-less degrade (never-wrong-reason doctrine): a signal exists
    /// but isn't strong enough to ground a specific class.
    Waiting,
}

/// Classify one agent's waiting reason from already-gathered facts.
/// Deterministic, total (no panics), and side-effect free.
pub fn classify(facts: &ObservedFacts, thresholds: &StalledThresholds) -> WaitingReason {
    if facts.agent_status == AgentActivity::Blocked {
        return WaitingReason::PermissionPrompt;
    }

    if facts.owned_task_blocked_by_incomplete {
        return WaitingReason::BlockedOnDependency;
    }

    if facts.agent_status.is_quiescent() {
        let transcript_secs = facts.seconds_since_transcript_activity;
        let inbox_secs = facts.seconds_since_unread_inbox;

        let stalled = transcript_secs.is_some_and(|s| s >= thresholds.stalled_secs);
        if stalled {
            return WaitingReason::Stalled {
                tier: StalledTier::Stalled,
                secs: transcript_secs.expect("stalled implies Some"),
            };
        }

        let quiet_from_transcript = transcript_secs.is_some_and(|s| s >= thresholds.quiet_secs);
        let quiet_from_inbox = inbox_secs.is_some_and(|s| s >= thresholds.inbox_accelerator_secs);
        if quiet_from_transcript || quiet_from_inbox {
            // Report whichever signal actually grounds the claim; prefer
            // transcript (the liveness ground truth) when both fired.
            let secs = transcript_secs.or(inbox_secs).unwrap_or(0);
            return WaitingReason::Stalled {
                tier: StalledTier::Quiet,
                secs,
            };
        }

        if facts.agent_status == AgentActivity::Unknown {
            // Quiescence itself is unverified (no pane-backed status) and no
            // liveness/inbox signal fired either — nothing to ground.
            return WaitingReason::Waiting;
        }

        return WaitingReason::TurnComplete;
    }

    if facts.agent_status == AgentActivity::Unknown {
        return WaitingReason::Waiting;
    }

    WaitingReason::TurnComplete
}

/// Herdr 0.7.4 `report-metadata --token` value cap this badge must fit
/// under, per the sidebar's telegraphic-token convention (`docs/marketplace-
/// notes.md` precedent, tokens.rs `MAX_TOKEN_VALUE_CHARS` is the *field*
/// cap; a reason badge is meant to be read at a glance, so it targets a
/// much tighter visible-character budget than that hard limit).
pub const MAX_BADGE_VISIBLE_CHARS: usize = 20;

/// Render a sidebar reason badge from engine output. `blocking_task_label`
/// is an already-truncated caller-supplied label (e.g. `"#3"`) for the
/// blocked-on-dependency class; pass `None` when the blocking task id isn't
/// available. Returns `None` for the unbadged-default classes
/// (`TurnComplete`, and the reason-less `Waiting` degrade — neither names a
/// concrete reason worth a badge).
pub fn reason_badge(reason: WaitingReason, blocking_task_label: Option<&str>) -> Option<String> {
    let badge = match reason {
        WaitingReason::PermissionPrompt => "permission".to_owned(),
        WaitingReason::BlockedOnDependency => match blocking_task_label {
            Some(label) => format!("blocked\u{2192}{label}"),
            None => "blocked".to_owned(),
        },
        WaitingReason::Stalled { tier, secs } => {
            format!("{} {}m", tier.label(), secs / 60)
        }
        WaitingReason::TurnComplete | WaitingReason::Waiting => return None,
    };
    Some(badge.chars().take(MAX_BADGE_VISIBLE_CHARS).collect())
}

impl StalledTier {
    fn label(self) -> &'static str {
        match self {
            Self::Quiet => "quiet",
            Self::Stalled => "stalled",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(agent_status: AgentActivity) -> ObservedFacts {
        ObservedFacts {
            agent_status,
            ..Default::default()
        }
    }

    #[test]
    fn permission_prompt_outranks_everything() {
        let mut f = facts(AgentActivity::Blocked);
        f.owned_task_blocked_by_incomplete = true;
        f.seconds_since_transcript_activity = Some(10 * 60);
        assert_eq!(
            classify(&f, &StalledThresholds::default()),
            WaitingReason::PermissionPrompt
        );
    }

    #[test]
    fn blocked_on_dependency_outranks_stalled() {
        let mut f = facts(AgentActivity::Idle);
        f.owned_task_blocked_by_incomplete = true;
        f.seconds_since_transcript_activity = Some(20 * 60);
        assert_eq!(
            classify(&f, &StalledThresholds::default()),
            WaitingReason::BlockedOnDependency
        );
    }

    #[test]
    fn quiet_tier_at_five_minutes() {
        let mut f = facts(AgentActivity::Idle);
        f.seconds_since_transcript_activity = Some(300);
        assert_eq!(
            classify(&f, &StalledThresholds::default()),
            WaitingReason::Stalled {
                tier: StalledTier::Quiet,
                secs: 300
            }
        );
    }

    #[test]
    fn stalled_tier_at_ten_minutes() {
        let mut f = facts(AgentActivity::Idle);
        f.seconds_since_transcript_activity = Some(600);
        assert_eq!(
            classify(&f, &StalledThresholds::default()),
            WaitingReason::Stalled {
                tier: StalledTier::Stalled,
                secs: 600
            }
        );
    }

    #[test]
    fn just_under_quiet_threshold_is_turn_complete() {
        let mut f = facts(AgentActivity::Idle);
        f.seconds_since_transcript_activity = Some(299);
        assert_eq!(
            classify(&f, &StalledThresholds::default()),
            WaitingReason::TurnComplete
        );
    }

    #[test]
    fn thresholds_are_configurable() {
        let mut f = facts(AgentActivity::Idle);
        f.seconds_since_transcript_activity = Some(90);
        let custom = StalledThresholds {
            quiet_secs: 60,
            stalled_secs: 120,
            inbox_accelerator_secs: 30,
        };
        assert_eq!(
            classify(&f, &custom),
            WaitingReason::Stalled {
                tier: StalledTier::Quiet,
                secs: 90
            }
        );
    }

    #[test]
    fn unread_inbox_accelerates_into_quiet_tier_before_transcript_would() {
        let mut f = facts(AgentActivity::Idle);
        // Transcript alone (90s) wouldn't clear the 5m quiet bar, but the
        // inbox has been unread for the 2m accelerator window.
        f.seconds_since_transcript_activity = Some(90);
        f.seconds_since_unread_inbox = Some(130);
        assert_eq!(
            classify(&f, &StalledThresholds::default()),
            WaitingReason::Stalled {
                tier: StalledTier::Quiet,
                secs: 90
            }
        );
    }

    #[test]
    fn inbox_accelerator_alone_grounds_quiet_tier_with_no_transcript_fact() {
        let mut f = facts(AgentActivity::Idle);
        f.seconds_since_unread_inbox = Some(130);
        assert_eq!(
            classify(&f, &StalledThresholds::default()),
            WaitingReason::Stalled {
                tier: StalledTier::Quiet,
                secs: 130
            }
        );
    }

    #[test]
    fn working_agent_with_no_other_signal_is_turn_complete() {
        let f = facts(AgentActivity::Working);
        assert_eq!(
            classify(&f, &StalledThresholds::default()),
            WaitingReason::TurnComplete
        );
    }

    #[test]
    fn degrades_to_reason_less_waiting_when_nothing_grounds_a_claim() {
        // No pane-backed status, no dependency fact, no liveness signal at
        // all: the never-wrong-reason doctrine forbids guessing a class.
        let f = facts(AgentActivity::Unknown);
        assert_eq!(
            classify(&f, &StalledThresholds::default()),
            WaitingReason::Waiting
        );
    }

    #[test]
    fn unknown_status_still_degrades_even_with_a_dependency_fact_absent() {
        let mut f = facts(AgentActivity::Unknown);
        f.seconds_since_transcript_activity = None;
        f.seconds_since_unread_inbox = None;
        assert_eq!(
            classify(&f, &StalledThresholds::default()),
            WaitingReason::Waiting
        );
    }

    #[test]
    fn from_status_str_matches_herdr_wire_values() {
        assert_eq!(
            AgentActivity::from_status_str(Some("blocked")),
            AgentActivity::Blocked
        );
        assert_eq!(
            AgentActivity::from_status_str(Some("idle")),
            AgentActivity::Idle
        );
        assert_eq!(
            AgentActivity::from_status_str(Some("working")),
            AgentActivity::Working
        );
        assert_eq!(
            AgentActivity::from_status_str(Some("done")),
            AgentActivity::Done
        );
        assert_eq!(
            AgentActivity::from_status_str(Some("unrecognized")),
            AgentActivity::Unknown
        );
        assert_eq!(AgentActivity::from_status_str(None), AgentActivity::Unknown);
    }

    #[test]
    fn stalled_badge_is_tier_and_minutes() {
        assert_eq!(
            reason_badge(
                WaitingReason::Stalled {
                    tier: StalledTier::Stalled,
                    secs: 720
                },
                None
            ),
            Some("stalled 12m".to_owned())
        );
        assert_eq!(
            reason_badge(
                WaitingReason::Stalled {
                    tier: StalledTier::Quiet,
                    secs: 300
                },
                None
            ),
            Some("quiet 5m".to_owned())
        );
    }

    #[test]
    fn blocked_on_dependency_badge_includes_label_when_given() {
        assert_eq!(
            reason_badge(WaitingReason::BlockedOnDependency, Some("#3")),
            Some("blocked\u{2192}#3".to_owned())
        );
        assert_eq!(
            reason_badge(WaitingReason::BlockedOnDependency, None),
            Some("blocked".to_owned())
        );
    }

    #[test]
    fn permission_prompt_badge_is_stable() {
        assert_eq!(
            reason_badge(WaitingReason::PermissionPrompt, None),
            Some("permission".to_owned())
        );
    }

    #[test]
    fn unbadged_default_classes_render_no_badge() {
        assert_eq!(reason_badge(WaitingReason::TurnComplete, None), None);
        assert_eq!(reason_badge(WaitingReason::Waiting, None), None);
    }

    #[test]
    fn all_badges_stay_within_the_telegraphic_visible_char_budget() {
        let cases = [
            reason_badge(WaitingReason::PermissionPrompt, None),
            reason_badge(WaitingReason::BlockedOnDependency, Some("#3")),
            reason_badge(
                WaitingReason::Stalled {
                    tier: StalledTier::Stalled,
                    secs: 59 * 60,
                },
                None,
            ),
        ];
        for badge in cases.into_iter().flatten() {
            assert!(
                badge.chars().count() <= MAX_BADGE_VISIBLE_CHARS,
                "badge {badge:?} exceeds {MAX_BADGE_VISIBLE_CHARS} visible chars"
            );
        }
    }
}
