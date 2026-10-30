# AGENTS.md: Build Instructions for AI Agents

This file tells an AI coding agent how to build InfiniteRoleplay step by step. Read it fully before writing code. The project vision, features, and configuration are described in the [README](README.md); this file is about *how to build it*.

## Ground Rules

1. **Rust only.** One language, one toolchain, one binary. No JavaScript, no web views.
2. **Provider-agnostic.** Never hard-code a single AI provider. All model access goes through the `LlmProvider` trait in `ir-core` (OpenAI-compatible API first, other adapters later). Provider names may appear only in presets and examples.
3. **Core has no UI.** `ir-core` must not depend on `egui`, `eframe`, or `ir-ui`. The dependency direction is `ir-app → ir-ui → egui` and `ir-app → ir-core`.
4. **Uniform styling.** Every visual element comes from the `ir-ui` design system (see [Design System](#design-system)). No ad-hoc colors, sizes, or fonts in `ir-app`.
5. **Secrets stay out of files.** API keys go in the OS keychain (`keyring`), never in `settings.toml`, logs, tests, or commits.
6. **Small, verified steps.** Finish one step, make it compile, run its tests, commit, then continue.
7. **Stay in scope.** Do not add features that are not in the README or the current step. Propose them instead.

## Definition of Done (for every step)

Run these before every commit; all must pass with no warnings:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Then commit with a short, descriptive message (imperative mood, e.g. `Add story tree storage`).

## Target Workspace Layout

```
InfiniteRoleplay/
├── Cargo.toml                  # Workspace manifest
├── crates/
│   ├── ir-core/                # Logic only: provider client, memory, tree, db, settings
│   ├── ir-ui/                  # Design system: theme tokens + reusable components (like shadcn/ui)
│   └── ir-app/                 # Desktop app: screens composed from ir-ui components
├── characters/                 # Example character cards (YAML)
├── worlds/                     # Example lorebooks (YAML)
└── prompts/                    # System prompts & style profiles
```

## Design System

`ir-ui` is the Rust equivalent of shadcn/ui for egui: small, composable, consistently styled components built on top of design tokens. Like shadcn, components are plain code in the repo that we own and edit, not an opaque dependency.

### Tokens (`ir-ui/src/theme.rs`)

Define a single `Theme` struct and apply it once at startup with `Theme::apply(&egui::Context)`. It sets `egui::Style` and `egui::Visuals` so even raw egui widgets match.

- **Colors (semantic names, as in shadcn):** `background`, `foreground`, `card`, `card_foreground`, `popover`, `primary`, `primary_foreground`, `secondary`, `secondary_foreground`, `muted`, `muted_foreground`, `accent`, `accent_foreground`, `destructive`, `destructive_foreground`, `border`, `input`, `ring`.
- **Light and dark variants** with a runtime toggle; default follows the OS.
- **Radius:** `radius_sm`, `radius_md`, `radius_lg`.
- **Spacing scale:** 4, 8, 12, 16, 24, 32 px. Use only these values.
- **Typography:** one UI font, one monospace font, and a fixed size scale (`text_xs`, `text_sm`, `text_base`, `text_lg`, `text_xl`, `text_2xl`).
- Provide semantic colors for the story tree: `tree_current`, `tree_explored`, `tree_open`.

### Components (`ir-ui/src/components/`)

One file per component. Each exposes a builder-style API and uses only theme tokens.

- `button.rs`: `Button` with variants `Default`, `Secondary`, `Outline`, `Ghost`, `Destructive`, `Link`, and sizes `Sm`, `Md`, `Lg`, `Icon`.
- `input.rs`, `textarea.rs`, `select.rs`, `checkbox.rs`, `switch.rs`, `slider.rs`
- `card.rs`: `Card` with header, content, and footer areas
- `dialog.rs`, `popover.rs`, `tooltip.rs`, `toast.rs`
- `tabs.rs`, `badge.rs`, `separator.rs`, `scroll_area.rs`
- `label.rs`, `form_field.rs` (label + control + help/error text)
- `chat_bubble.rs`: user, character, and narrator variants
- `tree_node.rs`: the node visual used by the story tree view

### Rules for components

- No hard-coded colors or sizes; read them from `Theme` (via `ui.ctx()` or a `ThemeExt` trait).
- Support hover, focus (use the `ring` color), active, and disabled states.
- Every interactive component is keyboard accessible.
- Add a `gallery` example (`cargo run -p ir-ui --example gallery`) that shows every component in every variant, in light and dark. Update it whenever a component changes.

## Step-by-Step Plan

### Step 1: Workspace skeleton

1. Create the Cargo workspace with `ir-core`, `ir-ui`, and `ir-app` (edition 2021).
2. Add `rustfmt.toml` (defaults are fine) and a `.gitignore` for `target/`.
3. Add a GitHub Actions workflow running the three Definition-of-Done commands on Linux and Windows.
4. Make `cargo run -p ir-app` open an empty window titled "InfiniteRoleplay".

### Step 2: Design system foundation (`ir-ui`)

1. Implement `Theme` with tokens and light/dark palettes, plus `Theme::apply`.
2. Bundle fonts and load them into egui.
3. Implement `Button`, `Label`, `Separator`, `Card`, `Badge`.
4. Create the `gallery` example. Verify visually in light and dark.

### Step 3: Settings and secrets (`ir-core`)

1. Define `Settings` structs matching the README `settings.toml` (serde + TOML) with defaults.
2. Load/save from the platform data directory (use the `directories` crate).
3. Store and read the API key via `keyring`, keyed by provider. Fall back to the `IR_API_KEY` environment variable for development.
4. Unit-test round-tripping and defaults.

### Step 4: Provider client (`ir-core/client.rs`)

1. Define the `LlmProvider` trait: a `stream_chat` method taking messages and generation parameters, returning an async stream of text chunks, plus a non-streaming `complete` for summaries.
2. Implement an OpenAI-compatible provider with `reqwest` (SSE parsing for streaming), configurable `base_url` and `model`.
3. Add provider presets (name, base URL, key required yes/no) as data, not code branches.
4. Map failures to typed errors with `thiserror` (auth, rate limit, network, bad response).
5. Test with a mock HTTP server; never call real APIs in tests.

### Step 5: Storage (`ir-core/db.rs`)

1. Create the SQLite schema with `rusqlite` (bundled) and versioned migrations: sessions, characters, nodes (with `parent_id`), facts, summaries.
2. Add typed query functions; keep SQL in this module only.
3. Test migrations and queries against an in-memory database.

### Step 6: Characters and lorebooks

1. Implement YAML loading for character cards and lorebooks, matching the README formats.
2. Implement keyword matching that returns lorebook entries triggered by recent messages.
3. Add example files to `characters/` and `worlds/`.
4. Test parsing, validation errors, and keyword matching.

### Step 7: Context and memory

1. Implement token budgeting (`context.rs`): build the prompt from system prompt, character, active lore, facts, summary, and recent messages, within `max_context_tokens`.
2. Implement summarization of messages older than `summarize_after` using the provider.
3. Implement fact extraction and storage when `memory.fact_extraction` is on.
4. Test budget trimming and that the newest messages are always kept.

### Step 8: Story tree (`ir-core/tree.rs`)

1. Implement nodes that snapshot message, summary state, facts, and active lore.
2. Implement branch, jump to node, bookmark, prune, and list branches.
3. Restoring a node must exclude facts learned after it.
4. Implement the text rendering used by `/tree`.
5. Test branching, rewinding, and pruning thoroughly.

### Step 9: Commands (`ir-core/commands.rs`)

1. Parse and execute every slash command listed in the README against the core APIs.
2. Return structured results so any front end can render them.
3. Test parsing and each command.

### Step 10: Remaining design system components

Implement `Input`, `Textarea`, `Select`, `Checkbox`, `Switch`, `Slider`, `Dialog`, `Popover`, `Tooltip`, `Toast`, `Tabs`, `ScrollArea`, `FormField`, `ChatBubble`, and `TreeNode`. Update the gallery.

### Step 11: App screens (`ir-app`)

Build screens using **only** `ir-ui` components:

1. App shell: sidebar (sessions, characters, worlds), main area, top bar with theme toggle.
2. Chat screen with streaming responses, `ChatBubble`s, and an input box that understands slash commands.
3. Settings screen: provider, base URL, model, API key, context, memory, style, and tree options.
4. Character and world editors.
5. Run all async work (network, database) on a `tokio` runtime and send results to the UI over channels. The UI thread must never block.

### Step 12: Story tree view

1. Draw the tree with egui painting using `TreeNode` and the tree theme colors.
2. Support pan, zoom, click to preview, second click to jump, and collapsing branches.
3. Highlight the current node and the current path.

### Step 13: Polish and release

1. Handle errors in the UI with `Toast` and `Dialog`, never panics.
2. Add `tracing` logging behind a debug setting, with API keys redacted.
3. Check startup time and binary size in release mode.
4. Add packaging (`cargo-packager`) and a release workflow for Windows and Linux.
5. Update the README to match reality (status banner, install steps, license file).

## Conventions

- **Errors:** `thiserror` in libraries, `anyhow` only in `ir-app`'s `main`.
- **Async:** `tokio`; never block the UI thread.
- **Naming:** `snake_case` files and functions, `CamelCase` types, crate names prefixed `ir-`.
- **Docs:** public items in `ir-core` and `ir-ui` get doc comments.
- **Dependencies:** keep them minimal; justify any new crate in the commit message.
- **Tests:** every `ir-core` module has unit tests. No test may require network access or a real API key.

## When Unsure

Prefer the simplest approach that satisfies the README. If a requirement is ambiguous or a step conflicts with these rules, stop and ask the maintainer instead of guessing.
