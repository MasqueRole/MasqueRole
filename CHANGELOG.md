# Changelog

All notable changes to InfiniteRoleplay. The project follows [Semantic Versioning](https://semver.org).
This file lists only the current and the previous version.

## [0.7.2] - 2026-10-31

### Changed

- **Story tree:** the tree now grows from the top down instead of from left to right.
- **Tree nodes:** a node is a big colored dot with no text: gold for where you are, green for the path you took, gray for other branches. Hover a dot to open a card with its title and date. A bookmark shows inside the dot, and a ring around a dot means a collapsed branch.
- **Toasts:** a toast has an icon that says what happened (copy, save, info, warning, and so on) in a round badge, then the text. New toasts fade in, and the close button is an icon.
- The tutorial text and screenshots follow the changes.

## [0.7.1] - 2026-10-31

### Added

- **Cost warning:** choosing a provider that bills by usage (any hosted provider, or a custom endpoint outside your own network) shows a warning in the Provider settings. Neither the app nor its creators are liable for API costs, including unexpectedly high ones.
- Button hover and click states fade between colors instead of jumping. This covers buttons, the rail, tabs, list rows, menus, and the command list.

### Changed

- **Token estimate:** tokens are now counted by a script-aware estimate instead of one token per four characters. Words, digits, punctuation, accented letters, and CJK text each count the way tokenizers usually treat them, and every request adds the reply's start. Your story's usage numbers can differ from 0.7.0.
- **Token donut:** the donut moved from the story header to the left side of the message box.
- **Message actions:** a clicked message no longer gets a border. The branch and copy buttons fade in from above instead.
- The tutorial text and screenshots follow the changes.
