# Changelog

All notable changes to InfiniteRoleplay. The project follows [Semantic Versioning](https://semver.org).
This file lists only the current and the previous version.

## [0.7.1] - 2026-10-31

### Added

- **Cost warning:** choosing a provider that bills by usage (any hosted provider, or a custom endpoint outside your own network) shows a warning in the Provider settings. Neither the app nor its creators are liable for API costs, including unexpectedly high ones.
- Button hover and click states fade between colors instead of jumping. This covers buttons, the rail, tabs, list rows, menus, and the command list.

### Changed

- **Token estimate:** tokens are now counted by a script-aware estimate instead of one token per four characters. Words, digits, punctuation, accented letters, and CJK text each count the way tokenizers usually treat them, and every request adds the reply's start. Your story's usage numbers can differ from 0.7.0.
- **Token donut:** the donut moved from the story header to the left side of the message box.
- **Message actions:** a clicked message no longer gets a border. The branch and copy buttons fade in from above instead.
- The tutorial text and screenshots follow the changes.

## [0.7.0] - 2026-10-31

### Added

- **Message actions:** click a message to show a *Branch from here* button and a *Copy* button under it.
- **Message tags:** every message shows a unique tag such as `#12`. Use it with `/goto #12` to jump to the message.
- **Sub-chats:** the sessions list shows the storylines of the open story as indented sub-chats. Click one to continue that storyline. They appear in the Stories section only, not in the Tree.
- **Story info:** click the story title, a badge, or the new info button above the chat to open a full page with the story's world and characters.
- **Token donut:** the token monitor above the chat is a donut chart. Hover it for a bar chart of how much of the limit is used.
- **`/compact`:** summarizes every message except the newest two (the last message of yours and the reply to it).
- **Marketplace:** a placeholder icon in the rail with a "coming soon" tooltip.

### Changed

- **Navigation:** Stories, Characters, Worlds, and Tree moved from the top bar to an icon rail at the left edge. Hover an icon to see its name.
- **Collapsible list:** the list that used to sit at the left is now a second sidebar next to the rail, and can be hidden with the sidebar button at the bottom of the rail.
- **Tree preview:** the preview of a selected node in the Tree now has a *Close preview* button.
- The tutorial text and screenshots follow the new layout.
