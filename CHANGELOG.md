# Changelog

All notable changes to MasqueRole. The project follows [Semantic Versioning](https://semver.org).
This file lists only the current and the previous version.

## [0.8.2] - 2026-10-31

### Changed

- **Story tree:** the tree grows from left to right again, now with smooth curved lines from every node to its replies. The line of the path you took is blue, the others are gray, and the node you opened has a blue outline.
- **Tree nodes:** nodes are cards again, with the title and a date pill, all of the same size so the columns stay tidy. Hovering a card shows its whole title and where it stands (where you are now, on the path you took, or another branch). A collapsed branch shows how many nodes it hides in front of the title.
- Opening a node still grows it into a larger card with the whole message and the buttons to jump or collapse.
- **Movable cards:** drag a card to put it anywhere. Each story saves where you put its cards and shows them there the next time. A moved card keeps its lines to its parent and replies. *Tidy* puts all cards of the story back in their automatic places.

## [0.8.1] - 2026-10-31

### Added

- **Keyword beans:** a new chip field. Type a keyword and press comma or Enter, and it turns into a bean. Click the cross on a bean to delete it, or press Backspace in the empty field to remove the last one. Pasting `a, b, c` makes three.

### Changed

- **Name:** the app is now called MasqueRole. Stories, settings, and saved API keys from the old name are found as before.
- **Story tree:** the preview bar at the bottom is gone. Click a dot and it grows into a card with the whole message, the date, and the *Jump here* and *Collapse branch* buttons. The card shrinks back into the dot when you close it with its cross or Escape.
- **Lore entries and example dialogue:** the forms inside boxes are gone. Entries are single lines with edit and delete buttons, and editing one opens an overlay with the keyword beans and the text. *Add entry* and *Add example* open the same overlay.
- **Close button:** the cross of the window's close button is red.
- **Red:** the red of the dark theme is brighter (`#dc2626`, as in the light theme), so the close button's cross, delete buttons, and error toasts read clearly.
