# Changelog

All notable changes to MasqueRole. The project follows [Semantic Versioning](https://semver.org).
This file lists only the current and the previous version.

## [0.16.0] - 2026-11-06

### Added

- **Let the AI write or improve a character text:** the personality, speaking style, background and greeting fields of the character editor each have a small sparkle button. It sends the whole card to your provider and puts the result in that field: it improves what is there, or writes the text from the rest of the card when the field is empty. Nothing is saved until you press Save.

### Changed

- **Export has its own color:** the *Export* button in the character editor is now a filled grey button, so it no longer looks like *Cancel*.
- **Import shows a download arrow** instead of a folder.
- **Text fields are compact again:** since the move to egui 0.37 single-line fields had grown taller than their text needs.

## [0.15.0] - 2026-11-06

### Added

- **Semantic memory (vector search):** a new setting, *Search the story by meaning* (`memory.semantic`, off by default). A small sentence model runs on your computer, is downloaded once (about 90 MB), and turns every message and fact into a vector kept in the story database. For each new message, the facts and older messages that fit best are brought back into the prompt, even when they use other words, under *Remembered from earlier*. In scenes with several characters each one only recalls what they witnessed or know. Stories that already exist catch up by themselves.

### Changed

- **Toolchain and dependencies:** built with Rust 1.99 and the newest versions of all libraries, including egui and eframe 0.37 (which removed a library with known security advisories) and the candle model libraries 0.11.
