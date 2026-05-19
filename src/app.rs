use std::{
    io,
    ops::{Deref, DerefMut},
    time::Duration,
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::game::{ASTRA_PRIME, AppMode, Difficulty, GameData, PANE_COUNT};
use crate::save::{
    FsSaveStore, SAVE_SLOT_COUNT, SAVE_VERSION, SaveGame, SaveStore, SavedAppMode, SavedContract,
    SavedContractState, SavedShip, SavedShipState,
};

pub(crate) const TICK_SPEEDS: [(&str, u64); 3] = [("Slow", 450), ("Standard", 250), ("Fast", 125)];
pub(crate) const DIFFICULTY_OPTIONS: [Difficulty; 3] =
    [Difficulty::Cozy, Difficulty::Normal, Difficulty::Insane];

pub(crate) struct App {
    pub(crate) screen: Screen,
    pub(crate) has_active_game: bool,
    pub(crate) menu_feedback: Option<String>,
    pub(crate) start_menu_selection: usize,
    pub(crate) active_save_slot: usize,
    pub(crate) load_slot_selection: usize,
    pub(crate) settings_selection: usize,
    pub(crate) settings_focus: usize,
    pub(crate) tick_speed_index: usize,
    pub(crate) difficulty_selection: usize,
    save_store: Box<dyn SaveStore>,
    pub(crate) game: GameData,
}

impl Deref for App {
    type Target = GameData;

    fn deref(&self) -> &Self::Target {
        &self.game
    }
}

impl DerefMut for App {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.game
    }
}

impl App {
    pub(crate) fn new() -> Self {
        let difficulty = Difficulty::Normal;
        let game = GameData::new(difficulty);
        Self::with_store(Box::new(FsSaveStore), game)
    }

    pub(crate) fn with_store(save_store: Box<dyn SaveStore>, game: GameData) -> Self {
        let difficulty = game.difficulty;
        Self {
            screen: Screen::StartMenu,
            has_active_game: false,
            menu_feedback: None,
            start_menu_selection: 0,
            active_save_slot: 0,
            load_slot_selection: 0,
            settings_selection: 1,
            settings_focus: 0,
            tick_speed_index: 1,
            difficulty_selection: difficulty.index(),
            save_store,
            game,
        }
    }

    pub(crate) fn reset_game(&mut self) {
        self.game = GameData::new(self.difficulty);
        self.settings_selection = self.tick_speed_index;
        self.difficulty_selection = self.difficulty.index();
    }

    pub(crate) fn mode_label(&self) -> String {
        self.game.mode_label()
    }

    pub(crate) fn controls_text(&self) -> String {
        match self.mode {
            AppMode::Browse => {
                "Tab/Shift+Tab or Left/Right: focus   Up/Down: select   Enter in Board/Fleet   f: refuel   t: transfer fuel   Esc: menu   q/Ctrl+C: quit"
                    .to_string()
            }
            AppMode::SelectingDestination { .. } => {
                "Up/Down: choose destination   Enter: confirm route   Esc: cancel   q/Ctrl+C: quit"
                    .to_string()
            }
        }
    }

    pub(crate) fn poll_duration(&self) -> Duration {
        Duration::from_millis(TICK_SPEEDS[self.tick_speed_index].1)
    }

    pub(crate) fn save_slot_label(slot_index: usize) -> String {
        format!("Slot {}", slot_index + 1)
    }

    pub(crate) fn selected_slot_summary_text(&self) -> String {
        self.save_slot_summary_text(self.load_slot_selection)
    }

    pub(crate) fn slot_brief(&self, slot_index: usize) -> &'static str {
        match self.read_save_summary(slot_index) {
            Ok(Some(_)) => "saved",
            Ok(None) => "empty",
            Err(_) => "error",
        }
    }

    pub(crate) fn save_slot_summary_text(&self, slot_index: usize) -> String {
        match self.read_save_summary(slot_index) {
            Ok(Some(summary)) => summary,
            Ok(None) => format!("{} is empty.", Self::save_slot_label(slot_index)),
            Err(error) => format!("Save file is unreadable: {error}"),
        }
    }

    fn read_save_summary(&self, slot_index: usize) -> Result<Option<String>, String> {
        let Some(save) = self.save_store.read_slot(slot_index)? else {
            return Ok(None);
        };

        let charted = save
            .discovered_locations
            .iter()
            .filter(|&&seen| seen)
            .count();
        let tracked = save
            .contracts
            .iter()
            .filter(|contract| {
                matches!(
                    contract.state,
                    SavedContractState::Accepted { .. } | SavedContractState::Assigned { .. }
                )
            })
            .count();

        Ok(Some(format!(
            "{}: T+{:04} | {} | {} credits | {}/{} charts | {} active contract(s)",
            Self::save_slot_label(slot_index),
            save.clock,
            save.difficulty.label(),
            save.credits,
            charted,
            save.discovered_locations.len(),
            tracked,
        )))
    }

    pub(crate) fn save_game(&self) -> io::Result<()> {
        if !self.has_active_game {
            return Ok(());
        }

        let save = self.snapshot();
        self.save_store
            .write_slot(self.active_save_slot, &save)
            .map_err(io::Error::other)
    }

    fn snapshot(&self) -> SaveGame {
        SaveGame {
            version: SAVE_VERSION,
            tick_speed_index: self.tick_speed_index,
            active_pane: self.active_pane,
            clock: self.clock,
            mode: match self.mode {
                AppMode::Browse => SavedAppMode::Browse,
                AppMode::SelectingDestination { ship_index } => {
                    SavedAppMode::SelectingDestination { ship_index }
                }
            },
            selected_location: self.selected_location,
            selected_ship: self.selected_ship,
            selected_contract: self.selected_contract,
            tracked_contract: self.tracked_contract,
            credits: self.credits,
            difficulty: self.difficulty,
            run_outcome: self.run_outcome.clone(),
            discovered_locations: self.discovered_locations.clone(),
            station_fuel: self.station_fuel.clone(),
            fleet: self
                .fleet
                .iter()
                .map(|ship| SavedShip {
                    current_location: ship.current_location,
                    current_fuel: ship.current_fuel,
                    max_fuel: ship.max_fuel,
                    state: match &ship.state {
                        crate::game::ShipState::Docked => SavedShipState::Docked,
                        crate::game::ShipState::EnRoute {
                            origin,
                            destination,
                            eta_remaining,
                            total_eta,
                            route,
                            condition_summary,
                            assigned_contract,
                        } => SavedShipState::EnRoute {
                            origin: *origin,
                            destination: *destination,
                            eta_remaining: *eta_remaining,
                            total_eta: *total_eta,
                            route: route.clone(),
                            condition_summary: condition_summary.clone(),
                            assigned_contract: *assigned_contract,
                        },
                    },
                })
                .collect(),
            contracts: self
                .contracts
                .iter()
                .map(|contract| SavedContract {
                    deadline: contract.deadline,
                    state: match contract.state {
                        crate::game::ContractState::Available => SavedContractState::Available,
                        crate::game::ContractState::Accepted { accepted_at } => {
                            SavedContractState::Accepted { accepted_at }
                        }
                        crate::game::ContractState::Assigned {
                            ship_name,
                            accepted_at,
                        } => SavedContractState::Assigned {
                            ship_index: self
                                .fleet
                                .iter()
                                .position(|ship| ship.name == ship_name)
                                .unwrap_or(0),
                            accepted_at,
                        },
                        crate::game::ContractState::Completed => SavedContractState::Completed,
                        crate::game::ContractState::Failed => SavedContractState::Failed,
                    },
                })
                .collect(),
            log: self.log.clone(),
        }
    }

    pub(crate) fn load_game(&mut self, slot_index: usize) -> Result<(), String> {
        let Some(save) = self.save_store.read_slot(slot_index)? else {
            return Err(format!("{} is empty", Self::save_slot_label(slot_index)));
        };

        self.apply_save(save)?;
        self.active_save_slot = slot_index;
        self.load_slot_selection = slot_index;
        self.has_active_game = true;
        self.sync_end_screen();
        if self.run_outcome.is_none() {
            self.screen = Screen::InGame;
            self.evaluate_run_outcome();
            self.sync_end_screen();
        }
        self.menu_feedback = None;
        Ok(())
    }

    fn apply_save(&mut self, save: SaveGame) -> Result<(), String> {
        self.reset_game();

        if save.discovered_locations.len() != self.discovered_locations.len() {
            return Err("save file has incompatible discovery data".to_string());
        }
        if save.fleet.len() != self.fleet.len() {
            return Err("save file has incompatible fleet data".to_string());
        }
        if save.contracts.len() != self.contracts.len() {
            return Err("save file has incompatible contract data".to_string());
        }

        self.tick_speed_index = save.tick_speed_index.min(TICK_SPEEDS.len() - 1);
        self.active_pane = save.active_pane.min(PANE_COUNT - 1);
        self.clock = save.clock;
        self.selected_location = save.selected_location.min(self.locations.len() - 1);
        self.selected_ship = save.selected_ship.min(self.fleet.len() - 1);
        self.selected_contract = save.selected_contract.min(self.contracts.len() - 1);
        self.tracked_contract = save
            .tracked_contract
            .filter(|&index| index < self.contracts.len());
        self.credits = save.credits;
        self.difficulty = save.difficulty;
        self.difficulty_selection = self.difficulty.index();
        self.run_outcome = save.run_outcome;
        self.discovered_locations = save.discovered_locations;
        self.discovered_locations[ASTRA_PRIME] = true;
        if save.station_fuel.len() == self.station_fuel.len() {
            self.station_fuel = save.station_fuel;
        }

        let locations_len = self.locations.len();
        let contracts_len = self.contracts.len();
        for (ship, saved_ship) in self.fleet.iter_mut().zip(save.fleet) {
            ship.current_location = saved_ship.current_location.min(locations_len - 1);
            if saved_ship.max_fuel > 0 {
                ship.max_fuel = saved_ship.max_fuel.max(1);
                ship.current_fuel = saved_ship.current_fuel.min(ship.max_fuel);
            }
            ship.low_fuel_alerted = false;
            ship.state = match saved_ship.state {
                SavedShipState::Docked => crate::game::ShipState::Docked,
                SavedShipState::EnRoute {
                    origin,
                    destination,
                    eta_remaining,
                    total_eta,
                    route,
                    condition_summary,
                    assigned_contract,
                } => crate::game::ShipState::EnRoute {
                    origin: origin.min(locations_len - 1),
                    destination: destination.min(locations_len - 1),
                    eta_remaining,
                    total_eta: total_eta.max(eta_remaining),
                    route,
                    condition_summary,
                    assigned_contract: assigned_contract.filter(|&index| index < contracts_len),
                },
            };
        }

        let discovered = self.discovered_locations.clone();
        let fleet_names: Vec<&'static str> = self.fleet.iter().map(|ship| ship.name).collect();
        for (contract, saved_contract) in self.contracts.iter_mut().zip(save.contracts) {
            contract.deadline = saved_contract.deadline;
            contract.state = match saved_contract.state {
                SavedContractState::Available => crate::game::ContractState::Available,
                SavedContractState::Accepted { accepted_at } => {
                    crate::game::ContractState::Accepted { accepted_at }
                }
                SavedContractState::Assigned {
                    ship_index,
                    accepted_at,
                } => crate::game::ContractState::Assigned {
                    ship_name: fleet_names
                        .get(ship_index)
                        .copied()
                        .unwrap_or(fleet_names[0]),
                    accepted_at,
                },
                SavedContractState::Completed => crate::game::ContractState::Completed,
                SavedContractState::Failed => crate::game::ContractState::Failed,
            };

            let unlocked = discovered[contract.origin]
                && discovered[contract.destination]
                && discovered[contract.unlock_location];

            if !unlocked && !matches!(contract.state, crate::game::ContractState::Completed) {
                contract.state = crate::game::ContractState::Available;
            }
        }

        self.mode = match save.mode {
            SavedAppMode::Browse => AppMode::Browse,
            SavedAppMode::SelectingDestination { ship_index } => AppMode::SelectingDestination {
                ship_index: ship_index.min(self.fleet.len() - 1),
            },
        };
        self.log = if save.log.is_empty() {
            vec!["[0000] Save loaded.".to_string()]
        } else {
            save.log.into_iter().take(8).collect()
        };

        Ok(())
    }

    pub(crate) fn start_menu_options(&self) -> Vec<StartMenuAction> {
        let mut options = Vec::new();

        if self.has_active_game {
            options.push(StartMenuAction::ResumeShift);
        }

        options.extend([
            StartMenuAction::NewGame,
            StartMenuAction::LoadGame,
            StartMenuAction::Settings,
            StartMenuAction::HowToPlay,
            StartMenuAction::Quit,
        ]);

        options
    }

    fn sync_end_screen(&mut self) {
        if self.run_outcome.is_some() {
            self.screen = Screen::EndGame;
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> bool {
        match self.screen {
            Screen::StartMenu => self.handle_start_menu_key(key),
            Screen::LoadGame => self.handle_load_game_key(key),
            Screen::Settings => self.handle_settings_key(key),
            Screen::HowToPlay => self.handle_how_to_key(key),
            Screen::InGame => self.handle_game_key(key),
            Screen::EndGame => self.handle_end_game_key(key),
        }
    }

    pub(crate) fn tick(&mut self) {
        if self.screen != Screen::InGame {
            return;
        }

        self.game.tick();
        self.sync_end_screen();
        let _ = self.save_game();
    }

    fn activate_start_menu_selection(&mut self) -> bool {
        let action = self.start_menu_options()[self.start_menu_selection];

        match action {
            StartMenuAction::ResumeShift => {
                self.menu_feedback = None;
                self.screen = Screen::InGame;
                false
            }
            StartMenuAction::NewGame => {
                self.active_save_slot = self.load_slot_selection;
                self.reset_game();
                self.has_active_game = true;
                self.menu_feedback = self
                    .save_game()
                    .err()
                    .map(|error| format!("Save failed: {error}"));
                self.screen = Screen::InGame;
                false
            }
            StartMenuAction::LoadGame => {
                self.menu_feedback = None;
                self.screen = Screen::LoadGame;
                false
            }
            StartMenuAction::Settings => {
                self.menu_feedback = None;
                self.settings_selection = self.tick_speed_index;
                self.difficulty_selection = self.difficulty.index();
                self.settings_focus = 0;
                self.screen = Screen::Settings;
                false
            }
            StartMenuAction::HowToPlay => {
                self.menu_feedback = None;
                self.screen = Screen::HowToPlay;
                false
            }
            StartMenuAction::Quit => true,
        }
    }

    fn handle_start_menu_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
            (KeyCode::Up, _) => {
                let len = self.start_menu_options().len();
                self.start_menu_selection =
                    crate::game::wrap_index(self.start_menu_selection, len, -1);
                false
            }
            (KeyCode::Down, _) => {
                let len = self.start_menu_options().len();
                self.start_menu_selection =
                    crate::game::wrap_index(self.start_menu_selection, len, 1);
                false
            }
            (KeyCode::Left, _) => {
                self.load_slot_selection =
                    crate::game::wrap_index(self.load_slot_selection, SAVE_SLOT_COUNT, -1);
                self.menu_feedback = None;
                false
            }
            (KeyCode::Right, _) => {
                self.load_slot_selection =
                    crate::game::wrap_index(self.load_slot_selection, SAVE_SLOT_COUNT, 1);
                self.menu_feedback = None;
                false
            }
            (KeyCode::Enter, _) => self.activate_start_menu_selection(),
            _ => false,
        }
    }

    fn handle_load_game_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
            (KeyCode::Up, _) => {
                self.load_slot_selection =
                    crate::game::wrap_index(self.load_slot_selection, SAVE_SLOT_COUNT, -1);
                self.menu_feedback = None;
                false
            }
            (KeyCode::Down, _) => {
                self.load_slot_selection =
                    crate::game::wrap_index(self.load_slot_selection, SAVE_SLOT_COUNT, 1);
                self.menu_feedback = None;
                false
            }
            (KeyCode::Esc, _) => {
                self.screen = Screen::StartMenu;
                false
            }
            (KeyCode::Enter, _) => match self.load_game(self.load_slot_selection) {
                Ok(()) => false,
                Err(error) => {
                    self.menu_feedback = Some(format!("Load failed: {error}"));
                    false
                }
            },
            _ => false,
        }
    }

    fn handle_settings_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
            (KeyCode::Esc, _) => {
                self.screen = Screen::StartMenu;
                false
            }
            (KeyCode::Left, _) => {
                self.settings_focus = self.settings_focus.saturating_sub(1);
                false
            }
            (KeyCode::Right, _) => {
                self.settings_focus = (self.settings_focus + 1).min(1);
                false
            }
            (KeyCode::Up, _) => {
                if self.settings_focus == 0 {
                    self.settings_selection =
                        crate::game::wrap_index(self.settings_selection, TICK_SPEEDS.len(), -1);
                } else {
                    self.difficulty_selection = crate::game::wrap_index(
                        self.difficulty_selection,
                        DIFFICULTY_OPTIONS.len(),
                        -1,
                    );
                }
                false
            }
            (KeyCode::Down, _) => {
                if self.settings_focus == 0 {
                    self.settings_selection =
                        crate::game::wrap_index(self.settings_selection, TICK_SPEEDS.len(), 1);
                } else {
                    self.difficulty_selection = crate::game::wrap_index(
                        self.difficulty_selection,
                        DIFFICULTY_OPTIONS.len(),
                        1,
                    );
                }
                false
            }
            (KeyCode::Enter, _) => {
                self.tick_speed_index = self.settings_selection;
                self.difficulty = Difficulty::from_index(self.difficulty_selection);
                if self.has_active_game {
                    self.menu_feedback = self
                        .save_game()
                        .err()
                        .map(|error| format!("Save failed: {error}"));
                }
                self.screen = Screen::StartMenu;
                false
            }
            _ => false,
        }
    }

    fn handle_how_to_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
            (KeyCode::Esc | KeyCode::Enter, _) => {
                self.screen = Screen::StartMenu;
                false
            }
            _ => false,
        }
    }

    fn handle_end_game_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
            (KeyCode::Enter | KeyCode::Esc, _) => {
                self.menu_feedback = self.run_outcome.as_ref().map(|outcome| {
                    format!(
                        "Last run: {} in {}.",
                        outcome.title(),
                        Self::save_slot_label(self.active_save_slot)
                    )
                });
                self.has_active_game = false;
                self.screen = Screen::StartMenu;
                self.start_menu_selection = 0;
                false
            }
            _ => false,
        }
    }

    fn handle_game_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
            (KeyCode::Esc, _) => {
                if self.cancel_dispatch() {
                    false
                } else {
                    self.menu_feedback = self
                        .save_game()
                        .err()
                        .map(|error| format!("Save failed: {error}"));
                    self.screen = Screen::StartMenu;
                    self.start_menu_selection = 0;
                    false
                }
            }
            (KeyCode::Tab | KeyCode::Right, _) => {
                self.active_pane = (self.active_pane + 1) % PANE_COUNT;
                false
            }
            (KeyCode::BackTab | KeyCode::Left, _) => {
                self.active_pane = (self.active_pane + PANE_COUNT - 1) % PANE_COUNT;
                false
            }
            (KeyCode::Up, _) => {
                self.move_selection(-1);
                false
            }
            (KeyCode::Down, _) => {
                self.move_selection(1);
                false
            }
            (KeyCode::Enter, _) => {
                self.activate_selection();
                self.sync_end_screen();
                let _ = self.save_game();
                false
            }
            (KeyCode::Char('f'), _) => {
                self.refuel_selected_ship();
                self.sync_end_screen();
                let _ = self.save_game();
                false
            }
            (KeyCode::Char('t'), _) => {
                self.transfer_fuel_to_selected_ship();
                self.sync_end_screen();
                let _ = self.save_game();
                false
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    StartMenu,
    LoadGame,
    Settings,
    HowToPlay,
    InGame,
    EndGame,
}

#[derive(Clone, Copy)]
pub(crate) enum StartMenuAction {
    ResumeShift,
    NewGame,
    LoadGame,
    Settings,
    HowToPlay,
    Quit,
}

impl StartMenuAction {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ResumeShift => "Resume Shift",
            Self::NewGame => "New Game",
            Self::LoadGame => "Load Game",
            Self::Settings => "Settings",
            Self::HowToPlay => "How To Play",
            Self::Quit => "Quit",
        }
    }

    pub(crate) fn description(self, app: &App) -> String {
        match self {
            Self::ResumeShift => {
                "Return to the current bridge and continue the active dispatch shift.".to_string()
            }
            Self::NewGame => format!(
                "Start a fresh {} shift in {} with 120 credits, two charted locations, and an unopened contract board.",
                app.difficulty.label(),
                App::save_slot_label(app.load_slot_selection)
            ),
            Self::LoadGame => format!(
                "Load {}. {}",
                App::save_slot_label(app.load_slot_selection),
                app.selected_slot_summary_text()
            ),
            Self::Settings => {
                "Adjust simulation speed before launching the live TUI bridge.".to_string()
            }
            Self::HowToPlay => {
                "Read the current goals, contract flow, and frontier discovery rules.".to_string()
            }
            Self::Quit => "Leave Starlane Courier and restore the terminal.".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::*;

    #[derive(Clone)]
    struct MemorySaveStore {
        slots: Rc<RefCell<Vec<Option<SaveGame>>>>,
    }

    impl MemorySaveStore {
        fn new() -> Self {
            Self {
                slots: Rc::new(RefCell::new((0..SAVE_SLOT_COUNT).map(|_| None).collect())),
            }
        }
    }

    impl SaveStore for MemorySaveStore {
        fn read_slot(&self, slot_index: usize) -> Result<Option<SaveGame>, String> {
            Ok(self.slots.borrow().get(slot_index).cloned().flatten())
        }

        fn write_slot(&self, slot_index: usize, save: &SaveGame) -> Result<(), String> {
            if let Some(slot) = self.slots.borrow_mut().get_mut(slot_index) {
                *slot = Some(save.clone());
                Ok(())
            } else {
                Err("invalid slot".to_string())
            }
        }
    }

    #[test]
    fn save_round_trip_restores_session() {
        let store = MemorySaveStore::new();
        let mut app = App::with_store(Box::new(store.clone()), GameData::new(Difficulty::Normal));
        app.has_active_game = true;
        app.screen = Screen::InGame;
        app.tick_speed_index = 2;
        app.difficulty = Difficulty::Insane;
        app.difficulty_selection = app.difficulty.index();
        app.clock = 42;
        app.selected_location = crate::game::DUST_HARBOR;
        app.selected_ship = 0;
        app.selected_contract = 0;
        app.tracked_contract = Some(0);
        app.credits = 321;
        app.discovered_locations[crate::game::KITE_STATION] = true;
        app.contracts[0].state = crate::game::ContractState::Assigned {
            ship_name: "SV Kestrel",
            accepted_at: 12,
        };
        app.fleet[0].current_fuel = 4;
        app.fleet[0].max_fuel = 14;
        app.fleet[0].state = crate::game::ShipState::EnRoute {
            origin: crate::game::ASTRA_PRIME,
            destination: crate::game::DUST_HARBOR,
            eta_remaining: 3,
            total_eta: 5,
            route: "Astra Prime -> Dust Harbor".to_string(),
            condition_summary: "Dust Corridor: clear lanes".to_string(),
            assigned_contract: Some(0),
        };
        app.log = vec!["[0042] persistence check".to_string()];

        app.save_game().unwrap();

        let mut restored = App::with_store(Box::new(store), GameData::new(Difficulty::Normal));
        restored.load_game(0).unwrap();

        assert_eq!(restored.tick_speed_index, 2);
        assert_eq!(restored.difficulty, Difficulty::Insane);
        assert_eq!(restored.clock, 42);
        assert_eq!(restored.credits, 321);
        assert_eq!(restored.tracked_contract, Some(0));
        assert!(restored.discovered_locations[crate::game::KITE_STATION]);
        assert_eq!(restored.log[0], "[0042] persistence check");
        assert_eq!(restored.fleet[0].current_fuel, 4);
        assert_eq!(restored.fleet[0].max_fuel, 14);
        assert!(matches!(
            restored.contracts[0].state,
            crate::game::ContractState::Assigned {
                ship_name: "SV Kestrel",
                accepted_at: 12
            }
        ));
        assert!(matches!(
            restored.fleet[0].state,
            crate::game::ShipState::EnRoute {
                destination: crate::game::DUST_HARBOR,
                assigned_contract: Some(0),
                ..
            }
        ));
    }
}
