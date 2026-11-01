# Changelog

All notable changes to MasqueRole. The project follows [Semantic Versioning](https://semver.org).
This file lists only the current and the previous version.

## [0.11.0] - 2026-11-02

### Changed

- **Names:** the crates are now `masquerole-core`, `masquerole-ui` and `masquerole-app` instead of the old `ir-` names. Build and run the app with `cargo run -p masquerole-app`.
- **Installers:** the installers are named `masquerole_<version>_...` instead of `ir-app_<version>_...`.
- **Program name:** the installed program is `masquerole.exe` (`masquerole` on Linux) instead of `ir-app.exe`.

## [0.10.9] - 2026-11-02

### Fixed

- **Rail:** the button that hides the list is only shown in sections that have a list next to the rail. The provider page has none, so the button is gone there.
