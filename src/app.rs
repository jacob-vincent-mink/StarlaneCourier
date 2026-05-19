use std::{
    io,
    ops::{Deref, DerefMut},
    time::Duration,
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::game::{ASTRA_PRIME, AppMode, Contract, Difficulty, GameData, PANE_COUNT, ShipState};
use crate::llm::{
    ContractFlavorGenerator, GeneratedContractFlavor, OpenAiCompatibleContractFlavorGenerator,
};
use crate::save::{
    FsSaveStore, SAVE_SLOT_COUNT, SAVE_VERSION, SaveGame, SaveStore, SavedAppMode, SavedContract,
    SavedContractState, SavedShip, SavedShipState,
};
use crate::settings::{
    AppSettings, FsSettingsStore, KeyringSecretStore, LlmSettings, SecretStore, SettingsStore,
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
    pub(crate) settings_edit: Option<SettingsEditState>,
    pub(crate) llm_field_selection: usize,
    pub(crate) tick_speed_index: usize,
    pub(crate) difficulty_selection: usize,
    save_store: Box<dyn SaveStore>,
    settings_store: Box<dyn SettingsStore>,
    secret_store: Box<dyn SecretStore>,
    flavor_generator: Box<dyn ContractFlavorGenerator>,
    pub(crate) llm_settings: LlmSettings,
    pub(crate) api_key_present: bool,
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
        let settings_store: Box<dyn SettingsStore> = Box::new(FsSettingsStore);
        let secret_store: Box<dyn SecretStore> = Box::new(KeyringSecretStore);
        let settings = settings_store.load().ok().flatten().unwrap_or_default();
        let difficulty = settings.difficulty;
        let game = GameData::new(difficulty);
        let api_key_present = secret_store.has_api_key().unwrap_or(false);
        let mut app = Self::with_dependencies(
            Box::new(FsSaveStore),
            settings_store,
            secret_store,
            Box::new(OpenAiCompatibleContractFlavorGenerator),
            game,
            settings,
            api_key_present,
        );
        app.hydrate_pending_contract_flavors();
        app
    }

    #[cfg(test)]
    pub(crate) fn with_store(save_store: Box<dyn SaveStore>, game: GameData) -> Self {
        let settings = AppSettings::default();
        Self::with_dependencies(
            save_store,
            Box::new(MemorySettingsStore::default()),
            Box::new(MemorySecretStore::default()),
            Box::new(NoopContractFlavorGenerator),
            game,
            settings,
            false,
        )
    }

    fn with_dependencies(
        save_store: Box<dyn SaveStore>,
        settings_store: Box<dyn SettingsStore>,
        secret_store: Box<dyn SecretStore>,
        flavor_generator: Box<dyn ContractFlavorGenerator>,
        game: GameData,
        settings: AppSettings,
        api_key_present: bool,
    ) -> Self {
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
            settings_edit: None,
            llm_field_selection: 0,
            tick_speed_index: settings.tick_speed_index.min(TICK_SPEEDS.len() - 1),
            difficulty_selection: difficulty.index(),
            save_store,
            settings_store,
            secret_store,
            flavor_generator,
            llm_settings: settings.llm,
            api_key_present,
            game,
        }
    }

    pub(crate) fn reset_game(&mut self) {
        self.game = GameData::new(self.difficulty);
        self.settings_selection = self.tick_speed_index;
        self.difficulty_selection = self.difficulty.index();
        self.llm_field_selection = 0;
        self.settings_edit = None;
    }

    pub(crate) fn mode_label(&self) -> String {
        self.game.mode_label()
    }

    pub(crate) fn controls_text(&self) -> String {
        match self.mode {
            AppMode::Browse => {
                "Tab/Shift+Tab or Left/Right: focus   Up/Down: select   Enter: board/fleet/alerts action   f: refuel   t: transfer fuel   u: upgrade ship   Esc: menu   q/Ctrl+C: quit"
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

    fn snapshot_settings(&self) -> AppSettings {
        AppSettings {
            tick_speed_index: self.tick_speed_index,
            difficulty: self.difficulty,
            llm: self.llm_settings.clone(),
        }
    }

    fn save_settings(&self) -> Result<(), String> {
        self.settings_store.save(&self.snapshot_settings())
    }

    pub(crate) fn llm_ready(&self) -> bool {
        self.llm_settings.enabled
            && self.api_key_present
            && !self.llm_settings.endpoint_url.trim().is_empty()
            && !self.llm_settings.model.trim().is_empty()
    }

    pub(crate) fn llm_summary(&self) -> String {
        if !self.llm_settings.enabled {
            return "LLM contracts: disabled".to_string();
        }

        if self.llm_ready() {
            format!(
                "LLM contracts: ready via {} / {}",
                self.llm_settings.endpoint_url, self.llm_settings.model
            )
        } else {
            "LLM contracts: incomplete configuration".to_string()
        }
    }

    pub(crate) fn selected_llm_field(&self) -> LlmField {
        LLM_FIELDS[self.llm_field_selection.min(LLM_FIELDS.len() - 1)]
    }

    pub(crate) fn llm_field_label(&self, field: LlmField) -> &'static str {
        match field {
            LlmField::Enabled => "LLM Contracts",
            LlmField::EndpointUrl => "Endpoint URL",
            LlmField::Model => "Model",
            LlmField::ApiKey => "API Key",
            LlmField::TimeoutSecs => "Timeout Seconds",
        }
    }

    pub(crate) fn llm_field_value(&self, field: LlmField) -> String {
        match field {
            LlmField::Enabled => if self.llm_settings.enabled {
                "Enabled"
            } else {
                "Disabled"
            }
            .to_string(),
            LlmField::EndpointUrl => self.llm_settings.endpoint_url.clone(),
            LlmField::Model => self.llm_settings.model.clone(),
            LlmField::ApiKey => if self.api_key_present {
                "Stored securely"
            } else {
                "Not set"
            }
            .to_string(),
            LlmField::TimeoutSecs => self.llm_settings.timeout_secs.to_string(),
        }
    }

    pub(crate) fn llm_field_description(&self, field: LlmField) -> String {
        match field {
            LlmField::Enabled => {
                "Toggle OpenAI-compatible contract flavor generation on refreshed contracts.".to_string()
            }
            LlmField::EndpointUrl => {
                "Full chat-completions endpoint URL. Example: https://api.openai.com/v1/chat/completions"
                    .to_string()
            }
            LlmField::Model => "Model name sent to the endpoint, such as gpt-4.1-mini.".to_string(),
            LlmField::ApiKey => {
                "Stored outside settings.json via the system keyring. Enter an empty value to clear it."
                    .to_string()
            }
            LlmField::TimeoutSecs => {
                "Blocking request timeout in seconds for contract flavor generation.".to_string()
            }
        }
    }

    fn persist_settings_and_llm(&mut self) {
        self.menu_feedback = self.save_settings().err();
        if self.llm_ready() {
            self.hydrate_pending_contract_flavors();
        }
        let _ = self.save_game();
    }

    fn begin_llm_edit(&mut self) {
        let field = self.selected_llm_field();
        match field {
            LlmField::Enabled => {
                self.llm_settings.enabled = !self.llm_settings.enabled;
                self.persist_settings_and_llm();
            }
            LlmField::ApiKey => {
                self.settings_edit = Some(SettingsEditState {
                    field,
                    buffer: String::new(),
                    secret: true,
                });
            }
            LlmField::EndpointUrl | LlmField::Model | LlmField::TimeoutSecs => {
                self.settings_edit = Some(SettingsEditState {
                    field,
                    buffer: self.llm_field_value(field),
                    secret: false,
                });
            }
        }
    }

    fn apply_settings_edit(&mut self) {
        let Some(edit) = self.settings_edit.take() else {
            return;
        };

        match edit.field {
            LlmField::EndpointUrl => {
                self.llm_settings.endpoint_url = edit.buffer.trim().to_string();
            }
            LlmField::Model => {
                self.llm_settings.model = edit.buffer.trim().to_string();
            }
            LlmField::TimeoutSecs => {
                if let Ok(timeout) = edit.buffer.trim().parse::<u64>() {
                    self.llm_settings.timeout_secs = timeout.max(5);
                } else {
                    self.menu_feedback =
                        Some("Invalid timeout; keep a whole number of seconds.".to_string());
                    return;
                }
            }
            LlmField::ApiKey => {
                if edit.buffer.trim().is_empty() {
                    if let Err(error) = self.secret_store.clear_api_key() {
                        self.menu_feedback = Some(format!("Failed to clear API key: {error}"));
                        return;
                    }
                    self.api_key_present = false;
                } else if let Err(error) = self.secret_store.set_api_key(edit.buffer.trim()) {
                    self.menu_feedback = Some(format!("Failed to store API key: {error}"));
                    return;
                } else {
                    self.api_key_present = true;
                }
            }
            LlmField::Enabled => {}
        }

        self.persist_settings_and_llm();
    }

    fn hydrate_pending_contract_flavors(&mut self) {
        if !self.llm_ready() {
            return;
        }

        let pending: Vec<usize> = self
            .contracts
            .iter()
            .enumerate()
            .filter_map(|(index, contract)| contract.pending_llm_flavor.then_some(index))
            .collect();

        for index in pending {
            let result = {
                let contract = &self.contracts[index];
                self.flavor_generator.generate_flavor(
                    &self.llm_settings,
                    &*self.secret_store,
                    contract,
                    self.difficulty,
                    &self.locations,
                )
            };

            match result {
                Ok(GeneratedContractFlavor { title, briefing }) => {
                    self.contracts[index].title = title;
                    self.contracts[index].briefing = briefing;
                    self.contracts[index].pending_llm_flavor = false;
                }
                Err(error) => {
                    self.contracts[index].pending_llm_flavor = false;
                    self.menu_feedback = Some(format!("LLM flavor failed: {error}"));
                    break;
                }
            }
        }
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
                    hull: ship.hull,
                    state: match &ship.state {
                        ShipState::Docked => SavedShipState::Docked,
                        ShipState::Repairing { ticks_remaining } => SavedShipState::Repairing {
                            ticks_remaining: *ticks_remaining,
                        },
                        ShipState::EnRoute {
                            origin,
                            destination,
                            eta_remaining,
                            total_eta,
                            route,
                            condition_summary,
                            assigned_contract,
                            repair_on_arrival,
                        } => SavedShipState::EnRoute {
                            origin: *origin,
                            destination: *destination,
                            eta_remaining: *eta_remaining,
                            total_eta: *total_eta,
                            route: route.clone(),
                            condition_summary: condition_summary.clone(),
                            assigned_contract: *assigned_contract,
                            repair_on_arrival: *repair_on_arrival,
                        },
                    },
                })
                .collect(),
            contracts: self
                .contracts
                .iter()
                .map(|contract| SavedContract {
                    archetype: contract.archetype,
                    title: contract.title.clone(),
                    briefing: contract.briefing.clone(),
                    origin: contract.origin,
                    destination: contract.destination,
                    reward: contract.reward,
                    max_eta: contract.max_eta,
                    deadline: contract.deadline,
                    unlock_location: contract.unlock_location,
                    pending_llm_flavor: contract.pending_llm_flavor,
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
        self.hydrate_pending_contract_flavors();
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
            ship.hull = saved_ship.hull.clamp(1, 100);
            ship.low_fuel_alerted = false;
            ship.state = match saved_ship.state {
                SavedShipState::Docked => ShipState::Docked,
                SavedShipState::Repairing { ticks_remaining } => {
                    ShipState::Repairing { ticks_remaining }
                }
                SavedShipState::EnRoute {
                    origin,
                    destination,
                    eta_remaining,
                    total_eta,
                    route,
                    condition_summary,
                    assigned_contract,
                    repair_on_arrival,
                } => ShipState::EnRoute {
                    origin: origin.min(locations_len - 1),
                    destination: destination.min(locations_len - 1),
                    eta_remaining,
                    total_eta: total_eta.max(eta_remaining),
                    route,
                    condition_summary,
                    assigned_contract: assigned_contract.filter(|&index| index < contracts_len),
                    repair_on_arrival,
                },
            };
        }

        let discovered = self.discovered_locations.clone();
        let fleet_names: Vec<&'static str> = self.fleet.iter().map(|ship| ship.name).collect();
        for (contract, saved_contract) in self.contracts.iter_mut().zip(save.contracts) {
            *contract = Contract::new(
                saved_contract.archetype,
                saved_contract.origin,
                saved_contract.destination,
                saved_contract.reward,
                saved_contract.max_eta,
                saved_contract.deadline,
                saved_contract.unlock_location,
            );
            if !saved_contract.title.is_empty() {
                contract.title = saved_contract.title;
            }
            if !saved_contract.briefing.is_empty() {
                contract.briefing = saved_contract.briefing;
            }
            contract.pending_llm_flavor = saved_contract.pending_llm_flavor;
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
        self.hydrate_pending_contract_flavors();
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
                self.hydrate_pending_contract_flavors();
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
                self.llm_field_selection = 0;
                self.settings_edit = None;
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
        if self.settings_edit.is_some() {
            return self.handle_settings_edit_key(key);
        }

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
                self.settings_focus = (self.settings_focus + 1).min(2);
                false
            }
            (KeyCode::Up, _) => {
                if self.settings_focus == 0 {
                    self.settings_selection =
                        crate::game::wrap_index(self.settings_selection, TICK_SPEEDS.len(), -1);
                } else if self.settings_focus == 1 {
                    self.difficulty_selection = crate::game::wrap_index(
                        self.difficulty_selection,
                        DIFFICULTY_OPTIONS.len(),
                        -1,
                    );
                } else {
                    self.llm_field_selection =
                        crate::game::wrap_index(self.llm_field_selection, LLM_FIELDS.len(), -1);
                }
                false
            }
            (KeyCode::Down, _) => {
                if self.settings_focus == 0 {
                    self.settings_selection =
                        crate::game::wrap_index(self.settings_selection, TICK_SPEEDS.len(), 1);
                } else if self.settings_focus == 1 {
                    self.difficulty_selection = crate::game::wrap_index(
                        self.difficulty_selection,
                        DIFFICULTY_OPTIONS.len(),
                        1,
                    );
                } else {
                    self.llm_field_selection =
                        crate::game::wrap_index(self.llm_field_selection, LLM_FIELDS.len(), 1);
                }
                false
            }
            (KeyCode::Enter, _) => {
                match self.settings_focus {
                    0 => {
                        self.tick_speed_index = self.settings_selection;
                        self.persist_settings_and_llm();
                    }
                    1 => {
                        self.difficulty = Difficulty::from_index(self.difficulty_selection);
                        self.persist_settings_and_llm();
                    }
                    _ => self.begin_llm_edit(),
                }
                false
            }
            (KeyCode::Delete, _)
                if self.settings_focus == 2
                    && matches!(self.selected_llm_field(), LlmField::ApiKey) =>
            {
                match self.secret_store.clear_api_key() {
                    Ok(()) => {
                        self.api_key_present = false;
                        self.persist_settings_and_llm();
                    }
                    Err(error) => {
                        self.menu_feedback = Some(format!("Failed to clear API key: {error}"));
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn handle_settings_edit_key(&mut self, key: KeyEvent) -> bool {
        let Some(edit) = self.settings_edit.as_mut() else {
            return false;
        };

        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.settings_edit = None;
                false
            }
            (KeyCode::Enter, _) => {
                self.apply_settings_edit();
                false
            }
            (KeyCode::Backspace, _) => {
                edit.buffer.pop();
                false
            }
            (KeyCode::Char(ch), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                edit.buffer.push(ch);
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
            (KeyCode::Char('u'), _) => {
                self.upgrade_selected_ship();
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

pub(crate) const LLM_FIELDS: [LlmField; 5] = [
    LlmField::Enabled,
    LlmField::EndpointUrl,
    LlmField::Model,
    LlmField::ApiKey,
    LlmField::TimeoutSecs,
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LlmField {
    Enabled,
    EndpointUrl,
    Model,
    ApiKey,
    TimeoutSecs,
}

pub(crate) struct SettingsEditState {
    pub(crate) field: LlmField,
    pub(crate) buffer: String,
    pub(crate) secret: bool,
}

#[cfg(test)]
#[derive(Default)]
struct MemorySettingsStore {
    settings: Option<AppSettings>,
}

#[cfg(test)]
impl SettingsStore for MemorySettingsStore {
    fn load(&self) -> Result<Option<AppSettings>, String> {
        Ok(self.settings.clone())
    }

    fn save(&self, _settings: &AppSettings) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
#[derive(Default)]
struct MemorySecretStore {
    api_key: Option<String>,
}

#[cfg(test)]
impl SecretStore for MemorySecretStore {
    fn get_api_key(&self) -> Result<Option<String>, String> {
        Ok(self.api_key.clone())
    }

    fn set_api_key(&self, _api_key: &str) -> Result<(), String> {
        Ok(())
    }

    fn clear_api_key(&self) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
struct NoopContractFlavorGenerator;

#[cfg(test)]
impl ContractFlavorGenerator for NoopContractFlavorGenerator {
    fn generate_flavor(
        &self,
        _settings: &LlmSettings,
        _secret_store: &dyn SecretStore,
        contract: &Contract,
        _difficulty: Difficulty,
        _locations: &[crate::game::Location],
    ) -> Result<GeneratedContractFlavor, String> {
        Ok(GeneratedContractFlavor {
            title: contract.title.clone(),
            briefing: contract.briefing.clone(),
        })
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

    #[derive(Clone, Default)]
    struct TestSettingsStore {
        settings: Rc<RefCell<Option<AppSettings>>>,
    }

    impl SettingsStore for TestSettingsStore {
        fn load(&self) -> Result<Option<AppSettings>, String> {
            Ok(self.settings.borrow().clone())
        }

        fn save(&self, settings: &AppSettings) -> Result<(), String> {
            *self.settings.borrow_mut() = Some(settings.clone());
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct TestSecretStore {
        api_key: Rc<RefCell<Option<String>>>,
    }

    impl SecretStore for TestSecretStore {
        fn get_api_key(&self) -> Result<Option<String>, String> {
            Ok(self.api_key.borrow().clone())
        }

        fn set_api_key(&self, api_key: &str) -> Result<(), String> {
            *self.api_key.borrow_mut() = Some(api_key.to_string());
            Ok(())
        }

        fn clear_api_key(&self) -> Result<(), String> {
            *self.api_key.borrow_mut() = None;
            Ok(())
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
            repair_on_arrival: 0,
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

    #[test]
    fn selecting_alert_focuses_related_ship() {
        let store = MemorySaveStore::new();
        let mut app = App::with_store(Box::new(store), GameData::new(Difficulty::Normal));
        app.active_pane = crate::game::LOG_PANE;
        app.fleet[0].current_fuel = 0;

        let alerts = app.current_alerts();
        let index = alerts
            .iter()
            .position(|alert| alert.summary.contains("SV Kestrel low fuel"))
            .unwrap();
        app.selected_alert = index;

        app.focus_selected_alert();

        assert_eq!(app.active_pane, crate::game::FLEET_PANE);
        assert_eq!(app.selected_ship, 0);
    }

    #[test]
    fn settings_and_api_key_persist_through_injected_stores() {
        let save_store = MemorySaveStore::new();
        let settings_store = TestSettingsStore::default();
        let secret_store = TestSecretStore::default();
        let mut app = App::with_dependencies(
            Box::new(save_store),
            Box::new(settings_store.clone()),
            Box::new(secret_store.clone()),
            Box::new(NoopContractFlavorGenerator),
            GameData::new(Difficulty::Normal),
            AppSettings::default(),
            false,
        );

        app.tick_speed_index = 2;
        app.difficulty = Difficulty::Insane;
        app.llm_settings.enabled = true;
        app.llm_settings.endpoint_url = "https://example.test/v1/chat/completions".to_string();
        app.llm_settings.model = "demo-model".to_string();
        app.secret_store.set_api_key("secret-token").unwrap();
        app.api_key_present = true;

        app.save_settings().unwrap();

        let persisted = settings_store.load().unwrap().unwrap();
        assert_eq!(persisted.tick_speed_index, 2);
        assert_eq!(persisted.difficulty, Difficulty::Insane);
        assert!(persisted.llm.enabled);
        assert_eq!(persisted.llm.model, "demo-model");
        assert_eq!(
            secret_store.get_api_key().unwrap().as_deref(),
            Some("secret-token")
        );
    }
}
