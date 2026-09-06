//! Pure-logic mapping from a parsed [`Teammate`] to a bounded, truncated
//! sidebar token set for `pane report-metadata` (D1 agent board, ADR-0012).
//!
//! Tokens are display-only (CONTEXT.md: "Sidebar token") and render as
//! `$name` in `[ui.sidebar.agents] rows`; semantic state stays with herdr's
//! own agent-status detection.

use crate::brain::FiveState;
use crate::teamfiles::Teammate;

/// `--source` value the board pump reports under (ADR-0012 D1; distinct from
/// the legacy `crate::metadata::SOURCE`).
pub const SOURCE: &str = "herdmates-board";

/// Every token name [`teammate_tokens`] can publish, in publish order.
///
/// This is the **clear vocabulary**: a name here is one the pump is entitled
/// to remove from a pane when it stops publishing it. It exists because
/// `pane report-metadata --token` only ever SETS — omitting a token leaves
/// whatever value the pane already had, forever. A live lead sat frozen at
/// `reap: 10 to stop` for three days with zero teammates left, because the
/// N -> 0 transition publishes nothing and nothing cleared the old value.
///
/// ⚠ **Clearing is NOT scoped to the reporting source** (measured
/// 2026-09-06: `--source herdmates-board --clear-token idle_ok` removed a
/// token published by an entirely different source). So every name here must
/// be one herdmates genuinely owns. Adding a name that another producer also
/// publishes — a statusline, another plugin — would silently wipe their
/// value on every pass. Check what else writes to the target panes before
/// extending this list.
pub const PUBLISHABLE_TOKEN_NAMES: [&str; 4] = ["task", "status", "reap", "model"];

/// Pure: the [`PUBLISHABLE_TOKEN_NAMES`] entries `set` does not carry — the
/// names a caller must explicitly clear so a stale value cannot survive.
/// Order follows the vocabulary, so the emitted argv is deterministic.
pub(crate) fn absent_names(set: &TokenSet) -> Vec<&'static str> {
    PUBLISHABLE_TOKEN_NAMES
        .into_iter()
        .filter(|name| !set.tokens().iter().any(|token| token.name == *name))
        .collect()
}

/// Herdr 0.7.4 hard limit on one token's rendered value.
pub const MAX_TOKEN_VALUE_CHARS: usize = 80;

/// Herdr 0.7.4 hard limit on tokens attached in a single `report-metadata` call.
pub const MAX_TOKENS_PER_REPORT: usize = 16;

/// One named, display-only sidebar value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub name: String,
    pub value: String,
}

/// An ordered, budget-enforced token set ready for one `report-metadata` call.
/// Always holds at most [`MAX_TOKENS_PER_REPORT`] tokens, each truncated to
/// at most [`MAX_TOKEN_VALUE_CHARS`] characters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenSet(Vec<Token>);

impl TokenSet {
    pub fn tokens(&self) -> &[Token] {
        &self.0
    }
}

impl IntoIterator for TokenSet {
    type Item = Token;
    type IntoIter = std::vec::IntoIter<Token>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// Sidebar rendering of one of amq-noc's five ambient states (issue #124,
/// ADR-0015 ambient layer: "permission-prompt distinct from
/// working/idle"). Reuses [`FiveState::label`] verbatim for the base
/// word — single-source rule #90, sidebar and brain verbs (`why`,
/// `roster`) must never disagree — and prepends an attention marker only
/// for `NeedsYou` (a permission prompt: the one state a human must act
/// on), so it reads as more urgent at a glance than working/idle even
/// though herdr renders every token value as plain, uncolored text
/// (display-only, never semantic — herdr 0.7.4 contract, ADR-0013).
///
/// Note: a lead whose status could not be read (herdr unreachable, pane
/// gone) classifies to the reason-less `Waiting`, so the sidebar shows
/// `waiting` for an unknown state — intentionally the same word `why`/
/// `roster` use for that degrade (single-source rule #90), not a claim
/// the lead is actually paused.
pub(crate) fn state_value(state: FiveState) -> String {
    if matches!(state, FiveState::NeedsYou) {
        format!("!! {}", state.label())
    } else {
        state.label().to_owned()
    }
}

/// Derive sidebar tokens from a parsed teammate and its priority-
/// differentiated ambient state (issue #124). `state` is computed by the
/// caller via `signal_engine::classify` + `brain::five_state` — this
/// module only renders, it never re-derives the classification.
///
/// `reap_count` is `brain::reapable_count` for the team this teammate
/// leads: how many of its teammates have finished and are still running.
/// It gets a token because a teammate is a persistent session that only
/// `TaskStop` ends, so an accumulating count is exactly the ambient signal
/// nothing carried before — a lead that spawned eleven agents kept eleven
/// live sessions and had no indication of it. Zero produces no token, like
/// every other absent source here.
///
/// Priority order (survives the budget cap first): `task`, `status`,
/// `reap`, `model` — `reap` above `model` because it names something to
/// act on. A source field that is absent or empty produces no token for
/// that name rather than an empty placeholder.
pub(crate) fn teammate_tokens(
    teammate: &Teammate,
    state: FiveState,
    reap_count: usize,
) -> TokenSet {
    let mut candidates = Vec::new();
    if let Some(task) = non_empty(teammate.task.as_deref()) {
        candidates.push(Token {
            name: "task".to_owned(),
            value: task.to_owned(),
        });
    }
    candidates.push(Token {
        name: "status".to_owned(),
        value: state_value(state),
    });
    if reap_count > 0 {
        candidates.push(Token {
            name: "reap".to_owned(),
            value: format!("{reap_count} to stop"),
        });
    }
    if let Some(model) = non_empty(teammate.model.as_deref()) {
        candidates.push(Token {
            name: "model".to_owned(),
            value: model.to_owned(),
        });
    }
    build_token_set(candidates)
}

/// Enforce the token budget over candidate tokens: cap to
/// [`MAX_TOKENS_PER_REPORT`] entries (earlier candidates win — callers order
/// by priority) and truncate each value to [`MAX_TOKEN_VALUE_CHARS`]
/// characters (Unicode scalar values, not bytes).
pub fn build_token_set(candidates: Vec<Token>) -> TokenSet {
    TokenSet(
        candidates
            .into_iter()
            .take(MAX_TOKENS_PER_REPORT)
            .map(|token| Token {
                value: truncate(&token.value),
                ..token
            })
            .collect(),
    )
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

fn truncate(value: &str) -> String {
    value.chars().take(MAX_TOKEN_VALUE_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn teammate(task: Option<&str>, is_active: bool, model: Option<&str>) -> Teammate {
        Teammate {
            name: "alpha".to_owned(),
            agent_id: "alpha@test".to_owned(),
            is_lead: false,
            tmux_pane_id: Some("%1".to_owned()),
            backend_type: Some("tmux".to_owned()),
            is_active,
            model: model.map(str::to_owned),
            task: task.map(str::to_owned),
            inbox: Vec::new(),
        }
    }

    #[test]
    fn a_set_reports_every_vocabulary_name_it_does_not_carry() {
        // Only `status` is unconditional, so a bare teammate with no task,
        // no model and nothing to reap leaves the other three absent.
        let set = teammate_tokens(&teammate(None, true, None), FiveState::Online, 0);

        assert_eq!(absent_names(&set), ["task", "reap", "model"]);
    }

    #[test]
    fn a_full_set_has_no_absent_names_to_clear() {
        let set = teammate_tokens(
            &teammate(Some("review the diff"), true, Some("claude-opus-4-8")),
            FiveState::Online,
            2,
        );

        assert!(absent_names(&set).is_empty());
    }

    #[test]
    fn every_name_the_builder_can_publish_is_in_the_clearable_vocabulary() {
        // The guard that keeps the clear path honest as tokens are added: a
        // name published but missing from the vocabulary can never be
        // cleared, which is exactly the `reap` bug this vocabulary exists to
        // prevent (it froze at "10 to stop" on a live pane for three days).
        let maximal = teammate_tokens(&teammate(Some("t"), true, Some("m")), FiveState::Online, 3);

        for token in maximal.tokens() {
            assert!(
                PUBLISHABLE_TOKEN_NAMES.contains(&token.name.as_str()),
                "`{}` is published but absent from PUBLISHABLE_TOKEN_NAMES, so nothing can ever clear it",
                token.name
            );
        }
    }

    #[test]
    fn a_reap_count_becomes_its_own_token_ahead_of_the_model() {
        let set = teammate_tokens(
            &teammate(Some("review the diff"), true, Some("claude-opus-4-8")),
            FiveState::Online,
            2,
        );

        let names = set
            .tokens()
            .iter()
            .map(|token| token.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["task", "status", "reap", "model"]);
        assert_eq!(
            set.tokens()
                .iter()
                .find(|token| token.name == "reap")
                .map(|token| token.value.as_str()),
            Some("2 to stop")
        );
    }

    #[test]
    fn no_reap_token_when_nothing_is_waiting_to_be_reaped() {
        let set = teammate_tokens(&teammate(None, true, None), FiveState::Online, 0);

        assert!(
            !set.tokens().iter().any(|token| token.name == "reap"),
            "an absent count produces no token rather than a `0` placeholder"
        );
    }

    #[test]
    fn full_teammate_produces_task_status_model_in_priority_order() {
        let set = teammate_tokens(
            &teammate(Some("write the haiku"), true, Some("claude-opus-4-8")),
            FiveState::Online,
            0,
        );

        assert_eq!(
            set.tokens(),
            [
                Token {
                    name: "task".to_owned(),
                    value: "write the haiku".to_owned()
                },
                Token {
                    name: "status".to_owned(),
                    value: "online".to_owned()
                },
                Token {
                    name: "model".to_owned(),
                    value: "claude-opus-4-8".to_owned()
                },
            ]
        );
    }

    #[test]
    fn absent_task_and_model_are_skipped_not_emitted_empty() {
        let set = teammate_tokens(&teammate(None, false, None), FiveState::Waiting, 0);

        assert_eq!(
            set.tokens(),
            [Token {
                name: "status".to_owned(),
                value: "waiting".to_owned()
            }]
        );
    }

    #[test]
    fn empty_string_task_is_treated_as_absent() {
        let set = teammate_tokens(&teammate(Some(""), true, None), FiveState::Online, 0);

        assert!(set.tokens().iter().all(|token| token.name != "task"));
    }

    #[test]
    fn status_token_reflects_passed_in_five_state() {
        assert_eq!(
            teammate_tokens(&teammate(None, true, None), FiveState::Online, 0).tokens()[0].value,
            "online"
        );
        assert_eq!(
            teammate_tokens(&teammate(None, true, None), FiveState::NeedsYou, 0).tokens()[0].value,
            "!! needs-you"
        );
    }

    // ── state_value (issue #124, ADR-0015 ambient layer) ───────────────────

    #[test]
    fn five_states_render_distinct_values() {
        let values: Vec<String> = [
            FiveState::Online,
            FiveState::NeedsYou,
            FiveState::Blocked,
            FiveState::Waiting,
            FiveState::Stale,
        ]
        .into_iter()
        .map(state_value)
        .collect();
        let unique: std::collections::BTreeSet<&String> = values.iter().collect();
        assert_eq!(
            unique.len(),
            values.len(),
            "every ambient state must render a distinct sidebar value: {values:?}"
        );
    }

    #[test]
    fn needs_you_alone_carries_the_attention_marker() {
        assert!(
            state_value(FiveState::NeedsYou).starts_with("!!"),
            "permission-prompt must be the most visually prominent state"
        );
        for other in [
            FiveState::Online,
            FiveState::Blocked,
            FiveState::Waiting,
            FiveState::Stale,
        ] {
            assert!(
                !state_value(other).starts_with("!!"),
                "only needs-you may use the attention marker, got it on {other:?}"
            );
        }
    }

    #[test]
    fn state_values_reuse_brains_five_state_vocabulary_verbatim() {
        assert_eq!(state_value(FiveState::Online), "online");
        assert_eq!(state_value(FiveState::Blocked), "blocked");
        assert_eq!(state_value(FiveState::Waiting), "waiting");
        assert_eq!(state_value(FiveState::Stale), "stale");
        assert_eq!(state_value(FiveState::NeedsYou), "!! needs-you");
    }

    #[test]
    fn all_state_values_stay_within_the_token_value_budget() {
        for state in [
            FiveState::Online,
            FiveState::NeedsYou,
            FiveState::Blocked,
            FiveState::Waiting,
            FiveState::Stale,
        ] {
            assert!(state_value(state).chars().count() <= MAX_TOKEN_VALUE_CHARS);
        }
    }

    #[test]
    fn build_token_set_truncates_values_at_80_unicode_chars() {
        let long_value = "é".repeat(200);
        let set = build_token_set(vec![Token {
            name: "task".to_owned(),
            value: long_value,
        }]);

        assert_eq!(set.tokens()[0].value.chars().count(), MAX_TOKEN_VALUE_CHARS);
    }

    #[test]
    fn build_token_set_caps_at_16_preserving_priority_order() {
        let candidates = (0..20)
            .map(|i| Token {
                name: format!("token-{i}"),
                value: format!("value-{i}"),
            })
            .collect::<Vec<_>>();

        let set = build_token_set(candidates);

        assert_eq!(set.tokens().len(), MAX_TOKENS_PER_REPORT);
        assert_eq!(set.tokens()[0].name, "token-0");
        assert_eq!(set.tokens()[15].name, "token-15");
    }

    #[test]
    fn short_value_is_unaffected_by_truncation() {
        let set = build_token_set(vec![Token {
            name: "status".to_owned(),
            value: "active".to_owned(),
        }]);

        assert_eq!(set.tokens()[0].value, "active");
    }
}
