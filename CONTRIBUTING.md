# Contributing to BudBuk

Thanks for your interest in contributing! This document explains how to get set up and
what we expect from changes.

## Code of Conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md). By participating,
you agree to uphold it.

## Getting started

1. Install Rust (1.85+):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
2. Fork and clone the repository.
3. Build and test:
   ```bash
   make build
   make test
   ```

## Development workflow

Before opening a pull request, run the full local check (mirrors CI):

```bash
make check
```

This runs, in order:

- `cargo fmt --all -- --check` — formatting
- `cargo clippy --workspace --all-targets -- -D warnings` — linting (warnings are errors)
- `cargo test --workspace` — all tests
- the 100% line-coverage gate (`make cov-check`)

You'll need `cargo-llvm-cov` for coverage:

```bash
cargo install cargo-llvm-cov
rustup component add llvm-tools-preview
```

## Standards

- **Formatting:** `rustfmt` (config in `rustfmt.toml`). No manual formatting exceptions.
- **Linting:** `clippy` clean with `-D warnings`.
- **Tests:** every change must keep line coverage at **100%**. Add tests alongside new
  code. Prefer testing library modules; keep binary entrypoints thin.
- **Errors:** return `Result` with a `ConnectorError` variant; never `panic!` in library
  code paths that can fail at runtime.
- **No secrets:** never commit tokens, passwords, or `.env` files. Use the mock mode or
  a local `.env` (git-ignored) for testing.
- **Commits:** clear, imperative messages. We follow
  [Conventional Commits](https://www.conventionalcommits.org/) (e.g. `feat: add github
  connector`, `fix: handle empty user page`).
- **Changelog:** update `CHANGELOG.md` under "Unreleased" for user-facing changes.

## Adding a new connector

BudBuk connectors implement the `connector_sdk::Connector` trait. In broad strokes:

1. Create a new crate under `crates/` (e.g. `github-connector`).
2. Define a config struct (per-instance URL + credentials).
3. Implement `Connector` (`name`, `discover`, `fetch`), reusing the SDK's cache and
   types. Add a `client` module for HTTP and a pushdown module for the source's query
   language.
4. Provide a `mock` mode so the connector runs without credentials.
5. Add tests to keep coverage at 100% (unit tests for pure logic;
   [`wiremock`](https://docs.rs/wiremock) for the HTTP client).

## Reporting bugs and requesting features

Use the [issue templates](.github/ISSUE_TEMPLATE). For security issues, follow
[SECURITY.md](SECURITY.md) instead of opening a public issue.

## License

By contributing, you agree that your contributions will be dual-licensed under
MIT OR Apache-2.0, matching the project license.
