# Changelog

All notable changes to MasqueRole. The project follows [Semantic Versioning](https://semver.org).
This file lists only the current and the previous version.

## [0.8.1] - 2026-10-31

### Added

- **Keyword beans:** a new chip field. Type a keyword and press comma or Enter, and it turns into a bean. Click the cross on a bean to delete it, or press Backspace in the empty field to remove the last one. Pasting `a, b, c` makes three.

### Changed

- **Name:** the app is now called MasqueRole. Stories, settings, and saved API keys from the old name are found as before.
- **Story tree:** the preview bar at the bottom is gone. Click a dot and it grows into a card with the whole message, the date, and the *Jump here* and *Collapse branch* buttons. The card shrinks back into the dot when you close it with its cross or Escape.
- **Lore entries and example dialogue:** the forms inside boxes are gone. Entries are single lines with edit and delete buttons, and editing one opens an overlay with the keyword beans and the text. *Add entry* and *Add example* open the same overlay.
- **Close button:** the cross of the window's close button is red.
- **Red:** the red of the dark theme is brighter (`#dc2626`, as in the light theme), so the close button's cross, delete buttons, and error toasts read clearly.

## [0.8.0] - 2026-10-31

### Added

- **Press feedback:** buttons and the rail icons shrink a little while pressed and settle back faster than they pressed, so the interface feels like it heard you.
- **Reduce motion:** a new switch in Settings, General (`ui.reduce_motion`). Nothing moves or scales with it on; fades and color changes stay.
- **Worst-case check:** the design-system gallery has a *Demo data / Worst case* toggle with long names, emoji, unbreakable links, and counts of one. New tests draw every screen with 1,000 stories and very long names.

### Changed

- **Motion:** all movement shares one strong ease-out curve (`ir_ui::motion`). The message buttons, tree dots, and toasts use it. Toasts slide up and fade in, and leave the same way, a little faster.
- **Long text:** a long story title, world name, or character name in the chat header is cut with an ellipsis, and hovering shows the whole name. The same goes for names cut in the sidebar lists.
- **Counts:** "1 message", "1 storyline", and "1 node" instead of "1 messages" and so on.
