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

pub(crate) struct Contract {
    pub(crate) title: &'static str,
    pub(crate) briefing: &'static str,
    pub(crate) origin: usize,
    pub(crate) destination: usize,
    pub(crate) reward: i32,
    pub(crate) max_eta: u16,
    pub(crate) deadline: u64,
    pub(crate) unlock_location: usize,
    pub(crate) state: ContractState,
}

impl Contract {
    pub(crate) fn new(
        title: &'static str,
        briefing: &'static str,
        origin: usize,
        destination: usize,
        reward: i32,
        max_eta: u16,
        deadline: u64,
        unlock_location: usize,
    ) -> Self {
        Self {
            title,
            briefing,
            origin,
            destination,
            reward,
            max_eta,
            deadline,
            unlock_location,
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

pub(crate) struct Ship {
    pub(crate) name: &'static str,
    pub(crate) current_location: usize,
    pub(crate) current_fuel: u16,
    pub(crate) max_fuel: u16,
    pub(crate) speed: u16,
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
    ) -> Self {
        Self {
            name,
            current_location: origin,
            current_fuel,
            max_fuel,
            speed,
            low_fuel_alerted: false,
            state: ShipState::EnRoute {
                origin,
                destination,
                eta_remaining,
                total_eta,
                route: route.to_string(),
                condition_summary: condition_summary.to_string(),
                assigned_contract,
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

    pub(crate) fn status_line(
        self,
        origin: &'static str,
        destination: &'static str,
        eta_remaining: u16,
    ) -> String {
        match self {
            Self::Undocking => format!("Undocking from {} | ETA {}", origin, eta_remaining),
            Self::Cruising => format!("Cruising to {} | ETA {}", destination, eta_remaining),
            Self::Approach => format!("Approaching {} | ETA {}", destination, eta_remaining),
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

pub(super) enum ShipEvent {
    Departed {
        name: &'static str,
        origin: usize,
        destination: usize,
    },
    Approaching {
        name: &'static str,
        destination: usize,
    },
}
