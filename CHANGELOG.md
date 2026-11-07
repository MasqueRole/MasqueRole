# Changelog

All notable changes to MasqueRole. The project follows [Semantic Versioning](https://semver.org).
This file lists only the current and the previous version.

## [0.16.1] - 2026-11-07

### Fixed

- **Menus were far too big:** the menu behind the three dots of a list row stretched across most of the window. It is 160 px wide again.

## [0.16.0] - 2026-11-06

### Added

- **Let the AI write or improve a character text:** the personality, speaking style, background and greeting fields of the character editor each have a small sparkle button. It sends the whole card to your provider and puts the result in that field: it improves what is there, or writes the text from the rest of the card when the field is empty. Nothing is saved until you press Save.

### Changed

- **Export has its own color:** the *Export* button in the character editor is now a filled grey button, so it no longer looks like *Cancel*.
- **Import shows a download arrow** instead of a folder.
- **Text fields are compact again:** since the move to egui 0.37 single-line fields had grown taller than their text needs.
