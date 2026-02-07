# Repository Guidelines

## Project Structure & Module Organization
TreeRAG is a Rust workspace. Core code lives in `crates/`:
- `treerag-daemon`: daemon lifecycle and request routing
- `treerag-cli`: CLI commands (`start`, `stop`, `status`, `init`, `project`, `ping`)
- `treerag-ipc`: Unix socket protocol/client/server
- `treerag-indexer`: scanning, parsing, storage, file watching
- `treerag-context`: context routing/rendering
- `treerag-core`: shared config, metrics, project management

Integration hooks and slash commands are in `claude-integration/`. Operational docs are in `docs/`, and launchd assets are in `integration/`. Build artifacts are under `target/` and should not be edited.

## Build, Test, and Development Commands
- `cargo build --release`: build all workspace crates.
- `cargo test --workspace`: run unit + integration tests across crates.
- `cargo clippy --workspace -D warnings`: enforce lint cleanliness.
- `cargo fmt --check`: verify formatting.
- `cargo run -p treerag-daemon -- --dev`: run daemon in foreground for local debugging.
- `cargo run -p treerag-cli -- status`: query daemon status from CLI.
- `./claude-integration/install.sh`: install Claude integration scripts.

## Coding Style & Naming Conventions
Use Rust 2021 defaults and `rustfmt` output (4-space indentation, trailing commas where formatted). Follow standard Rust naming: `snake_case` for functions/modules/files, `PascalCase` for types/traits, `UPPER_SNAKE_CASE` for constants. Keep modules focused by crate responsibility; shared contracts belong in `treerag-ipc` or `treerag-core`, not duplicated across crates.

## Testing Guidelines
Keep unit tests near implementation with `#[cfg(test)]`. Place cross-component tests in `crates/<crate>/tests/` and prefer descriptive files such as `integration_ipc.rs` or `integration_context.rs`. New behavior should include happy-path, error-path, and boundary-case coverage. The testing strategy in `docs/implementation/testing-strategy.md` targets high coverage (`>90%` unit, `>80%` integration) and requires `test`, `clippy`, and `fmt` checks for PR readiness.

## Commit & Pull Request Guidelines
This workspace snapshot does not include `.git`, so local history cannot be inspected for existing commit style. Use Conventional Commit prefixes (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`) with imperative summaries. PRs should include: scope by crate, linked task/issue, commands run (for example `cargo test --workspace`), and any behavior evidence for CLI/hook changes.
