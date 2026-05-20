use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum Difficulty {
    Cozy,
    Normal,
    Insane,
}

impl Difficulty {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Cozy => "Cozy",
            Self::Normal => "Normal",
            Self::Insane => "Insane",
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::Cozy => {
                "No reward decay or contract failure pressure, but fuel still costs a little."
            }
            Self::Normal => "Rewards slowly decay after acceptance, but contracts never fail.",
            Self::Insane => {
                "Rewards decay faster and accepted contracts fail if their delivery window expires."
            }
        }
    }

    pub(crate) fn index(self) -> usize {
        match self {
            Self::Cozy => 0,
            Self::Normal => 1,
            Self::Insane => 2,
        }
    }

    pub(crate) fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Cozy,
            1 => Self::Normal,
            _ => Self::Insane,
        }
    }

    pub(crate) fn reward_decay(self, base_reward: i32, elapsed_ticks: u64) -> i32 {
        match self {
            Self::Cozy => base_reward,
            Self::Normal => {
                let decay_steps = (elapsed_ticks / 3) as i32;
                (base_reward - decay_steps * 8).max(base_reward / 2)
            }
            Self::Insane => {
                let decay_steps = (elapsed_ticks / 2) as i32;
                (base_reward - decay_steps * 14).max((base_reward / 4).max(30))
            }
        }
    }

    pub(crate) fn enforces_time_limit(self) -> bool {
        matches!(self, Self::Insane)
    }

    pub(crate) fn uses_fuel_economy(self) -> bool {
        true
    }

    pub(crate) fn fuel_price_per_unit(self) -> i32 {
        match self {
            Self::Cozy => 2,
            Self::Normal => 4,
            Self::Insane => 6,
        }
    }

    pub(crate) fn exploration_discovery_width(self) -> f64 {
        match self {
            Self::Cozy => 8.0,
            Self::Normal => 6.0,
            Self::Insane => 4.0,
        }
    }

    pub(crate) fn exploration_glancing_width(self) -> f64 {
        match self {
            Self::Cozy => 15.0,
            Self::Normal => 11.0,
            Self::Insane => 8.0,
        }
    }
}

pub(crate) fn default_difficulty() -> Difficulty {
    Difficulty::Normal
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum RunOutcome {
    Won,
    Lost { reason: String },
}

impl RunOutcome {
    pub(crate) fn title(&self) -> &'static str {
        match self {
            Self::Won => "Shift Complete",
            Self::Lost { .. } => "Shift Lost",
        }
    }

    pub(crate) fn message(&self) -> &str {
        match self {
            Self::Won => "You charted the environment and hit the credit target.",
            Self::Lost { reason } => reason,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlertTarget {
    Contract(usize),
    Ship(usize),
    Location(usize),
    None,
}

pub(crate) struct Incident {
    pub(crate) summary: String,
    pub(crate) severity: AlertSeverity,
    pub(crate) target: AlertTarget,
}

#[derive(Clone, Copy)]
pub(crate) enum AppMode {
    Browse,
    SelectingDestination {
        ship_index: usize,
        intent: DispatchIntent,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DispatchIntent {
    Standard,
    Exploration,
}

impl DispatchIntent {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Standard => "Dispatch",
            Self::Exploration => "Exploration",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ContractState {
    Available,
    Accepted { accepted_at: u64 },
    Assigned { ship_index: usize, accepted_at: u64 },
    Completed,
    Failed,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum ContractArchetype {
    SurveyDrop,
    ReliefReturn,
    Medlift,
    CourierRun,
    DrydockRefit,
    RelayCalibration,
    OutboundCourier,
    ReturnFreight,
    PriorityRelay,
    FrontierSupply,
}

impl ContractArchetype {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::SurveyDrop => "Frontier Survey Drop",
            Self::ReliefReturn => "Harbor Relief Return",
            Self::Medlift => "Medlift to Dust",
            Self::CourierRun => "Kite Courier Run",
            Self::DrydockRefit => "Ion Drydock Refit",
            Self::RelayCalibration => "Relay Calibration Window",
            Self::OutboundCourier => "Outbound Courier Run",
            Self::ReturnFreight => "Return Freight Window",
            Self::PriorityRelay => "Priority Relay Transfer",
            Self::FrontierSupply => "Frontier Supply Sweep",
        }
    }

    pub(crate) fn briefing(self) -> &'static str {
        match self {
            Self::SurveyDrop => "Carry survey drones to Dust Harbor and expand the frontier.",
            Self::ReliefReturn => {
                "Bring relief crates back from Dust Harbor before local stores spoil."
            }
            Self::Medlift => "Rush medical pallets to Dust Harbor on the same shift.",
            Self::CourierRun => {
                "Open commercial traffic with Kite Station once the chart is confirmed."
            }
            Self::DrydockRefit => {
                "Deliver replacement coils to Ion Anchorage for a high-value refit."
            }
            Self::RelayCalibration => {
                "Reach Outer Ring Relay and stabilize the signal array before the window closes."
            }
            Self::OutboundCourier => {
                "Push fresh cargo out from Astra Prime to keep the lanes alive."
            }
            Self::ReturnFreight => {
                "Bring station cargo back to Astra Prime for payout and resupply."
            }
            Self::PriorityRelay => "Handle a time-sensitive handoff between charted stations.",
            Self::FrontierSupply => {
                "Move supplies along the frontier to keep expansion on schedule."
            }
        }
    }

    pub(crate) fn effect_summary(self) -> &'static str {
        match self {
            Self::SurveyDrop | Self::FrontierSupply => {
                "Completion boosts destination station fuel reserves."
            }
            Self::ReliefReturn | Self::ReturnFreight => {
                "Completion improves fuel reserves at Astra Prime."
            }
            Self::Medlift | Self::PriorityRelay => {
                "Completion grants a priority handling credit bonus."
            }
            Self::CourierRun | Self::OutboundCourier => {
                "Fast ships earn a courier speed bonus on completion."
            }
            Self::DrydockRefit => "Completion immediately restores the delivery ship to full hull.",
            Self::RelayCalibration => "Completion triggers a network-wide station fuel boost.",
        }
    }
}

#[derive(Clone)]
pub(crate) struct Contract {
    pub(crate) archetype: ContractArchetype,
    pub(crate) title: String,
    pub(crate) briefing: String,
    pub(crate) origin: usize,
    pub(crate) destination: usize,
    pub(crate) reward: i32,
    pub(crate) max_eta: u16,
    pub(crate) deadline: u64,
    pub(crate) unlock_location: usize,
    pub(crate) pending_llm_flavor: bool,
    pub(crate) state: ContractState,
}

impl Contract {
    pub(crate) fn new(
        archetype: ContractArchetype,
        origin: usize,
        destination: usize,
        reward: i32,
        max_eta: u16,
        deadline: u64,
        unlock_location: usize,
    ) -> Self {
        Self {
            archetype,
            title: archetype.title().to_string(),
            briefing: archetype.briefing().to_string(),
            origin,
            destination,
            reward,
            max_eta,
            deadline,
            unlock_location,
            pending_llm_flavor: true,
            state: ContractState::Available,
        }
    }
}

#[derive(Clone)]
pub(crate) struct WorldLocationFlavor {
    pub(crate) region_name: String,
    pub(crate) sector_name: String,
    pub(crate) name: String,
    pub(crate) short_label: String,
    pub(crate) lane_name: String,
    pub(crate) description: String,
    pub(crate) cluster_name: String,
    pub(crate) system_name: String,
}

#[derive(Clone)]
pub(crate) struct WorldShipFlavor {
    pub(crate) name: String,
    pub(crate) class_name: String,
    pub(crate) description: String,
}

#[derive(Clone)]
pub(crate) struct WorldFlavor {
    pub(crate) environment_name: String,
    pub(crate) environment_summary: String,
    pub(crate) locations: Vec<WorldLocationFlavor>,
    pub(crate) starter_ships: Vec<WorldShipFlavor>,
    pub(crate) shipyard_offers: Vec<WorldShipFlavor>,
}

#[derive(Clone, Copy)]
pub(crate) struct MapPoint {
    pub(crate) x: i16,
    pub(crate) y: i16,
}

impl MapPoint {
    pub(crate) fn as_tuple(self) -> (f64, f64) {
        (f64::from(self.x), f64::from(self.y))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplorationTraceOutcome {
    Discovery,
    Miss,
    Empty,
}

#[derive(Clone, Copy)]
pub(crate) struct ExplorationTrace {
    pub(crate) origin: usize,
    pub(crate) target: MapPoint,
    pub(crate) outcome: ExplorationTraceOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MapZoom {
    Region,
    Sector,
    Cluster,
    System,
}

impl MapZoom {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Region => "Region",
            Self::Sector => "Sector",
            Self::Cluster => "Cluster",
            Self::System => "System",
        }
    }

    pub(crate) fn zoom_in(self) -> Self {
        match self {
            Self::Region => Self::Sector,
            Self::Sector => Self::Cluster,
            Self::Cluster => Self::System,
            Self::System => Self::System,
        }
    }

    pub(crate) fn zoom_out(self) -> Self {
        match self {
            Self::Region => Self::Region,
            Self::Sector => Self::Region,
            Self::Cluster => Self::Sector,
            Self::System => Self::Cluster,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Location {
    pub(crate) region_name: String,
    pub(crate) sector_name: String,
    pub(crate) name: String,
    pub(crate) short_label: String,
    pub(crate) lane_name: String,
    pub(crate) description: String,
    pub(crate) cluster_name: String,
    pub(crate) system_name: String,
    pub(crate) region_coords: MapPoint,
    pub(crate) sector_coords: MapPoint,
    pub(crate) cluster_coords: MapPoint,
    pub(crate) system_coords: MapPoint,
    pub(crate) travel_time_from_hub: u16,
    pub(crate) reveal_on_arrival: Option<usize>,
    pub(crate) exploration_attempts: u8,
    pub(crate) exploration_exhausted: bool,
    pub(crate) charted_empty: bool,
}

#[derive(Clone)]
pub(crate) enum ShipState {
    Docked,
    Repairing {
        ticks_remaining: u16,
    },
    EnRoute {
        origin: usize,
        destination: usize,
        eta_remaining: u16,
        total_eta: u16,
        exploration_run: bool,
        exploration_target: Option<MapPoint>,
        exploration_discoveries: Vec<usize>,
        exploration_revealed_count: usize,
        exploration_outcome: Option<ExplorationTraceOutcome>,
        segments: Vec<(usize, usize)>,
        segment_costs: Vec<u16>,
        route: String,
        condition_summary: String,
        assigned_contract: Option<usize>,
        repair_on_arrival: u16,
    },
}

#[derive(Clone)]
pub(crate) struct Ship {
    pub(crate) name: String,
    pub(crate) class_name: String,
    pub(crate) description: String,
    pub(crate) current_location: usize,
    pub(crate) current_fuel: u16,
    pub(crate) max_fuel: u16,
    pub(crate) speed: u16,
    pub(crate) hull: u16,
    pub(crate) low_fuel_alerted: bool,
    pub(crate) state: ShipState,
}

impl Ship {
    pub(crate) fn docked(
        name: impl Into<String>,
        class_name: impl Into<String>,
        description: impl Into<String>,
        current_location: usize,
        current_fuel: u16,
        max_fuel: u16,
        speed: u16,
    ) -> Self {
        Self {
            name: name.into(),
            class_name: class_name.into(),
            description: description.into(),
            current_location,
            current_fuel,
            max_fuel,
            speed,
            hull: 100,
            low_fuel_alerted: false,
            state: ShipState::Docked,
        }
    }

    pub(crate) fn is_docked(&self) -> bool {
        matches!(&self.state, ShipState::Docked)
    }

    pub(crate) fn map_tag(&self) -> String {
        self.name
            .split_whitespace()
            .last()
            .unwrap_or(self.name.as_str())
            .chars()
            .take(2)
            .collect::<String>()
            .to_uppercase()
    }
}

#[derive(Clone)]
pub(crate) struct ShipShopOffer {
    pub(crate) name: String,
    pub(crate) class_name: String,
    pub(crate) description: String,
    pub(crate) speed: u16,
    pub(crate) max_fuel: u16,
    pub(crate) price: i32,
}

impl ShipShopOffer {
    pub(crate) fn to_ship(&self, location: usize) -> Ship {
        Ship::docked(
            self.name.clone(),
            self.class_name.clone(),
            self.description.clone(),
            location,
            self.max_fuel,
            self.max_fuel,
            self.speed,
        )
    }
}

#[derive(Clone)]
pub(crate) struct ShipShop {
    pub(crate) offers: Vec<ShipShopOffer>,
    pub(crate) last_refresh: u64,
}

pub(crate) enum UpgradeKind {
    Engine,
    FuelTank,
}

pub(crate) struct UpgradeOffer {
    pub(crate) kind: UpgradeKind,
    pub(crate) cost: i32,
    pub(crate) description: String,
}

#[derive(Clone, Copy)]
pub(crate) enum TransitPhase {
    Undocking,
    Cruising,
    Approach,
}

impl TransitPhase {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Undocking => "Undocking",
            Self::Cruising => "Cruising",
            Self::Approach => "Approach",
        }
    }
}

pub(crate) fn transit_phase(eta_remaining: u16, total_eta: u16) -> TransitPhase {
    if eta_remaining == total_eta {
        TransitPhase::Undocking
    } else if eta_remaining <= 1 {
        TransitPhase::Approach
    } else {
        TransitPhase::Cruising
    }
}

pub(crate) struct RoutePlan {
    pub(crate) path: String,
    pub(crate) segments: Vec<(usize, usize)>,
    pub(crate) segment_costs: Vec<u16>,
    pub(crate) eta: u16,
    pub(crate) fuel_required: u16,
    pub(crate) condition_summary: String,
}

pub(crate) enum RefuelPlan {
    NotNeeded,
    CanPurchase {
        units: u16,
        cost: i32,
    },
    EmergencyReserve {
        purchased_units: u16,
        reserve_units: u16,
        cost: i32,
    },
    NeedTransfer {
        units: u16,
    },
    ExceedsTank,
    BlockedByCredits {
        cost: i32,
    },
    BlockedByStation {
        units: u16,
    },
}

#[derive(Clone, Copy)]
pub(crate) enum LaneCondition {
    Clear,
    Traffic,
    Debris,
    Solar,
}

impl LaneCondition {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Clear => "clear lanes",
            Self::Traffic => "traffic delay (+1)",
            Self::Debris => "debris interference (+2)",
            Self::Solar => "solar static (+3)",
        }
    }

    pub(crate) fn penalty(self) -> u16 {
        match self {
            Self::Clear => 0,
            Self::Traffic => 1,
            Self::Debris => 2,
            Self::Solar => 3,
        }
    }

    pub(crate) fn report_phrase(self) -> &'static str {
        match self {
            Self::Clear => "clear lanes",
            Self::Traffic => "heavy traffic",
            Self::Debris => "debris interference",
            Self::Solar => "solar static bursts",
        }
    }
}
