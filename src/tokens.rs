//! Pure-logic mapping from a parsed [`Teammate`] to a bounded, truncated
//! sidebar token set for `pane report-metadata` (D1 agent board, ADR-0012).
//!
//! Tokens are display-only (CONTEXT.md: "Sidebar token") and render as
//! `$name` in `[ui.sidebar.agents] rows`; semantic state stays with herdr's
//! own agent-status detection.

use crate::teamfiles::Teammate;

/// `--source` value the board pump reports under (ADR-0012 D1; distinct from
/// the legacy `crate::metadata::SOURCE`).
pub const SOURCE: &str = "herdmates-board";

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

/// Derive sidebar tokens from a parsed teammate.
///
/// Priority order (survives the budget cap first): `task`, `status`,
/// `model`. A source field that is absent or empty produces no token for
/// that name rather than an empty placeholder.
pub fn teammate_tokens(teammate: &Teammate) -> TokenSet {
    let mut candidates = Vec::new();
    if let Some(task) = non_empty(teammate.task.as_deref()) {
        candidates.push(Token {
            name: "task".to_owned(),
            value: task.to_owned(),
        });
    }
    candidates.push(Token {
        name: "status".to_owned(),
        value: if teammate.is_active { "active" } else { "idle" }.to_owned(),
    });
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
    fn full_teammate_produces_task_status_model_in_priority_order() {
        let set = teammate_tokens(&teammate(
            Some("write the haiku"),
            true,
            Some("claude-opus-4-8"),
        ));

        assert_eq!(
            set.tokens(),
            [
                Token {
                    name: "task".to_owned(),
                    value: "write the haiku".to_owned()
                },
                Token {
                    name: "status".to_owned(),
                    value: "active".to_owned()
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
        let set = teammate_tokens(&teammate(None, false, None));

        assert_eq!(
            set.tokens(),
            [Token {
                name: "status".to_owned(),
                value: "idle".to_owned()
            }]
        );
    }

    #[test]
    fn empty_string_task_is_treated_as_absent() {
        let set = teammate_tokens(&teammate(Some(""), true, None));

        assert!(set.tokens().iter().all(|token| token.name != "task"));
    }

    #[test]
    fn status_token_reflects_is_active_flag() {
        assert_eq!(
            teammate_tokens(&teammate(None, true, None)).tokens()[0].value,
            "active"
        );
        assert_eq!(
            teammate_tokens(&teammate(None, false, None)).tokens()[0].value,
            "idle"
        );
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
