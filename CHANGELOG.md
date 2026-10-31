# Changelog

All notable changes to InfiniteRoleplay. The project follows [Semantic Versioning](https://semver.org).
This file lists only the current and the previous version.

## [0.8.0] - 2026-10-31

### Added

- **Press feedback:** buttons and the rail icons shrink a little while pressed and settle back faster than they pressed, so the interface feels like it heard you.
- **Reduce motion:** a new switch in Settings, General (`ui.reduce_motion`). Nothing moves or scales with it on; fades and color changes stay.
- **Worst-case check:** the design-system gallery has a *Demo data / Worst case* toggle with long names, emoji, unbreakable links, and counts of one. New tests draw every screen with 1,000 stories and very long names.

### Changed

- **Motion:** all movement shares one strong ease-out curve (`ir_ui::motion`). The message buttons, tree dots, and toasts use it. Toasts slide up and fade in, and leave the same way, a little faster.
- **Long text:** a long story title, world name, or character name in the chat header is cut with an ellipsis, and hovering shows the whole name. The same goes for names cut in the sidebar lists.
- **Counts:** "1 message", "1 storyline", and "1 node" instead of "1 messages" and so on.

## [0.7.2] - 2026-10-31

### Changed

- **Story tree:** the tree now grows from the top down instead of from left to right.
- **Tree nodes:** a node is a big colored dot with no text: gold for where you are, green for the path you took, gray for other branches. Hover a dot to open a card with its title and date. A bookmark shows inside the dot, and a ring around a dot means a collapsed branch.
- **Toasts:** a toast has an icon that says what happened (copy, save, info, warning, and so on) in a round badge, then the text. New toasts fade in, and the close button is an icon.
- The tutorial text and screenshots follow the changes.
