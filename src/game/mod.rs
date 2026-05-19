mod contracts;
mod fuel;
mod outcome;
mod routing;
mod ships;
mod types;

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
    pub(crate) selected_alert: usize,
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
            selected_alert: 0,
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
                    0,
                ),
                Ship::docked("HMV Orpheus", ASTRA_PRIME, 13, 18, 3),
            ],
            contracts: vec![
                Contract::new(
                    ContractArchetype::SurveyDrop,
                    ASTRA_PRIME,
                    DUST_HARBOR,
                    160,
                    6,
                    28,
                    DUST_HARBOR,
                ),
                Contract::new(
                    ContractArchetype::ReliefReturn,
                    DUST_HARBOR,
                    ASTRA_PRIME,
                    140,
                    6,
                    34,
                    DUST_HARBOR,
                ),
                Contract::new(
                    ContractArchetype::Medlift,
                    ASTRA_PRIME,
                    DUST_HARBOR,
                    190,
                    4,
                    24,
                    DUST_HARBOR,
                ),
                Contract::new(
                    ContractArchetype::CourierRun,
                    ASTRA_PRIME,
                    KITE_STATION,
                    230,
                    3,
                    46,
                    KITE_STATION,
                ),
                Contract::new(
                    ContractArchetype::DrydockRefit,
                    ASTRA_PRIME,
                    ION_ANCHORAGE,
                    300,
                    4,
                    60,
                    ION_ANCHORAGE,
                ),
                Contract::new(
                    ContractArchetype::RelayCalibration,
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
            LOG_PANE => {
                let alert_count = self.current_alerts().len().max(1);
                self.selected_alert = wrap_index(self.selected_alert, alert_count, delta);
            }
            _ => {}
        }
    }

    pub(crate) fn activate_selection(&mut self) {
        match self.mode {
            AppMode::Browse => match self.active_pane {
                CONTRACTS_PANE => self.toggle_contract_tracking(),
                FLEET_PANE => self.begin_dispatch(),
                LOG_PANE => self.focus_selected_alert(),
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
        let mut travel_logs = Vec::new();
        let location_names: Vec<&'static str> = self
            .locations
            .iter()
            .map(|location| location.name)
            .collect();

        for (ship_index, ship) in self.fleet.iter_mut().enumerate() {
            if let ShipState::Repairing { ticks_remaining } = &mut ship.state {
                if *ticks_remaining > 0 {
                    *ticks_remaining -= 1;
                }
                if *ticks_remaining == 0 {
                    ship.hull = 100;
                    ship.state = ShipState::Docked;
                    let message = format!(
                        "[{clock:04}] {name} completed port repairs and is flight-ready again.",
                        clock = self.clock,
                        name = ship.name,
                    );
                    travel_logs.push(message.clone());
                }
                continue;
            }

            if let ShipState::EnRoute {
                destination,
                eta_remaining,
                total_eta,
                assigned_contract,
                repair_on_arrival,
                ..
            } = &mut ship.state
            {
                let destination_index = *destination;
                let contract_assignment = *assigned_contract;
                let mut event_happened = false;

                if let Some(travel_event) = travel_event_at(
                    self.clock,
                    ship_index,
                    destination_index,
                    *eta_remaining,
                    self.difficulty,
                ) {
                    event_happened = true;
                    match travel_event {
                        TravelEvent::Tailwind => {
                            if *eta_remaining > 1 {
                                *eta_remaining = eta_remaining.saturating_sub(1);
                                *total_eta = total_eta.saturating_sub(1).max(*eta_remaining);
                                let message = format!(
                                    "[{clock:04}] Tailwind burst: {name} gains a lane on the run to {destination}.",
                                    clock = self.clock,
                                    name = ship.name,
                                    destination = location_names[destination_index],
                                );
                                travel_logs.push(message);
                            }
                        }
                        TravelEvent::Delay => {
                            *eta_remaining = eta_remaining.saturating_add(1);
                            *total_eta = total_eta.saturating_add(1);
                            let message = format!(
                                "[{clock:04}] Lane congestion: {name} loses time on the way to {destination}.",
                                clock = self.clock,
                                name = ship.name,
                                destination = location_names[destination_index],
                            );
                            travel_logs.push(message);
                        }
                        TravelEvent::Damage => {
                            *eta_remaining = eta_remaining.saturating_add(2);
                            *total_eta = total_eta.saturating_add(2);
                            *repair_on_arrival = repair_on_arrival.saturating_add(2);
                            ship.hull = ship.hull.saturating_sub(25).max(10);
                            let message = format!(
                                "[{clock:04}] Micrometeor strike: {name} is damaged and will need repairs after reaching {destination}.",
                                clock = self.clock,
                                name = ship.name,
                                destination = location_names[destination_index],
                            );
                            travel_logs.push(message.clone());
                        }
                    }
                }

                if *eta_remaining > 0 {
                    *eta_remaining -= 1;
                }

                if *eta_remaining == 0 {
                    ship.current_location = destination_index;
                    let repair_ticks = *repair_on_arrival;
                    ship.state = if repair_ticks > 0 {
                        ShipState::Repairing {
                            ticks_remaining: repair_ticks,
                        }
                    } else {
                        ShipState::Docked
                    };
                    arrivals.push((
                        ship_index,
                        ship.name,
                        destination_index,
                        contract_assignment,
                        repair_ticks,
                    ));
                } else if !event_happened
                    && self.clock % 48 == 0
                    && *eta_remaining == *total_eta / 2
                {
                    travel_logs.push(format!(
                        "[{clock:04}] {name} reports steady travel toward {destination}.",
                        clock = self.clock,
                        name = ship.name,
                        destination = location_names[destination_index],
                    ));
                }
            }
        }

        for entry in travel_logs {
            self.push_log(entry);
        }

        let had_arrivals = !arrivals.is_empty();
        for (ship_index, ship_name, destination_index, contract_assignment, repair_ticks) in
            arrivals
        {
            self.push_log(format!(
                "[{clock:04}] {name} arrived at {destination}.",
                clock = self.clock,
                name = ship_name,
                destination = self.location_name(destination_index),
            ));

            if repair_ticks > 0 {
                self.push_log(format!(
                    "[{clock:04}] {name} enters port repairs for {} ticks.",
                    repair_ticks,
                    clock = self.clock,
                    name = ship_name,
                ));
            }

            self.sync_low_fuel_alert(ship_index);

            if let Some(contract_index) = contract_assignment {
                self.resolve_contract_arrival(
                    contract_index,
                    ship_index,
                    ship_name,
                    destination_index,
                );
            }

            if let Some(discovery) = self.reveal_from_arrival(destination_index) {
                self.push_log(discovery);
            }
        }

        self.update_contract_deadlines();
        self.restock_station_fuel();

        if self.clock % 32 == 0 && !had_arrivals {
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

    pub(crate) fn current_alerts(&self) -> Vec<Incident> {
        let mut alerts = Vec::new();

        for (ship_index, ship) in self.fleet.iter().enumerate() {
            match ship.state {
                ShipState::Repairing { ticks_remaining } => alerts.push(Incident {
                    summary: format!(
                        "{} repairing: {} ticks remaining",
                        ship.name, ticks_remaining
                    ),
                    severity: AlertSeverity::Warning,
                    target: AlertTarget::Ship(ship_index),
                }),
                ShipState::EnRoute {
                    repair_on_arrival, ..
                } if repair_on_arrival > 0 => alerts.push(Incident {
                    summary: format!(
                        "{} damaged in transit; repairs queued on arrival",
                        ship.name
                    ),
                    severity: AlertSeverity::Warning,
                    target: AlertTarget::Ship(ship_index),
                }),
                _ => {}
            }

            if ship.current_fuel <= Self::fuel_alert_threshold(ship) {
                alerts.push(Incident {
                    summary: format!(
                        "{} low fuel: {}/{}",
                        ship.name, ship.current_fuel, ship.max_fuel
                    ),
                    severity: if ship.current_fuel == 0 {
                        AlertSeverity::Critical
                    } else {
                        AlertSeverity::Warning
                    },
                    target: AlertTarget::Ship(ship_index),
                });
            }
        }

        if let Some(contract_index) = self.tracked_contract {
            alerts.push(Incident {
                summary: format!("Tracked contract: {}", self.contracts[contract_index].title),
                severity: AlertSeverity::Info,
                target: AlertTarget::Contract(contract_index),
            });
        }

        for (location_index, fuel) in self.station_fuel.iter().enumerate() {
            if self.is_discovered(location_index) && *fuel <= 4 {
                alerts.push(Incident {
                    summary: format!(
                        "{} low fuel stock: {} units",
                        self.location_name(location_index),
                        fuel
                    ),
                    severity: AlertSeverity::Warning,
                    target: AlertTarget::Location(location_index),
                });
            }
        }

        if self.difficulty.uses_fuel_economy() && self.credits <= 20 {
            alerts.push(Incident {
                summary: format!(
                    "Low credits: {} cr available for fuel and repairs",
                    self.credits
                ),
                severity: AlertSeverity::Warning,
                target: AlertTarget::None,
            });
        }

        if let Some(outcome) = &self.run_outcome {
            alerts.push(Incident {
                summary: outcome.message().to_string(),
                severity: if matches!(outcome, RunOutcome::Won) {
                    AlertSeverity::Info
                } else {
                    AlertSeverity::Critical
                },
                target: AlertTarget::None,
            });
        }

        if alerts.is_empty() {
            alerts.push(Incident {
                summary: "No urgent incidents. Keep the lanes moving.".to_string(),
                severity: AlertSeverity::Info,
                target: AlertTarget::None,
            });
        }

        alerts
    }

    pub(crate) fn focus_selected_alert(&mut self) {
        let alerts = self.current_alerts();
        if alerts.is_empty() {
            return;
        }

        match alerts[self.selected_alert.min(alerts.len() - 1)].target {
            AlertTarget::Contract(contract_index) => {
                self.active_pane = CONTRACTS_PANE;
                self.selected_contract = contract_index.min(self.contracts.len() - 1);
            }
            AlertTarget::Ship(ship_index) => {
                self.active_pane = FLEET_PANE;
                self.selected_ship = ship_index.min(self.fleet.len() - 1);
            }
            AlertTarget::Location(location_index) => {
                self.active_pane = MAP_PANE;
                self.selected_location = location_index.min(self.locations.len() - 1);
            }
            AlertTarget::None => {}
        }
    }

    pub(crate) fn push_log(&mut self, entry: String) {
        self.log.insert(0, entry);
        self.log.truncate(8);
    }

    fn restock_station_fuel(&mut self) {
        if self.clock == 0 || !self.clock.is_multiple_of(20) {
            return;
        }

        for index in 0..self.station_fuel.len() {
            let delta = 3 + ((self.clock / 20 + index as u64) % 4) as u16;
            self.station_fuel[index] = self.station_fuel[index].saturating_add(delta).min(60);
        }
        self.push_log(format!(
            "[{clock:04}] Fuel convoys topped up station reserves across the lanes.",
            clock = self.clock,
        ));
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

enum TravelEvent {
    Tailwind,
    Delay,
    Damage,
}

fn travel_event_at(
    clock: u64,
    ship_index: usize,
    destination_index: usize,
    eta_remaining: u16,
    difficulty: Difficulty,
) -> Option<TravelEvent> {
    if eta_remaining <= 1 {
        return None;
    }

    let seed = clock
        .wrapping_mul(31)
        .wrapping_add((ship_index as u64 + 1) * 17)
        .wrapping_add((destination_index as u64 + 1) * 13)
        .wrapping_add(u64::from(eta_remaining));

    match difficulty {
        Difficulty::Cozy => match seed % 19 {
            0 => Some(TravelEvent::Tailwind),
            1 => Some(TravelEvent::Delay),
            _ => None,
        },
        Difficulty::Normal => match seed % 17 {
            0 => Some(TravelEvent::Tailwind),
            1 | 2 => Some(TravelEvent::Delay),
            3 => Some(TravelEvent::Damage),
            _ => None,
        },
        Difficulty::Insane => match seed % 13 {
            0 => Some(TravelEvent::Tailwind),
            1 | 2 | 3 => Some(TravelEvent::Delay),
            4 | 5 => Some(TravelEvent::Damage),
            _ => None,
        },
    }
}
