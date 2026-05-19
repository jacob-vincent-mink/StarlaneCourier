use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::game::{Difficulty, RunOutcome, default_difficulty};

pub(crate) const SAVE_DIR: &str = "saves";
pub(crate) const SAVE_SLOT_COUNT: usize = 3;
pub(crate) const SAVE_VERSION: u8 = 1;

pub(crate) fn save_slot_path(slot_index: usize) -> PathBuf {
    Path::new(SAVE_DIR).join(format!("slot-{}.json", slot_index + 1))
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SaveGame {
    pub(crate) version: u8,
    pub(crate) tick_speed_index: usize,
    pub(crate) active_pane: usize,
    pub(crate) clock: u64,
    pub(crate) mode: SavedAppMode,
    pub(crate) selected_location: usize,
    pub(crate) selected_ship: usize,
    pub(crate) selected_contract: usize,
    pub(crate) tracked_contract: Option<usize>,
    pub(crate) credits: i32,
    #[serde(default = "default_difficulty")]
    pub(crate) difficulty: Difficulty,
    #[serde(default)]
    pub(crate) run_outcome: Option<RunOutcome>,
    pub(crate) discovered_locations: Vec<bool>,
    #[serde(default)]
    pub(crate) station_fuel: Vec<u16>,
    pub(crate) fleet: Vec<SavedShip>,
    pub(crate) contracts: Vec<SavedContract>,
    pub(crate) log: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum SavedAppMode {
    Browse,
    SelectingDestination { ship_index: usize },
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedShip {
    pub(crate) current_location: usize,
    #[serde(default)]
    pub(crate) current_fuel: u16,
    #[serde(default)]
    pub(crate) max_fuel: u16,
    pub(crate) state: SavedShipState,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum SavedShipState {
    Docked,
    EnRoute {
        origin: usize,
        destination: usize,
        eta_remaining: u16,
        total_eta: u16,
        route: String,
        condition_summary: String,
        assigned_contract: Option<usize>,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedContract {
    pub(crate) deadline: u64,
    pub(crate) state: SavedContractState,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum SavedContractState {
    Available,
    Accepted { accepted_at: u64 },
    Assigned { ship_index: usize, accepted_at: u64 },
    Completed,
    Failed,
}

pub(crate) trait SaveStore {
    fn read_slot(&self, slot_index: usize) -> Result<Option<SaveGame>, String>;
    fn write_slot(&self, slot_index: usize, save: &SaveGame) -> Result<(), String>;
}

pub(crate) struct FsSaveStore;

impl SaveStore for FsSaveStore {
    fn read_slot(&self, slot_index: usize) -> Result<Option<SaveGame>, String> {
        let path = save_slot_path(slot_index);
        if !path.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let save: SaveGame = serde_json::from_str(&raw).map_err(|error| error.to_string())?;

        if save.version != SAVE_VERSION {
            return Err(format!(
                "unsupported save version {} (expected {})",
                save.version, SAVE_VERSION
            ));
        }

        Ok(Some(save))
    }

    fn write_slot(&self, slot_index: usize, save: &SaveGame) -> Result<(), String> {
        let path = save_slot_path(slot_index);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        let json = serde_json::to_string_pretty(save).map_err(|error| error.to_string())?;
        fs::write(path, json).map_err(|error| error.to_string())
    }
}
