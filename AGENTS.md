# AGENTS.md: Build Instructions for AI Agents

This file tells an AI coding agent how to work on InfiniteRoleplay. Read it fully before writing code. The project vision, features, and configuration are described in the [README](README.md); this file is about *how to build and change it*.

## Ground Rules

1. **Rust only.** One language, one toolchain, one binary. No JavaScript, no web views.
2. **Provider-agnostic.** Never hard-code a single AI provider. All model access goes through the `LlmProvider` trait in `ir-core` (OpenAI-compatible API first, other adapters later). Provider names may appear only in presets and examples.
3. **Core has no UI.** `ir-core` must not depend on `egui`, `eframe`, or `ir-ui`. The dependency direction is `ir-app → ir-ui → egui` and `ir-app → ir-core`.
4. **Uniform styling.** Every visual element comes from the `ir-ui` design system (see [Design System](#design-system)). No ad-hoc colors, sizes, or fonts in `ir-app`.
5. **Secrets stay out of files.** API keys go in the OS keychain (`keyring`), never in `settings.toml`, logs, tests, or commits.
6. **Small, verified changes.** Make a change compile, run the tests, commit, then continue.
7. **Stay in scope.** Do not add features that were not asked for or are not in the README. Propose them instead.

## Before Every Code Change

**Pull the latest version from `main` first** (`git pull origin main`), so you build on the current code and avoid conflicts later.

## Definition of Done (for every change)

Run these before every commit; all must pass with no warnings:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

**Before every new commit, pull the latest version from `main`** (`git pull origin main`) and resolve any conflicts, then run the checks above again if anything came in.

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

## Conventions

- **Errors:** `thiserror` in libraries, `anyhow` only in `ir-app`'s `main`.
- **Async:** `tokio`; never block the UI thread.
- **Naming:** `snake_case` files and functions, `CamelCase` types, crate names prefixed `ir-`.
- **Docs:** public items in `ir-core` and `ir-ui` get doc comments.
- **Dependencies:** keep them minimal; justify any new crate in the commit message.
- **Tests:** every `ir-core` module has unit tests. No test may require network access or a real API key.

## When Unsure

Prefer the simplest approach that satisfies the README. If a requirement is ambiguous or a step conflicts with these rules, stop and ask the maintainer instead of guessing.
