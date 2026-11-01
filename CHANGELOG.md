# Changelog

All notable changes to MasqueRole. The project follows [Semantic Versioning](https://semver.org).
This file lists only the current and the previous version.

## [0.12.2] - 2026-11-02

### Fixed

- **Several characters:** while the second character was answering, its bubble showed the first character's answer for a while and then went on writing after it. Each answer now streams into a bubble of its own, and the next character takes over as soon as the one before is stored.

## [0.12.1] - 2026-11-02

### Fixed

- **Command messages:** the message a command leaves in the chat (for example from `/enter`, `/leave` or `/tree`) now stays where it happened, between the messages around it, instead of piling up at the bottom of the chat.
- **Dismiss:** every such message has an X button that removes it.
