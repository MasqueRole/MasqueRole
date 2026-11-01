//! Turning command results into text for the chat view.

use masquerole_core::commands::CommandOutput;
use masquerole_core::db::{Node, NodeId};

/// `1 message`, `2 messages`: the count with `noun` in the right number. `noun` takes a plain
/// `s` in the plural.
pub fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// A message from the app itself, shown in the chat but not part of the story.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    /// The text.
    pub text: String,
    /// Show in a fixed-width font, for aligned output.
    pub monospace: bool,
}

impl Notice {
    /// A notice in the normal font.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            monospace: false,
        }
    }

    fn aligned(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            monospace: true,
        }
    }
}

/// Describe what a command did.
pub fn notice(output: &CommandOutput) -> Notice {
    match output {
        CommandOutput::Saved { session } => {
            Notice::plain(format!("Saved \u{201c}{}\u{201d}.", session.name))
        }
        CommandOutput::Loaded { session, .. } => {
            Notice::plain(format!("Loaded \u{201c}{}\u{201d}.", session.name))
        }
        CommandOutput::Regenerate { .. } => Notice::plain("Regenerating the last response."),
        CommandOutput::Compact => Notice::plain("Compacting the story so far."),
        CommandOutput::Moved(snapshot) => Notice::plain(format!(
            "Now at #{}: {}",
            snapshot.node.id.0,
            preview(&snapshot.node)
        )),
        CommandOutput::Tree(text) => Notice::aligned(text.trim_end()),
        CommandOutput::Branched(point) => {
            let name = point
                .label
                .as_deref()
                .map(|l| format!(" as \u{201c}{l}\u{201d}"))
                .unwrap_or_default();
            let others = match point.existing_children {
                0 => String::new(),
                n => format!(
                    " It already has {n} continuation{}.",
                    if n == 1 { "" } else { "s" }
                ),
            };
            Notice::plain(format!(
                "Forked at #{}{name}.{others} What you write next starts a new storyline.",
                point.node.0
            ))
        }
        CommandOutput::Marked { node, label } => Notice::plain(format!(
            "Bookmarked #{} as \u{201c}{label}\u{201d}.",
            node.0
        )),
        CommandOutput::Branches(branches) if branches.is_empty() => {
            Notice::plain("There are no branches yet.")
        }
        CommandOutput::Branches(branches) => {
            let lines: Vec<String> = branches
                .iter()
                .map(|b| {
                    format!(
                        "{} #{:<4} {:<3} {}",
                        if b.contains_current {
                            "\u{25c6}"
                        } else {
                            "\u{25cb}"
                        },
                        b.tip.0,
                        b.depth,
                        b.label
                    )
                })
                .collect();
            Notice::aligned(format!("   tip   len  label\n{}", lines.join("\n")))
        }
        CommandOutput::Pruned(pruned) => Notice::plain(format!(
            "Deleted {} node{}.",
            pruned.removed,
            if pruned.removed == 1 { "" } else { "s" }
        )),
        CommandOutput::Facts(facts) if facts.is_empty() => {
            Notice::plain("No facts remembered yet.")
        }
        CommandOutput::Facts(facts) => {
            let lines: Vec<String> = facts
                .iter()
                .map(|f| format!("#{} {}", f.id.0, f.text))
                .collect();
            Notice::plain(format!("Remembered facts:\n{}", lines.join("\n")))
        }
        CommandOutput::Forgot(fact) => Notice::plain(format!("Forgot: {}", fact.text)),
        CommandOutput::Summary(Some(summary)) => Notice::plain(summary.text.clone()),
        CommandOutput::Summary(None) => Notice::plain("There is no summary yet."),
        CommandOutput::Character { name, added: true } => {
            Notice::plain(format!("{name} joined the scene."))
        }
        CommandOutput::Character { name, added: false } => {
            Notice::plain(format!("{name} is already in the scene."))
        }
        CommandOutput::Style(profile) => Notice::plain(format!("Style set to {profile}.")),
        CommandOutput::Narrator(true) => Notice::plain("Narrator mode is on."),
        CommandOutput::Narrator(false) => Notice::plain("Narrator mode is off."),
    }
}

/// A node's creation time for display, in the local time zone: `30 Oct 14:32`.
pub fn timestamp(unix_seconds: i64) -> String {
    format_time(unix_seconds, &chrono::Local)
}

/// [`timestamp`] in a given time zone, so tests do not depend on where they run.
fn format_time<Tz: chrono::TimeZone>(unix_seconds: i64, zone: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    chrono::DateTime::from_timestamp(unix_seconds, 0)
        .map(|time| time.with_timezone(zone).format("%-d %b %H:%M").to_string())
        .unwrap_or_default()
}

/// A token count for display: `950`, `12.3k`, `1.5M`. Tokens are only estimated, so the
/// short form is plenty.
pub fn tokens(count: i64) -> String {
    let count = count.max(0);
    let short = |value: f64, unit: &str| {
        let text = format!("{value:.1}");
        format!("{}{unit}", text.strip_suffix(".0").unwrap_or(&text))
    };
    match count {
        0..=999 => count.to_string(),
        1_000..=999_949 => short(count as f64 / 1_000.0, "k"),
        _ => short(count as f64 / 1_000_000.0, "M"),
    }
}

/// The tag of a message, such as `#12`. It is unique in the database and is what `/goto`
/// and `/prune` accept.
pub fn tag(id: NodeId) -> String {
    format!("#{}", id.0)
}

/// A short single-line preview of a node's text.
pub fn preview(node: &Node) -> String {
    const MAX: usize = 60;
    let text = node
        .content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if text.chars().count() <= MAX {
        return text;
    }
    let cut: String = text.chars().take(MAX - 1).collect();
    format!("{}\u{2026}", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use masquerole_core::db::{
        Fact, FactId, NodeId, NodeKind, Session, SessionId, Summary, SummaryId,
    };
    use masquerole_core::tree::{Branch, BranchPoint, Pruned, Snapshot};

    use super::*;

    fn node(id: i64, content: &str) -> Node {
        Node {
            id: NodeId(id),
            session_id: SessionId(1),
            parent_id: None,
            kind: NodeKind::User,
            speaker: None,
            content: content.into(),
            label: None,
            bookmark: None,
            active_lore: vec![],
            created_at: 0,
        }
    }

    fn session(name: &str) -> Session {
        Session {
            id: SessionId(1),
            name: name.into(),
            world: None,
            current_node_id: None,
            created_at: 0,
            updated_at: 0,
            tokens_used: 0,
        }
    }

    fn fact(id: i64, text: &str) -> Fact {
        Fact {
            id: FactId(id),
            session_id: SessionId(1),
            node_id: NodeId(1),
            text: text.into(),
            created_at: 0,
        }
    }

    #[test]
    fn every_output_has_a_notice() {
        let snapshot = Snapshot {
            node: node(7, "I step inside."),
            path: vec![],
            facts: vec![],
            summary: None,
        };
        let cases: Vec<(CommandOutput, &str)> = vec![
            (
                CommandOutput::Saved {
                    session: session("Harbor"),
                },
                "Saved \u{201c}Harbor\u{201d}.",
            ),
            (
                CommandOutput::Loaded {
                    session: session("Reef"),
                    snapshot: None,
                },
                "Loaded \u{201c}Reef\u{201d}.",
            ),
            (
                CommandOutput::Regenerate {
                    from: snapshot.clone(),
                    replaced: None,
                },
                "Regenerating",
            ),
            (CommandOutput::Moved(snapshot), "Now at #7: I step inside."),
            (
                CommandOutput::Marked {
                    node: NodeId(3),
                    label: "fork".into(),
                },
                "Bookmarked #3 as \u{201c}fork\u{201d}.",
            ),
            (
                CommandOutput::Pruned(Pruned {
                    removed: 1,
                    current: None,
                }),
                "Deleted 1 node.",
            ),
            (
                CommandOutput::Pruned(Pruned {
                    removed: 4,
                    current: None,
                }),
                "Deleted 4 nodes.",
            ),
            (
                CommandOutput::Forgot(fact(2, "Mira lies.")),
                "Forgot: Mira lies.",
            ),
            (CommandOutput::Summary(None), "no summary"),
            (
                CommandOutput::Summary(Some(Summary {
                    id: SummaryId(1),
                    session_id: SessionId(1),
                    node_id: NodeId(1),
                    text: "They met.".into(),
                    created_at: 0,
                })),
                "They met.",
            ),
            (CommandOutput::Style("noir".into()), "Style set to noir."),
            (CommandOutput::Narrator(true), "on."),
            (CommandOutput::Narrator(false), "off."),
        ];
        for (output, expected) in cases {
            let text = notice(&output).text;
            assert!(text.contains(expected), "{output:?} -> {text:?}");
        }
    }

    #[test]
    fn the_tree_is_shown_in_a_fixed_width_font() {
        let n = notice(&CommandOutput::Tree("\u{25cf} Start  [#1]\n".into()));
        assert!(n.monospace);
        assert_eq!(n.text, "\u{25cf} Start  [#1]");
        assert!(!notice(&CommandOutput::Narrator(true)).monospace);
    }

    #[test]
    fn forks_describe_existing_continuations() {
        let point = |children, label: Option<&str>| {
            notice(&CommandOutput::Branched(BranchPoint {
                node: NodeId(5),
                label: label.map(Into::into),
                existing_children: children,
            }))
            .text
        };
        assert!(point(0, None).starts_with("Forked at #5."));
        assert!(
            point(1, Some("x")).contains("as \u{201c}x\u{201d}. It already has 1 continuation.")
        );
        assert!(point(3, None).contains("3 continuations"));
    }

    #[test]
    fn facts_and_branches_list_their_items() {
        assert!(notice(&CommandOutput::Facts(vec![]))
            .text
            .contains("No facts"));
        let text = notice(&CommandOutput::Facts(vec![fact(1, "A."), fact(2, "B.")])).text;
        assert!(text.contains("#1 A.") && text.contains("#2 B."));

        assert!(notice(&CommandOutput::Branches(vec![]))
            .text
            .contains("no branches"));
        let branches = vec![
            Branch {
                tip: NodeId(7),
                label: "Shipwreck".into(),
                depth: 4,
                forked_at: Some(NodeId(2)),
                contains_current: false,
            },
            Branch {
                tip: NodeId(9),
                label: "Hunted".into(),
                depth: 5,
                forked_at: Some(NodeId(3)),
                contains_current: true,
            },
        ];
        let n = notice(&CommandOutput::Branches(branches));
        assert!(n.monospace);
        assert!(n.text.contains("\u{25cb} #7") && n.text.contains("\u{25c6} #9"));
    }

    #[test]
    fn characters_report_whether_they_joined() {
        let joined = notice(&CommandOutput::Character {
            name: "Mira".into(),
            added: true,
        })
        .text;
        let already = notice(&CommandOutput::Character {
            name: "Mira".into(),
            added: false,
        })
        .text;
        assert!(joined.contains("joined") && already.contains("already"));
    }

    #[test]
    fn timestamps_show_day_month_and_time() {
        // 2026-10-30 14:32:00 UTC.
        assert_eq!(format_time(1_793_370_720, &chrono::Utc), "30 Oct 14:32");
        // A single-digit day has no leading zero; midnight keeps its zeros.
        assert_eq!(format_time(1_767_225_600, &chrono::Utc), "1 Jan 00:00");
        assert_eq!(
            format_time(i64::MAX, &chrono::Utc),
            "",
            "out of range is blank"
        );
        assert!(!timestamp(0).is_empty());
    }

    #[test]
    fn token_counts_are_short() {
        assert_eq!(tokens(-5), "0");
        assert_eq!(tokens(0), "0");
        assert_eq!(tokens(950), "950");
        assert_eq!(tokens(1_000), "1k");
        assert_eq!(tokens(12_345), "12.3k");
        assert_eq!(tokens(200_000), "200k");
        assert_eq!(tokens(1_500_000), "1.5M");
    }

    #[test]
    fn previews_are_one_line_and_short() {
        assert_eq!(preview(&node(1, "a\n  b")), "a b");
        let long = preview(&node(1, &"word ".repeat(40)));
        assert!(long.chars().count() <= 60 && long.ends_with('\u{2026}'));
    }

    #[test]
    fn counts_agree_with_their_noun() {
        assert_eq!(plural(0, "message"), "0 messages");
        assert_eq!(plural(1, "message"), "1 message");
        assert_eq!(plural(2, "storyline"), "2 storylines");
    }
}
