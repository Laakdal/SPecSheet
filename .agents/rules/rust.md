---
description: Rust conventions (cargo, errors, unsafe, clippy, tests)
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
---

# Rust

- **Workflow.** Before reporting done, run `cargo fmt --check`, `cargo clippy --all-targets` (with the project's lint settings) and `cargo test`. In workspaces, scope with `-p <crate>` while iterating, then run the whole workspace.
- **Errors.** Libraries return typed errors (`thiserror` or hand-written enums). Binaries may use `anyhow` with `.context(...)`. No `unwrap()`/`expect()` on fallible paths in non-test code unless an invariant makes failure impossible; say which in the `expect` message.
- **Unsafe.** Avoid it. When it's required, keep it in small, documented functions with a `// SAFETY:` comment stating the invariants, and add tests that exercise the boundaries.
- **APIs.** Accept borrowed types (`&str`, `&[T]`, `impl AsRef<Path>`) and return owned ones. Derive `Debug` on public types. Keep public surface minimal and document it with `///`, including examples.
- **Dependencies.** Add with `cargo add`. Enable only the features you need. Check `cargo audit` or `cargo deny` if the project uses them.
- **Performance.** Measure with `criterion`/`cargo bench` or a profiler before optimizing. Avoid needless `clone()` and allocation in hot loops.
- **Tests.** Unit tests in `#[cfg(test)] mod tests` next to the code; integration tests in `tests/`. Use `proptest`/`quickcheck` for invariants if the project already does.
- **Desktop/GUI (Tauri/egui/bevy).** Keep core logic in a UI-free crate or module so it can be tested without a window. Respect platform differences (paths, line endings, high-DPI) and test on the OSes the project ships to.
