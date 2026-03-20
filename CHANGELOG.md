# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.3] - 2026-03-20

### Added
- Unit tests for counter blox covering all acceptance criteria from spec
- `readme = "README.md"` in workspace.package for crates.io display

### Fixed
- Doc links in `event_tag.rs` now correctly reference `StateRule` instead of internal `TransitionRule`
- Clippy warning in `worker-blox/tests.rs` - replaced manual `Default` impl with derive
- Pool blox context now uses `#[ctor]` annotation correctly instead of deprecated `#[self_id]`

### Changed
- Completely rewrote `skills/building-with-bloxide/SKILL.md` with accurate examples from codebase
- Completely rewrote `skills/building-with-bloxide/reference.md` with correct macro syntax and patterns
- Completely rewrote `skills/contributing-to-bloxide/SKILL.md` with current framework architecture
- Updated skill documentation to reflect naming-convention-based field detection in `#[derive(BloxCtx)]`

## [0.0.2] - 2026-03-20

### Added
- Domain-specific peer control types (`WorkerCtrl<R>`, `HasWorkerPeers<R>`) that enable `#[delegates]` to work

### Changed  
- Peer introduction now uses domain-specific control types instead of generic framework types

## [0.0.1] - Initial Release

Initial release of the Bloxide framework.
