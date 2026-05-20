use crate::app::{App, Screen};
use crate::save::SaveGame;
use serde::Serialize;
use std::io::{self, BufRead};

#[derive(Serialize)]
struct AgentStatus {
    screen: &'static str,
    menu_feedback: Option<String>,
    popup_message: Option<String>,
    action_feedback: Option<String>,
    active_save_slot: usize,
    llm_enabled: bool,
    last_llm_status: String,
    game: Option<SaveGame>,
}

fn print_status(app: &App) {
    let screen = match app.screen {
        Screen::LlmGate => "LlmGate",
        Screen::StartMenu => "StartMenu",
        Screen::LoadGame => "LoadGame",
        Screen::InitializingWorld => "InitializingWorld",
        Screen::Settings => "Settings",
        Screen::HowToPlay => "HowToPlay",
        Screen::InGame => "InGame",
        Screen::EndGame => "EndGame",
    };

    let status = AgentStatus {
        screen,
        menu_feedback: app.menu_feedback.clone(),
        popup_message: app.popup_message.clone(),
        action_feedback: app.game.action_feedback.clone(),
        active_save_slot: app.active_save_slot,
        llm_enabled: app.llm_settings.enabled,
        last_llm_status: app.last_llm_status.clone(),
        game: if app.has_active_game {
            Some(app.snapshot())
        } else {
            None
        },
    };

    if let Ok(json) = serde_json::to_string(&status) {
        println!("{}", json);
    }
}

pub(crate) fn run_agent_mode(app: &mut App) -> io::Result<()> {
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        app.sync_background_work();
        print_status(app);

        let Some(line) = lines.next() else {
            break;
        };
        let line = line?;
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        let cmd = tokens[0];
        let args = &tokens[1..];

        match cmd {
            "status" => {
                // print_status is run automatically at start of loop
            }
            "new-game" => {
                if args.is_empty() {
                    let msg =
                        "Error: new-game requires a difficulty index (0=Cozy, 1=Normal, 2=Insane)";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                    continue;
                }
                let Ok(diff_idx) = args[0].parse::<usize>() else {
                    let msg = "Error: invalid difficulty index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                    continue;
                };
                if diff_idx > 2 {
                    let msg = "Error: difficulty index must be 0, 1, or 2";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                    continue;
                }
                let slot = if args.len() > 1 {
                    match args[1].parse::<usize>() {
                        Ok(s) => s,
                        Err(_) => {
                            let msg = "Error: invalid slot index";
                            app.game.set_action_feedback(msg);
                            eprintln!("{}", msg);
                            continue;
                        }
                    }
                } else {
                    0
                };
                let difficulty = crate::app::DIFFICULTY_OPTIONS[diff_idx];
                app.game.difficulty = difficulty;
                app.difficulty_selection = diff_idx;
                app.load_slot_selection = slot;
                app.begin_new_game();
            }
            "load-game" => {
                if args.is_empty() {
                    let msg = "Error: load-game requires slot index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                    continue;
                }
                let Ok(slot) = args[0].parse::<usize>() else {
                    let msg = "Error: invalid slot index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                    continue;
                };
                if let Err(e) = app.load_game(slot) {
                    let msg = format!("Error: load game failed: {}", e);
                    app.game.set_action_feedback(&msg);
                    eprintln!("{}", msg);
                }
            }
            "save-game" => {
                let slot = if !args.is_empty() {
                    match args[0].parse::<usize>() {
                        Ok(s) => s,
                        Err(_) => {
                            let msg = "Error: invalid slot index";
                            app.game.set_action_feedback(msg);
                            eprintln!("{}", msg);
                            continue;
                        }
                    }
                } else {
                    app.active_save_slot
                };
                app.active_save_slot = slot;
                if let Err(e) = app.save_game() {
                    let msg = format!("Error: save game failed: {}", e);
                    app.game.set_action_feedback(&msg);
                    eprintln!("{}", msg);
                }
            }
            "pane" => {
                if args.is_empty() {
                    let msg = "Error: pane requires a pane index or name";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                    continue;
                }
                let pane_idx = match args[0] {
                    "contracts" | "board" | "mission" => Some(crate::game::CONTRACTS_PANE),
                    "map" | "sector" => Some(crate::game::MAP_PANE),
                    "fleet" | "ships" => Some(crate::game::FLEET_PANE),
                    "shipyard" | "shipyards" | "shop" => Some(crate::game::SHIPYARD_PANE),
                    "log" | "logs" | "alerts" => Some(crate::game::LOG_PANE),
                    other => other.parse::<usize>().ok(),
                };
                if let Some(idx) = pane_idx {
                    if idx < crate::game::PANE_COUNT {
                        app.game.active_pane = idx;
                    } else {
                        let msg = "Error: pane index out of bounds";
                        app.game.set_action_feedback(msg);
                        eprintln!("{}", msg);
                    }
                } else {
                    let msg = "Error: invalid pane index or name";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                }
            }
            "select-contract" => {
                if args.is_empty() {
                    let msg = "Error: select-contract requires an index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                    continue;
                }
                if let Ok(idx) = args[0].parse::<usize>() {
                    app.game.selected_contract = idx;
                } else {
                    let msg = "Error: invalid index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                }
            }
            "select-ship" => {
                if args.is_empty() {
                    let msg = "Error: select-ship requires an index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                    continue;
                }
                if let Ok(idx) = args[0].parse::<usize>() {
                    app.game.selected_ship = idx;
                } else {
                    let msg = "Error: invalid index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                }
            }
            "select-location" => {
                if args.is_empty() {
                    let msg = "Error: select-location requires an index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                    continue;
                }
                if let Ok(idx) = args[0].parse::<usize>() {
                    app.game.selected_location = idx;
                } else {
                    let msg = "Error: invalid index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                }
            }
            "select-shipyard-offer" => {
                if args.is_empty() {
                    let msg = "Error: select-shipyard-offer requires an index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                    continue;
                }
                if let Ok(idx) = args[0].parse::<usize>() {
                    app.game.selected_shipyard_offer = idx;
                } else {
                    let msg = "Error: invalid index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                }
            }
            "select-alert" => {
                if args.is_empty() {
                    let msg = "Error: select-alert requires an index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                    continue;
                }
                if let Ok(idx) = args[0].parse::<usize>() {
                    app.game.selected_alert = idx;
                } else {
                    let msg = "Error: invalid index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                }
            }
            "press" => {
                if args.is_empty() {
                    let msg = "Error: press requires key label";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                    continue;
                }
                let mut valid = true;
                match args[0] {
                    "enter" => app.activate_selection(),
                    "esc" => {
                        let _ = app.cancel_dispatch();
                    }
                    "f" | "refuel" => app.refuel_selected_ship(),
                    "t" | "transfer" => app.transfer_fuel_to_selected_ship(),
                    "u" | "upgrade" => app.upgrade_selected_ship(),
                    "e" | "explore" => app.begin_exploration(),
                    "m" | "move-player" => app.transfer_player_to_selected_location(),
                    "b" | "buy-ship" => app.purchase_ship_at_selected_location(),
                    "r" | "regenerate" => app.regenerate_selected_contract_flavor(),
                    other => {
                        let msg = format!("Error: unknown key '{}'", other);
                        app.game.set_action_feedback(&msg);
                        eprintln!("{}", msg);
                        valid = false;
                    }
                }
                if valid {
                    app.sync_action_feedback_popup();
                    app.sync_end_screen();
                    let _ = app.save_game();
                }
            }
            "tick" => {
                let ticks = if !args.is_empty() {
                    match args[0].parse::<usize>() {
                        Ok(t) => t,
                        Err(_) => {
                            let msg = "Error: invalid tick count";
                            app.game.set_action_feedback(msg);
                            eprintln!("{}", msg);
                            continue;
                        }
                    }
                } else {
                    1
                };
                for _ in 0..ticks {
                    app.tick();
                }
            }
            "accept-contract" | "track-contract" => {
                if args.is_empty() {
                    let msg = "Error: accept-contract requires an index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                    continue;
                }
                if let Ok(idx) = args[0].parse::<usize>() {
                    app.game.active_pane = crate::game::CONTRACTS_PANE;
                    app.game.selected_contract = idx;
                    app.game.toggle_contract_tracking();
                    app.sync_action_feedback_popup();
                    app.sync_end_screen();
                    let _ = app.save_game();
                } else {
                    let msg = "Error: invalid index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                }
            }
            "refuel-ship" => {
                if args.is_empty() {
                    let msg = "Error: refuel-ship requires an index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                    continue;
                }
                if let Ok(idx) = args[0].parse::<usize>() {
                    app.game.active_pane = crate::game::FLEET_PANE;
                    app.game.selected_ship = idx;
                    app.refuel_selected_ship();
                    app.sync_action_feedback_popup();
                    app.sync_end_screen();
                    let _ = app.save_game();
                } else {
                    let msg = "Error: invalid index";
                    app.game.set_action_feedback(msg);
                    eprintln!("{}", msg);
                }
            }
            "quit" | "exit" => {
                let _ = app.save_game();
                break;
            }
            other => {
                let msg = format!("Error: unknown command '{}'", other);
                app.game.set_action_feedback(&msg);
                eprintln!("{}", msg);
            }
        }
    }

    Ok(())
}
