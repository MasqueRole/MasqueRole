//! Import and export of characters in the formats other roleplay tools use.
//!
//! Supported: Character Card V2 and V1 as JSON, V2 and V3 cards embedded in a PNG picture
//! (the `chara` / `ccv3` text chunk), and MasqueRole's own YAML. Export writes V2 JSON, V2 PNG,
//! or YAML. A MasqueRole character has a few things V2 has no field for (nicknames, a speaking
//! style); they travel in the card's `extensions` and in the personality text, so a card
//! exported here and imported again comes back unchanged.

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use base64::Engine;
use serde_json::{json, Map, Value};

use crate::cards::{Card, CardError};
use crate::character::{Character, ExampleDialogue};

/// The first eight bytes of every PNG file.
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// The text chunk keyword of a V2 (or V1) card.
const KEY_V2: &str = "chara";

/// The text chunk keyword of a V3 card.
const KEY_V3: &str = "ccv3";

/// Key under `extensions` for what only MasqueRole understands.
const EXTENSION: &str = "masquerole";

/// How the speaking style is marked inside the V2 personality text.
const STYLE_PREFIX: &str = "Speaking style:";

/// The separator between example exchanges in V2's `mes_example`.
const START: &str = "<START>";

/// Why a character could not be imported or exported.
#[derive(Debug, thiserror::Error)]
pub enum ExchangeError {
    /// A PNG file without a character card inside.
    #[error("this picture has no character card inside")]
    NoCardInPicture,
    /// A PNG file that is cut off or damaged.
    #[error("the picture is damaged: {0}")]
    BadPicture(&'static str),
    /// The embedded text is not valid base64.
    #[error("the card inside the picture is damaged")]
    BadEncoding,
    /// The file is not text.
    #[error("this file is not a character card (it is not text)")]
    NotText,
    /// JSON that is not a character card.
    #[error("not a character card: {0}")]
    BadJson(String),
    /// A MasqueRole YAML card that was rejected.
    #[error(transparent)]
    Card(#[from] CardError),
}

/// A character read from a file.
#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    /// The character, without a picture reference. Store [`Imported::picture`] and set the
    /// card's `image` to it.
    pub character: Character,
    /// The picture of a PNG card, as the original file bytes. `None` for text cards.
    pub picture: Option<Vec<u8>>,
    /// Parts of the card that MasqueRole has no place for, such as a lorebook. They are not
    /// imported.
    pub skipped: Vec<String>,
}

/// The formats a character can be exported in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    /// A V2 card as a PNG picture with the card inside.
    Png,
    /// A V2 card as JSON.
    Json,
    /// MasqueRole's own YAML.
    Yaml,
}

impl ExportFormat {
    /// The file extension, without a dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Json => "json",
            Self::Yaml => "yaml",
        }
    }

    /// The format a file name asks for, by its extension. Unknown extensions give `None`.
    pub fn from_file_name(name: &str) -> Option<Self> {
        let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
        match extension.as_str() {
            "png" => Some(Self::Png),
            "json" => Some(Self::Json),
            "yaml" | "yml" => Some(Self::Yaml),
            _ => None,
        }
    }
}

/// Read a character from the bytes of a file: a PNG card, a JSON card, or MasqueRole YAML.
pub fn import(bytes: &[u8]) -> Result<Imported, ExchangeError> {
    if bytes.starts_with(&PNG_SIGNATURE) {
        let json = card_json_from_png(bytes)?;
        let mut imported = from_json(&json)?;
        imported.picture = Some(bytes.to_vec());
        return Ok(imported);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ExchangeError::NotText)?;
    let text = text.trim_start_matches('\u{feff}');
    if text.trim_start().starts_with('{') {
        return from_json(text);
    }
    Ok(Imported {
        character: Character::from_yaml(text)?,
        picture: None,
        skipped: Vec::new(),
    })
}

/// Read a V1, V2 or V3 card from JSON text.
pub fn from_json(text: &str) -> Result<Imported, ExchangeError> {
    let root: Value =
        serde_json::from_str(text).map_err(|e| ExchangeError::BadJson(e.to_string()))?;
    // V2 and V3 keep the fields under `data`; V1 has them at the top.
    let data = match root.get("data") {
        Some(Value::Object(_)) => &root["data"],
        _ => &root,
    };
    let name = field(data, "name").trim().to_owned();
    if name.is_empty() {
        return Err(ExchangeError::BadJson("the card has no name".into()));
    }
    let fill = |s: String| fill_placeholders(&s, &name);

    let (personality, speaking_style) = split_personality(&fill(field(data, "personality")));
    let aliases = data
        .pointer(&format!("/extensions/{EXTENSION}/aliases"))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();

    let character = Character {
        name: name.clone(),
        aliases,
        description: fill(field(data, "description")).trim().to_owned(),
        personality,
        speaking_style,
        background: fill(field(data, "scenario")).trim().to_owned(),
        greeting: fill(field(data, "first_mes")).trim().to_owned(),
        example_dialogue: parse_examples(&field(data, "mes_example"), &name),
        image: None,
    };
    character.validate()?;

    let mut skipped = Vec::new();
    let has = |key: &str| match data.get(key) {
        Some(Value::String(s)) => !s.trim().is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        _ => false,
    };
    if has("character_book") {
        skipped.push("its lorebook".to_owned());
    }
    if has("alternate_greetings") {
        skipped.push("its alternate greetings".to_owned());
    }
    if has("system_prompt") || has("post_history_instructions") {
        skipped.push("its system prompt".to_owned());
    }
    Ok(Imported {
        character,
        picture: None,
        skipped,
    })
}

/// A V2 card as pretty-printed JSON.
pub fn to_json(character: &Character) -> String {
    let mut extension = Map::new();
    if !character.aliases.is_empty() {
        extension.insert("aliases".into(), json!(character.aliases));
    }
    let card = json!({
        "spec": "chara_card_v2",
        "spec_version": "2.0",
        "data": {
            "name": character.name,
            "description": character.description,
            "personality": join_personality(character),
            "scenario": character.background,
            "first_mes": character.greeting,
            "mes_example": join_examples(&character.example_dialogue),
            "creator_notes": "",
            "system_prompt": "",
            "post_history_instructions": "",
            "alternate_greetings": [],
            "character_book": null,
            "tags": [],
            "creator": "",
            "character_version": "",
            "extensions": { EXTENSION: Value::Object(extension) },
        },
    });
    // Serializing a `Value` cannot fail.
    serde_json::to_string_pretty(&card).unwrap_or_default()
}

/// MasqueRole YAML for a character, without its picture reference.
pub fn to_yaml(character: &Character) -> Result<String, ExchangeError> {
    let plain = Character {
        image: None,
        ..character.clone()
    };
    serde_yaml_ng::to_string(&plain).map_err(|e| ExchangeError::Card(CardError::Parse(e)))
}

/// Put a V2 card into a PNG picture. Any card already in the picture is replaced.
pub fn to_png(character: &Character, png: &[u8]) -> Result<Vec<u8>, ExchangeError> {
    let encoded = STANDARD.encode(to_json(character));
    let mut text = Vec::with_capacity(KEY_V2.len() + 1 + encoded.len());
    text.extend_from_slice(KEY_V2.as_bytes());
    text.push(0);
    text.extend_from_slice(encoded.as_bytes());

    let mut out = Vec::with_capacity(png.len() + text.len() + 12);
    out.extend_from_slice(&PNG_SIGNATURE);
    let mut placed = false;
    for chunk in chunks(png)? {
        if chunk.kind == *b"tEXt" && is_card_keyword(chunk.data) {
            continue;
        }
        if chunk.kind == *b"IEND" && !placed {
            write_chunk(&mut out, *b"tEXt", &text);
            placed = true;
        }
        write_chunk(&mut out, chunk.kind, chunk.data);
    }
    if !placed {
        return Err(ExchangeError::BadPicture("it has no end"));
    }
    Ok(out)
}

/// The card JSON inside a PNG, preferring V3 over V2.
fn card_json_from_png(png: &[u8]) -> Result<String, ExchangeError> {
    let mut v2 = None;
    let mut v3 = None;
    for chunk in chunks(png)? {
        if chunk.kind != *b"tEXt" {
            continue;
        }
        let Some(split) = chunk.data.iter().position(|&b| b == 0) else {
            continue;
        };
        let (keyword, value) = (&chunk.data[..split], &chunk.data[split + 1..]);
        if keyword.eq_ignore_ascii_case(KEY_V3.as_bytes()) {
            v3 = Some(value);
        } else if keyword.eq_ignore_ascii_case(KEY_V2.as_bytes()) {
            v2 = Some(value);
        }
    }
    let encoded = v3.or(v2).ok_or(ExchangeError::NoCardInPicture)?;
    let encoded: Vec<u8> = encoded
        .iter()
        .copied()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        .collect();
    let decoded = STANDARD_NO_PAD
        .decode(encoded)
        .map_err(|_| ExchangeError::BadEncoding)?;
    String::from_utf8(decoded).map_err(|_| ExchangeError::BadEncoding)
}

fn is_card_keyword(text_chunk: &[u8]) -> bool {
    let keyword = text_chunk.split(|&b| b == 0).next().unwrap_or_default();
    keyword.eq_ignore_ascii_case(KEY_V2.as_bytes())
        || keyword.eq_ignore_ascii_case(KEY_V3.as_bytes())
}

struct Chunk<'a> {
    kind: [u8; 4],
    data: &'a [u8],
}

/// Split a PNG into its chunks. Checks the structure, not the checksums.
fn chunks(png: &[u8]) -> Result<Vec<Chunk<'_>>, ExchangeError> {
    if !png.starts_with(&PNG_SIGNATURE) {
        return Err(ExchangeError::BadPicture("it is not a PNG"));
    }
    let mut rest = &png[PNG_SIGNATURE.len()..];
    let mut out = Vec::new();
    while !rest.is_empty() {
        if rest.len() < 12 {
            return Err(ExchangeError::BadPicture("it is cut off"));
        }
        let len = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
        let end = len
            .checked_add(12)
            .filter(|&end| end <= rest.len())
            .ok_or(ExchangeError::BadPicture("it is cut off"))?;
        out.push(Chunk {
            kind: [rest[4], rest[5], rest[6], rest[7]],
            data: &rest[8..8 + len],
        });
        rest = &rest[end..];
    }
    Ok(out)
}

fn write_chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&kind);
    hasher.update(data);
    out.extend_from_slice(&hasher.finalize().to_be_bytes());
}

/// A string field, or empty if it is missing or not a string.
fn field(data: &Value, key: &str) -> String {
    data.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Replace the placeholders other tools use for the character and the user.
fn fill_placeholders(text: &str, name: &str) -> String {
    const CHAR: [&str; 2] = ["{{char}}", "<bot>"];
    const USER: [&str; 2] = ["{{user}}", "<user>"];
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    'scan: while i < text.len() {
        for (tokens, value) in [(CHAR, name), (USER, "the user")] {
            for token in tokens {
                if lower[i..].starts_with(token) {
                    out.push_str(value);
                    i += token.len();
                    continue 'scan;
                }
            }
        }
        let step = text[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&text[i..i + step]);
        i += step;
    }
    out
}

/// One trait per line, then the speaking style on its own line.
fn join_personality(character: &Character) -> String {
    let mut lines = character.personality.clone();
    if !character.speaking_style.trim().is_empty() {
        lines.push(format!(
            "{STYLE_PREFIX} {}",
            character.speaking_style.trim()
        ));
    }
    lines.join("\n")
}

/// The inverse of [`join_personality`], also understanding the comma-separated lists other
/// tools write.
fn split_personality(text: &str) -> (Vec<String>, String) {
    let mut style = String::new();
    let mut lines = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        match line.strip_prefix(STYLE_PREFIX) {
            Some(rest) => style = rest.trim().to_owned(),
            None => lines.push(line),
        }
    }
    let traits = if lines.len() == 1 {
        lines[0]
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .collect()
    } else {
        lines.into_iter().map(str::to_owned).collect()
    };
    (traits, style)
}

fn join_examples(examples: &[ExampleDialogue]) -> String {
    examples
        .iter()
        .map(|e| {
            format!(
                "{START}\n{{{{user}}}}: {}\n{{{{char}}}}: {}\n",
                e.user, e.character
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Speaker {
    User,
    Character,
}

/// If `line` starts a spoken line, who speaks and what they say.
fn speaker_of<'a>(line: &'a str, name: &str) -> Option<(Speaker, &'a str)> {
    let (label, text) = line.split_once(':')?;
    let label = label.trim().to_ascii_lowercase();
    let speaker = match label.as_str() {
        "{{user}}" | "<user>" => Speaker::User,
        "{{char}}" | "<bot>" => Speaker::Character,
        other if other == name.to_ascii_lowercase() => Speaker::Character,
        _ => return None,
    };
    Some((speaker, text.trim()))
}

/// Read `mes_example` into exchanges. A line from the character with no user line before it
/// has nothing to answer, so it is dropped.
fn parse_examples(text: &str, name: &str) -> Vec<ExampleDialogue> {
    let mut examples = Vec::new();
    let normalized = text.replace(&START.to_ascii_lowercase(), START);
    for block in normalized.split(START) {
        let mut messages: Vec<(Speaker, String)> = Vec::new();
        for line in block.lines() {
            match speaker_of(line, name) {
                Some((speaker, said)) => match messages.last_mut() {
                    Some((last, text)) if *last == speaker => {
                        text.push('\n');
                        text.push_str(said);
                    }
                    _ => messages.push((speaker, said.to_owned())),
                },
                None => {
                    if let Some((_, text)) = messages.last_mut() {
                        text.push('\n');
                        text.push_str(line);
                    }
                }
            }
        }
        let mut iter = messages.into_iter().peekable();
        while let Some((speaker, user)) = iter.next() {
            if speaker != Speaker::User {
                continue;
            }
            if let Some((Speaker::Character, _)) = iter.peek() {
                let (_, reply) = iter.next().unwrap_or((Speaker::Character, String::new()));
                let (user, reply) = (
                    fill_placeholders(user.trim(), name),
                    fill_placeholders(reply.trim(), name),
                );
                if !user.is_empty() && !reply.is_empty() {
                    examples.push(ExampleDialogue {
                        user,
                        character: reply,
                    });
                }
            }
        }
    }
    examples
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valid 1x1 PNG.
    fn tiny_png() -> Vec<u8> {
        let mut out = PNG_SIGNATURE.to_vec();
        let ihdr = [0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0];
        write_chunk(&mut out, *b"IHDR", &ihdr);
        write_chunk(&mut out, *b"IDAT", &[0x78, 0x9c, 0x63, 0, 1, 0, 0, 5, 0, 1]);
        write_chunk(&mut out, *b"IEND", &[]);
        out
    }

    fn mira() -> Character {
        Character {
            name: "Mira Valen".into(),
            aliases: vec!["Cartographer".into()],
            description: "A cartographer.".into(),
            personality: vec!["curious".into(), "fiercely loyal, to friends".into()],
            speaking_style: "Dry, short sentences.".into(),
            background: "Grew up in the harbor towns.".into(),
            greeting: "Ah, another one who got lost?".into(),
            example_dialogue: vec![
                ExampleDialogue {
                    user: "Where to next?".into(),
                    character: "Wherever the map runs out.\nThat is where it gets interesting."
                        .into(),
                },
                ExampleDialogue {
                    user: "Why?".into(),
                    character: "Because.".into(),
                },
            ],
            image: None,
        }
    }

    #[test]
    fn json_round_trips_everything_a_character_has() {
        let imported = import(to_json(&mira()).as_bytes()).unwrap();
        assert_eq!(imported.character, mira());
        assert_eq!(imported.picture, None);
        assert!(imported.skipped.is_empty());
    }

    #[test]
    fn png_round_trips_and_keeps_the_picture() {
        let png = to_png(&mira(), &tiny_png()).unwrap();
        let imported = import(&png).unwrap();
        assert_eq!(imported.character, mira());
        assert_eq!(imported.picture.as_deref(), Some(&png[..]));
        // Still a well-formed PNG with its original chunks in order.
        let kinds: Vec<_> = chunks(&png).unwrap().iter().map(|c| c.kind).collect();
        assert_eq!(kinds, [*b"IHDR", *b"IDAT", *b"tEXt", *b"IEND"]);
    }

    #[test]
    fn exporting_again_replaces_the_card_instead_of_adding_one() {
        let once = to_png(&mira(), &tiny_png()).unwrap();
        let other = Character {
            name: "Orrin".into(),
            ..Character::default()
        };
        let twice = to_png(&other, &once).unwrap();
        let texts = chunks(&twice)
            .unwrap()
            .iter()
            .filter(|c| c.kind == *b"tEXt")
            .count();
        assert_eq!(texts, 1);
        assert_eq!(import(&twice).unwrap().character.name, "Orrin");
    }

    #[test]
    fn chunk_checksums_are_valid() {
        let png = to_png(&mira(), &tiny_png()).unwrap();
        let mut rest = &png[8..];
        while !rest.is_empty() {
            let len = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
            let crc = u32::from_be_bytes(rest[8 + len..12 + len].try_into().unwrap());
            assert_eq!(crc, crc32fast::hash(&rest[4..8 + len]));
            rest = &rest[12 + len..];
        }
    }

    #[test]
    fn reads_a_card_made_by_another_tool() {
        let card = r#"{
            "spec": "chara_card_v2", "spec_version": "2.0",
            "data": {
                "name": "Seraphine",
                "description": "{{char}} is a knight who distrusts {{user}}.",
                "personality": "brave, stubborn, kind",
                "scenario": "A border fort in winter.",
                "first_mes": "Halt! State your business, {{user}}.",
                "mes_example": "<START>\n{{user}}: Who are you?\n{{char}}: Seraphine.\nI hold this wall.\n<START>\n{{char}}: Unanswered.\n<START>\n{{user}}: Hi\n{{char}}: Hm.",
                "alternate_greetings": ["Hello."],
                "character_book": {"entries": [{"keys": ["x"], "content": "y"}]},
                "system_prompt": "Be terse.",
                "tags": ["knight"]
            }
        }"#;
        let imported = import(card.as_bytes()).unwrap();
        let c = &imported.character;
        assert_eq!(
            c.description,
            "Seraphine is a knight who distrusts the user."
        );
        assert_eq!(c.personality, ["brave", "stubborn", "kind"]);
        assert_eq!(c.speaking_style, "");
        assert_eq!(c.background, "A border fort in winter.");
        assert_eq!(c.greeting, "Halt! State your business, the user.");
        assert_eq!(c.example_dialogue.len(), 2);
        assert_eq!(
            c.example_dialogue[0].character,
            "Seraphine.\nI hold this wall."
        );
        assert_eq!(c.example_dialogue[1].user, "Hi");
        assert_eq!(
            imported.skipped,
            [
                "its lorebook",
                "its alternate greetings",
                "its system prompt"
            ]
        );
    }

    #[test]
    fn reads_a_flat_v1_card() {
        let c = import(br#"{"name":"Bo","description":"Hi","first_mes":"Hey","personality":"calm","scenario":"","mes_example":""}"#)
            .unwrap()
            .character;
        assert_eq!((c.name.as_str(), c.greeting.as_str()), ("Bo", "Hey"));
        assert_eq!(c.personality, ["calm"]);
    }

    #[test]
    fn prefers_a_v3_chunk_over_a_v2_chunk() {
        let mut png = PNG_SIGNATURE.to_vec();
        write_chunk(&mut png, *b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
        for (key, name) in [(KEY_V2, "Old"), (KEY_V3, "New")] {
            let json = format!(r#"{{"data":{{"name":"{name}"}}}}"#);
            let mut text = format!("{key}\0").into_bytes();
            // Padding is optional in the wild.
            text.extend(STANDARD_NO_PAD.encode(json).bytes());
            write_chunk(&mut png, *b"tEXt", &text);
        }
        write_chunk(&mut png, *b"IEND", &[]);
        assert_eq!(import(&png).unwrap().character.name, "New");
    }

    #[test]
    fn reads_masquerole_yaml() {
        let imported = import(b"\xEF\xBB\xBFname: Bo\ngreeting: Hi").unwrap();
        assert_eq!(imported.character.greeting, "Hi");
        let text = to_yaml(&Character {
            image: Some("x.png".into()),
            ..mira()
        })
        .unwrap();
        assert!(!text.contains("image"));
        assert_eq!(import(text.as_bytes()).unwrap().character, mira());
    }

    #[test]
    fn rejects_files_that_are_not_cards() {
        assert!(matches!(
            import(&tiny_png()),
            Err(ExchangeError::NoCardInPicture)
        ));
        assert!(matches!(
            import(&[0xff, 0xfe, 0x00]),
            Err(ExchangeError::NotText)
        ));
        assert!(matches!(import(b"{ nope"), Err(ExchangeError::BadJson(_))));
        assert!(matches!(
            import(br#"{"data":{"name":"  "}}"#),
            Err(ExchangeError::BadJson(_))
        ));
        assert!(matches!(import(b"just words"), Err(ExchangeError::Card(_))));
        let mut cut = tiny_png();
        cut.truncate(cut.len() - 5);
        assert!(matches!(import(&cut), Err(ExchangeError::BadPicture(_))));
        assert!(to_png(&mira(), b"not a png").is_err());
    }

    #[test]
    fn a_damaged_embedded_card_is_reported() {
        let mut png = PNG_SIGNATURE.to_vec();
        write_chunk(&mut png, *b"tEXt", b"chara\0!!!not base64!!!");
        write_chunk(&mut png, *b"IEND", &[]);
        assert!(matches!(import(&png), Err(ExchangeError::BadEncoding)));
    }

    #[test]
    fn export_formats_are_chosen_by_file_name() {
        assert_eq!(
            ExportFormat::from_file_name("a.PNG"),
            Some(ExportFormat::Png)
        );
        assert_eq!(
            ExportFormat::from_file_name("a.json"),
            Some(ExportFormat::Json)
        );
        assert_eq!(
            ExportFormat::from_file_name("a.yml"),
            Some(ExportFormat::Yaml)
        );
        assert_eq!(ExportFormat::from_file_name("a.txt"), None);
        assert_eq!(ExportFormat::from_file_name("png"), None);
    }

    #[test]
    fn placeholders_are_replaced_in_any_case_and_text_stays_intact() {
        assert_eq!(
            fill_placeholders("{{Char}} meets <USER>, ünïcode ok. <BOT>!", "Bo"),
            "Bo meets the user, ünïcode ok. Bo!"
        );
    }
}
