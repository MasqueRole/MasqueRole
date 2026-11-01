# Changelog

All notable changes to MasqueRole. The project follows [Semantic Versioning](https://semver.org).
This file lists only the current and the previous version.

## [0.12.3] - 2026-11-02

### Changed

- **Waiting for an answer:** the blinking cursor is gone. While a character or the narrator has not written anything yet, the bubble says "Thinking..." and a shimmer travels across its letters. With *Reduce motion* on, the text stays still. Once the first words arrive they stream in without a cursor.

## [0.12.2] - 2026-11-02

### Fixed

- **Several characters:** while the second character was answering, its bubble showed the first character's answer for a while and then went on writing after it. Each answer now streams into a bubble of its own, and the next character takes over as soon as the one before is stored.
