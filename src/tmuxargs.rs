//! Argv parser for teammux's inventoried tmux verb shapes (issue #85 commit 2).
//!
//! Translate-don't-emulate (issue #85 decision doc): only the shapes below
//! parse; anything else is a loud [`ParseError`], never silent success. The
//! 18 shapes come from `docs/research/spike-tmux-verbs-2026-07-16/REPORT.md`
//! §1/§3 (verbatim fixtures in the tests below, from `.spike-tmux-calls.log`).
//! The tmux geometry format-string fields (`pane_width`, `pane_height`,
//! `pane_left`, `pane_top`, `window_width`, `window_height`) are recognized
//! here too, per `docs/research/cmux-comparative-2026-07-16/REPORT.md`
//! (cmux's `tmuxEnrichContextWithGeometry`) — parsing only; no herdr lookup
//! yet, that lands in a later commit.
//!
//! No real tmux session exists anywhere in the shim's model (cmux
//! comparative research correction a: cmux fakes both the `tmux` binary and
//! the `TMUX`/`TMUX_PANE` env vars) — this parser has no notion of a real
//! tmux socket or session state; `-S`/`-L` are accepted and carried through
//! as opaque metadata, never dereferenced.

use std::collections::VecDeque;
use thiserror::Error;

/// A tmux `%N` (pane) or `@N` (window) id, validated to carry its prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxId(String);

impl TmuxId {
    /// Validate that `raw` carries the `%`/`@` prefix tmux ids always have.
    pub fn parse(raw: &str) -> Result<Self, ParseError> {
        if raw.starts_with('%') || raw.starts_with('@') {
            Ok(Self(raw.to_owned()))
        } else {
            Err(ParseError::InvalidTmuxId {
                value: raw.to_owned(),
            })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Global `-S`/`-L` flags, present on nearly every observed call. Parsed and
/// carried through; never dereferenced (no real tmux server to contact).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlobalFlags {
    pub socket_path: Option<String>,
    pub server_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// `display-message -p #{FIELD}` / `display-message -t TARGET -p #{FIELD}`
/// formats recognized by the shim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayField {
    ClientTermtype,
    WindowId,
    PaneWidth,
    PaneHeight,
    PaneLeft,
    PaneTop,
    WindowWidth,
    WindowHeight,
}

impl DisplayField {
    fn parse(format: &str) -> Result<Self, ParseError> {
        let name = format
            .strip_prefix("#{")
            .and_then(|rest| rest.strip_suffix('}'))
            .ok_or_else(|| ParseError::UnrecognizedShape {
                verb: "display-message".to_owned(),
                argv: vec![format.to_owned()],
            })?;
        match name {
            "client_termtype" => Ok(Self::ClientTermtype),
            "window_id" => Ok(Self::WindowId),
            "pane_width" => Ok(Self::PaneWidth),
            "pane_height" => Ok(Self::PaneHeight),
            "pane_left" => Ok(Self::PaneLeft),
            "pane_top" => Ok(Self::PaneTop),
            "window_width" => Ok(Self::WindowWidth),
            "window_height" => Ok(Self::WindowHeight),
            other => Err(ParseError::UnrecognizedShape {
                verb: "display-message".to_owned(),
                argv: vec![format!("#{{{other}}}")],
            }),
        }
    }
}

/// One of the inventoried tmux verb shapes, parsed into typed fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verb {
    ShowMouse,
    ShowFocusEvents,
    DisplayMessage {
        target: Option<TmuxId>,
        field: DisplayField,
    },
    ListPaneIds {
        window: TmuxId,
    },
    SplitWindow {
        target: TmuxId,
        direction: SplitDirection,
        size: Option<String>,
        command: Vec<String>,
    },
    SetWindowStyle {
        pane: TmuxId,
        style: String,
    },
    SetPaneBorderStyle {
        pane: TmuxId,
        style: String,
    },
    SetPaneActiveBorderStyle {
        pane: TmuxId,
        style: String,
    },
    SetPaneBorderFormat {
        pane: TmuxId,
        format: String,
    },
    SetPaneBorderStatusTop {
        window: TmuxId,
    },
    SetRemainOnExit {
        pane: TmuxId,
        mode: String,
    },
    SelectPaneTitle {
        pane: TmuxId,
        title: String,
    },
    RespawnPane {
        pane: TmuxId,
        command: String,
    },
    SelectLayout {
        window: TmuxId,
        layout: String,
    },
    ResizePane {
        pane: TmuxId,
        amount: String,
    },
    KillPane {
        pane: TmuxId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCall {
    pub globals: GlobalFlags,
    pub verb: Verb,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("empty argv")]
    Empty,
    #[error("unrecognized verb `{verb}`")]
    UnrecognizedVerb { verb: String },
    #[error("unrecognized shape for `{verb}`: {argv:?}")]
    UnrecognizedShape { verb: String, argv: Vec<String> },
    #[error("invalid tmux id `{value}` (expected a leading `%` or `@`)")]
    InvalidTmuxId { value: String },
    #[error("missing value for `{flag}`")]
    MissingArgument { flag: String },
    #[error("trailing arguments after `{verb}`: {remaining:?}")]
    TrailingArguments {
        verb: String,
        remaining: Vec<String>,
    },
}

/// Parse one shim invocation's argv (excluding the program name) into a
/// [`ParsedCall`]. Unknown verbs or shapes fail loudly — never silent success.
pub fn parse(argv: &[String]) -> Result<ParsedCall, ParseError> {
    let mut args: VecDeque<&str> = argv.iter().map(String::as_str).collect();
    let mut globals = GlobalFlags::default();
    loop {
        match args.front().copied() {
            Some("-S") => {
                args.pop_front();
                globals.socket_path = Some(take_value(&mut args, "-S")?);
            }
            Some("-L") => {
                args.pop_front();
                globals.server_name = Some(take_value(&mut args, "-L")?);
            }
            _ => break,
        }
    }

    let verb_name = args.pop_front().ok_or(ParseError::Empty)?;
    let verb = match verb_name {
        "show" => parse_show(&mut args)?,
        "display-message" => parse_display_message(&mut args)?,
        "list-panes" => parse_list_panes(&mut args)?,
        "split-window" => parse_split_window(&mut args)?,
        "set-option" => parse_set_option(&mut args)?,
        "select-pane" => parse_select_pane(&mut args)?,
        "respawn-pane" => parse_respawn_pane(&mut args)?,
        "select-layout" => parse_select_layout(&mut args)?,
        "resize-pane" => parse_resize_pane(&mut args)?,
        "kill-pane" => parse_kill_pane(&mut args)?,
        other => {
            return Err(ParseError::UnrecognizedVerb {
                verb: other.to_owned(),
            })
        }
    };

    if !args.is_empty() {
        return Err(ParseError::TrailingArguments {
            verb: verb_name.to_owned(),
            remaining: args.into_iter().map(str::to_owned).collect(),
        });
    }
    Ok(ParsedCall { globals, verb })
}

fn take_value(args: &mut VecDeque<&str>, flag: &str) -> Result<String, ParseError> {
    args.pop_front()
        .map(str::to_owned)
        .ok_or_else(|| ParseError::MissingArgument {
            flag: flag.to_owned(),
        })
}

fn expect_flag(
    args: &mut VecDeque<&str>,
    flag: &'static str,
    verb: &'static str,
) -> Result<(), ParseError> {
    match args.pop_front() {
        Some(actual) if actual == flag => Ok(()),
        other => Err(ParseError::UnrecognizedShape {
            verb: verb.to_owned(),
            argv: other.into_iter().map(str::to_owned).collect(),
        }),
    }
}

fn parse_show(args: &mut VecDeque<&str>) -> Result<Verb, ParseError> {
    match (args.pop_front(), args.pop_front()) {
        (Some("-Av"), Some("mouse")) => Ok(Verb::ShowMouse),
        (Some("-gv"), Some("focus-events")) => Ok(Verb::ShowFocusEvents),
        (flag, name) => Err(ParseError::UnrecognizedShape {
            verb: "show".to_owned(),
            argv: [flag, name]
                .into_iter()
                .flatten()
                .map(str::to_owned)
                .collect(),
        }),
    }
}

fn parse_display_message(args: &mut VecDeque<&str>) -> Result<Verb, ParseError> {
    let target = if args.front() == Some(&"-t") {
        args.pop_front();
        Some(TmuxId::parse(&take_value(args, "-t")?)?)
    } else {
        None
    };
    expect_flag(args, "-p", "display-message")?;
    let format = take_value(args, "-p")?;
    let field = DisplayField::parse(&format)?;
    Ok(Verb::DisplayMessage { target, field })
}

fn parse_list_panes(args: &mut VecDeque<&str>) -> Result<Verb, ParseError> {
    expect_flag(args, "-t", "list-panes")?;
    let window = TmuxId::parse(&take_value(args, "-t")?)?;
    expect_flag(args, "-F", "list-panes")?;
    let format = take_value(args, "-F")?;
    if format != "#{pane_id}" {
        return Err(ParseError::UnrecognizedShape {
            verb: "list-panes".to_owned(),
            argv: vec![format],
        });
    }
    Ok(Verb::ListPaneIds { window })
}

fn parse_split_window(args: &mut VecDeque<&str>) -> Result<Verb, ParseError> {
    expect_flag(args, "-d", "split-window")?;
    expect_flag(args, "-t", "split-window")?;
    let target = TmuxId::parse(&take_value(args, "-t")?)?;
    let direction = match args.pop_front() {
        Some("-h") => SplitDirection::Horizontal,
        Some("-v") => SplitDirection::Vertical,
        other => {
            return Err(ParseError::UnrecognizedShape {
                verb: "split-window".to_owned(),
                argv: other.into_iter().map(str::to_owned).collect(),
            })
        }
    };
    let size = if args.front() == Some(&"-l") {
        args.pop_front();
        Some(take_value(args, "-l")?)
    } else {
        None
    };
    expect_flag(args, "-P", "split-window")?;
    expect_flag(args, "-F", "split-window")?;
    let format = take_value(args, "-F")?;
    if format != "#{pane_id}" {
        return Err(ParseError::UnrecognizedShape {
            verb: "split-window".to_owned(),
            argv: vec![format],
        });
    }
    expect_flag(args, "--", "split-window")?;
    let command = args.drain(..).map(str::to_owned).collect();
    Ok(Verb::SplitWindow {
        target,
        direction,
        size,
        command,
    })
}

enum Scope {
    Pane,
    Window,
}

fn parse_set_option(args: &mut VecDeque<&str>) -> Result<Verb, ParseError> {
    let scope = match args.pop_front() {
        Some("-p") => Scope::Pane,
        Some("-w") => Scope::Window,
        other => {
            return Err(ParseError::UnrecognizedShape {
                verb: "set-option".to_owned(),
                argv: other.into_iter().map(str::to_owned).collect(),
            })
        }
    };
    expect_flag(args, "-t", "set-option")?;
    let target = TmuxId::parse(&take_value(args, "-t")?)?;
    let option = args
        .pop_front()
        .ok_or_else(|| ParseError::MissingArgument {
            flag: "option-name".to_owned(),
        })?;
    match (scope, option) {
        (Scope::Pane, "window-style") => Ok(Verb::SetWindowStyle {
            pane: target,
            style: take_value(args, "window-style")?,
        }),
        (Scope::Pane, "pane-border-style") => Ok(Verb::SetPaneBorderStyle {
            pane: target,
            style: take_value(args, "pane-border-style")?,
        }),
        (Scope::Pane, "pane-active-border-style") => Ok(Verb::SetPaneActiveBorderStyle {
            pane: target,
            style: take_value(args, "pane-active-border-style")?,
        }),
        (Scope::Pane, "pane-border-format") => Ok(Verb::SetPaneBorderFormat {
            pane: target,
            format: take_value(args, "pane-border-format")?,
        }),
        (Scope::Pane, "remain-on-exit") => Ok(Verb::SetRemainOnExit {
            pane: target,
            mode: take_value(args, "remain-on-exit")?,
        }),
        (Scope::Window, "pane-border-status") => {
            let value = take_value(args, "pane-border-status")?;
            if value != "top" {
                return Err(ParseError::UnrecognizedShape {
                    verb: "set-option".to_owned(),
                    argv: vec![value],
                });
            }
            Ok(Verb::SetPaneBorderStatusTop { window: target })
        }
        (_, other) => Err(ParseError::UnrecognizedShape {
            verb: "set-option".to_owned(),
            argv: vec![other.to_owned()],
        }),
    }
}

fn parse_select_pane(args: &mut VecDeque<&str>) -> Result<Verb, ParseError> {
    expect_flag(args, "-t", "select-pane")?;
    let pane = TmuxId::parse(&take_value(args, "-t")?)?;
    expect_flag(args, "-T", "select-pane")?;
    let title = take_value(args, "-T")?;
    Ok(Verb::SelectPaneTitle { pane, title })
}

fn parse_respawn_pane(args: &mut VecDeque<&str>) -> Result<Verb, ParseError> {
    expect_flag(args, "-k", "respawn-pane")?;
    expect_flag(args, "-t", "respawn-pane")?;
    let pane = TmuxId::parse(&take_value(args, "-t")?)?;
    expect_flag(args, "--", "respawn-pane")?;
    let command = take_value(args, "--")?;
    Ok(Verb::RespawnPane { pane, command })
}

fn parse_select_layout(args: &mut VecDeque<&str>) -> Result<Verb, ParseError> {
    expect_flag(args, "-t", "select-layout")?;
    let window = TmuxId::parse(&take_value(args, "-t")?)?;
    let layout = take_value(args, "layout-name")?;
    Ok(Verb::SelectLayout { window, layout })
}

fn parse_resize_pane(args: &mut VecDeque<&str>) -> Result<Verb, ParseError> {
    expect_flag(args, "-t", "resize-pane")?;
    let pane = TmuxId::parse(&take_value(args, "-t")?)?;
    expect_flag(args, "-x", "resize-pane")?;
    let amount = take_value(args, "-x")?;
    Ok(Verb::ResizePane { pane, amount })
}

fn parse_kill_pane(args: &mut VecDeque<&str>) -> Result<Verb, ParseError> {
    expect_flag(args, "-t", "kill-pane")?;
    let pane = TmuxId::parse(&take_value(args, "-t")?)?;
    Ok(Verb::KillPane { pane })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reconstruct argv from one `.spike-tmux-calls.log` command-column
    /// string: an unescaped-space delimits tokens, `\X` is a literal `X`
    /// (including `\ ` for a literal space inside one argv element — the
    /// logger's convention for showing a single argument that itself
    /// contained spaces, e.g. `respawn-pane`'s shell-command operand).
    fn tokenize(raw: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut in_token = false;
        let mut chars = raw.trim_end().chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                        in_token = true;
                    }
                }
                ' ' => {
                    if in_token {
                        tokens.push(std::mem::take(&mut current));
                        in_token = false;
                    }
                }
                other => {
                    current.push(other);
                    in_token = true;
                }
            }
        }
        if in_token {
            tokens.push(current);
        }
        tokens
    }

    fn parses(raw: &str) -> ParsedCall {
        let argv = tokenize(raw);
        parse(&argv).unwrap_or_else(|error| panic!("failed to parse `{raw}`: {error}"))
    }

    fn with_socket(verb: Verb) -> ParsedCall {
        ParsedCall {
            globals: GlobalFlags {
                socket_path: Some("/tmp/tmux-1000/default".to_owned()),
                server_name: None,
            },
            verb,
        }
    }

    fn no_globals(verb: Verb) -> ParsedCall {
        ParsedCall {
            globals: GlobalFlags::default(),
            verb,
        }
    }

    /// Table of (description, verbatim `.spike-tmux-calls.log` command
    /// column, expected parse). Every one of the spike report's 18 deduped
    /// shapes appears at least once (both `split-window` direction variants
    /// are included to exercise the `-l SIZE` branch).
    fn inventoried_cases() -> Vec<(&'static str, &'static str, ParsedCall)> {
        vec![
            (
                ".spike-tmux-calls.log:1 — show -Av mouse startup probe",
                "show -Av mouse ",
                no_globals(Verb::ShowMouse),
            ),
            (
                ".spike-tmux-calls.log:2 — show -gv focus-events startup probe",
                "show -gv focus-events ",
                no_globals(Verb::ShowFocusEvents),
            ),
            (
                ".spike-tmux-calls.log:3 — display-message client_termtype probe",
                "display-message -p \\#\\{client_termtype\\} ",
                no_globals(Verb::DisplayMessage {
                    target: None,
                    field: DisplayField::ClientTermtype,
                }),
            ),
            (
                ".spike-tmux-calls.log:4 — display-message window_id query",
                "-S /tmp/tmux-1000/default display-message -t %0 -p \\#\\{window_id\\} ",
                with_socket(Verb::DisplayMessage {
                    target: Some(TmuxId::parse("%0").unwrap()),
                    field: DisplayField::WindowId,
                }),
            ),
            (
                ".spike-tmux-calls.log:5 — list-panes pane_id enumeration",
                "-S /tmp/tmux-1000/default list-panes -t @0 -F \\#\\{pane_id\\} ",
                with_socket(Verb::ListPaneIds {
                    window: TmuxId::parse("@0").unwrap(),
                }),
            ),
            (
                ".spike-tmux-calls.log:6 — split-window -h -l 70% (1st teammate)",
                "-S /tmp/tmux-1000/default split-window -d -t %0 -h -l 70% -P -F \\#\\{pane_id\\} -- cat ",
                with_socket(Verb::SplitWindow {
                    target: TmuxId::parse("%0").unwrap(),
                    direction: SplitDirection::Horizontal,
                    size: Some("70%".to_owned()),
                    command: vec!["cat".to_owned()],
                }),
            ),
            (
                ".spike-tmux-calls.log:21 — split-window -v, no -l (2nd teammate)",
                "-S /tmp/tmux-1000/default split-window -d -t %1 -v -P -F \\#\\{pane_id\\} -- cat ",
                with_socket(Verb::SplitWindow {
                    target: TmuxId::parse("%1").unwrap(),
                    direction: SplitDirection::Vertical,
                    size: None,
                    command: vec!["cat".to_owned()],
                }),
            ),
            (
                ".spike-tmux-calls.log:7 — set-option window-style",
                "-S /tmp/tmux-1000/default set-option -p -t %1 window-style bg=default\\,fg=blue ",
                with_socket(Verb::SetWindowStyle {
                    pane: TmuxId::parse("%1").unwrap(),
                    style: "bg=default,fg=blue".to_owned(),
                }),
            ),
            (
                ".spike-tmux-calls.log:8 — set-option pane-border-style",
                "-S /tmp/tmux-1000/default set-option -p -t %1 pane-border-style fg=blue ",
                with_socket(Verb::SetPaneBorderStyle {
                    pane: TmuxId::parse("%1").unwrap(),
                    style: "fg=blue".to_owned(),
                }),
            ),
            (
                ".spike-tmux-calls.log:9 — set-option pane-active-border-style",
                "-S /tmp/tmux-1000/default set-option -p -t %1 pane-active-border-style fg=blue ",
                with_socket(Verb::SetPaneActiveBorderStyle {
                    pane: TmuxId::parse("%1").unwrap(),
                    style: "fg=blue".to_owned(),
                }),
            ),
            (
                ".spike-tmux-calls.log:10 — select-pane title",
                "-S /tmp/tmux-1000/default select-pane -t %1 -T alpha ",
                with_socket(Verb::SelectPaneTitle {
                    pane: TmuxId::parse("%1").unwrap(),
                    title: "alpha".to_owned(),
                }),
            ),
            (
                ".spike-tmux-calls.log:11 — set-option pane-border-format",
                "-S /tmp/tmux-1000/default set-option -p -t %1 pane-border-format \\#\\[fg=blue\\,bold\\]\\ #\\{pane_title\\}\\ #\\[default\\] ",
                with_socket(Verb::SetPaneBorderFormat {
                    pane: TmuxId::parse("%1").unwrap(),
                    format: "#[fg=blue,bold] #{pane_title} #[default]".to_owned(),
                }),
            ),
            (
                ".spike-tmux-calls.log:13 — set-option pane-border-status top",
                "-S /tmp/tmux-1000/default set-option -w -t @0 pane-border-status top ",
                with_socket(Verb::SetPaneBorderStatusTop {
                    window: TmuxId::parse("@0").unwrap(),
                }),
            ),
            (
                ".spike-tmux-calls.log:14 — set-option remain-on-exit",
                "-S /tmp/tmux-1000/default set-option -p -t %1 remain-on-exit failed ",
                with_socket(Verb::SetRemainOnExit {
                    pane: TmuxId::parse("%1").unwrap(),
                    mode: "failed".to_owned(),
                }),
            ),
            (
                ".spike-tmux-calls.log:15 — respawn-pane -k launches the teammate",
                "-S /tmp/tmux-1000/default respawn-pane -k -t %1 -- cd\\ /home/caio/Projects/herdmates-spike-recon\\ \\&\\&\\ env\\ CLAUDECODE=1\\ CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1\\ /home/caio/.local/share/claude/versions/2.1.211\\ --agent-id\\ alpha@session-620e0f77\\ --agent-name\\ alpha\\ --team-name\\ session-620e0f77\\ --agent-color\\ blue\\ --parent-session-id\\ 620e0f77-a2a2-4360-b8e0-2b79ced4d59e\\ --dangerously-skip-permissions\\ --effort\\ medium\\ --settings\\ /home/caio/Projects/herdmates-spike-recon/spike-settings.json\\ --model\\ claude-opus-4-8 ",
                with_socket(Verb::RespawnPane {
                    pane: TmuxId::parse("%1").unwrap(),
                    command: "cd /home/caio/Projects/herdmates-spike-recon && env CLAUDECODE=1 CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1 /home/caio/.local/share/claude/versions/2.1.211 --agent-id alpha@session-620e0f77 --agent-name alpha --team-name session-620e0f77 --agent-color blue --parent-session-id 620e0f77-a2a2-4360-b8e0-2b79ced4d59e --dangerously-skip-permissions --effort medium --settings /home/caio/Projects/herdmates-spike-recon/spike-settings.json --model claude-opus-4-8".to_owned(),
                }),
            ),
            (
                ".spike-tmux-calls.log:28 — select-layout main-vertical",
                "-S /tmp/tmux-1000/default select-layout -t @0 main-vertical ",
                with_socket(Verb::SelectLayout {
                    window: TmuxId::parse("@0").unwrap(),
                    layout: "main-vertical".to_owned(),
                }),
            ),
            (
                ".spike-tmux-calls.log:29 — resize-pane -x shrinks the lead pane",
                "-S /tmp/tmux-1000/default resize-pane -t %0 -x 30% ",
                with_socket(Verb::ResizePane {
                    pane: TmuxId::parse("%0").unwrap(),
                    amount: "30%".to_owned(),
                }),
            ),
            (
                ".spike-tmux-calls.log:35 — kill-pane teardown",
                "-S /tmp/tmux-1000/default kill-pane -t %2 ",
                with_socket(Verb::KillPane {
                    pane: TmuxId::parse("%2").unwrap(),
                }),
            ),
        ]
    }

    #[test]
    fn parses_all_inventoried_verb_shapes() {
        for (name, raw, expected) in inventoried_cases() {
            let argv = tokenize(raw);
            let actual =
                parse(&argv).unwrap_or_else(|error| panic!("{name}: parse failed: {error}"));
            assert_eq!(actual, expected, "{name}");
        }
    }

    /// New shape (cmux comparative research, correction b, 2026-07-16): not
    /// present in the live spike capture, hand-authored from tmux's format
    /// string grammar and cmux's `tmuxEnrichContextWithGeometry` field list.
    #[test]
    fn parses_geometry_format_string_queries() {
        let cases = [
            ("#{pane_width}", DisplayField::PaneWidth, "%1"),
            ("#{pane_height}", DisplayField::PaneHeight, "%1"),
            ("#{pane_left}", DisplayField::PaneLeft, "%1"),
            ("#{pane_top}", DisplayField::PaneTop, "%1"),
            ("#{window_width}", DisplayField::WindowWidth, "@0"),
            ("#{window_height}", DisplayField::WindowHeight, "@0"),
        ];
        for (format, field, target) in cases {
            let raw = format!("-S /tmp/tmux-1000/default display-message -t {target} -p {format} ");
            let escaped = raw
                .replace('#', "\\#")
                .replace('{', "\\{")
                .replace('}', "\\}");
            let actual = parses(&escaped);
            assert_eq!(
                actual,
                with_socket(Verb::DisplayMessage {
                    target: Some(TmuxId::parse(target).unwrap()),
                    field,
                }),
                "geometry field {format}"
            );
        }
    }

    #[test]
    fn global_dash_l_flag_is_parsed_and_carried_through() {
        let actual = parses("-L myserver -S /tmp/tmux-1000/default show -Av mouse ");
        assert_eq!(
            actual,
            ParsedCall {
                globals: GlobalFlags {
                    socket_path: Some("/tmp/tmux-1000/default".to_owned()),
                    server_name: Some("myserver".to_owned()),
                },
                verb: Verb::ShowMouse,
            }
        );
    }

    #[test]
    fn unrecognized_verb_fails_loudly() {
        match parse(&[
            "frobnicate-pane".to_owned(),
            "-t".to_owned(),
            "%0".to_owned(),
        ]) {
            Err(ParseError::UnrecognizedVerb { verb }) => assert_eq!(verb, "frobnicate-pane"),
            other => panic!("expected UnrecognizedVerb, got {other:?}"),
        }
    }

    #[test]
    fn invalid_tmux_id_without_prefix_is_rejected() {
        match parse(&["kill-pane".to_owned(), "-t".to_owned(), "0".to_owned()]) {
            Err(ParseError::InvalidTmuxId { value }) => assert_eq!(value, "0"),
            other => panic!("expected InvalidTmuxId, got {other:?}"),
        }
    }

    #[test]
    fn trailing_arguments_after_a_complete_shape_are_rejected() {
        match parse(&[
            "kill-pane".to_owned(),
            "-t".to_owned(),
            "%0".to_owned(),
            "extra".to_owned(),
        ]) {
            Err(ParseError::TrailingArguments { verb, remaining }) => {
                assert_eq!(verb, "kill-pane");
                assert_eq!(remaining, vec!["extra".to_owned()]);
            }
            other => panic!("expected TrailingArguments, got {other:?}"),
        }
    }

    #[test]
    fn empty_argv_is_rejected() {
        assert_eq!(parse(&[]).unwrap_err(), ParseError::Empty);
    }
}
