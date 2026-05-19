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
            Self::Cozy => "No reward decay and no contract failure pressure.",
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
        !matches!(self, Self::Cozy)
    }

    pub(crate) fn fuel_price_per_unit(self) -> i32 {
        match self {
            Self::Cozy => 0,
            Self::Normal => 4,
            Self::Insane => 6,
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
            Self::Won => "You charted the sector and hit the credit target.",
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
    SelectingDestination { ship_index: usize },
}

#[derive(Clone, Copy)]
pub(crate) enum ContractState {
    Available,
    Accepted {
        accepted_at: u64,
    },
    Assigned {
        ship_name: &'static str,
        accepted_at: u64,
    },
    Completed,
    Failed,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
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

pub(crate) struct Location {
    pub(crate) name: &'static str,
    pub(crate) lane_name: &'static str,
    pub(crate) travel_time_from_hub: u16,
    pub(crate) reveal_on_arrival: Option<usize>,
}

impl Location {
    pub(crate) fn hub(name: &'static str) -> Self {
        Self {
            name,
            lane_name: "Central Exchange",
            travel_time_from_hub: 0,
            reveal_on_arrival: None,
        }
    }

    pub(crate) fn new(
        name: &'static str,
        lane_name: &'static str,
        travel_time_from_hub: u16,
        reveal_on_arrival: Option<usize>,
    ) -> Self {
        Self {
            name,
            lane_name,
            travel_time_from_hub,
            reveal_on_arrival,
        }
    }
}

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
        route: String,
        condition_summary: String,
        assigned_contract: Option<usize>,
        repair_on_arrival: u16,
    },
}

pub(crate) struct Ship {
    pub(crate) name: &'static str,
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
        name: &'static str,
        current_location: usize,
        current_fuel: u16,
        max_fuel: u16,
        speed: u16,
    ) -> Self {
        Self {
            name,
            current_location,
            current_fuel,
            max_fuel,
            speed,
            hull: 100,
            low_fuel_alerted: false,
            state: ShipState::Docked,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn en_route(
        name: &'static str,
        origin: usize,
        destination: usize,
        eta_remaining: u16,
        total_eta: u16,
        route: &'static str,
        condition_summary: &'static str,
        assigned_contract: Option<usize>,
        current_fuel: u16,
        max_fuel: u16,
        speed: u16,
        repair_on_arrival: u16,
    ) -> Self {
        Self {
            name,
            current_location: origin,
            current_fuel,
            max_fuel,
            speed,
            hull: 100,
            low_fuel_alerted: false,
            state: ShipState::EnRoute {
                origin,
                destination,
                eta_remaining,
                total_eta,
                route: route.to_string(),
                condition_summary: condition_summary.to_string(),
                assigned_contract,
                repair_on_arrival,
            },
        }
    }

    pub(crate) fn is_docked(&self) -> bool {
        matches!(self.state, ShipState::Docked)
    }

    pub(crate) fn map_tag(&self) -> String {
        self.name
            .split_whitespace()
            .last()
            .unwrap_or(self.name)
            .chars()
            .take(2)
            .collect::<String>()
            .to_uppercase()
    }
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
    pub(crate) eta: u16,
    pub(crate) fuel_required: u16,
    pub(crate) condition_summary: String,
}

pub(crate) enum RefuelPlan {
    NotNeeded,
    CanPurchase { units: u16, cost: i32 },
    NeedTransfer { units: u16 },
    ExceedsTank,
    BlockedByCredits { cost: i32 },
    BlockedByStation { units: u16 },
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
