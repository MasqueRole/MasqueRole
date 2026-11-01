# AGENTS.md: Build Instructions for AI Agents

This file tells an AI coding agent how to work on MasqueRole. Read it fully before writing code. The project vision, features, and configuration are described in the [README](README.md); this file is about *how to build and change it*.

## Ground Rules

1. **Rust only.** One language, one toolchain, one binary. No JavaScript, no web views.
2. **Provider-agnostic.** Never hard-code a single AI provider. All model access goes through the `LlmProvider` trait in `masquerole-core` (OpenAI-compatible API first, other adapters later). Provider names may appear only in presets and examples.
3. **Core has no UI.** `masquerole-core` must not depend on `egui`, `eframe`, or `masquerole-ui`. The dependency direction is `masquerole-app → masquerole-ui → egui` and `masquerole-app → masquerole-core`.
4. **Uniform styling.** Every visual element comes from the `masquerole-ui` design system (see [Design System](#design-system)). No ad-hoc colors, sizes, or fonts in `masquerole-app`.
5. **Secrets stay out of files.** API keys go in the OS keychain (`keyring`), never in `settings.toml`, logs, tests, or commits.
6. **Small, verified changes.** Make a change compile, run the tests, commit, then continue.
7. **Stay in scope.** Do not add features that were not asked for or are not in the README. Propose them instead.

## Before Every Code Change

**Pull the latest version from `main` first** (`git pull origin main`), so you build on the current code and avoid conflicts later.

## Branches and Merging — The Last Step Is Always Human

**Never push directly to `main`.** Work on a branch, push that branch, and stop. Merging into `main` (and tagging a release) is done by a human after review; an AI agent never does the last step.

**Versioning:** [SemVer](https://semver.org).

**Branch names** use one of these prefixes:

- `feature/<feature-name>`: new functionality, enhancements, or user stories (e.g. `feature/user-login`).
- `fix/<bug-name>` or `bugfix/<bug-name>`: errors, regressions, or system faults (e.g. `fix/session-timeout`).
- `release/<version>`: preparing code for deployment or testing a specific version (e.g. `release/v1.2.0`).
- `hotfix/<urgent-fix-name>`: critical patches applied directly to production environments.

## Tutorial Screenshots

The first-run tutorial shows screenshots of the app (`assets/tutorial/`, one 1100x720 PNG per tutorial page, in `crates/masquerole-app/src/tutorial.rs`). **Whenever a change alters how the app looks, retake the screenshots** so they match, and adjust the tutorial text if it no longer fits. Take them from the running app with demo data (point `IR_DATA_DIR` at a demo folder), in the dark theme, with the window's inner size at 1100x720.

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
MasqueRole/
├── Cargo.toml                  # Workspace manifest
├── crates/
│   ├── masquerole-core/                # Logic only: provider client, memory, tree, db, settings
│   ├── masquerole-ui/                  # Design system: theme tokens + reusable components (like shadcn/ui)
│   └── masquerole-app/                 # Desktop app: screens composed from masquerole-ui components
├── characters/                 # Example character cards (YAML)
├── worlds/                     # Example lorebooks (YAML)
├── docs/                       # GitHub Pages site (empty for now)
└── prompts/                    # System prompts & style profiles
```

## Design System

`masquerole-ui` is the Rust equivalent of shadcn/ui for egui: small, composable, consistently styled components built on top of design tokens. Like shadcn, components are plain code in the repo that we own and edit, not an opaque dependency.

### Tokens (`masquerole-ui/src/theme.rs`)

Define a single `Theme` struct and apply it once at startup with `Theme::apply(&egui::Context)`. It sets `egui::Style` and `egui::Visuals` so even raw egui widgets match.

- **Colors (semantic names, as in shadcn):** `background`, `foreground`, `card`, `card_foreground`, `popover`, `primary`, `primary_foreground`, `secondary`, `secondary_foreground`, `muted`, `muted_foreground`, `accent`, `accent_foreground`, `destructive`, `destructive_foreground`, `border`, `input`, `ring`.
- **Light and dark variants** with a runtime toggle; default follows the OS.
- **Radius:** `radius_sm`, `radius_md`, `radius_lg`.
- **Spacing scale:** 4, 8, 12, 16, 24, 32 px. Use only these values.
- **Typography:** one UI font, one monospace font, and a fixed size scale (`text_xs`, `text_sm`, `text_base`, `text_lg`, `text_xl`, `text_2xl`).
- Provide semantic colors for the story tree: `tree_current`, `tree_explored`, `tree_open`.

### Components (`masquerole-ui/src/components/`)

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
- Add a `gallery` example (`cargo run -p masquerole-ui --example gallery`) that shows every component in every variant, in light and dark. Update it whenever a component changes.

## Conventions

- **Errors:** `thiserror` in libraries, `anyhow` only in `masquerole-app`'s `main`.
- **Async:** `tokio`; never block the UI thread.
- **Naming:** `snake_case` files and functions, `CamelCase` types, crate names prefixed `masquerole-`.
- **Docs:** public items in `masquerole-core` and `masquerole-ui` get doc comments.
- **Dependencies:** keep them minimal; justify any new crate in the commit message.
- **Tests:** every `masquerole-core` module has unit tests. No test may require network access or a real API key.

## When Unsure

Prefer the simplest approach that satisfies the README. If a requirement is ambiguous or a step conflicts with these rules, stop and ask the maintainer instead of guessing.
