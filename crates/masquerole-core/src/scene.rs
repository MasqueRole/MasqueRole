//! Scenes with several characters: who is present, who a message is for, and who saw what.
//!
//! Everything here is pure. The story keeps the facts of a scene on its nodes (see
//! [`NodeScene`]), and these functions read them back along the path of the story, so
//! branching and rewinding restore the right scene by themselves.

use std::collections::HashMap;

use crate::character::Character;
use crate::db::{Node, NodeId, NodeKind, NodeScene};

/// The scene records of the nodes on a path.
pub type Scenes = HashMap<NodeId, NodeScene>;

/// Most characters that answer one message when the setting does not say otherwise.
pub const DEFAULT_MAX_RESPONDERS: usize = 2;

/// Words that address everyone who is present.
const EVERYONE: &[&str] = &[
    "everyone",
    "everybody",
    "all of you",
    "you all",
    "you both",
    "you two",
    "you three",
];

/// Why the characters who answer were picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The message names them: `@Name`, a name at its start, or a name it ends on.
    Direct,
    /// The message mentions them and nobody else had the floor.
    Mentioned,
    /// They were in the conversation already.
    Floor,
    /// They are the only one present.
    OnlyOne,
    /// Nobody could be picked. The narrator answers.
    Nobody,
}

/// Who answers a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    /// The characters who answer, in order. Empty when the narrator answers.
    pub responders: Vec<String>,
    /// The characters the message was newly addressed to. Empty when it continues the
    /// conversation that was going on.
    pub addressed: Vec<String>,
    /// Why they were picked.
    pub reason: Reason,
}

/// Who is present at the end of `path`: the newest list of present characters set on it,
/// or everyone in `cast` when none was. Only names in `cast` are kept, in cast order.
pub fn present_on(path: &[Node], scenes: &Scenes, cast: &[String]) -> Vec<String> {
    let set = path
        .iter()
        .rev()
        .find_map(|node| scenes.get(&node.id).and_then(|s| s.present.as_ref()));
    cast.iter()
        .filter(|name| set.is_none_or(|set| set.contains(name)))
        .cloned()
        .collect()
}

/// Who has the floor at the end of `path`: the characters the conversation is with.
///
/// The newest message addressed to someone sets the group. A character outside that group
/// who spoke afterwards takes the floor alone, and without any addressed message the last
/// speaker has it.
pub fn floor(path: &[Node], scenes: &Scenes, cast: &[String]) -> Vec<String> {
    let mut speaker: Option<&str> = None;
    for node in path.iter().rev() {
        if node.kind == NodeKind::Character && speaker.is_none() {
            speaker = node
                .speaker
                .as_deref()
                .filter(|name| cast.iter().any(|c| c == name));
        }
        let Some(scene) = scenes.get(&node.id) else {
            continue;
        };
        if scene.addressed.is_empty() {
            continue;
        }
        let group: Vec<String> = scene
            .addressed
            .iter()
            .filter(|name| cast.contains(name))
            .cloned()
            .collect();
        return match speaker {
            Some(s) if !group.iter().any(|g| g == s) => vec![s.to_string()],
            _ => group,
        };
    }
    speaker.map(|s| vec![s.to_string()]).unwrap_or_default()
}

/// Whether `who` saw a message with this scene record. A message without a record, or
/// without a list of witnesses, was seen by everyone.
pub fn witnessed(scene: Option<&NodeScene>, who: &str) -> bool {
    scene
        .and_then(|s| s.witnesses.as_ref())
        .is_none_or(|w| w.iter().any(|name| name == who))
}

/// The nodes of `path` that `who` saw, oldest first.
pub fn witnessed_by<'a>(path: &'a [Node], scenes: &Scenes, who: &str) -> Vec<&'a Node> {
    path.iter()
        .filter(|node| witnessed(scenes.get(&node.id), who))
        .collect()
}

/// The words a character is called by: the full name, each part of it that no other
/// character shares, and the aliases on the card.
fn call_names(character: &Character, cast: &[Character]) -> Vec<String> {
    let mut names = vec![character.name.trim().to_lowercase()];
    for part in character.name.split_whitespace() {
        let part = part
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();
        let shared = cast.iter().any(|other| {
            other.name != character.name
                && other.name.split_whitespace().any(|p| {
                    p.trim_matches(|c: char| !c.is_alphanumeric())
                        .to_lowercase()
                        == part
                })
        });
        if part.chars().count() >= 2 && !shared {
            names.push(part);
        }
    }
    names.extend(
        character
            .aliases
            .iter()
            .map(|a| a.trim().to_lowercase())
            .filter(|a| !a.is_empty()),
    );
    names.sort_by_key(|n| std::cmp::Reverse(n.len()));
    names.dedup();
    names
}

/// Where `needle` occurs in `haystack` as a whole word, from `from` on.
fn find_word(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    haystack[from..]
        .match_indices(needle)
        .find_map(|(at, found)| {
            let start = from + at;
            let before = haystack[..start].chars().next_back();
            let after = haystack[start + found.len()..].chars().next();
            let clear = |c: Option<char>| !c.is_some_and(char::is_alphanumeric);
            (clear(before) && clear(after)).then_some(start)
        })
}

/// Where a character is named in `text` (already lowercase): the first position of any of
/// its call names, and how long that name is.
fn first_mention(text: &str, names: &[String]) -> Option<(usize, usize)> {
    names
        .iter()
        .filter_map(|n| find_word(text, n, 0).map(|at| (at, n.len())))
        .min_by_key(|(at, _)| *at)
}

/// Whether the mention at `at..at + len` addresses the character: `@Name`, a name that
/// opens the message, or a name the message ends on after a comma.
fn is_direct(text: &str, at: usize, len: usize) -> bool {
    let before = &text[..at];
    let after = &text[at + len..];
    if before.trim_end().ends_with('@') && before.ends_with('@') {
        return true;
    }
    if before.trim().is_empty() {
        return true;
    }
    let tail_is_end = after
        .chars()
        .all(|c| matches!(c, '?' | '!' | '.' | ' ' | '"' | '\u{201d}'));
    before.trim_end().ends_with(',') && tail_is_end
}

/// The character a typed name stands for: the full name in any case, or a part of the name
/// or a nickname that no other character shares.
pub fn lookup<'a>(cast: &'a [Character], query: &str) -> Option<&'a Character> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return None;
    }
    let mut found = cast.iter().filter(|c| call_names(c, cast).contains(&query));
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

/// Decide who answers `text`.
///
/// In order: a direct address (`@Name`, a name opening the message, a name closing it after
/// a comma, or words like "everyone"); then names mentioned in the text when nobody had the
/// floor; then the characters who had the floor; then the only one present. When none of
/// that picks anyone, [`Reason::Nobody`] says the narrator answers.
///
/// Only characters in `present` can answer. At most `max` answer.
pub fn resolve(
    text: &str,
    cast: &[Character],
    present: &[String],
    floor: &[String],
    max: usize,
) -> Resolution {
    let max = max.max(1);
    let lower = text.to_lowercase();
    let here: Vec<&Character> = present
        .iter()
        .filter_map(|name| cast.iter().find(|c| &c.name == name))
        .collect();

    let mut direct: Vec<(usize, String)> = Vec::new();
    let mut mentioned: Vec<(usize, String)> = Vec::new();
    for character in &here {
        let names = call_names(character, cast);
        if let Some((at, len)) = first_mention(&lower, &names) {
            let list = if is_direct(&lower, at, len) {
                &mut direct
            } else {
                &mut mentioned
            };
            list.push((at, character.name.clone()));
        }
    }
    direct.sort();
    mentioned.sort();
    let names_of = |list: Vec<(usize, String)>| -> Vec<String> {
        list.into_iter().map(|(_, name)| name).collect()
    };

    let everyone = EVERYONE.iter().any(|w| find_word(&lower, w, 0).is_some());
    let pick = |names: Vec<String>, reason: Reason, addressed: bool| {
        let responders: Vec<String> = names.into_iter().take(max).collect();
        Resolution {
            addressed: if addressed {
                responders.clone()
            } else {
                Vec::new()
            },
            responders,
            reason,
        }
    };

    if !direct.is_empty() {
        return pick(names_of(direct), Reason::Direct, true);
    }
    if everyone && !present.is_empty() {
        return pick(present.to_vec(), Reason::Direct, true);
    }
    let on_floor: Vec<String> = floor
        .iter()
        .filter(|name| present.contains(name))
        .cloned()
        .collect();
    if on_floor.is_empty() && !mentioned.is_empty() {
        return pick(names_of(mentioned), Reason::Mentioned, true);
    }
    if !on_floor.is_empty() {
        return pick(on_floor, Reason::Floor, false);
    }
    if let [only] = present {
        return pick(vec![only.clone()], Reason::OnlyOne, true);
    }
    Resolution {
        responders: Vec::new(),
        addressed: Vec::new(),
        reason: Reason::Nobody,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SessionId;

    fn card(name: &str, aliases: &[&str]) -> Character {
        Character {
            name: name.into(),
            aliases: aliases.iter().map(|a| a.to_string()).collect(),
            ..Character::default()
        }
    }

    fn cast() -> Vec<Character> {
        vec![
            card("Mira Valen", &["Cartographer"]),
            card("Orrin Hale", &[]),
            card("Mira Stone", &[]),
        ]
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn node(id: i64, kind: NodeKind, speaker: Option<&str>) -> Node {
        Node {
            id: NodeId(id),
            session_id: SessionId(1),
            parent_id: (id > 1).then_some(NodeId(id - 1)),
            kind,
            speaker: speaker.map(Into::into),
            content: String::new(),
            label: None,
            bookmark: None,
            active_lore: Vec::new(),
            created_at: 0,
        }
    }

    fn addressed(to: &[&str]) -> NodeScene {
        NodeScene {
            addressed: names(to),
            ..NodeScene::default()
        }
    }

    #[test]
    fn a_name_that_opens_the_message_addresses_that_character() {
        let c = cast();
        let present = names(&["Mira Valen", "Orrin Hale"]);
        for text in [
            "Orrin, what was that?",
            "orrin what was that",
            "@Orrin hello",
            "Hello there, Orrin?",
        ] {
            let r = resolve(text, &c, &present, &[], 2);
            assert_eq!(r.responders, names(&["Orrin Hale"]), "{text}");
            assert_eq!(r.reason, Reason::Direct, "{text}");
            assert_eq!(r.addressed, names(&["Orrin Hale"]), "{text}");
        }
    }

    #[test]
    fn nicknames_and_first_names_work_but_shared_ones_do_not() {
        let c = cast();
        let present = names(&["Mira Valen", "Orrin Hale", "Mira Stone"]);
        let r = resolve("Cartographer, look at this", &c, &present, &[], 2);
        assert_eq!(r.responders, names(&["Mira Valen"]));
        // "Mira" alone belongs to two characters, so it names neither.
        let r = resolve("Mira, look at this", &c, &present, &[], 2);
        assert_eq!(r.reason, Reason::Nobody);
        let r = resolve("Mira Stone, look at this", &c, &present, &[], 2);
        assert_eq!(r.responders, names(&["Mira Stone"]));
        let r = resolve("Valen, look at this", &c, &present, &[], 2);
        assert_eq!(r.responders, names(&["Mira Valen"]));
    }

    #[test]
    fn a_name_inside_a_sentence_only_counts_when_nobody_had_the_floor() {
        let c = cast();
        let present = names(&["Mira Valen", "Orrin Hale"]);
        let r = resolve("What do you think of Orrin?", &c, &present, &[], 2);
        assert_eq!(
            (r.reason, r.responders),
            (Reason::Mentioned, names(&["Orrin Hale"]))
        );
        let r = resolve("What do you think, Orrin?", &c, &present, &[], 2);
        assert_eq!(
            (r.reason, r.responders),
            (Reason::Direct, names(&["Orrin Hale"]))
        );
        let r = resolve("I think Orrin is hiding something", &c, &present, &[], 2);
        assert_eq!(
            (r.reason, r.responders),
            (Reason::Mentioned, names(&["Orrin Hale"]))
        );
        let r = resolve(
            "I think Orrin is hiding something",
            &c,
            &present,
            &names(&["Mira Valen"]),
            2,
        );
        assert_eq!(
            (r.reason, r.responders),
            (Reason::Floor, names(&["Mira Valen"]))
        );
        assert!(
            r.addressed.is_empty(),
            "continuing a conversation addresses no one anew"
        );
    }

    #[test]
    fn everyone_answers_up_to_the_cap() {
        let c = cast();
        let present = names(&["Mira Valen", "Orrin Hale", "Mira Stone"]);
        let r = resolve("Everyone, listen up.", &c, &present, &[], 2);
        assert_eq!(r.responders, names(&["Mira Valen", "Orrin Hale"]));
        let r = resolve("Everyone, listen up.", &c, &present, &[], 3);
        assert_eq!(r.responders.len(), 3);
    }

    #[test]
    fn the_only_character_present_answers_and_nobody_leaves_it_to_the_narrator() {
        let c = cast();
        let r = resolve("Hello?", &c, &names(&["Orrin Hale"]), &[], 2);
        assert_eq!(
            (r.reason, r.responders),
            (Reason::OnlyOne, names(&["Orrin Hale"]))
        );
        let r = resolve("Hello?", &c, &names(&["Orrin Hale", "Mira Valen"]), &[], 2);
        assert_eq!((r.reason, r.responders), (Reason::Nobody, Vec::new()));
    }

    #[test]
    fn absent_characters_cannot_be_called() {
        let c = cast();
        let present = names(&["Mira Valen", "Orrin Hale"]);
        let r = resolve("Mira Stone, where are you?", &c, &present, &[], 2);
        assert_eq!(r.reason, Reason::Nobody);
        let r = resolve("Carry on.", &c, &present, &names(&["Mira Stone"]), 2);
        assert_eq!(
            r.reason,
            Reason::Nobody,
            "someone who left no longer has the floor"
        );
    }

    #[test]
    fn part_of_a_word_is_not_a_name() {
        let c = vec![card("Al", &[]), card("Bo", &[])];
        let present = names(&["Al", "Bo"]);
        let r = resolve("Albert came by", &c, &present, &[], 2);
        assert_eq!(r.reason, Reason::Nobody);
    }

    #[test]
    fn the_floor_follows_the_conversation() {
        let cast = names(&["Mira Valen", "Orrin Hale"]);
        let path = vec![
            node(1, NodeKind::User, None),
            node(2, NodeKind::Character, Some("Mira Valen")),
        ];
        let mut scenes = Scenes::new();
        assert_eq!(floor(&path, &scenes, &cast), names(&["Mira Valen"]));

        // The player addressed Orrin, then Orrin answered.
        let path = vec![
            node(1, NodeKind::User, None),
            node(2, NodeKind::Character, Some("Orrin Hale")),
        ];
        scenes.insert(NodeId(1), addressed(&["Orrin Hale"]));
        assert_eq!(floor(&path, &scenes, &cast), names(&["Orrin Hale"]));

        // The player addressed both; both answered. They keep the floor together.
        let path = vec![
            node(1, NodeKind::User, None),
            node(2, NodeKind::Character, Some("Mira Valen")),
            node(3, NodeKind::Character, Some("Orrin Hale")),
        ];
        scenes.insert(NodeId(1), addressed(&["Mira Valen", "Orrin Hale"]));
        assert_eq!(
            floor(&path, &scenes, &cast),
            names(&["Mira Valen", "Orrin Hale"])
        );

        // Someone the player did not address cut in: they have it alone.
        scenes.insert(NodeId(1), addressed(&["Mira Valen"]));
        assert_eq!(floor(&path, &scenes, &cast), names(&["Orrin Hale"]));

        assert!(floor(&[], &Scenes::new(), &cast).is_empty());
    }

    #[test]
    fn presence_comes_from_the_newest_record_on_the_path() {
        let cast = names(&["Mira Valen", "Orrin Hale"]);
        let path = vec![
            node(1, NodeKind::User, None),
            node(2, NodeKind::System, None),
        ];
        let mut scenes = Scenes::new();
        assert_eq!(present_on(&path, &scenes, &cast), cast);
        scenes.insert(
            NodeId(2),
            NodeScene {
                present: Some(names(&["Orrin Hale", "Gone Person"])),
                ..NodeScene::default()
            },
        );
        assert_eq!(present_on(&path, &scenes, &cast), names(&["Orrin Hale"]));
        assert_eq!(
            present_on(&path[..1], &scenes, &cast),
            cast,
            "before it changed"
        );
    }

    #[test]
    fn a_message_without_a_record_was_seen_by_everyone() {
        let seen = NodeScene {
            witnesses: Some(names(&["Orrin Hale"])),
            ..NodeScene::default()
        };
        assert!(witnessed(None, "Mira Valen"));
        assert!(witnessed(Some(&NodeScene::default()), "Mira Valen"));
        assert!(witnessed(Some(&seen), "Orrin Hale"));
        assert!(!witnessed(Some(&seen), "Mira Valen"));
        let path = vec![node(1, NodeKind::User, None), node(2, NodeKind::User, None)];
        let scenes = Scenes::from([(NodeId(2), seen)]);
        let ids: Vec<_> = witnessed_by(&path, &scenes, "Mira Valen")
            .iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(ids, vec![NodeId(1)]);
    }
}
