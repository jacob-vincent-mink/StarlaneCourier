# AGENTS

## Commands

- Run the app from the repo root with `cargo run`.
- Default verification is `cargo fmt && cargo check && cargo test`.
- There are no CI workflows or task runners yet.

## Structure

- This repo is a single binary crate, not a workspace.
- The binary entrypoint is still `src/main.rs`, but most logic now lives in modules under `src/`.
- `src/app.rs` is the TUI shell and session orchestration layer.
- `src/game/` holds gameplay state and pure-ish game rules.
- `src/game/routing.rs` holds route planning and dispatch rules.
- `src/game/fuel.rs` holds refuel, transfer, and fuel-planning rules.
- `src/game/contracts.rs` holds contract state and payout/deadline rules.
- `src/game/outcome.rs` holds discovery progression plus win/lose checks.
- `src/save.rs` holds save types plus the `SaveStore` persistence boundary.
- `src/ui.rs` holds ratatui rendering and text-formatting helpers.
- The package name in `Cargo.toml` is `starlane-courier`, while the app itself is presented as `Starlane Courier` in the UI and README.

## TUI Implementation Notes

- The TUI stack is `ratatui` + `crossterm`.
- Terminal setup/cleanup is manual: `setup_terminal()` enters raw mode and the alternate screen; `TerminalGuard` restores the terminal on drop. Preserve that restore path when changing startup, shutdown, or error handling.
- The main loop lives in `run_app()`: key handling runs when `event::poll()` returns input, and simulation updates currently happen through `App::tick()` on poll timeouts.
- `GameData` in `src/game/mod.rs` is the core state boundary. Keep new pure game rules there when possible instead of in `App` or `ui`.
- Save/load is behind the `SaveStore` trait in `src/save.rs` with `FsSaveStore` for production and a memory-backed test double in unit tests. Prefer extending that boundary instead of adding more direct filesystem access in gameplay code.

## Change Guidance

- Keep changes small and local by default. Prefer putting core rule changes in the relevant `src/game/*.rs` module, shell/menu changes in `src/app.rs`, and purely presentational changes in `src/ui.rs`.
- After interactive changes, do a quick manual smoke test with `cargo run` to confirm the terminal still restores cleanly after exit.
