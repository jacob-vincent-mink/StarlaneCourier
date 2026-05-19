# Starlane Courier

A Rust terminal UI prototype for a sci-fi courier and dispatch simulator.

## Prerequisites

- A working Rust toolchain with `cargo` available
- A terminal that supports full-screen terminal apps

## Run

From the project root, build and start the app with:

```bash
cargo run
```

Notes:

- The first run may take longer while Cargo downloads and compiles dependencies.
- The app takes over the terminal while running and restores the screen when you exit.
- Use `q` or `Ctrl+C` to quit.

## Start Screen

- The app now opens on a start screen before the live bridge launches.
- Current menu options are `New Game`, `Load Game`, `Settings`, `How To Play`, and `Quit`.
- `Left` / `Right` on the start screen changes the selected archive slot for `New Game` and `Load Game`.
- `Load Game` restores a shift from the selected archive slot.
- `Settings` lets you change both simulation speed and difficulty.

## Persistence

- The app saves shifts into slot files under `saves/` in the project root.
- Current slots are `saves/slot-1.json`, `saves/slot-2.json`, and `saves/slot-3.json`.
- Saves are written on simulation ticks, after mission/dispatch changes, and when exiting.
- `Load Game` restores the saved contracts, fleet states, discoveries, credits, difficulty, and event log.

## Difficulty

- `Cozy`: fixed rewards, no fuel pressure, and no win/lose enforcement.
- `Normal`: fuel costs matter, rewards decay after a contract is accepted, and the run ends if you hit the win condition or run out of viable progress.
- `Insane`: fuel costs matter, rewards decay faster, accepted contracts can fail if their delivery window expires, and the run ends on win or dead-end loss.

## Win and Lose

- `Normal` and `Insane` have a real end state.
- Win by charting the full sector and reaching `600` credits.
- Lose when no viable contract or frontier-progress route remains.
- In practice, the main failure case is running out of credits needed to refuel ships for meaningful runs.

## Fuel

- Outside `Cozy`, ships have finite fuel reserves and tank sizes.
- Stations now have finite fuel stock, shown directly on the sector map and in route previews.
- Route previews show required fuel, fuel aboard, and the current station's fuel stock before launch.
- Ships do not auto-refuel on dispatch anymore.
- Use `f` to buy fuel for the selected docked ship from the current station.
- Use `t` to transfer fuel from another docked ship at the same station.
- Long routes can become temporarily impossible if a ship's tank is too small or if you cannot afford the refuel.
- A ship with `0` fuel cannot depart until it is refueled or receives transferred fuel.

## Ship Roles

- Ships now differ by `speed`, `max fuel`, and current fuel reserve.
- Faster ships can satisfy tighter contract ETA requirements.
- Larger tanks let ships cover longer routes without extra staging.
- This means a contract may be feasible for one ship but too slow or too short-ranged for another.

## Development Checks

Format and compile-check the project with:

```bash
cargo fmt && cargo check && cargo test
```

## Controls

Start screen:

- `Up` / `Down`: move between menu items
- `Left` / `Right`: change the selected archive slot
- `Enter`: confirm the selected item
- `q` or `Ctrl+C`: quit

Load game screen:

- `Up` / `Down`: choose a save slot
- `Enter`: load the selected slot
- `Esc`: return to the start screen

In game:

- `Tab` / `Shift+Tab`: move focus between panes
- `Left` / `Right`: move focus between panes
- `Up` / `Down`: move the current selection in the focused pane
- `Enter` in `Mission Board`: accept or release the selected contract
- `Enter` in `Fleet`: start route planning for the selected ship
- `Enter` in `Sector Map`: confirm the selected destination while route planning
- `f`: refuel the selected docked ship from the current station
- `t`: transfer fuel from another docked ship at the same station to the selected ship
- The in-app `Mission` pane explains the current goals, contract flow, and ship phases
- `Esc`: cancel an in-progress route, or return to the start menu when not route planning
- `q` or `Ctrl+C`: quit

## Current Gameplay Slice

- The current top-level goals are to chart the full sector and reach `600` credits
- Accept a contract from the `Mission Board`
- Select a ship in `Fleet`, refuel or transfer fuel if needed, and press `Enter` to plan a route
- Move to a charted destination on the `Sector Map`
- Press `Enter` again to confirm the route
- If the route matches the tracked contract, the ship carries that contract until delivery
- Outside `Cozy`, route affordability also depends on fuel and refuel cost
- Watch the ship move through `Undocking`, `Cruising`, `Approach`, and `Arrived`

## Mission Loop

- The `Mission` pane shows the exploration objective, credit target, tracked contract, and input flow
- The `Mission Board` now contains structured contracts with origins, destinations, and rewards
- Contracts also have ETA targets, so a slower ship may not qualify for the same job as a faster one
- Frontier locations reveal deeper destinations on first arrival, so exploration now has a clear progression goal
- The early objective chain is: `Dust Harbor` -> `Kite Station` -> `Ion Anchorage` -> `Outer Ring Relay`
- Ship movement is shown as explicit phases: `Route Planning` -> `Undocking` -> `Cruising` -> `Approach` -> `Arrived`
- Contract pressure now depends on difficulty: none in `Cozy`, reward decay plus bankruptcy risk in `Normal`, and reward decay plus delivery windows in `Insane`

## Sector Map

- The center pane is now a graphical sector view rather than a text-only route list
- Charted locations are drawn directly on the map, ships are shown at docks or in transit, and uncharted contacts appear as `???` in unexplored space
- The highlighted route shows the currently selected dispatch preview
- Some charted locations are marked as frontier leads; the first arrival there reveals a new destination
- The current discovery chain starts with `Astra Prime` and `Dust Harbor`, then expands outward as ships arrive at frontier locations
