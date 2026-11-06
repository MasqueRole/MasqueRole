# Changelog

All notable changes to MasqueRole. The project follows [Semantic Versioning](https://semver.org).
This file lists only the current and the previous version.

## [0.15.0] - 2026-11-06

### Added

- **Semantic memory (vector search):** a new setting, *Search the story by meaning* (`memory.semantic`, off by default). A small sentence model runs on your computer, is downloaded once (about 90 MB), and turns every message and fact into a vector kept in the story database. For each new message, the facts and older messages that fit best are brought back into the prompt, even when they use other words, under *Remembered from earlier*. In scenes with several characters each one only recalls what they witnessed or know. Stories that already exist catch up by themselves.

### Changed

- **Toolchain and dependencies:** built with Rust 1.99 and the newest versions of all libraries, including egui and eframe 0.37 (which removed a library with known security advisories) and the candle model libraries 0.11.

## [0.14.0] - 2026-11-06

### Changed

- **A softer look, closer to shadcn:** buttons are pills, text fields are filled and have no outline, and cards, bubbles and dialogs have larger rounded corners. The dark theme got slightly lighter cards on a near-black page, the light theme a soft grey card on white.
- **More room:** buttons, fields and dialogs have more padding, and the space between form rows grew. The tutorial screenshots were retaken.
