use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::game::{ContractArchetype, Difficulty, MapPoint, RunOutcome, default_difficulty};

const APP_DATA_DIR: &str = "spacecourier";
pub(crate) const SAVE_DIR: &str = "saves";
pub(crate) const SAVE_SLOT_COUNT: usize = 3;
pub(crate) const SAVE_VERSION: u8 = 2;

pub(crate) fn save_slot_path(slot_index: usize) -> Result<PathBuf, String> {
    Ok(data_dir()?
        .join(SAVE_DIR)
        .join(save_slot_file_name(slot_index)))
}

#[cfg(test)]
fn save_slot_path_from_roots(
    xdg_data_home: Option<&Path>,
    home: Option<&Path>,
    slot_index: usize,
) -> Result<PathBuf, String> {
    Ok(data_dir_from_roots(xdg_data_home, home)?
        .join(SAVE_DIR)
        .join(save_slot_file_name(slot_index)))
}

fn legacy_save_slot_path(slot_index: usize) -> PathBuf {
    Path::new(SAVE_DIR).join(save_slot_file_name(slot_index))
}

fn save_slot_file_name(slot_index: usize) -> String {
    format!("slot-{}.json", slot_index + 1)
}

fn data_dir() -> Result<PathBuf, String> {
    data_dir_from_roots(
        env::var_os("XDG_DATA_HOME").as_deref().map(Path::new),
        env::var_os("HOME").as_deref().map(Path::new),
    )
}

fn data_dir_from_roots(
    xdg_data_home: Option<&Path>,
    home: Option<&Path>,
) -> Result<PathBuf, String> {
    if let Some(path) = xdg_data_home {
        return Ok(path.join(APP_DATA_DIR));
    }

    if let Some(path) = home {
        return Ok(path.join(".local").join("share").join(APP_DATA_DIR));
    }

    Err("HOME or XDG_DATA_HOME is not set".to_string())
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SaveGame {
    pub(crate) version: u8,
    pub(crate) tick_speed_index: usize,
    pub(crate) active_pane: usize,
    pub(crate) clock: u64,
    pub(crate) mode: SavedAppMode,
    #[serde(default)]
    pub(crate) player_location: usize,
    #[serde(default)]
    pub(crate) player_in_transit_ship: Option<usize>,
    pub(crate) selected_location: usize,
    pub(crate) selected_ship: usize,
    pub(crate) selected_contract: usize,
    pub(crate) tracked_contract: Option<usize>,
    pub(crate) credits: i32,
    #[serde(default = "default_difficulty")]
    pub(crate) difficulty: Difficulty,
    #[serde(default)]
    pub(crate) run_outcome: Option<RunOutcome>,
    #[serde(default)]
    pub(crate) world_seed: u64,
    #[serde(default)]
    pub(crate) sector_name: String,
    #[serde(default)]
    pub(crate) sector_summary: String,
    #[serde(default)]
    pub(crate) locations: Vec<SavedLocation>,
    pub(crate) discovered_locations: Vec<bool>,
    #[serde(default)]
    pub(crate) station_fuel: Vec<u16>,
    #[serde(default)]
    pub(crate) station_ship_shops: Vec<Option<SavedShipShop>>,
    pub(crate) fleet: Vec<SavedShip>,
    pub(crate) contracts: Vec<SavedContract>,
    pub(crate) log: Vec<String>,
    #[serde(default)]
    pub(crate) mission_history: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum SavedAppMode {
    Browse,
    SelectingDestination {
        ship_index: usize,
        #[serde(default)]
        exploration: bool,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedLocation {
    #[serde(default)]
    pub(crate) region_name: String,
    #[serde(default)]
    pub(crate) sector_name: String,
    pub(crate) name: String,
    pub(crate) short_label: String,
    pub(crate) lane_name: String,
    pub(crate) description: String,
    pub(crate) cluster_name: String,
    pub(crate) system_name: String,
    #[serde(default)]
    pub(crate) region_coords: SavedMapPoint,
    pub(crate) sector_coords: SavedMapPoint,
    pub(crate) cluster_coords: SavedMapPoint,
    pub(crate) system_coords: SavedMapPoint,
    pub(crate) travel_time_from_hub: u16,
    pub(crate) reveal_on_arrival: Option<usize>,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
pub(crate) struct SavedMapPoint {
    #[serde(default)]
    pub(crate) x: i16,
    #[serde(default)]
    pub(crate) y: i16,
}

impl From<MapPoint> for SavedMapPoint {
    fn from(value: MapPoint) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

impl From<SavedMapPoint> for MapPoint {
    fn from(value: SavedMapPoint) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedShip {
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) class_name: String,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) current_location: usize,
    #[serde(default)]
    pub(crate) current_fuel: u16,
    #[serde(default)]
    pub(crate) max_fuel: u16,
    #[serde(default = "default_ship_speed")]
    pub(crate) speed: u16,
    #[serde(default = "default_hull")]
    pub(crate) hull: u16,
    pub(crate) state: SavedShipState,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum SavedShipState {
    Docked,
    Repairing {
        ticks_remaining: u16,
    },
    EnRoute {
        origin: usize,
        destination: usize,
        eta_remaining: u16,
        total_eta: u16,
        #[serde(default)]
        exploration_run: bool,
        #[serde(default)]
        segments: Vec<(usize, usize)>,
        #[serde(default)]
        segment_costs: Vec<u16>,
        route: String,
        condition_summary: String,
        assigned_contract: Option<usize>,
        #[serde(default)]
        repair_on_arrival: u16,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedShipShop {
    #[serde(default)]
    pub(crate) offers: Vec<SavedShipOffer>,
    #[serde(default)]
    pub(crate) legacy_offer: Option<SavedShipOffer>,
    #[serde(default)]
    pub(crate) last_refresh: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedShipOffer {
    pub(crate) name: String,
    pub(crate) class_name: String,
    pub(crate) description: String,
    pub(crate) speed: u16,
    pub(crate) max_fuel: u16,
    pub(crate) price: i32,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedContract {
    #[serde(default = "default_contract_archetype")]
    pub(crate) archetype: ContractArchetype,
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) briefing: String,
    #[serde(default = "default_contract_origin")]
    pub(crate) origin: usize,
    #[serde(default = "default_contract_destination")]
    pub(crate) destination: usize,
    #[serde(default)]
    pub(crate) reward: i32,
    #[serde(default = "default_contract_eta")]
    pub(crate) max_eta: u16,
    pub(crate) deadline: u64,
    #[serde(default = "default_contract_destination")]
    pub(crate) unlock_location: usize,
    #[serde(default = "default_pending_llm_flavor")]
    pub(crate) pending_llm_flavor: bool,
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

#[derive(Deserialize)]
struct SaveVersionProbe {
    version: u8,
}

#[derive(Deserialize)]
struct LegacySaveGameV1 {
    version: u8,
    tick_speed_index: usize,
    active_pane: usize,
    clock: u64,
    mode: SavedAppMode,
    selected_location: usize,
    selected_ship: usize,
    selected_contract: usize,
    tracked_contract: Option<usize>,
    credits: i32,
    #[serde(default = "default_difficulty")]
    difficulty: Difficulty,
    #[serde(default)]
    run_outcome: Option<RunOutcome>,
    discovered_locations: Vec<bool>,
    #[serde(default)]
    station_fuel: Vec<u16>,
    fleet: Vec<LegacySavedShipV1>,
    contracts: Vec<SavedContract>,
    log: Vec<String>,
}

#[derive(Deserialize)]
struct LegacySavedShipV1 {
    current_location: usize,
    #[serde(default)]
    current_fuel: u16,
    #[serde(default)]
    max_fuel: u16,
    #[serde(default = "default_hull")]
    hull: u16,
    state: SavedShipState,
}

impl LegacySaveGameV1 {
    fn into_current(self) -> SaveGame {
        SaveGame {
            version: SAVE_VERSION,
            tick_speed_index: self.tick_speed_index,
            active_pane: self.active_pane,
            clock: self.clock,
            mode: self.mode,
            player_location: 0,
            player_in_transit_ship: None,
            selected_location: self.selected_location,
            selected_ship: self.selected_ship,
            selected_contract: self.selected_contract,
            tracked_contract: self.tracked_contract,
            credits: self.credits,
            difficulty: self.difficulty,
            run_outcome: self.run_outcome,
            world_seed: 0,
            sector_name: String::new(),
            sector_summary: String::new(),
            locations: Vec::new(),
            discovered_locations: self.discovered_locations,
            station_fuel: self.station_fuel,
            station_ship_shops: Vec::new(),
            fleet: self
                .fleet
                .into_iter()
                .enumerate()
                .map(|(index, ship)| ship.into_current(index))
                .collect(),
            contracts: self.contracts,
            log: self.log,
            mission_history: Vec::new(),
        }
    }
}

impl LegacySavedShipV1 {
    fn into_current(self, index: usize) -> SavedShip {
        let (name, class_name, description, speed, max_fuel) = legacy_ship_defaults(index);
        SavedShip {
            name: name.to_string(),
            class_name: class_name.to_string(),
            description: description.to_string(),
            current_location: self.current_location,
            current_fuel: self.current_fuel.min(self.max_fuel.max(max_fuel)),
            max_fuel: self.max_fuel.max(max_fuel),
            speed,
            hull: self.hull,
            state: self.state,
        }
    }
}

fn default_contract_archetype() -> ContractArchetype {
    ContractArchetype::SurveyDrop
}

fn default_contract_origin() -> usize {
    0
}

fn default_contract_destination() -> usize {
    3
}

fn default_contract_eta() -> u16 {
    4
}

fn default_pending_llm_flavor() -> bool {
    true
}

fn default_hull() -> u16 {
    100
}

fn default_ship_speed() -> u16 {
    2
}

fn legacy_ship_defaults(index: usize) -> (&'static str, &'static str, &'static str, u16, u16) {
    match index {
        0 => (
            "SV Kestrel",
            "Courier Cutter",
            "A fast dispatch hull tuned for urgent packets and short-hop contracts.",
            2,
            14,
        ),
        1 => (
            "CSV Lantern",
            "Utility Freighter",
            "A dependable work barge with broad tanks and enough room for frontier cargo.",
            1,
            16,
        ),
        2 => (
            "HMV Orpheus",
            "Survey Tender",
            "A balanced exploration ship fitted for chart work and steady courier lanes.",
            3,
            18,
        ),
        _ => (
            "Recovered Hull",
            "Legacy Transfer",
            "A hull imported from an older save schema. Stats were reconstructed during load.",
            2,
            16,
        ),
    }
}

impl SaveStore for FsSaveStore {
    fn read_slot(&self, slot_index: usize) -> Result<Option<SaveGame>, String> {
        let preferred_path = save_slot_path(slot_index).ok();
        let legacy_path = legacy_save_slot_path(slot_index);

        let source_path = if let Some(path) = preferred_path.as_ref().filter(|path| path.exists()) {
            path.clone()
        } else if legacy_path.exists() {
            legacy_path.clone()
        } else if preferred_path.is_none() {
            return Err("HOME or XDG_DATA_HOME is not set".to_string());
        } else {
            return Ok(None);
        };

        let raw = fs::read_to_string(&source_path).map_err(|error| error.to_string())?;
        let version =
            serde_json::from_str::<SaveVersionProbe>(&raw).map_err(|error| error.to_string())?;

        match version.version {
            1 => {
                let legacy: LegacySaveGameV1 =
                    serde_json::from_str(&raw).map_err(|error| error.to_string())?;
                if legacy.version != 1 {
                    return Err("save version probe mismatch".to_string());
                }
                let save = legacy.into_current();
                if source_path == legacy_path {
                    let _ = self.write_slot(slot_index, &save);
                }
                Ok(Some(save))
            }
            SAVE_VERSION => {
                let save: SaveGame =
                    serde_json::from_str(&raw).map_err(|error| error.to_string())?;
                if source_path == legacy_path {
                    if let Some(path) = preferred_path {
                        let _ = write_raw_slot(&path, &raw);
                    }
                }
                Ok(Some(save))
            }
            other => Err(format!(
                "unsupported save version {} (expected 1 or {})",
                other, SAVE_VERSION
            )),
        }
    }

    fn write_slot(&self, slot_index: usize, save: &SaveGame) -> Result<(), String> {
        let path = save_slot_path(slot_index)?;
        let json = serde_json::to_string_pretty(save).map_err(|error| error.to_string())?;
        write_raw_slot(&path, &json)
    }
}

fn write_raw_slot(path: &Path, raw: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    fs::write(path, raw).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_contract() -> SavedContract {
        SavedContract {
            archetype: ContractArchetype::SurveyDrop,
            title: "Survey Drop".to_string(),
            briefing: "Legacy payload".to_string(),
            origin: 0,
            destination: 3,
            reward: 160,
            max_eta: 6,
            deadline: 28,
            unlock_location: 3,
            pending_llm_flavor: true,
            state: SavedContractState::Available,
        }
    }

    #[test]
    fn legacy_v1_save_migrates_ship_metadata_and_version() {
        let legacy = LegacySaveGameV1 {
            version: 1,
            tick_speed_index: 1,
            active_pane: 2,
            clock: 42,
            mode: SavedAppMode::Browse,
            selected_location: 0,
            selected_ship: 1,
            selected_contract: 0,
            tracked_contract: None,
            credits: 250,
            difficulty: Difficulty::Normal,
            run_outcome: None,
            discovered_locations: vec![true, false, false, true, false],
            station_fuel: vec![48, 20, 18, 22, 10],
            fleet: vec![
                LegacySavedShipV1 {
                    current_location: 0,
                    current_fuel: 9,
                    max_fuel: 14,
                    hull: 100,
                    state: SavedShipState::Docked,
                },
                LegacySavedShipV1 {
                    current_location: 0,
                    current_fuel: 2,
                    max_fuel: 16,
                    hull: 100,
                    state: SavedShipState::EnRoute {
                        origin: 0,
                        destination: 3,
                        eta_remaining: 7,
                        total_eta: 7,
                        exploration_run: false,
                        segments: vec![(0, 3)],
                        segment_costs: vec![7],
                        route: "Astra Prime -> Dust Harbor".to_string(),
                        condition_summary: "Dust Corridor: debris interference".to_string(),
                        assigned_contract: None,
                        repair_on_arrival: 0,
                    },
                },
                LegacySavedShipV1 {
                    current_location: 0,
                    current_fuel: 13,
                    max_fuel: 18,
                    hull: 100,
                    state: SavedShipState::Docked,
                },
            ],
            contracts: vec![sample_contract()],
            log: vec!["[0042] legacy save".to_string()],
        };

        let migrated = legacy.into_current();

        assert_eq!(migrated.version, SAVE_VERSION);
        assert_eq!(migrated.fleet[0].name, "SV Kestrel");
        assert_eq!(migrated.fleet[1].class_name, "Utility Freighter");
        assert_eq!(migrated.fleet[2].speed, 3);
        assert!(migrated.station_ship_shops.is_empty());
    }

    #[test]
    fn save_slot_path_prefers_xdg_data_home() {
        let path = save_slot_path_from_roots(Some(Path::new("/tmp/xdg-data")), None, 1).unwrap();
        assert_eq!(
            path,
            Path::new("/tmp/xdg-data")
                .join(APP_DATA_DIR)
                .join(SAVE_DIR)
                .join("slot-2.json")
        );
    }

    #[test]
    fn save_slot_path_falls_back_to_home_local_share() {
        let path = save_slot_path_from_roots(None, Some(Path::new("/tmp/home")), 0).unwrap();
        assert_eq!(
            path,
            Path::new("/tmp/home")
                .join(".local")
                .join("share")
                .join(APP_DATA_DIR)
                .join(SAVE_DIR)
                .join("slot-1.json")
        );
    }

    #[test]
    fn legacy_save_slot_path_stays_repo_local() {
        assert_eq!(
            legacy_save_slot_path(2),
            Path::new(SAVE_DIR).join("slot-3.json")
        );
    }
}
