# ♾️ InfiniteRoleplay

**A native desktop app, written in Rust, for roleplaying with the AI model of your choice: endless, consistent, and persistent sessions.**

InfiniteRoleplay sits between you and the language model provider you choose (xAI, OpenAI, Anthropic, OpenRouter, or a local model via Ollama or LM Studio). It handles character cards, world knowledge, long-term memory, and a branching story tree, so stories don't fall apart when they hit context limits or the model forgets what happened.

> 🚧 **Status: early development.** The repository currently contains only this README. Everything below describes the planned design and is not yet implemented. Installation steps, commands, and the project layout will work as described once the first release lands.

---

## Table of Contents

- [Features](#features)
- [Tech Stack](#tech-stack)
- [Requirements](#requirements)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Configuration](#configuration)
- [Character Cards](#character-cards)
- [Worlds & Scenarios](#worlds--scenarios)
- [Memory System](#memory-system)
- [Story Tree](#story-tree)
- [Commands](#commands)
- [Project Structure](#project-structure)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [Usage Notes](#usage-notes)
- [License](#license)

---

## Features

- **Native desktop app:** Windows and Linux. Ships as a single small binary with fast startup. No browser, no web runtime, no server to run.
- **Endless sessions:** Automatic summarization of older messages keeps the context lean without losing the plot.
- **Story Tree:** Branch your story into parallel storylines, visualized as a skill tree. Jump back to any node and continue from there.
- **Character cards:** Personality, speaking style, background, and example dialogue per character in a simple file.
- **World knowledge (lorebook):** Entries are injected only when matching keywords appear in the conversation.
- **Layered memory:** Short-term, summary, and fact memory.
- **Local-first:** Sessions are stored in a local SQLite database. Your API key lives in the operating system's secure storage.
- **Streaming:** Responses appear token by token.
- **Multiple characters:** Group scenes with several characters and a narrator mode.
- **Uniform design system:** Every screen is built from a shared component library (`ir-ui`), a shadcn-style set of themed components for egui, with light and dark themes.
- **Customizable prompts:** System prompt templates and style profiles (e.g. fantasy, noir, slice of life).

## Tech Stack

The goal is a **lightweight native desktop app in pure Rust** with as little complexity as possible: one language, one toolchain, one binary.

| Layer            | Choice                                                    | Why                                                                 |
|------------------|-----------------------------------------------------------|---------------------------------------------------------------------|
| Language         | Rust                                                      | Fast, memory-safe, and everything (UI and logic) in one language.   |
| GUI              | [egui](https://github.com/emilk/egui) / `eframe`          | Simple immediate-mode UI with no web stack. Custom drawing makes the skill-tree view easy. |
| Async runtime    | `tokio`                                                   | Runs API calls and streaming in the background without freezing the UI. |
| HTTP / streaming | `reqwest`                                                 | Calls the provider's API and reads streamed (SSE) responses.               |
| Storage          | SQLite via `rusqlite` (bundled)                           | One local file, reliable, trivial to back up.                       |
| Serialization    | `serde` with JSON/TOML/YAML                               | Settings, character cards, and lorebooks.                           |
| Secrets          | `keyring`                                                 | Stores the API key in the OS keychain, never in plain text.         |
| Errors & logging | `anyhow`, `thiserror`, `tracing`                          | Clear errors and optional debug logs.                               |

The code is split into a UI-independent **core library** and a thin **GUI app**. That keeps the logic easy to test and leaves the door open for other front ends (such as a CLI) later.

**Why not the alternatives?**

- **Tauri / Electron:** Both need a web frontend and a JavaScript toolchain on top of Rust. That is more moving parts than this project needs.
- **Iced / Slint:** Good Rust GUI toolkits, but egui is the quickest to get started with and the easiest for custom-drawn views like the story tree.
- **Python + Qt:** Good on desktop, but packaging and distribution are painful.

## Requirements

**To use the app:** An API key for any supported provider and an internet connection, or a locally running model (e.g. Ollama or LM Studio) that needs no key and no internet.

**To build from source:**

- [Rust](https://rustup.rs) (stable toolchain, edition 2021 or newer)
- On Linux: a few system libraries for windowing and keychain access (see the [eframe documentation](https://github.com/emilk/egui/tree/master/crates/eframe) for the exact packages for your distribution)

## Installation

### Download

Prebuilt binaries will be published on the [Releases](https://github.com/baicore/InfiniteRoleplay/releases) page once the first version is out.

### Build from source

```bash
git clone https://github.com/baicore/InfiniteRoleplay.git
cd InfiniteRoleplay

cargo run -p ir-app               # run in development mode
cargo build --release -p ir-app   # optimized binary in target/release/
```

## Quick Start

1. Launch InfiniteRoleplay.
2. Open **Settings → API** and choose a **provider** and paste your API key (not needed for local models).
3. Create or import a **character** and, optionally, a **world**.
4. Press **New Session** and start writing.

For development, you can also provide the key through the `IR_API_KEY` environment variable.

## Configuration

Most options are available in the **Settings** screen. They are stored in `settings.toml` in the app's data directory:

```toml
[api]
provider = "openai-compatible"   # or a built-in preset, e.g. "xai", "openai", "anthropic", "ollama"
base_url = "https://api.example.com/v1" # endpoint of your provider
model = "your-model-name"         # check your provider's docs for current model names
temperature = 0.9
max_tokens = 800
stream = true

[context]
max_context_tokens = 24000
recent_messages = 20
summarize_after = 30

[memory]
enabled = true
fact_extraction = true

[style]
profile = "default"
narrator_mode = false

[tree]
auto_checkpoint = true
node_label = "auto"       # "auto" = short AI-generated label, "manual" = ask
show_tree_on_branch = true
```

| Setting                    | Description                                                    |
|----------------------------|----------------------------------------------------------------|
| `api.provider`             | Provider preset or `openai-compatible` for any compatible endpoint |
| `api.base_url`             | API endpoint (e.g. a local server at `http://localhost:11434/v1`) |
| `context.max_context_tokens` | Upper limit for the context sent to the API                  |
| `context.recent_messages`  | Number of messages kept verbatim                               |
| `context.summarize_after`  | Older messages are condensed beyond this point                 |
| `memory.fact_extraction`   | Automatically remember important facts                         |
| `tree.auto_checkpoint`     | Create a story-tree node after every turn                      |
| `tree.node_label`          | `auto` = short AI-generated label, `manual` = ask every time   |

> ⚠️ The API key is stored in your operating system's keychain, never in `settings.toml`. Don't commit keys to version control.

## Character Cards

Characters are YAML files that you can create in the app or import from disk:

```yaml
name: "Mira Valen"
description: "A cartographer and reluctant adventurer."
personality:
  - curious
  - sarcastic
  - fiercely loyal to friends
speaking_style: "Dry, short sentences, the occasional cartography term."
background: |
  Mira grew up in the harbor towns of the north and has
  been mapping uncharted coastlines for ten years.
greeting: "Ah, another one who got lost?"
example_dialogue:
  - user: "Where to next?"
    character: "Wherever the map runs out. That's where it gets interesting."
```

## Worlds & Scenarios

A lorebook provides background knowledge that is only added to the prompt when needed:

```yaml
name: "The Misty Coast"
setting: "A coastal region where constant fog makes navigation nearly impossible."
entries:
  - keys: ["lighthouse", "keeper"]
    content: "The Karrow lighthouse has been guarded by an order for centuries."
  - keys: ["fogglass"]
    content: "Fogglass is a rare mineral that briefly clears fog."
```

## Memory System

InfiniteRoleplay uses three layers:

1. **Short-term memory:** the most recent messages, verbatim.
2. **Summary:** a condensed version of older events, continuously updated.
3. **Fact store:** persistent facts (names, relationships, places, items) that are re-injected when relevant.

This way, even after hundreds of messages, the story remembers who promised what to whom.

## Story Tree

Every story is stored as a **tree of nodes** instead of a single linear chat log. Each node is a point in the story (a turn or a key moment). Whenever you want to try a different path, you branch off from any node. The result looks and works like a skill tree: you see every path you have explored, which one you are on right now, and the roads not taken.

```mermaid
graph TD
    A((Start)) --> B[Meet Mira at the harbor]
    B --> C[Enter the lighthouse]
    B --> D[Sail into the fog]
    C --> E[Trust the keeper]
    C --> F[Steal the fogglass]
    D --> G[Shipwreck on the reef]
    F --> H[The order hunts you]
    E --> I[Learn the keeper's secret]

    classDef current fill:#f5c542,stroke:#333,stroke-width:3px,color:#000;
    classDef explored fill:#8fd694,stroke:#333,color:#000;
    classDef open fill:#e0e0e0,stroke:#999,stroke-dasharray: 4 4,color:#000;
    class A,B,C,F explored;
    class H current;
    class D,E,G,I open;
```

*Gold = current position, green = path you took, gray dashed = alternative branches.*

In the app, the tree is an interactive view drawn with egui: click a node to preview it, and jump there with a second click. Scroll and drag to navigate large trees. The `/tree` command prints a text version:

```
● Start
└─● Meet Mira at the harbor                     [#2]
  ├─● Enter the lighthouse                      [#5]
  │ ├─○ Trust the keeper                        [#9]
  │ │ └─○ Learn the keeper's secret             [#14]
  │ └─● Steal the fogglass                      [#8]
  │   └─◆ The order hunts you   ← you are here  [#12]
  └─○ Sail into the fog                         [#4]
    └─○ Shipwreck on the reef                   [#7]

● on current path   ◆ current node   ○ other branch
```

### How it works

- **Nodes:** Each node stores the message, the memory state (summary and facts), and the active lorebook entries at that moment. Jumping back restores exactly that state.
- **Branching:** Branching marks the current node as a fork point. Whatever you write next starts a new storyline. The old one stays untouched.
- **Going back:** Jump to any earlier node. Facts learned *after* that node are not remembered on the new path, so there are no spoilers from the other timeline.
- **Labels & bookmarks:** Nodes get short auto-generated labels. Bookmark important decision points and return to them by name.
- **Switching branches:** You can jump between branches at any time. Nothing is deleted unless you explicitly prune it.
- **Storage:** Nodes are stored in the session's SQLite database, with each node pointing to its parent.

## Commands

Everything is also available through buttons and menus in the UI. In the chat input, these slash commands work as shortcuts:

| Command                | Function                                 |
|------------------------|------------------------------------------|
| `/save [name]`         | Save the session                         |
| `/load <name>`         | Load a session                           |
| `/retry`               | Regenerate the last response             |
| `/undo`                | Step back to the parent node             |
| `/tree`                | Show the story tree                      |
| `/branch [label]`      | Fork the story at the current node       |
| `/goto <node\|label>`  | Jump to any node or bookmark             |
| `/mark <label>`        | Bookmark the current node                |
| `/branches`            | List all branches and their tips         |
| `/prune <node>`        | Delete a node and its subtree            |
| `/memory`              | Show stored facts                        |
| `/forget <id>`         | Remove a fact                            |
| `/summary`             | Show the current summary                 |
| `/char <name>`         | Switch or add a character                |
| `/style <profile>`     | Switch style profile                     |
| `/narrator on\|off`    | Toggle narrator mode                     |

## Project Structure

A Cargo workspace with a UI-independent core library, a UI component library, and the desktop app:

```
InfiniteRoleplay/
├── Cargo.toml                  # Workspace manifest
├── crates/
│   ├── ir-core/                # Core logic, no UI code
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── client.rs       # Provider-agnostic LLM client (streaming)
│   │       ├── context.rs      # Context building & token budget
│   │       ├── memory.rs       # Summaries & fact store
│   │       ├── lorebook.rs     # Keyword-triggered injection
│   │       ├── tree.rs         # Story tree: nodes, branches, rewind
│   │       ├── session.rs      # Save, load, resume
│   │       ├── commands.rs     # Slash commands
│   │       ├── db.rs           # SQLite schema and queries
│   │       └── settings.rs     # Settings and keychain access
│   ├── ir-ui/                  # Design system: theme tokens + reusable components
│   └── ir-app/                 # Desktop GUI (egui / eframe)
│       └── src/
│           ├── main.rs
│           ├── chat.rs
│           ├── tree_view.rs    # Skill-tree view
│           ├── characters.rs
│           └── settings_ui.rs
├── characters/                 # Example character cards
├── worlds/                     # Example lorebooks
├── prompts/                    # System prompts & style profiles
├── .agent/
│   └── AGENTS.md               # Step-by-step build instructions for AI agents
└── README.md
```

## Roadmap

- [ ] First desktop release for Windows and Linux
- [ ] Installers (e.g. via `cargo-packager`)
- [ ] Story tree improvements (zoom, drag, collapse branches)
- [ ] Merge facts between branches
- [ ] Vector-based memory search
- [ ] Import of character cards from common formats
- [ ] Image support for scenes
- [ ] Additional native provider adapters beyond OpenAI-compatible APIs
- [ ] Plugin system for custom commands
- [ ] Optional encrypted backup and export
- [ ] Voice output

## Contributing

Contributions are welcome!

1. Fork [baicore/InfiniteRoleplay](https://github.com/baicore/InfiniteRoleplay)
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Commit your changes
4. Open a pull request

Please describe your changes clearly, run `cargo fmt` and `cargo clippy` before submitting, and keep UI code out of `ir-core`, and build new screens from `ir-ui` components. AI coding agents should follow [`.agent/AGENTS.md`](.agent/AGENTS.md).

## Usage Notes

- InfiniteRoleplay is a tool for **creative writing and entertainment**. All content is generated by the underlying model.
- The usage policies of your chosen provider apply. This app does not bypass or alter their safeguards.
- Sessions are stored locally, but conversation content is sent to your chosen provider's API for processing (unless you use a local model). Do not enter sensitive personal data.
- This project is not affiliated with any model provider.

## License

Planned to be released under the MIT License. A `LICENSE` file will be added to the repository.

---

<p align="center">Made for stories that never end. ♾️</p>
