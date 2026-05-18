# AGENTS

## Commands

- Run the app from the repo root with `cargo run`.
- Default verification is `cargo fmt && cargo check`.
- There are no tests, CI workflows, task runners, or extra repo-local instructions yet.

## Structure

- This repo is a single binary crate, not a workspace.
- The only code entrypoint is `src/main.rs`.
- The package name in `Cargo.toml` is `starlane-courier`, while the app itself is presented as `Starlane Courier` in the UI and README.

## TUI Implementation Notes

- The TUI stack is `ratatui` + `crossterm`.
- Terminal setup/cleanup is manual: `setup_terminal()` enters raw mode and the alternate screen; `TerminalGuard` restores the terminal on drop. Preserve that restore path when changing startup, shutdown, or error handling.
- The main loop lives in `run_app()`: key handling runs when `event::poll()` returns input, and simulation updates currently happen through `App::tick()` on poll timeouts.

## Change Guidance

- Keep changes small and local by default. The codebase currently keeps all app state, rendering, and input handling in `src/main.rs`.
- After interactive changes, do a quick manual smoke test with `cargo run` to confirm the terminal still restores cleanly after exit.
