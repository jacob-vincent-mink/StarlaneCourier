mod contracts;
mod fuel;
mod outcome;
mod routing;
mod types;

use self::types::ShipEvent;

pub(crate) use self::types::*;

pub(crate) const PANE_TITLES: [&str; 4] = ["Mission Board", "Sector Map", "Fleet", "Event Log"];
pub(crate) const PANE_COUNT: usize = PANE_TITLES.len();
pub(crate) const CONTRACTS_PANE: usize = 0;
pub(crate) const MAP_PANE: usize = 1;
pub(crate) const FLEET_PANE: usize = 2;
pub(crate) const LOG_PANE: usize = 3;

pub(crate) const ASTRA_PRIME: usize = 0;
pub(crate) const KITE_STATION: usize = 1;
pub(crate) const ION_ANCHORAGE: usize = 2;
pub(crate) const DUST_HARBOR: usize = 3;
pub(crate) const OUTER_RING_RELAY: usize = 4;

pub(crate) const GOAL_CREDITS: i32 = 600;

pub(crate) struct GameData {
    pub(crate) difficulty: Difficulty,
    pub(crate) run_outcome: Option<RunOutcome>,
    pub(crate) active_pane: usize,
    pub(crate) clock: u64,
    pub(crate) mode: AppMode,
    pub(crate) selected_location: usize,
    pub(crate) selected_ship: usize,
    pub(crate) selected_contract: usize,
    pub(crate) tracked_contract: Option<usize>,
    pub(crate) credits: i32,
    pub(crate) locations: Vec<Location>,
    pub(crate) discovered_locations: Vec<bool>,
    pub(crate) station_fuel: Vec<u16>,
    pub(crate) fleet: Vec<Ship>,
    pub(crate) contracts: Vec<Contract>,
    pub(crate) log: Vec<String>,
}

impl GameData {
    pub(crate) fn new(difficulty: Difficulty) -> Self {
        Self {
            difficulty,
            run_outcome: None,
            active_pane: CONTRACTS_PANE,
            clock: 0,
            mode: AppMode::Browse,
            selected_location: DUST_HARBOR,
            selected_ship: 0,
            selected_contract: 0,
            tracked_contract: None,
            credits: 120,
            locations: vec![
                Location::hub("Astra Prime"),
                Location::new("Kite Station", "Kite Spur", 4, Some(ION_ANCHORAGE)),
                Location::new("Ion Anchorage", "Ion Run", 5, Some(OUTER_RING_RELAY)),
                Location::new("Dust Harbor", "Dust Corridor", 6, Some(KITE_STATION)),
                Location::new("Outer Ring Relay", "Relay Ascent", 7, None),
            ],
            discovered_locations: vec![true, false, false, true, false],
            station_fuel: vec![48, 20, 18, 22, 10],
            fleet: vec![
                Ship::docked("SV Kestrel", ASTRA_PRIME, 9, 14, 2),
                Ship::en_route(
                    "CSV Lantern",
                    ASTRA_PRIME,
                    DUST_HARBOR,
                    7,
                    7,
                    "Astra Prime -> Dust Harbor",
                    "Dust Corridor: debris interference",
                    None,
                    2,
                    16,
                    1,
                ),
                Ship::docked("HMV Orpheus", ASTRA_PRIME, 13, 18, 3),
            ],
            contracts: vec![
                Contract::new(
                    "Frontier Survey Drop",
                    "Carry survey drones to Dust Harbor and expand the frontier.",
                    ASTRA_PRIME,
                    DUST_HARBOR,
                    160,
                    6,
                    28,
                    DUST_HARBOR,
                ),
                Contract::new(
                    "Harbor Relief Return",
                    "Bring relief crates back from Dust Harbor before local stores spoil.",
                    DUST_HARBOR,
                    ASTRA_PRIME,
                    140,
                    6,
                    34,
                    DUST_HARBOR,
                ),
                Contract::new(
                    "Medlift to Dust",
                    "Rush medical pallets to Dust Harbor on the same shift.",
                    ASTRA_PRIME,
                    DUST_HARBOR,
                    190,
                    4,
                    24,
                    DUST_HARBOR,
                ),
                Contract::new(
                    "Kite Courier Run",
                    "Open commercial traffic with Kite Station once the chart is confirmed.",
                    ASTRA_PRIME,
                    KITE_STATION,
                    230,
                    3,
                    46,
                    KITE_STATION,
                ),
                Contract::new(
                    "Ion Drydock Refit",
                    "Deliver replacement coils to Ion Anchorage for a high-value refit.",
                    ASTRA_PRIME,
                    ION_ANCHORAGE,
                    300,
                    4,
                    60,
                    ION_ANCHORAGE,
                ),
                Contract::new(
                    "Relay Calibration Window",
                    "Reach Outer Ring Relay and stabilize the signal array before the window closes.",
                    ION_ANCHORAGE,
                    OUTER_RING_RELAY,
                    420,
                    4,
                    82,
                    OUTER_RING_RELAY,
                ),
            ],
            log: vec![
                "[0000] Shift started. Dispatch board synced.".into(),
                "[0000] Primary objective: chart the sector and build credits through contracts."
                    .into(),
                "[0000] Accept a contract from the Mission Board, then assign a ship to the route."
                    .into(),
                "[0000] Frontier arrivals unlock new charts deeper in the map.".into(),
            ],
        }
    }

    pub(crate) fn mode_label(&self) -> String {
        match self.mode {
            AppMode::Browse => "Browse".to_string(),
            AppMode::SelectingDestination { ship_index } => {
                format!("Route Planning {}", self.fleet[ship_index].name)
            }
        }
    }

    pub(crate) fn move_selection(&mut self, delta: isize) {
        match self.active_pane {
            CONTRACTS_PANE => {
                self.selected_contract =
                    wrap_index(self.selected_contract, self.contracts.len(), delta);
            }
            MAP_PANE => {
                self.selected_location =
                    self.next_discovered_location(self.selected_location, delta);
            }
            FLEET_PANE => {
                self.selected_ship = wrap_index(self.selected_ship, self.fleet.len(), delta);
            }
            _ => {}
        }
    }

    pub(crate) fn activate_selection(&mut self) {
        match self.mode {
            AppMode::Browse => match self.active_pane {
                CONTRACTS_PANE => self.toggle_contract_tracking(),
                FLEET_PANE => self.begin_dispatch(),
                _ => {}
            },
            AppMode::SelectingDestination { .. } => {
                if self.active_pane == MAP_PANE {
                    self.confirm_dispatch();
                }
            }
        }
    }

    pub(crate) fn tick(&mut self) {
        self.clock += 1;
        let mut arrivals = Vec::new();
        let mut events = Vec::new();

        for (ship_index, ship) in self.fleet.iter_mut().enumerate() {
            if let ShipState::EnRoute {
                destination,
                eta_remaining,
                total_eta,
                assigned_contract,
                ..
            } = &mut ship.state
            {
                let destination_index = *destination;
                let contract_assignment = *assigned_contract;

                if *eta_remaining == *total_eta {
                    events.push(ShipEvent::Departed {
                        name: ship.name,
                        origin: ship.current_location,
                        destination: destination_index,
                    });
                }

                if *eta_remaining > 0 {
                    *eta_remaining -= 1;
                }

                if *eta_remaining == 1 {
                    events.push(ShipEvent::Approaching {
                        name: ship.name,
                        destination: destination_index,
                    });
                }

                if *eta_remaining == 0 {
                    ship.current_location = destination_index;
                    ship.state = ShipState::Docked;
                    arrivals.push((
                        ship_index,
                        ship.name,
                        destination_index,
                        contract_assignment,
                    ));
                }
            }
        }

        for event in events {
            match event {
                ShipEvent::Departed {
                    name,
                    origin,
                    destination,
                } => self.push_log(format!(
                    "[{clock:04}] {name} cleared {origin} and is outbound for {destination}.",
                    clock = self.clock,
                    name = name,
                    origin = self.location_name(origin),
                    destination = self.location_name(destination),
                )),
                ShipEvent::Approaching { name, destination } => self.push_log(format!(
                    "[{clock:04}] {name} is on final approach to {destination}.",
                    clock = self.clock,
                    name = name,
                    destination = self.location_name(destination),
                )),
            }
        }

        for (ship_index, ship_name, destination_index, contract_assignment) in arrivals {
            self.push_log(format!(
                "[{clock:04}] {name} arrived at {destination}.",
                clock = self.clock,
                name = ship_name,
                destination = self.location_name(destination_index),
            ));

            self.sync_low_fuel_alert(ship_index);

            if let Some(contract_index) = contract_assignment {
                self.resolve_contract_arrival(contract_index, ship_name, destination_index);
            }

            if let Some(discovery) = self.reveal_from_arrival(destination_index) {
                self.push_log(discovery);
            }
        }

        self.update_contract_deadlines();

        if self.clock % 16 == 0 {
            self.push_log(self.ambient_event());
        }

        self.evaluate_run_outcome();
    }

    pub(crate) fn low_fuel_ship_names(&self) -> Vec<&'static str> {
        self.fleet
            .iter()
            .filter(|ship| ship.current_fuel <= Self::fuel_alert_threshold(ship))
            .map(|ship| ship.name)
            .collect()
    }

    pub(crate) fn push_log(&mut self, entry: String) {
        self.log.insert(0, entry);
        self.log.truncate(8);
    }
}

pub(crate) fn wrap_index(current: usize, len: usize, delta: isize) -> usize {
    ((current as isize + delta).rem_euclid(len as isize)) as usize
}

pub(crate) fn ceil_div_u16(value: u16, divisor: u16) -> u16 {
    if divisor == 0 {
        return value;
    }

    value.div_ceil(divisor)
}
