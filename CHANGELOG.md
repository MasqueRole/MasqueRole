# Changelog

All notable changes to MasqueRole. The project follows [Semantic Versioning](https://semver.org).
This file lists only the current and the previous version.

## [0.12.0] - 2026-11-02

### Added

- **Several characters in one chat:** every character answers in their own voice instead of one narrator for the whole scene. The app works out who you are talking to: an `@name`, a name at the start or end of your message, words like *everyone*, the character you were just talking with, or the only one present. Chips above the message box show who will answer and why, and clicking one adds `@name`.
- **Scenes:** `/to <name>`, `/all`, `/enter <name>`, `/leave <name>` and `/scene`, plus *Leave*, *Enter* and *Add a character* on the story's info page. Characters who are away neither answer nor hear what is said.
- **Whispers:** `/whisper <name>: <text>` is heard by that character alone, and so is their answer.
- **Knowledge per character:** a character's prompt holds only the messages they witnessed, their own card in full, and only the name and description of the others. Facts are remembered with the characters who know them, each character keeps their own summary, and lore entries can list who knows them with `known_by`. The narrator still sees everything.
- **Settings and cards:** `style.max_responders` (default 2) caps how many characters answer one message, since every answer is another request. Character cards got *Nicknames* that call them in a group scene.

## [0.11.0] - 2026-11-02

### Changed

- **Names:** the crates are now `masquerole-core`, `masquerole-ui` and `masquerole-app` instead of the old `ir-` names. Build and run the app with `cargo run -p masquerole-app`.
- **Installers:** the installers are named `masquerole_<version>_...` instead of `ir-app_<version>_...`.
- **Program name:** the installed program is `masquerole.exe` (`masquerole` on Linux) instead of `ir-app.exe`.
