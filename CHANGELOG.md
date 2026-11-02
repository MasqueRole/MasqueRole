# Changelog

All notable changes to MasqueRole. The project follows [Semantic Versioning](https://semver.org).
This file lists only the current and the previous version.

## [0.13.0] - 2026-11-02

### Added

- **Import and export of characters:** the folder button above the character list imports a character card from a file. It reads Character Card V2 and V1 (JSON), cards embedded in a PNG picture (V2 and V3), and MasqueRole's own YAML. The picture of a PNG card becomes the character's picture, `{{char}}` and `{{user}}` are filled in, and example chats are converted. Parts MasqueRole has no place for (a lorebook, alternate greetings, a system prompt) are left out, and the message after the import says so.
- **Export:** an *Export* button in the character editor saves the character as a PNG card (the character's picture, or a plain panel without one), as V2 JSON, or as MasqueRole YAML, by the file extension you choose. Nicknames and speaking style are kept, so a card exported here and imported again is unchanged.

## [0.12.3] - 2026-11-02

### Changed

- **Waiting for an answer:** the blinking cursor is gone. While a character or the narrator has not written anything yet, the bubble says "Thinking..." and a shimmer travels across its letters. With *Reduce motion* on, the text stays still. Once the first words arrive they stream in without a cursor.

