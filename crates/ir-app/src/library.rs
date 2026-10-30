//! The character and world library: YAML files in the app's data directory.

use std::path::{Path, PathBuf};

use ir_core::cards::{Card, LoadError};
use ir_core::character::Character;
use ir_core::lorebook::Lorebook;

/// A library item and the file it lives in.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry<T> {
    /// The file the item was loaded from or saved to.
    pub path: PathBuf,
    /// The item.
    pub item: T,
}

/// Characters and worlds stored as YAML files under one root directory.
#[derive(Clone, Debug)]
pub struct Library {
    root: PathBuf,
}

impl Library {
    /// A library stored under `root`. Nothing is created until it is used.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn characters_dir(&self) -> PathBuf {
        self.root.join("characters")
    }

    fn worlds_dir(&self) -> PathBuf {
        self.root.join("worlds")
    }

    /// Every character in the library, plus a message for each file that could not be read.
    pub fn characters(&self) -> (Vec<Entry<Character>>, Vec<String>) {
        load_all(&self.characters_dir(), Character::load)
    }

    /// Every world in the library, plus a message for each file that could not be read.
    pub fn worlds(&self) -> (Vec<Entry<Lorebook>>, Vec<String>) {
        load_all(&self.worlds_dir(), Lorebook::load)
    }

    /// Save a character. With `existing`, that file is overwritten; otherwise a new file
    /// named after the character is created. Invalid cards are rejected, not written.
    pub fn save_character(
        &self,
        existing: Option<&Path>,
        character: &Character,
    ) -> Result<PathBuf, String> {
        character.validate().map_err(|e| e.to_string())?;
        let yaml = serde_yaml_ng::to_string(character).map_err(|e| e.to_string())?;
        self.write(&self.characters_dir(), existing, &character.name, &yaml)
    }

    /// Save a world. See [`Library::save_character`].
    pub fn save_world(&self, existing: Option<&Path>, world: &Lorebook) -> Result<PathBuf, String> {
        world.validate().map_err(|e| e.to_string())?;
        let yaml = serde_yaml_ng::to_string(world).map_err(|e| e.to_string())?;
        self.write(&self.worlds_dir(), existing, &world.name, &yaml)
    }

    /// Delete a library file. Paths outside the library are refused.
    pub fn delete(&self, path: &Path) -> Result<(), String> {
        if !path.starts_with(&self.root) {
            return Err(format!("{} is not in the library", path.display()));
        }
        std::fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn write(
        &self,
        dir: &Path,
        existing: Option<&Path>,
        name: &str,
        yaml: &str,
    ) -> Result<PathBuf, String> {
        let path = match existing {
            Some(path) if path.starts_with(dir) => path.to_path_buf(),
            Some(path) => return Err(format!("{} is not in the library", path.display())),
            None => unique_path(dir, name),
        };
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(&path, yaml).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }
}

/// Load every YAML file in `dir`, sorted by file name. A missing directory is an empty library.
fn load_all<T>(
    dir: &Path,
    load: impl Fn(&Path) -> Result<T, LoadError>,
) -> (Vec<Entry<T>>, Vec<String>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return (Vec::new(), Vec::new());
    };
    let mut paths: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                    e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml")
                })
        })
        .collect();
    paths.sort();

    let (mut entries, mut errors) = (Vec::new(), Vec::new());
    for path in paths {
        match load(&path) {
            Ok(item) => entries.push(Entry { path, item }),
            Err(e) => errors.push(e.to_string()),
        }
    }
    (entries, errors)
}

/// A file name for `name` that does not exist yet: `mira-valen.yaml`, then `mira-valen-2.yaml`.
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let stem = slug(name);
    let first = dir.join(format!("{stem}.yaml"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{stem}-{n}.yaml")))
        .find(|p| !p.exists())
        .expect("an unused file name exists")
}

/// Lowercase letters and digits, with runs of anything else turned into one `-`.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "untitled".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique library root in the temp directory, removed on drop.
    struct TempLibrary(PathBuf);

    impl TempLibrary {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("ir-app-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            Self(root)
        }

        fn library(&self) -> Library {
            Library::new(&self.0)
        }
    }

    impl Drop for TempLibrary {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn character(name: &str) -> Character {
        Character::from_yaml(&format!("name: {name:?}\ndescription: test")).unwrap()
    }

    #[test]
    fn slugs_are_safe_file_names() {
        assert_eq!(slug("Mira Valen"), "mira-valen");
        assert_eq!(slug("  Orrin   O'Karrow! "), "orrin-o-karrow");
        assert_eq!(slug("../../etc/passwd"), "etc-passwd");
        assert_eq!(slug("Caf\u{e9}"), "caf\u{e9}");
        assert_eq!(slug("!!!"), "untitled");
        assert_eq!(slug(""), "untitled");
    }

    #[test]
    fn a_missing_library_is_empty() {
        let tmp = TempLibrary::new("missing");
        let lib = tmp.library();
        assert_eq!(lib.characters(), (Vec::new(), Vec::new()));
    }

    #[test]
    fn new_characters_get_unique_files_and_round_trip() {
        let tmp = TempLibrary::new("save");
        let lib = tmp.library();
        let first = lib.save_character(None, &character("Mira Valen")).unwrap();
        let second = lib.save_character(None, &character("Mira Valen")).unwrap();
        assert_eq!(first.file_name().unwrap(), "mira-valen.yaml");
        assert_eq!(second.file_name().unwrap(), "mira-valen-2.yaml");

        let (loaded, errors) = lib.characters();
        assert!(errors.is_empty());
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].item, character("Mira Valen"));
    }

    #[test]
    fn saving_over_an_existing_file_keeps_its_path_even_when_renamed() {
        let tmp = TempLibrary::new("rename");
        let lib = tmp.library();
        let path = lib.save_character(None, &character("Mira")).unwrap();
        let again = lib
            .save_character(Some(&path), &character("Mira Valen"))
            .unwrap();
        assert_eq!(again, path);
        let (loaded, _) = lib.characters();
        assert_eq!(
            (loaded.len(), loaded[0].item.name.as_str()),
            (1, "Mira Valen")
        );
    }

    #[test]
    fn invalid_cards_are_rejected_and_nothing_is_written() {
        let tmp = TempLibrary::new("invalid");
        let lib = tmp.library();
        let blank = Character {
            name: "  ".into(),
            ..Character::default()
        };
        assert!(lib
            .save_character(None, &blank)
            .unwrap_err()
            .contains("name"));
        assert!(lib.characters().0.is_empty());
        assert!(!tmp.0.join("characters").exists());

        let world = Lorebook {
            name: "W".into(),
            entries: vec![ir_core::lorebook::LoreEntry {
                keys: vec![],
                content: "x".into(),
            }],
            ..Lorebook::default()
        };
        assert!(lib.save_world(None, &world).is_err());
    }

    #[test]
    fn worlds_save_and_load() {
        let tmp = TempLibrary::new("world");
        let lib = tmp.library();
        let world = Lorebook::from_yaml(
            "name: Reef\nentries:\n  - keys: [teeth]\n    content: Sharp rocks.\n",
        )
        .unwrap();
        let path = lib.save_world(None, &world).unwrap();
        assert_eq!(path.file_name().unwrap(), "reef.yaml");
        assert_eq!(lib.worlds().0[0].item, world);
    }

    #[test]
    fn broken_files_are_reported_and_the_rest_still_load() {
        let tmp = TempLibrary::new("broken");
        let lib = tmp.library();
        lib.save_character(None, &character("Good")).unwrap();
        std::fs::write(tmp.0.join("characters").join("bad.yaml"), "name: [").unwrap();
        std::fs::write(tmp.0.join("characters").join("notes.txt"), "ignored").unwrap();

        let (loaded, errors) = lib.characters();
        assert_eq!(loaded.len(), 1);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("bad.yaml"), "{errors:?}");
    }

    #[test]
    fn only_library_files_can_be_deleted_or_overwritten() {
        let tmp = TempLibrary::new("scope");
        let lib = tmp.library();
        let outside =
            std::env::temp_dir().join(format!("ir-app-outside-{}.yaml", std::process::id()));
        std::fs::write(&outside, "name: Outside").unwrap();

        assert!(lib.delete(&outside).is_err());
        assert!(lib.save_character(Some(&outside), &character("X")).is_err());
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "name: Outside");
        let _ = std::fs::remove_file(outside);

        let path = lib.save_character(None, &character("Gone")).unwrap();
        lib.delete(&path).unwrap();
        assert!(!path.exists());
        assert!(lib.delete(&path).is_err(), "already gone");
    }
}
