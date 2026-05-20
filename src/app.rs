use std::{
    collections::BTreeSet,
    io,
    ops::{Deref, DerefMut},
    panic::{self, AssertUnwindSafe},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::game::{
    ASTRA_PRIME, AppMode, Contract, ContractArchetype, Difficulty, GameData, Location, PANE_COUNT,
    Ship, ShipShop, ShipShopOffer, ShipState, WorldFlavor,
};
use crate::llm::{
    ContractFlavorGenerator, GeneratedContractFlavor, OpenAiCompatibleContractFlavorGenerator,
};
use crate::save::{
    FsSaveStore, SAVE_SLOT_COUNT, SAVE_VERSION, SaveGame, SaveStore, SavedAppMode, SavedContract,
    SavedContractState, SavedLocation, SavedShip, SavedShipOffer, SavedShipShop, SavedShipState,
};
use crate::settings::{
    AppSettings, FsSecretStore, FsSettingsStore, LLM_PROVIDER_PRESETS, LlmProviderPreset,
    LlmSettings, SecretStore, SettingsStore,
};

pub(crate) const TICK_SPEEDS: [(&str, u64); 3] = [("Slow", 450), ("Standard", 250), ("Fast", 125)];
pub(crate) const DIFFICULTY_OPTIONS: [Difficulty; 3] =
    [Difficulty::Cozy, Difficulty::Normal, Difficulty::Insane];

pub(crate) struct App {
    pub(crate) screen: Screen,
    settings_return_screen: Screen,
    pub(crate) has_active_game: bool,
    pub(crate) menu_feedback: Option<String>,
    pub(crate) popup_message: Option<String>,
    pub(crate) llm_gate_selection: usize,
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
    flavor_generator: Arc<dyn ContractFlavorGenerator>,
    llm_result_sender: Sender<ContractFlavorJobResult>,
    llm_result_receiver: Receiver<ContractFlavorJobResult>,
    world_result_sender: Sender<WorldInitializationJobResult>,
    world_result_receiver: Receiver<WorldInitializationJobResult>,
    in_flight_contract_flavors: BTreeSet<usize>,
    pending_world_initialization: Option<WorldInitializationState>,
    pub(crate) llm_settings: LlmSettings,
    pub(crate) api_key_present: bool,
    session_api_key: Option<String>,
    pub(crate) last_llm_status: String,
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
        let secret_store: Box<dyn SecretStore> = Box::new(FsSecretStore);
        let settings = settings_store.load().ok().flatten().unwrap_or_default();
        let difficulty = settings.difficulty;
        let game = GameData::new(difficulty);
        let api_key_present = secret_store.has_api_key().unwrap_or(false);
        let mut app = Self::with_dependencies(
            Box::new(FsSaveStore),
            settings_store,
            secret_store,
            Arc::new(OpenAiCompatibleContractFlavorGenerator),
            game,
            settings,
            api_key_present,
        );
        app.evaluate_startup_llm_gate();
        app
    }

    #[cfg(test)]
    pub(crate) fn with_store(save_store: Box<dyn SaveStore>, game: GameData) -> Self {
        let settings = AppSettings::default();
        Self::with_dependencies(
            save_store,
            Box::new(MemorySettingsStore::default()),
            Box::new(MemorySecretStore::default()),
            Arc::new(NoopContractFlavorGenerator),
            game,
            settings,
            false,
        )
    }

    fn with_dependencies(
        save_store: Box<dyn SaveStore>,
        settings_store: Box<dyn SettingsStore>,
        secret_store: Box<dyn SecretStore>,
        flavor_generator: Arc<dyn ContractFlavorGenerator>,
        game: GameData,
        settings: AppSettings,
        api_key_present: bool,
    ) -> Self {
        let difficulty = game.difficulty;
        let (llm_result_sender, llm_result_receiver) = mpsc::channel();
        let (world_result_sender, world_result_receiver) = mpsc::channel();
        Self {
            screen: Screen::StartMenu,
            settings_return_screen: Screen::StartMenu,
            has_active_game: false,
            menu_feedback: None,
            popup_message: None,
            llm_gate_selection: 0,
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
            llm_result_sender,
            llm_result_receiver,
            world_result_sender,
            world_result_receiver,
            in_flight_contract_flavors: BTreeSet::new(),
            pending_world_initialization: None,
            llm_settings: settings.llm,
            api_key_present,
            session_api_key: None,
            last_llm_status: "LLM not tested yet.".to_string(),
            game,
        }
    }

    fn reset_game_seeded(&mut self, world_seed: u64, world_flavor: Option<WorldFlavor>) {
        self.game = GameData::new_seeded(self.difficulty, world_seed, world_flavor);
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
                "Tab/Shift+Tab or Left/Right: focus   Up/Down: select   Enter: board/fleet/shipyard/alerts action   e: exploration run   m: move player   z/x: map zoom   g: map auto-focus   b: buy selected shipyard hull   f: refuel   t: transfer fuel   u: upgrade ship   r: regenerate flavor   s: settings   Esc: menu   q/Ctrl+C: quit"
                    .to_string()
            }
            AppMode::SelectingDestination { intent, .. } => {
                format!(
                    "Up/Down: choose destination   z/x: map zoom   g: focus ship   Enter: confirm {}   Esc: cancel   q/Ctrl+C: quit",
                    intent.label().to_lowercase()
                )
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
            && (!self.llm_settings.provider.requires_api_key() || self.effective_api_key_present())
            && !self.llm_settings.endpoint_url.trim().is_empty()
            && !self.llm_settings.model.trim().is_empty()
    }

    fn effective_api_key_present(&self) -> bool {
        self.session_api_key.is_some() || self.api_key_present
    }

    fn resolved_api_key(&self) -> Result<Option<String>, String> {
        if !self.llm_settings.provider.requires_api_key() {
            return Ok(None);
        }

        if let Some(api_key) = &self.session_api_key {
            Ok(Some(api_key.clone()))
        } else if self.api_key_present {
            self.secret_store.get_api_key()
        } else {
            Ok(None)
        }
    }

    pub(crate) fn pending_contract_flavor_count(&self) -> usize {
        self.contracts
            .iter()
            .filter(|contract| contract.pending_llm_flavor)
            .count()
    }

    pub(crate) fn llm_summary(&self) -> String {
        if !self.llm_settings.enabled {
            return "LLM contracts: disabled".to_string();
        }

        if self.llm_ready() {
            format!(
                "LLM contracts: {} via {} / {}",
                self.llm_settings.provider.label(),
                self.llm_settings.endpoint_url,
                self.llm_settings.model
            )
        } else {
            "LLM contracts: incomplete configuration".to_string()
        }
    }

    pub(crate) fn pending_world_seed(&self) -> Option<u64> {
        self.pending_world_initialization
            .as_ref()
            .map(|state| state.seed)
    }

    fn sync_action_feedback_popup(&mut self) {
        if let Some(message) = self.game.take_action_feedback() {
            self.popup_message = Some(message);
        }
    }

    pub(crate) fn selected_llm_field(&self) -> LlmField {
        LLM_FIELDS[self.llm_field_selection.min(LLM_FIELDS.len() - 1)]
    }

    pub(crate) fn llm_field_label(&self, field: LlmField) -> &'static str {
        match field {
            LlmField::Enabled => "LLM Contracts",
            LlmField::Provider => "Provider Preset",
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
            LlmField::Provider => self.llm_settings.provider.label().to_string(),
            LlmField::EndpointUrl => self.llm_settings.endpoint_url.clone(),
            LlmField::Model => self.llm_settings.model.clone(),
            LlmField::ApiKey => if self.effective_api_key_present() {
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
                "Toggle background OpenAI-compatible contract flavor generation on refreshed contracts."
                    .to_string()
            }
            LlmField::Provider => {
                "Choose a provider preset with Enter. Presets prefill endpoint and model for common OpenAI-compatible providers.".to_string()
            }
            LlmField::EndpointUrl => {
                "Base OpenAI-compatible endpoint or full chat-completions URL. Example: http://localhost:8049/v1"
                    .to_string()
            }
            LlmField::Model => "Model name sent to the endpoint, such as gpt-4.1-mini.".to_string(),
            LlmField::ApiKey => {
                "Stored separately from settings.json under ~/.config/spacecourier/api-key with sticky persistence across builds. Enter an empty value to clear it."
                    .to_string()
            }
            LlmField::TimeoutSecs => {
                "Background request timeout in seconds for contract flavor generation."
                    .to_string()
            }
        }
    }

    fn persist_settings_and_llm(&mut self) {
        self.menu_feedback = self.save_settings().err();
        let _ = self.save_game();
    }

    fn cycle_llm_provider(&mut self) {
        let next = crate::game::wrap_index(
            self.llm_settings.provider.index(),
            LLM_PROVIDER_PRESETS.len(),
            1,
        );
        let preset = LlmProviderPreset::from_index(next);
        self.llm_settings.provider = preset;
        if !matches!(preset, LlmProviderPreset::Custom) {
            self.llm_settings.endpoint_url = preset.default_endpoint().to_string();
            self.llm_settings.model = preset.default_model().to_string();
        }
        self.persist_settings_and_llm();
    }

    fn begin_llm_edit(&mut self) {
        let field = self.selected_llm_field();
        match field {
            LlmField::Enabled => {
                self.llm_settings.enabled = !self.llm_settings.enabled;
                self.persist_settings_and_llm();
            }
            LlmField::Provider => {
                self.cycle_llm_provider();
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
                    self.session_api_key = None;
                    self.api_key_present = false;
                    self.menu_feedback = Some("API key cleared from secure storage.".to_string());
                } else if let Err(error) = self.secret_store.set_api_key(edit.buffer.trim()) {
                    self.menu_feedback = Some(format!("Failed to store API key: {error}"));
                    return;
                } else {
                    let key = edit.buffer.trim().to_string();
                    self.session_api_key = Some(key);
                    self.api_key_present = true;
                    let stored = self.secret_store.get_api_key().ok().flatten().is_some();
                    self.menu_feedback = Some(if stored {
                        "API key stored securely.".to_string()
                    } else {
                        "API key accepted for this session, but secure-store read-back failed."
                            .to_string()
                    });
                }
            }
            LlmField::Enabled | LlmField::Provider => {}
        }

        self.persist_settings_and_llm();
    }

    fn queue_pending_contract_flavors(&mut self) {
        if !self.llm_ready() || self.screen == Screen::LlmGate {
            return;
        }

        let pending: Vec<usize> = self
            .contracts
            .iter()
            .enumerate()
            .filter_map(|(index, contract)| {
                (contract.pending_llm_flavor && !self.in_flight_contract_flavors.contains(&index))
                    .then_some(index)
            })
            .collect();

        if pending.is_empty() {
            return;
        }

        let api_key = match self.resolved_api_key() {
            Ok(api_key) => api_key,
            Err(error) => {
                self.fail_pending_contract_flavors(&pending, &error);
                return;
            }
        };

        for index in pending {
            self.spawn_contract_flavor_job(index, api_key.clone(), ContractFlavorTrigger::Hydrate);
        }
    }

    fn fail_pending_contract_flavors(&mut self, pending: &[usize], error: &str) {
        for &index in pending {
            self.contracts[index].pending_llm_flavor = false;
        }

        let message = format!("LLM flavor unavailable: {error}");
        self.last_llm_status = message.clone();
        self.menu_feedback = Some(message);
    }

    fn spawn_contract_flavor_job(
        &mut self,
        index: usize,
        api_key: Option<String>,
        trigger: ContractFlavorTrigger,
    ) {
        if self.in_flight_contract_flavors.contains(&index) {
            return;
        }

        let settings = self.llm_settings.clone();
        let contract = self.contracts[index].clone();
        let difficulty = self.difficulty;
        let locations = self.locations.clone();
        let signature = ContractFlavorSignature::from_contract(&contract);
        let provider_label = settings.provider.label().to_string();
        let model = settings.model.clone();
        let flavor_generator = Arc::clone(&self.flavor_generator);
        let sender = self.llm_result_sender.clone();

        self.in_flight_contract_flavors.insert(index);
        std::thread::spawn(move || {
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                flavor_generator.generate_flavor(
                    &settings,
                    api_key.as_deref(),
                    &contract,
                    difficulty,
                    &locations,
                )
            }))
            .unwrap_or_else(|_| Err("LLM flavor generation panicked".to_string()));

            let _ = sender.send(ContractFlavorJobResult {
                contract_index: index,
                signature,
                trigger,
                provider_label,
                model,
                result,
            });
        });
    }

    fn generate_world_seed(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| {
                duration.as_secs()
                    ^ ((self.load_slot_selection as u64 + 1) << 12)
                    ^ ((self.difficulty.index() as u64 + 1) << 20)
            })
            .unwrap_or_else(|_| {
                ((self.load_slot_selection as u64 + 1) << 12)
                    ^ ((self.difficulty.index() as u64 + 1) << 20)
            })
    }

    fn begin_new_game(&mut self) {
        self.active_save_slot = self.load_slot_selection;
        let world_seed = self.generate_world_seed();
        self.pending_world_initialization = None;

        if self.llm_ready() {
            match self.resolved_api_key() {
                Ok(api_key) => {
                    self.reset_game_seeded(world_seed, None);
                    self.pending_world_initialization = Some(WorldInitializationState {
                        seed: world_seed,
                        slot_index: self.active_save_slot,
                    });
                    self.has_active_game = false;
                    self.menu_feedback = None;
                    self.last_llm_status = format!(
                        "Initializing sector from seed {} via {} / {}",
                        world_seed,
                        self.llm_settings.provider.label(),
                        self.llm_settings.model
                    );
                    self.screen = Screen::InitializingWorld;
                    self.spawn_world_initialization_job(world_seed, api_key);
                    return;
                }
                Err(error) => {
                    self.menu_feedback = Some(format!("LLM sector bootstrap unavailable: {error}"));
                }
            }
        }

        self.complete_new_game(world_seed, None);
    }

    fn complete_new_game(&mut self, world_seed: u64, world_flavor: Option<WorldFlavor>) {
        self.reset_game_seeded(world_seed, world_flavor);
        self.pending_world_initialization = None;
        self.has_active_game = true;
        self.queue_pending_contract_flavors();
        self.screen = Screen::InGame;
        self.sync_end_screen();
        if let Err(error) = self.save_game() {
            self.menu_feedback = Some(format!("Save failed: {error}"));
        }
    }

    fn spawn_world_initialization_job(&mut self, world_seed: u64, api_key: Option<String>) {
        let settings = self.llm_settings.clone();
        let provider_label = settings.provider.label().to_string();
        let model = settings.model.clone();
        let flavor_generator = Arc::clone(&self.flavor_generator);
        let sender = self.world_result_sender.clone();

        std::thread::spawn(move || {
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                flavor_generator.generate_sector(&settings, api_key.as_deref(), world_seed)
            }))
            .unwrap_or_else(|_| Err("LLM sector bootstrap panicked".to_string()));
            let _ = sender.send(WorldInitializationJobResult {
                seed: world_seed,
                provider_label,
                model,
                result,
            });
        });
    }

    fn cancel_world_initialization(&mut self) {
        self.pending_world_initialization = None;
        self.has_active_game = false;
        self.screen = Screen::StartMenu;
        self.menu_feedback = Some("Environment initialization canceled.".to_string());
        self.last_llm_status = "LLM sector bootstrap canceled.".to_string();
    }

    pub(crate) fn sync_background_work(&mut self) {
        let mut save_needed = false;

        while let Ok(job) = self.world_result_receiver.try_recv() {
            let Some(state) = self.pending_world_initialization.take() else {
                continue;
            };
            if state.seed != job.seed {
                self.pending_world_initialization = Some(state);
                continue;
            }

            self.active_save_slot = state.slot_index;
            self.load_slot_selection = state.slot_index;

            match job.result {
                Ok(world_flavor) => {
                    let sector_name = world_flavor.environment_name.clone();
                    self.last_llm_status =
                        format!("LLM sector OK: {} via {}", job.provider_label, job.model);
                    self.menu_feedback = Some(format!("Initialized environment: {}.", sector_name));
                    if panic::catch_unwind(AssertUnwindSafe(|| {
                        self.complete_new_game(job.seed, Some(world_flavor));
                    }))
                    .is_err()
                    {
                        self.last_llm_status =
                            "LLM sector apply failed; using fallback environment.".to_string();
                        self.menu_feedback = Some(
                            "Generated sector application failed. Continuing with seeded fallback."
                                .to_string(),
                        );
                        self.complete_new_game(job.seed, None);
                    }
                }
                Err(error) => {
                    self.last_llm_status = format!("LLM sector bootstrap failed: {error}");
                    self.menu_feedback = Some(format!(
                        "LLM sector bootstrap failed: {error}. Continuing with seeded fallback."
                    ));
                    self.complete_new_game(job.seed, None);
                }
            }
        }

        while let Ok(job) = self.llm_result_receiver.try_recv() {
            self.in_flight_contract_flavors.remove(&job.contract_index);

            let Some(contract) = self.contracts.get(job.contract_index) else {
                continue;
            };
            if ContractFlavorSignature::from_contract(contract) != job.signature {
                continue;
            }

            match job.result {
                Ok(GeneratedContractFlavor { title, briefing }) => {
                    self.contracts[job.contract_index].title = title;
                    self.contracts[job.contract_index].briefing = briefing;
                    self.contracts[job.contract_index].pending_llm_flavor = false;
                    self.last_llm_status =
                        format!("LLM flavor OK: {} via {}", job.provider_label, job.model);
                    if matches!(job.trigger, ContractFlavorTrigger::Regenerate) {
                        let contract_title = self.contracts[job.contract_index].title.clone();
                        let clock = self.clock;
                        self.push_log(format!(
                            "[{clock:04}] Regenerated contract flavor for {}.",
                            contract_title,
                            clock = clock,
                        ));
                        self.menu_feedback = Some("LLM flavor regeneration complete.".to_string());
                    }
                    save_needed = true;
                }
                Err(error) => {
                    self.contracts[job.contract_index].pending_llm_flavor = false;
                    let message = match job.trigger {
                        ContractFlavorTrigger::Hydrate => format!("LLM flavor failed: {error}"),
                        ContractFlavorTrigger::Regenerate => {
                            format!("LLM regenerate failed: {error}")
                        }
                    };
                    self.last_llm_status = message.clone();
                    self.menu_feedback = Some(message);
                    save_needed = true;
                }
            }
        }

        if save_needed {
            let _ = self.save_game();
        }

        if self.has_active_game {
            self.queue_pending_contract_flavors();
        }
    }

    fn test_llm_connection(&self) -> Result<(), String> {
        if !self.llm_settings.enabled {
            return Err("LLM mode is disabled.".to_string());
        }

        let api_key_present = self.effective_api_key_present();
        if (!api_key_present && self.llm_settings.provider.requires_api_key())
            || self.llm_settings.endpoint_url.trim().is_empty()
            || self.llm_settings.model.trim().is_empty()
        {
            return Err("LLM settings are incomplete.".to_string());
        }

        let api_key = self.resolved_api_key()?;
        self.flavor_generator
            .test_connection(&self.llm_settings, api_key.as_deref())
    }

    fn evaluate_startup_llm_gate(&mut self) {
        if !self.llm_settings.enabled {
            return;
        }

        if let Err(error) = self.test_llm_connection() {
            self.last_llm_status = format!("LLM connection failed: {error}");
            self.menu_feedback = Some(format!("LLM connection failed: {error}"));
            self.llm_gate_selection = 0;
            self.screen = Screen::LlmGate;
        } else {
            self.last_llm_status = format!(
                "LLM connection OK: {} via {}",
                self.llm_settings.provider.label(),
                self.llm_settings.model
            );
        }
    }

    fn disable_llm_mode_and_continue(&mut self) {
        self.llm_settings.enabled = false;
        if let Err(error) = self.save_settings() {
            self.menu_feedback = Some(format!("Failed to save settings: {error}"));
        } else {
            self.menu_feedback =
                Some("LLM mode disabled. Continuing with the deterministic storyline.".to_string());
            self.last_llm_status = "LLM disabled; deterministic storyline active.".to_string();
        }
        self.screen = Screen::StartMenu;
    }

    fn regenerate_selected_contract_flavor(&mut self) {
        let index = self.selected_contract;

        if !self.llm_ready() {
            self.menu_feedback = Some("LLM contract flavor is not fully configured.".to_string());
            return;
        }

        if self.in_flight_contract_flavors.contains(&index) {
            self.menu_feedback =
                Some("LLM flavor is already generating in background.".to_string());
            return;
        }

        let api_key = match self.resolved_api_key() {
            Ok(api_key) => api_key,
            Err(error) => {
                self.last_llm_status = format!("LLM regenerate failed: {error}");
                self.menu_feedback = Some(format!("LLM regenerate failed: {error}"));
                return;
            }
        };

        self.contracts[index].pending_llm_flavor = true;
        self.last_llm_status = format!(
            "LLM flavor queued: {} via {}",
            self.llm_settings.provider.label(),
            self.llm_settings.model
        );
        self.menu_feedback = Some("LLM flavor regeneration queued.".to_string());
        self.spawn_contract_flavor_job(index, api_key, ContractFlavorTrigger::Regenerate);
        let _ = self.save_game();
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
                AppMode::SelectingDestination { ship_index, intent } => {
                    SavedAppMode::SelectingDestination {
                        ship_index,
                        exploration: matches!(intent, crate::game::DispatchIntent::Exploration),
                    }
                }
            },
            player_location: self.player_location,
            player_in_transit_ship: self.player_in_transit_ship,
            selected_location: self.selected_location,
            selected_ship: self.selected_ship,
            selected_contract: self.selected_contract,
            tracked_contract: self.tracked_contract,
            credits: self.credits,
            difficulty: self.difficulty,
            run_outcome: self.run_outcome.clone(),
            world_seed: self.world_seed,
            sector_name: self.sector_name.clone(),
            sector_summary: self.sector_summary.clone(),
            locations: self
                .locations
                .iter()
                .map(|location| SavedLocation {
                    region_name: location.region_name.clone(),
                    sector_name: location.sector_name.clone(),
                    name: location.name.clone(),
                    short_label: location.short_label.clone(),
                    lane_name: location.lane_name.clone(),
                    description: location.description.clone(),
                    cluster_name: location.cluster_name.clone(),
                    system_name: location.system_name.clone(),
                    region_coords: location.region_coords.into(),
                    sector_coords: location.sector_coords.into(),
                    cluster_coords: location.cluster_coords.into(),
                    system_coords: location.system_coords.into(),
                    travel_time_from_hub: location.travel_time_from_hub,
                    reveal_on_arrival: location.reveal_on_arrival,
                })
                .collect(),
            discovered_locations: self.discovered_locations.clone(),
            station_fuel: self.station_fuel.clone(),
            station_ship_shops: self
                .station_ship_shops
                .iter()
                .map(|shop| {
                    shop.as_ref().map(|shop| SavedShipShop {
                        offers: shop
                            .offers
                            .iter()
                            .map(|offer| SavedShipOffer {
                                name: offer.name.clone(),
                                class_name: offer.class_name.clone(),
                                description: offer.description.clone(),
                                speed: offer.speed,
                                max_fuel: offer.max_fuel,
                                price: offer.price,
                            })
                            .collect(),
                        legacy_offer: None,
                        last_refresh: shop.last_refresh,
                    })
                })
                .collect(),
            fleet: self
                .fleet
                .iter()
                .map(|ship| SavedShip {
                    name: ship.name.clone(),
                    class_name: ship.class_name.clone(),
                    description: ship.description.clone(),
                    current_location: ship.current_location,
                    current_fuel: ship.current_fuel,
                    max_fuel: ship.max_fuel,
                    speed: ship.speed,
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
                            exploration_run,
                            segments,
                            segment_costs,
                            route,
                            condition_summary,
                            assigned_contract,
                            repair_on_arrival,
                        } => SavedShipState::EnRoute {
                            origin: *origin,
                            destination: *destination,
                            eta_remaining: *eta_remaining,
                            total_eta: *total_eta,
                            exploration_run: *exploration_run,
                            segments: segments.clone(),
                            segment_costs: segment_costs.clone(),
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
                            ship_index,
                            accepted_at,
                        } => SavedContractState::Assigned {
                            ship_index,
                            accepted_at,
                        },
                        crate::game::ContractState::Completed => SavedContractState::Completed,
                        crate::game::ContractState::Failed => SavedContractState::Failed,
                    },
                })
                .collect(),
            log: self.log.clone(),
            mission_history: self.mission_history.clone(),
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
        self.queue_pending_contract_flavors();
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
        let saved_world_flavor = if !save.locations.is_empty()
            && save.locations.len() % crate::game::SECTOR_LOCATION_COUNT == 0
        {
            Some(WorldFlavor {
                environment_name: save.sector_name.clone(),
                environment_summary: save.sector_summary.clone(),
                locations: save
                    .locations
                    .iter()
                    .map(|location| crate::game::WorldLocationFlavor {
                        region_name: if location.region_name.is_empty() {
                            "Recovered Region".to_string()
                        } else {
                            location.region_name.clone()
                        },
                        sector_name: if location.sector_name.is_empty() {
                            "Recovered Sector".to_string()
                        } else {
                            location.sector_name.clone()
                        },
                        name: location.name.clone(),
                        short_label: location.short_label.clone(),
                        lane_name: location.lane_name.clone(),
                        description: location.description.clone(),
                        cluster_name: location.cluster_name.clone(),
                        system_name: location.system_name.clone(),
                    })
                    .collect(),
                starter_ships: Vec::new(),
                shipyard_offers: Vec::new(),
            })
        } else {
            None
        };

        self.game = GameData::new_seeded(save.difficulty, save.world_seed, saved_world_flavor);
        self.settings_selection = self.tick_speed_index;
        self.difficulty_selection = self.difficulty.index();
        self.llm_field_selection = 0;
        self.settings_edit = None;

        if save.discovered_locations.len() > self.discovered_locations.len() {
            return Err("save file has incompatible discovery data".to_string());
        }
        if save.contracts.len() != self.contracts.len() {
            return Err("save file has incompatible contract data".to_string());
        }
        if save.fleet.is_empty() {
            return Err("save file has no fleet data".to_string());
        }

        let SaveGame {
            tick_speed_index,
            active_pane,
            clock,
            mode,
            player_location,
            player_in_transit_ship,
            selected_location,
            selected_ship,
            selected_contract,
            tracked_contract,
            credits,
            difficulty,
            run_outcome,
            world_seed,
            sector_name,
            sector_summary,
            locations: saved_locations,
            discovered_locations: saved_discovered_locations,
            station_fuel: saved_station_fuel,
            station_ship_shops: saved_ship_shops,
            fleet: saved_fleet,
            contracts: saved_contracts,
            log,
            mission_history,
            ..
        } = save;

        self.tick_speed_index = tick_speed_index.min(TICK_SPEEDS.len() - 1);
        self.active_pane = active_pane.min(PANE_COUNT - 1);
        self.clock = clock;
        self.player_location = player_location.min(self.locations.len() - 1);
        self.player_in_transit_ship = player_in_transit_ship;
        self.selected_location = selected_location.min(self.locations.len() - 1);
        self.selected_contract = selected_contract.min(self.contracts.len() - 1);
        self.tracked_contract = tracked_contract.filter(|&index| index < self.contracts.len());
        self.credits = credits;
        self.difficulty = difficulty;
        self.difficulty_selection = self.difficulty.index();
        self.run_outcome = run_outcome;
        self.world_seed = world_seed;
        if !sector_name.trim().is_empty() {
            self.sector_name = sector_name;
        }
        if !sector_summary.trim().is_empty() {
            self.sector_summary = sector_summary;
        }

        if !saved_locations.is_empty() {
            for (index, location) in saved_locations.into_iter().enumerate() {
                if index >= self.locations.len() {
                    break;
                }
                self.locations[index] = Location {
                    region_name: if location.region_name.is_empty() {
                        self.locations[index].region_name.clone()
                    } else {
                        location.region_name
                    },
                    sector_name: if location.sector_name.is_empty() {
                        self.locations[index].sector_name.clone()
                    } else {
                        location.sector_name
                    },
                    name: location.name,
                    short_label: location.short_label,
                    lane_name: location.lane_name,
                    description: location.description,
                    cluster_name: location.cluster_name,
                    system_name: location.system_name,
                    region_coords: location.region_coords.into(),
                    sector_coords: location.sector_coords.into(),
                    cluster_coords: location.cluster_coords.into(),
                    system_coords: location.system_coords.into(),
                    travel_time_from_hub: location.travel_time_from_hub,
                    reveal_on_arrival: location.reveal_on_arrival,
                };
            }
        }

        for (index, seen) in saved_discovered_locations.into_iter().enumerate() {
            self.discovered_locations[index] = seen;
        }
        self.discovered_locations[ASTRA_PRIME] = true;

        for (index, fuel) in saved_station_fuel.into_iter().enumerate() {
            if index >= self.station_fuel.len() {
                break;
            }
            self.station_fuel[index] = fuel;
        }

        for (index, shop) in saved_ship_shops.into_iter().enumerate() {
            if index >= self.station_ship_shops.len() {
                break;
            }
            self.station_ship_shops[index] = shop.map(|shop| ShipShop {
                offers: if shop.offers.is_empty() {
                    shop.legacy_offer
                        .into_iter()
                        .map(|offer| ShipShopOffer {
                            name: offer.name,
                            class_name: offer.class_name,
                            description: offer.description,
                            speed: offer.speed,
                            max_fuel: offer.max_fuel,
                            price: offer.price,
                        })
                        .collect()
                } else {
                    shop.offers
                        .into_iter()
                        .map(|offer| ShipShopOffer {
                            name: offer.name,
                            class_name: offer.class_name,
                            description: offer.description,
                            speed: offer.speed,
                            max_fuel: offer.max_fuel,
                            price: offer.price,
                        })
                        .collect()
                },
                last_refresh: shop.last_refresh,
            });
        }

        let locations_len = self.locations.len();
        let contracts_len = self.contracts.len();
        self.fleet = saved_fleet
            .into_iter()
            .map(|saved_ship| Ship {
                name: saved_ship.name,
                class_name: saved_ship.class_name,
                description: saved_ship.description,
                current_location: saved_ship.current_location.min(locations_len - 1),
                current_fuel: saved_ship.current_fuel.min(saved_ship.max_fuel.max(1)),
                max_fuel: saved_ship.max_fuel.max(1),
                speed: saved_ship.speed.max(1),
                hull: saved_ship.hull.clamp(1, 100),
                low_fuel_alerted: false,
                state: match saved_ship.state {
                    SavedShipState::Docked => ShipState::Docked,
                    SavedShipState::Repairing { ticks_remaining } => {
                        ShipState::Repairing { ticks_remaining }
                    }
                    SavedShipState::EnRoute {
                        origin,
                        destination,
                        eta_remaining,
                        total_eta,
                        exploration_run,
                        segments,
                        segment_costs,
                        route,
                        condition_summary,
                        assigned_contract,
                        repair_on_arrival,
                    } => ShipState::EnRoute {
                        origin: origin.min(locations_len - 1),
                        destination: destination.min(locations_len - 1),
                        eta_remaining,
                        total_eta: total_eta.max(eta_remaining),
                        exploration_run,
                        segments: if segments.is_empty() {
                            vec![(
                                origin.min(locations_len - 1),
                                destination.min(locations_len - 1),
                            )]
                        } else {
                            segments
                                .into_iter()
                                .map(|(start, end)| {
                                    (start.min(locations_len - 1), end.min(locations_len - 1))
                                })
                                .collect()
                        },
                        segment_costs: if segment_costs.is_empty() {
                            vec![total_eta.max(1)]
                        } else {
                            segment_costs
                        },
                        route,
                        condition_summary,
                        assigned_contract: assigned_contract.filter(|&index| index < contracts_len),
                        repair_on_arrival,
                    },
                },
            })
            .collect();
        self.selected_ship = selected_ship.min(self.fleet.len() - 1);
        self.player_in_transit_ship = self
            .player_in_transit_ship
            .filter(|&index| index < self.fleet.len());

        let discovered = self.discovered_locations.clone();
        let fleet_len = self.fleet.len();
        for (contract, saved_contract) in self.contracts.iter_mut().zip(saved_contracts) {
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
                    ship_index: ship_index.min(fleet_len - 1),
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

        self.mode = match mode {
            SavedAppMode::Browse => AppMode::Browse,
            SavedAppMode::SelectingDestination {
                ship_index,
                exploration,
            } => AppMode::SelectingDestination {
                ship_index: ship_index.min(self.fleet.len() - 1),
                intent: if exploration {
                    crate::game::DispatchIntent::Exploration
                } else {
                    crate::game::DispatchIntent::Standard
                },
            },
        };
        self.sync_map_focus_to_selected_location();
        self.log = if log.is_empty() {
            vec!["[0000] Save loaded.".to_string()]
        } else {
            log.into_iter().take(8).collect()
        };
        self.mission_history = mission_history.into_iter().take(6).collect();

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

    pub(crate) fn llm_gate_options(&self) -> [LlmGateAction; 4] {
        [
            LlmGateAction::DisableAndContinue,
            LlmGateAction::RetryConnection,
            LlmGateAction::OpenSettings,
            LlmGateAction::Quit,
        ]
    }

    fn sync_end_screen(&mut self) {
        if self.run_outcome.is_some() {
            self.screen = Screen::EndGame;
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> bool {
        match self.screen {
            Screen::LlmGate => self.handle_llm_gate_key(key),
            Screen::StartMenu => self.handle_start_menu_key(key),
            Screen::LoadGame => self.handle_load_game_key(key),
            Screen::InitializingWorld => self.handle_initializing_world_key(key),
            Screen::Settings => self.handle_settings_key(key),
            Screen::HowToPlay => self.handle_how_to_key(key),
            Screen::InGame => self.handle_game_key(key),
            Screen::EndGame => self.handle_end_game_key(key),
        }
    }

    fn handle_llm_gate_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
            (KeyCode::Up, _) => {
                self.llm_gate_selection = crate::game::wrap_index(
                    self.llm_gate_selection,
                    self.llm_gate_options().len(),
                    -1,
                );
                false
            }
            (KeyCode::Down, _) => {
                self.llm_gate_selection = crate::game::wrap_index(
                    self.llm_gate_selection,
                    self.llm_gate_options().len(),
                    1,
                );
                false
            }
            (KeyCode::Enter, _) => {
                match self.llm_gate_options()[self.llm_gate_selection] {
                    LlmGateAction::DisableAndContinue => self.disable_llm_mode_and_continue(),
                    LlmGateAction::RetryConnection => match self.test_llm_connection() {
                        Ok(()) => {
                            self.last_llm_status = format!(
                                "LLM connection OK: {} via {}",
                                self.llm_settings.provider.label(),
                                self.llm_settings.model
                            );
                            self.menu_feedback = Some("LLM connection restored.".to_string());
                            self.screen = Screen::StartMenu;
                        }
                        Err(error) => {
                            self.menu_feedback = Some(format!("LLM connection failed: {error}"));
                        }
                    },
                    LlmGateAction::OpenSettings => {
                        self.settings_focus = 2;
                        self.llm_field_selection = 0;
                        self.settings_return_screen = Screen::LlmGate;
                        self.screen = Screen::Settings;
                    }
                    LlmGateAction::Quit => return true,
                }
                false
            }
            _ => false,
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
                self.begin_new_game();
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
                self.settings_return_screen = Screen::StartMenu;
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
                self.screen = self.settings_return_screen;
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

    fn handle_initializing_world_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
            (KeyCode::Esc, _) => {
                self.cancel_world_initialization();
                false
            }
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
                self.screen = self.settings_return_screen;
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
            (KeyCode::Char('c'), _) if self.settings_focus == 2 => {
                self.menu_feedback = Some(match self.test_llm_connection() {
                    Ok(()) => {
                        self.last_llm_status = format!(
                            "LLM connection OK: {} via {}",
                            self.llm_settings.provider.label(),
                            self.llm_settings.model
                        );
                        "LLM connection OK.".to_string()
                    }
                    Err(error) => {
                        self.last_llm_status = format!("LLM connection failed: {error}");
                        format!("LLM connection failed: {error}")
                    }
                });
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
        if self.popup_message.is_some() {
            return match (key.code, key.modifiers) {
                (KeyCode::Char('q'), _) => true,
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
                _ => {
                    self.popup_message = None;
                    false
                }
            };
        }

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
            (KeyCode::Char('z'), _) if self.active_pane == crate::game::MAP_PANE => {
                self.zoom_in_map();
                false
            }
            (KeyCode::Char('x'), _) if self.active_pane == crate::game::MAP_PANE => {
                self.zoom_out_map();
                false
            }
            (KeyCode::Char('g'), _) if self.active_pane == crate::game::MAP_PANE => {
                self.auto_focus_map();
                false
            }
            (KeyCode::Enter, _) => {
                self.activate_selection();
                self.sync_action_feedback_popup();
                self.sync_end_screen();
                let _ = self.save_game();
                false
            }
            (KeyCode::Char('f'), _) => {
                self.refuel_selected_ship();
                self.sync_action_feedback_popup();
                self.sync_end_screen();
                let _ = self.save_game();
                false
            }
            (KeyCode::Char('t'), _) => {
                self.transfer_fuel_to_selected_ship();
                self.sync_action_feedback_popup();
                self.sync_end_screen();
                let _ = self.save_game();
                false
            }
            (KeyCode::Char('u'), _) => {
                self.upgrade_selected_ship();
                self.sync_action_feedback_popup();
                self.sync_end_screen();
                let _ = self.save_game();
                false
            }
            (KeyCode::Char('e'), _) if self.active_pane == crate::game::FLEET_PANE => {
                self.begin_exploration();
                self.sync_action_feedback_popup();
                self.sync_end_screen();
                let _ = self.save_game();
                false
            }
            (KeyCode::Char('m'), _)
                if self.active_pane == crate::game::MAP_PANE
                    && matches!(self.mode, crate::game::AppMode::Browse) =>
            {
                self.transfer_player_to_selected_location();
                self.sync_action_feedback_popup();
                self.sync_end_screen();
                let _ = self.save_game();
                false
            }
            (KeyCode::Char('b'), _)
                if matches!(
                    self.active_pane,
                    crate::game::MAP_PANE | crate::game::SHIPYARD_PANE
                ) =>
            {
                self.purchase_ship_at_selected_location();
                self.sync_action_feedback_popup();
                self.sync_end_screen();
                let _ = self.save_game();
                false
            }
            (KeyCode::Char('s'), _) => {
                self.settings_return_screen = Screen::InGame;
                self.settings_selection = self.tick_speed_index;
                self.difficulty_selection = self.difficulty.index();
                self.settings_focus = 0;
                self.llm_field_selection = 0;
                self.settings_edit = None;
                self.screen = Screen::Settings;
                false
            }
            (KeyCode::Char('r'), _) if self.active_pane == crate::game::CONTRACTS_PANE => {
                self.regenerate_selected_contract_flavor();
                false
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ContractFlavorSignature {
    archetype: ContractArchetype,
    origin: usize,
    destination: usize,
    reward: i32,
    max_eta: u16,
    deadline: u64,
}

impl ContractFlavorSignature {
    fn from_contract(contract: &Contract) -> Self {
        Self {
            archetype: contract.archetype,
            origin: contract.origin,
            destination: contract.destination,
            reward: contract.reward,
            max_eta: contract.max_eta,
            deadline: contract.deadline,
        }
    }
}

#[derive(Clone, Copy)]
enum ContractFlavorTrigger {
    Hydrate,
    Regenerate,
}

struct ContractFlavorJobResult {
    contract_index: usize,
    signature: ContractFlavorSignature,
    trigger: ContractFlavorTrigger,
    provider_label: String,
    model: String,
    result: Result<GeneratedContractFlavor, String>,
}

struct WorldInitializationState {
    seed: u64,
    slot_index: usize,
}

struct WorldInitializationJobResult {
    seed: u64,
    provider_label: String,
    model: String,
    result: Result<WorldFlavor, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    LlmGate,
    StartMenu,
    LoadGame,
    InitializingWorld,
    Settings,
    HowToPlay,
    InGame,
    EndGame,
}

#[derive(Clone, Copy)]
pub(crate) enum LlmGateAction {
    DisableAndContinue,
    RetryConnection,
    OpenSettings,
    Quit,
}

impl LlmGateAction {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::DisableAndContinue => "Disable LLM",
            Self::RetryConnection => "Retry Connection",
            Self::OpenSettings => "Open Settings",
            Self::Quit => "Quit",
        }
    }
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

pub(crate) const LLM_FIELDS: [LlmField; 6] = [
    LlmField::Enabled,
    LlmField::Provider,
    LlmField::EndpointUrl,
    LlmField::Model,
    LlmField::ApiKey,
    LlmField::TimeoutSecs,
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LlmField {
    Enabled,
    Provider,
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
    fn test_connection(
        &self,
        _settings: &LlmSettings,
        _api_key: Option<&str>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn generate_sector(
        &self,
        _settings: &LlmSettings,
        _api_key: Option<&str>,
        _world_seed: u64,
    ) -> Result<WorldFlavor, String> {
        Ok(test_world_flavor("Noop Sector"))
    }

    fn generate_flavor(
        &self,
        _settings: &LlmSettings,
        _api_key: Option<&str>,
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
struct FailingConnectionGenerator;

#[cfg(test)]
impl ContractFlavorGenerator for FailingConnectionGenerator {
    fn test_connection(
        &self,
        _settings: &LlmSettings,
        _api_key: Option<&str>,
    ) -> Result<(), String> {
        Err("connection unavailable".to_string())
    }

    fn generate_sector(
        &self,
        _settings: &LlmSettings,
        _api_key: Option<&str>,
        _world_seed: u64,
    ) -> Result<WorldFlavor, String> {
        Ok(test_world_flavor("Failing Sector"))
    }

    fn generate_flavor(
        &self,
        _settings: &LlmSettings,
        _api_key: Option<&str>,
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
struct BlockingFlavorGenerator {
    world_ready: Arc<std::sync::atomic::AtomicBool>,
    flavor_ready: Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(test)]
impl ContractFlavorGenerator for BlockingFlavorGenerator {
    fn test_connection(
        &self,
        _settings: &LlmSettings,
        _api_key: Option<&str>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn generate_sector(
        &self,
        _settings: &LlmSettings,
        _api_key: Option<&str>,
        _world_seed: u64,
    ) -> Result<WorldFlavor, String> {
        while !self.world_ready.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
        }

        Ok(test_world_flavor("Bootstrap Sector"))
    }

    fn generate_flavor(
        &self,
        _settings: &LlmSettings,
        _api_key: Option<&str>,
        contract: &Contract,
        _difficulty: Difficulty,
        _locations: &[crate::game::Location],
    ) -> Result<GeneratedContractFlavor, String> {
        while !self.flavor_ready.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
        }

        Ok(GeneratedContractFlavor {
            title: format!("LLM {}", contract.archetype.title()),
            briefing: format!("Generated flavor for {}.", contract.archetype.title()),
        })
    }
}

#[cfg(test)]
fn test_world_flavor(sector_name: &str) -> WorldFlavor {
    WorldFlavor {
        environment_name: sector_name.to_string(),
        environment_summary: "Generated test environment summary.".to_string(),
        locations: vec![
            crate::game::WorldLocationFlavor {
                region_name: "Helios Frontier".to_string(),
                sector_name: "Astra Corridor".to_string(),
                name: "Astra Prime".to_string(),
                short_label: "Astra".to_string(),
                lane_name: "Central Exchange".to_string(),
                description: "A generated hub.".to_string(),
                cluster_name: "Helios Delta".to_string(),
                system_name: "Astra Line".to_string(),
            },
            crate::game::WorldLocationFlavor {
                region_name: "Helios Frontier".to_string(),
                sector_name: "Astra Corridor".to_string(),
                name: "Kite Station".to_string(),
                short_label: "Kite".to_string(),
                lane_name: "Kite Spur".to_string(),
                description: "A generated relay.".to_string(),
                cluster_name: "Ravel Spur".to_string(),
                system_name: "Kite Rise".to_string(),
            },
            crate::game::WorldLocationFlavor {
                region_name: "Helios Frontier".to_string(),
                sector_name: "Astra Corridor".to_string(),
                name: "Ion Anchorage".to_string(),
                short_label: "Ion".to_string(),
                lane_name: "Ion Run".to_string(),
                description: "A generated anchorage.".to_string(),
                cluster_name: "Ion Expanse".to_string(),
                system_name: "Relay Verge".to_string(),
            },
            crate::game::WorldLocationFlavor {
                region_name: "Helios Frontier".to_string(),
                sector_name: "Astra Corridor".to_string(),
                name: "Dust Harbor".to_string(),
                short_label: "Dust".to_string(),
                lane_name: "Dust Corridor".to_string(),
                description: "A generated harbor.".to_string(),
                cluster_name: "Helios Delta".to_string(),
                system_name: "Astra Line".to_string(),
            },
            crate::game::WorldLocationFlavor {
                region_name: "Helios Frontier".to_string(),
                sector_name: "Astra Corridor".to_string(),
                name: "Outer Ring Relay".to_string(),
                short_label: "Relay".to_string(),
                lane_name: "Relay Ascent".to_string(),
                description: "A generated relay terminus.".to_string(),
                cluster_name: "Ion Expanse".to_string(),
                system_name: "Relay Verge".to_string(),
            },
            crate::game::WorldLocationFlavor {
                region_name: "Perihelion Reach".to_string(),
                sector_name: "Vesper March".to_string(),
                name: "Vesper Exchange".to_string(),
                short_label: "Vesper".to_string(),
                lane_name: "March Nexus".to_string(),
                description: "A generated second hub.".to_string(),
                cluster_name: "Vesper Crown".to_string(),
                system_name: "March Line".to_string(),
            },
            crate::game::WorldLocationFlavor {
                region_name: "Perihelion Reach".to_string(),
                sector_name: "Vesper March".to_string(),
                name: "Wick Relay".to_string(),
                short_label: "Wick".to_string(),
                lane_name: "Wick Spur".to_string(),
                description: "A generated second relay.".to_string(),
                cluster_name: "Cinder Spur".to_string(),
                system_name: "Wick Rise".to_string(),
            },
            crate::game::WorldLocationFlavor {
                region_name: "Perihelion Reach".to_string(),
                sector_name: "Vesper March".to_string(),
                name: "Cinder Anchorage".to_string(),
                short_label: "Cinder".to_string(),
                lane_name: "Cinder Run".to_string(),
                description: "A generated second anchorage.".to_string(),
                cluster_name: "Cinder Spur".to_string(),
                system_name: "Foundry Verge".to_string(),
            },
            crate::game::WorldLocationFlavor {
                region_name: "Perihelion Reach".to_string(),
                sector_name: "Vesper March".to_string(),
                name: "Ember Harbor".to_string(),
                short_label: "Ember".to_string(),
                lane_name: "Ember Drift".to_string(),
                description: "A generated second harbor.".to_string(),
                cluster_name: "Vesper Crown".to_string(),
                system_name: "March Line".to_string(),
            },
            crate::game::WorldLocationFlavor {
                region_name: "Perihelion Reach".to_string(),
                sector_name: "Vesper March".to_string(),
                name: "Far Signal Array".to_string(),
                short_label: "Signal".to_string(),
                lane_name: "Signal Ascent".to_string(),
                description: "A generated remote array.".to_string(),
                cluster_name: "Foundry Verge".to_string(),
                system_name: "Foundry Verge".to_string(),
            },
        ],
        starter_ships: vec![
            crate::game::WorldShipFlavor {
                name: "SV Kestrel".to_string(),
                class_name: "Courier Cutter".to_string(),
                description: "A generated starter courier.".to_string(),
            },
            crate::game::WorldShipFlavor {
                name: "CSV Lantern".to_string(),
                class_name: "Utility Freighter".to_string(),
                description: "A generated starter freighter.".to_string(),
            },
            crate::game::WorldShipFlavor {
                name: "HMV Orpheus".to_string(),
                class_name: "Survey Tender".to_string(),
                description: "A generated starter tender.".to_string(),
            },
        ],
        shipyard_offers: vec![
            crate::game::WorldShipFlavor {
                name: "RSV Venture".to_string(),
                class_name: "Relay Skiff".to_string(),
                description: "A generated yard offer.".to_string(),
            },
            crate::game::WorldShipFlavor {
                name: "TSS Halcyon".to_string(),
                class_name: "Tank Sloop".to_string(),
                description: "A generated yard offer.".to_string(),
            },
            crate::game::WorldShipFlavor {
                name: "MVS Drift".to_string(),
                class_name: "Maintenance Sloop".to_string(),
                description: "A generated yard offer.".to_string(),
            },
            crate::game::WorldShipFlavor {
                name: "RSV Zephyr".to_string(),
                class_name: "Relay Skiff".to_string(),
                description: "A generated yard offer.".to_string(),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::{Arc, atomic::AtomicBool},
        time::Duration,
    };

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
            ship_index: 0,
            accepted_at: 12,
        };
        app.fleet[0].current_fuel = 4;
        app.fleet[0].max_fuel = 14;
        app.fleet[0].state = crate::game::ShipState::EnRoute {
            origin: crate::game::ASTRA_PRIME,
            destination: crate::game::DUST_HARBOR,
            eta_remaining: 3,
            total_eta: 5,
            exploration_run: false,
            segments: vec![(crate::game::ASTRA_PRIME, crate::game::DUST_HARBOR)],
            segment_costs: vec![5],
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
                ship_index: 0,
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
    fn purchased_ship_round_trips_through_save_load() {
        let store = MemorySaveStore::new();
        let mut app = App::with_store(Box::new(store.clone()), GameData::new(Difficulty::Normal));
        app.has_active_game = true;
        app.screen = Screen::InGame;
        app.credits = 500;
        app.selected_location = crate::game::ASTRA_PRIME;
        app.purchase_ship_at_selected_location();

        let purchased_name = app.fleet.last().unwrap().name.clone();
        let purchased_class = app.fleet.last().unwrap().class_name.clone();
        let purchased_description = app.fleet.last().unwrap().description.clone();
        let purchased_speed = app.fleet.last().unwrap().speed;
        let purchased_fuel = app.fleet.last().unwrap().max_fuel;

        app.save_game().unwrap();

        let mut restored = App::with_store(Box::new(store), GameData::new(Difficulty::Normal));
        restored.load_game(0).unwrap();

        let restored_ship = restored.fleet.last().unwrap();
        assert_eq!(restored_ship.name, purchased_name);
        assert_eq!(restored_ship.class_name, purchased_class);
        assert_eq!(restored_ship.description, purchased_description);
        assert_eq!(restored_ship.speed, purchased_speed);
        assert_eq!(restored_ship.max_fuel, purchased_fuel);
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
            Arc::new(NoopContractFlavorGenerator),
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

    #[test]
    fn llm_enabled_with_bad_connection_triggers_gate() {
        let mut app = App::with_dependencies(
            Box::new(MemorySaveStore::new()),
            Box::new(TestSettingsStore::default()),
            Box::new(TestSecretStore {
                api_key: Rc::new(RefCell::new(Some("token".to_string()))),
            }),
            Arc::new(FailingConnectionGenerator),
            GameData::new(Difficulty::Normal),
            AppSettings {
                tick_speed_index: 1,
                difficulty: Difficulty::Normal,
                llm: LlmSettings {
                    enabled: true,
                    provider: LlmProviderPreset::OpenAI,
                    endpoint_url: "https://bad.example/v1/chat/completions".to_string(),
                    model: "demo-model".to_string(),
                    timeout_secs: 5,
                },
            },
            true,
        );

        app.evaluate_startup_llm_gate();

        assert_eq!(app.screen, Screen::LlmGate);
        assert!(
            app.menu_feedback
                .as_ref()
                .is_some_and(|message| message.contains("connection unavailable"))
        );
    }

    #[test]
    fn new_game_queues_contract_flavor_generation_in_background() {
        let world_ready = Arc::new(AtomicBool::new(false));
        let flavor_ready = Arc::new(AtomicBool::new(false));
        let mut app = App::with_dependencies(
            Box::new(MemorySaveStore::new()),
            Box::new(TestSettingsStore::default()),
            Box::new(TestSecretStore::default()),
            Arc::new(BlockingFlavorGenerator {
                world_ready: Arc::clone(&world_ready),
                flavor_ready: Arc::clone(&flavor_ready),
            }),
            GameData::new(Difficulty::Normal),
            AppSettings {
                tick_speed_index: 1,
                difficulty: Difficulty::Normal,
                llm: LlmSettings {
                    enabled: true,
                    provider: LlmProviderPreset::Ollama,
                    endpoint_url: "http://localhost:11434/v1".to_string(),
                    model: "llama3.1".to_string(),
                    timeout_secs: 5,
                },
            },
            false,
        );

        assert!(!app.activate_start_menu_selection());
        assert_eq!(app.screen, Screen::InitializingWorld);
        assert!(!app.has_active_game);

        world_ready.store(true, std::sync::atomic::Ordering::SeqCst);
        for _ in 0..50 {
            app.sync_background_work();
            if app.screen == Screen::InGame {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(app.screen, Screen::InGame);
        assert!(app.has_active_game);
        assert!(app.pending_contract_flavor_count() > 0);
        assert!(!app.in_flight_contract_flavors.is_empty());

        flavor_ready.store(true, std::sync::atomic::Ordering::SeqCst);
        for _ in 0..50 {
            app.sync_background_work();
            if app.pending_contract_flavor_count() == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(app.pending_contract_flavor_count(), 0);
        assert!(
            app.contracts
                .iter()
                .all(|contract| contract.title.starts_with("LLM "))
        );
    }

    #[test]
    fn stale_background_flavor_result_does_not_overwrite_replaced_contract() {
        let ready = Arc::new(AtomicBool::new(false));
        let mut app = App::with_dependencies(
            Box::new(MemorySaveStore::new()),
            Box::new(TestSettingsStore::default()),
            Box::new(TestSecretStore::default()),
            Arc::new(BlockingFlavorGenerator {
                world_ready: Arc::clone(&ready),
                flavor_ready: Arc::clone(&ready),
            }),
            GameData::new(Difficulty::Normal),
            AppSettings {
                tick_speed_index: 1,
                difficulty: Difficulty::Normal,
                llm: LlmSettings {
                    enabled: true,
                    provider: LlmProviderPreset::LmStudio,
                    endpoint_url: "http://localhost:1234/v1".to_string(),
                    model: "local-model".to_string(),
                    timeout_secs: 5,
                },
            },
            false,
        );

        app.contracts[0].pending_llm_flavor = true;
        app.spawn_contract_flavor_job(0, None, ContractFlavorTrigger::Hydrate);
        app.contracts[0] = Contract::new(
            ContractArchetype::PriorityRelay,
            crate::game::ASTRA_PRIME,
            crate::game::KITE_STATION,
            999,
            2,
            12,
            crate::game::KITE_STATION,
        );
        app.contracts[0].pending_llm_flavor = false;
        let replacement_title = app.contracts[0].title.clone();

        ready.store(true, std::sync::atomic::Ordering::SeqCst);
        for _ in 0..50 {
            app.sync_background_work();
            if app.in_flight_contract_flavors.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(app.contracts[0].title, replacement_title);
    }
}
