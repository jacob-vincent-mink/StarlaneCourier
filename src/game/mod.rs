mod contracts;
mod fuel;
mod outcome;
mod routing;
mod ships;
mod types;

pub(crate) use self::types::*;

pub(crate) const PANE_TITLES: [&str; 5] = [
    "Mission Board",
    "Sector Map",
    "Fleet",
    "Shipyards",
    "Event Log",
];
pub(crate) const PANE_COUNT: usize = PANE_TITLES.len();
pub(crate) const CONTRACTS_PANE: usize = 0;
pub(crate) const MAP_PANE: usize = 1;
pub(crate) const FLEET_PANE: usize = 2;
pub(crate) const SHIPYARD_PANE: usize = 3;
pub(crate) const LOG_PANE: usize = 4;

pub(crate) const ASTRA_PRIME: usize = 0;
pub(crate) const KITE_STATION: usize = 1;
pub(crate) const ION_ANCHORAGE: usize = 2;
pub(crate) const DUST_HARBOR: usize = 3;
pub(crate) const OUTER_RING_RELAY: usize = 4;
pub(crate) const SECTOR_LOCATION_COUNT: usize = 5;

pub(crate) const GOAL_CREDITS: i32 = 600;

pub(crate) struct GameData {
    pub(crate) difficulty: Difficulty,
    pub(crate) run_outcome: Option<RunOutcome>,
    pub(crate) world_seed: u64,
    pub(crate) sector_name: String,
    pub(crate) sector_summary: String,
    pub(crate) active_pane: usize,
    pub(crate) clock: u64,
    pub(crate) mode: AppMode,
    pub(crate) map_zoom: MapZoom,
    pub(crate) map_focus_location: usize,
    pub(crate) player_location: usize,
    pub(crate) player_in_transit_ship: Option<usize>,
    pub(crate) selected_alert: usize,
    pub(crate) selected_location: usize,
    pub(crate) selected_ship: usize,
    pub(crate) selected_shipyard_offer: usize,
    pub(crate) selected_contract: usize,
    pub(crate) tracked_contract: Option<usize>,
    pub(crate) credits: i32,
    pub(crate) locations: Vec<Location>,
    pub(crate) discovered_locations: Vec<bool>,
    pub(crate) station_fuel: Vec<u16>,
    pub(crate) station_ship_shops: Vec<Option<ShipShop>>,
    pub(crate) fleet: Vec<Ship>,
    pub(crate) contracts: Vec<Contract>,
    pub(crate) log: Vec<String>,
    pub(crate) action_feedback: Option<String>,
    pub(crate) mission_history: Vec<String>,
}

impl GameData {
    pub(crate) fn new(difficulty: Difficulty) -> Self {
        Self::new_seeded(difficulty, 0, None)
    }

    pub(crate) fn new_seeded(
        difficulty: Difficulty,
        world_seed: u64,
        world_flavor: Option<WorldFlavor>,
    ) -> Self {
        let world_flavor_ref = world_flavor.as_ref();
        let (sector_name, sector_summary, locations) =
            generated_locations(world_seed, world_flavor_ref);
        let location_count = locations.len();
        let station_ship_shops = ships::starting_ship_shops(locations.len(), world_flavor_ref);
        let fleet = ships::starting_fleet(world_flavor_ref);
        Self {
            difficulty,
            run_outcome: None,
            world_seed,
            sector_name,
            sector_summary,
            active_pane: CONTRACTS_PANE,
            clock: 0,
            mode: AppMode::Browse,
            map_zoom: MapZoom::Sector,
            map_focus_location: DUST_HARBOR,
            player_location: ASTRA_PRIME,
            player_in_transit_ship: None,
            selected_alert: 0,
            selected_location: DUST_HARBOR,
            selected_ship: 0,
            selected_shipyard_offer: 0,
            selected_contract: 0,
            tracked_contract: None,
            credits: 120,
            station_ship_shops,
            locations,
            discovered_locations: starting_discovered_locations(location_count),
            station_fuel: starting_station_fuel(location_count),
            fleet,
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
                "[0000] Primary objective: chart the full environment and build credits through contracts."
                    .into(),
                "[0000] Accept a contract from the Mission Board, then assign a ship to the route."
                    .into(),
                "[0000] Frontier arrivals unlock new charts deeper in the map.".into(),
            ],
            action_feedback: None,
            mission_history: Vec::new(),
        }
    }

    pub(crate) fn set_action_feedback(&mut self, message: impl Into<String>) {
        self.action_feedback = Some(message.into());
    }

    pub(crate) fn take_action_feedback(&mut self) -> Option<String> {
        self.action_feedback.take()
    }

    pub(crate) fn record_mission_history(&mut self, entry: String) {
        self.mission_history.insert(0, entry);
        self.mission_history.truncate(6);
    }

    pub(crate) fn player_is_in_transit(&self) -> bool {
        self.player_in_transit_ship.is_some()
    }

    pub(crate) fn mode_label(&self) -> String {
        match self.mode {
            AppMode::Browse => "Browse".to_string(),
            AppMode::SelectingDestination { ship_index, intent } => {
                format!("{} {}", intent.label(), self.fleet[ship_index].name)
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
                self.map_focus_location = self.selected_location;
                self.selected_shipyard_offer = 0;
            }
            FLEET_PANE => {
                self.selected_ship = wrap_index(self.selected_ship, self.fleet.len(), delta);
            }
            SHIPYARD_PANE => {
                let offer_count = self.shipyard_offer_count(self.selected_location);
                if offer_count > 0 {
                    self.selected_shipyard_offer =
                        wrap_index(self.selected_shipyard_offer, offer_count, delta);
                }
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
                SHIPYARD_PANE => self.purchase_ship_at_selected_location(),
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
        let location_names: Vec<&str> = self
            .locations
            .iter()
            .map(|location| location.name.as_str())
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
                exploration_run,
                assigned_contract,
                repair_on_arrival,
                ..
            } = &mut ship.state
            {
                let destination_index = *destination;
                let exploring = *exploration_run;
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
                    let player_riding = self.player_in_transit_ship == Some(ship_index);
                    ship.state = if repair_ticks > 0 {
                        ShipState::Repairing {
                            ticks_remaining: repair_ticks,
                        }
                    } else {
                        ShipState::Docked
                    };
                    arrivals.push((
                        ship_index,
                        ship.name.clone(),
                        destination_index,
                        player_riding,
                        exploring,
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
        for (
            ship_index,
            ship_name,
            destination_index,
            player_riding,
            exploring,
            contract_assignment,
            repair_ticks,
        ) in arrivals
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

            if player_riding {
                self.player_in_transit_ship = None;
                self.player_location = destination_index;
                self.push_log(format!(
                    "[{clock:04}] You disembark at {}.",
                    self.location_name(destination_index),
                    clock = self.clock,
                ));
            }

            self.sync_low_fuel_alert(ship_index);

            if let Some(contract_index) = contract_assignment {
                self.resolve_contract_arrival(
                    contract_index,
                    ship_index,
                    &ship_name,
                    destination_index,
                );
            }

            if exploring && let Some(discovery) = self.reveal_from_arrival(destination_index) {
                self.push_log(discovery);
            }
        }

        self.update_contract_deadlines();
        self.restock_station_fuel();
        self.restock_ship_shops();

        if self.clock % 32 == 0 && !had_arrivals {
            self.push_log(self.ambient_event());
        }

        self.evaluate_run_outcome();
    }

    pub(crate) fn low_fuel_ship_names(&self) -> Vec<&str> {
        self.fleet
            .iter()
            .filter(|ship| ship.current_fuel <= Self::fuel_alert_threshold(ship))
            .map(|ship| ship.name.as_str())
            .collect()
    }

    pub(crate) fn current_alerts(&self) -> Vec<Incident> {
        let mut alerts = Vec::new();

        for (ship_index, ship) in self.fleet.iter().enumerate() {
            match &ship.state {
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
                } if *repair_on_arrival > 0 => alerts.push(Incident {
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
            let summary = match self.contracts[contract_index].state {
                ContractState::Assigned { ship_index, .. } => format!(
                    "Active mission: {} via {}",
                    self.contracts[contract_index].title, self.fleet[ship_index].name,
                ),
                _ => format!("Active mission: {}", self.contracts[contract_index].title),
            };
            alerts.push(Incident {
                summary,
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
                self.sync_map_focus_to_selected_location();
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

fn generated_locations(
    world_seed: u64,
    world_flavor: Option<&WorldFlavor>,
) -> (String, String, Vec<Location>) {
    let fallback_environment_name = if world_seed == 0 {
        "Helios Dispatch".to_string()
    } else {
        format!("Seeded Frontier {}", world_seed % 10_000)
    };
    let fallback_environment_summary = if world_seed == 0 {
        "A courier environment spanning multiple frontier sectors and jump-linked relay chains."
            .to_string()
    } else {
        format!(
            "A procedurally seeded courier environment initialized from bootstrap seed {}.",
            world_seed
        )
    };

    let fallback_locations = fallback_world_locations();
    let world_flavor = world_flavor.filter(|flavor| {
        flavor.locations.len() >= SECTOR_LOCATION_COUNT * 2
            && flavor.locations.len() % SECTOR_LOCATION_COUNT == 0
    });

    let environment_name = world_flavor
        .map(|flavor| flavor.environment_name.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(fallback_environment_name);
    let environment_summary = world_flavor
        .map(|flavor| flavor.environment_summary.clone())
        .filter(|summary| !summary.trim().is_empty())
        .unwrap_or(fallback_environment_summary);
    let flavor_locations = world_flavor
        .map(|flavor| flavor.locations.as_slice())
        .unwrap_or(fallback_locations.as_slice());
    let sector_count = flavor_locations.len() / SECTOR_LOCATION_COUNT;
    let sector_columns = if sector_count <= 1 { 1 } else { 2 };

    let mut locations: Vec<Location> = flavor_locations
        .iter()
        .enumerate()
        .map(|(index, flavor)| {
            let sector_index = index / SECTOR_LOCATION_COUNT;
            let role_index = index % SECTOR_LOCATION_COUNT;
            let row = (sector_index / sector_columns) as i16;
            let col = (sector_index % sector_columns) as i16;
            let region_x = if sector_columns == 1 {
                50
            } else {
                24 + col * 46
            };
            let region_y = 20 + row * 22;
            let sector_center = MapPoint {
                x: region_x,
                y: region_y + 10,
            };
            let sector_offsets = [
                MapPoint { x: 0, y: 0 },
                MapPoint { x: 18, y: -14 },
                MapPoint { x: 16, y: 15 },
                MapPoint { x: -18, y: 14 },
                MapPoint { x: 32, y: 1 },
            ];
            let cluster_offsets = [
                MapPoint { x: 0, y: 0 },
                MapPoint { x: 16, y: -12 },
                MapPoint { x: 13, y: 15 },
                MapPoint { x: -15, y: 13 },
                MapPoint { x: 28, y: 2 },
            ];
            let system_offsets = [
                MapPoint { x: 0, y: -5 },
                MapPoint { x: 16, y: -13 },
                MapPoint { x: 12, y: 15 },
                MapPoint { x: -14, y: 12 },
                MapPoint { x: 27, y: 1 },
            ];

            let reveal_on_arrival = match role_index {
                1 => Some(sector_index * SECTOR_LOCATION_COUNT + 2),
                2 => Some(sector_index * SECTOR_LOCATION_COUNT + 4),
                3 => Some(sector_index * SECTOR_LOCATION_COUNT + 1),
                4 => {
                    let next_sector_hub = (sector_index + 1) * SECTOR_LOCATION_COUNT;
                    (next_sector_hub < flavor_locations.len()).then_some(next_sector_hub)
                }
                _ => None,
            };

            Location {
                region_name: flavor.region_name.clone(),
                sector_name: flavor.sector_name.clone(),
                name: flavor.name.clone(),
                short_label: flavor.short_label.clone(),
                lane_name: flavor.lane_name.clone(),
                description: flavor.description.clone(),
                cluster_name: flavor.cluster_name.clone(),
                system_name: flavor.system_name.clone(),
                region_coords: sector_center,
                sector_coords: MapPoint {
                    x: sector_center.x + sector_offsets[role_index].x,
                    y: sector_center.y + sector_offsets[role_index].y,
                },
                cluster_coords: MapPoint {
                    x: 34 + cluster_offsets[role_index].x,
                    y: 30 + cluster_offsets[role_index].y,
                },
                system_coords: MapPoint {
                    x: 34 + system_offsets[role_index].x,
                    y: 30 + system_offsets[role_index].y,
                },
                travel_time_from_hub: [0, 4, 5, 6, 7][role_index],
                reveal_on_arrival,
            }
        })
        .collect();

    scatter_generated_coordinates(&mut locations);

    (environment_name, environment_summary, locations)
}

#[derive(Clone, Copy)]
enum CoordLayer {
    Region,
    Sector,
    Cluster,
    System,
}

fn scatter_generated_coordinates(locations: &mut [Location]) {
    if locations.is_empty() {
        return;
    }

    let sector_count = locations.len() / SECTOR_LOCATION_COUNT;
    let hub_indices: Vec<usize> = (0..sector_count)
        .map(|sector_index| sector_index * SECTOR_LOCATION_COUNT)
        .collect();
    scatter_layer(
        locations,
        &hub_indices,
        CoordLayer::Region,
        28.0,
        (12, 88, 14, 48),
        Some(0),
    );

    for sector_index in 0..sector_count {
        let start = sector_index * SECTOR_LOCATION_COUNT;
        let indices: Vec<usize> = (start..start + SECTOR_LOCATION_COUNT).collect();
        scatter_layer(
            locations,
            &indices,
            CoordLayer::Sector,
            18.0,
            (8, 94, 8, 54),
            Some(0),
        );
        scatter_layer(
            locations,
            &indices,
            CoordLayer::Cluster,
            16.0,
            (10, 90, 8, 52),
            Some(0),
        );
        scatter_layer(
            locations,
            &indices,
            CoordLayer::System,
            16.0,
            (10, 90, 8, 52),
            Some(0),
        );
    }
}

fn scatter_layer(
    locations: &mut [Location],
    indices: &[usize],
    layer: CoordLayer,
    min_distance: f64,
    bounds: (i16, i16, i16, i16),
    anchored_index: Option<usize>,
) {
    if indices.len() < 2 {
        return;
    }

    let mut points: Vec<(f64, f64)> = indices
        .iter()
        .map(|&index| map_point_for_layer(&locations[index], layer).as_tuple())
        .collect();

    for _ in 0..40 {
        let mut deltas = vec![(0.0, 0.0); points.len()];
        let mut moved = false;

        for left in 0..points.len() {
            for right in (left + 1)..points.len() {
                let dx = points[right].0 - points[left].0;
                let dy = points[right].1 - points[left].1;
                let dist_sq = dx * dx + dy * dy;
                let distance = dist_sq.sqrt();

                if distance >= min_distance {
                    continue;
                }

                let (unit_x, unit_y) = if distance <= f64::EPSILON {
                    let angle = ((left * 37 + right * 19 + 11) % 360) as f64;
                    let radians = angle.to_radians();
                    (radians.cos(), radians.sin())
                } else {
                    (dx / distance, dy / distance)
                };
                let push = (min_distance - distance.max(0.1)) / 2.0;

                if anchored_index != Some(left) {
                    deltas[left].0 -= unit_x * push;
                    deltas[left].1 -= unit_y * push;
                }
                if anchored_index != Some(right) {
                    deltas[right].0 += unit_x * push;
                    deltas[right].1 += unit_y * push;
                }
            }
        }

        for (point_index, point) in points.iter_mut().enumerate() {
            if anchored_index == Some(point_index) {
                continue;
            }

            let next_x =
                (point.0 + deltas[point_index].0).clamp(f64::from(bounds.0), f64::from(bounds.1));
            let next_y =
                (point.1 + deltas[point_index].1).clamp(f64::from(bounds.2), f64::from(bounds.3));
            moved |= (next_x - point.0).abs() > 0.05 || (next_y - point.1).abs() > 0.05;
            *point = (next_x, next_y);
        }

        if !moved {
            break;
        }
    }

    for (point_index, &location_index) in indices.iter().enumerate() {
        set_map_point_for_layer(
            &mut locations[location_index],
            layer,
            MapPoint {
                x: points[point_index].0.round() as i16,
                y: points[point_index].1.round() as i16,
            },
        );
    }
}

fn map_point_for_layer(location: &Location, layer: CoordLayer) -> MapPoint {
    match layer {
        CoordLayer::Region => location.region_coords,
        CoordLayer::Sector => location.sector_coords,
        CoordLayer::Cluster => location.cluster_coords,
        CoordLayer::System => location.system_coords,
    }
}

fn set_map_point_for_layer(location: &mut Location, layer: CoordLayer, point: MapPoint) {
    match layer {
        CoordLayer::Region => location.region_coords = point,
        CoordLayer::Sector => location.sector_coords = point,
        CoordLayer::Cluster => location.cluster_coords = point,
        CoordLayer::System => location.system_coords = point,
    }
}

fn fallback_world_locations() -> Vec<WorldLocationFlavor> {
    vec![
        WorldLocationFlavor {
            region_name: "Helios Frontier".to_string(),
            sector_name: "Astra Corridor".to_string(),
            name: "Astra Prime".to_string(),
            short_label: "Astra".to_string(),
            lane_name: "Central Exchange".to_string(),
            description:
                "A dense trade citadel wrapped around the courier authority's core dispatch rings."
                    .to_string(),
            cluster_name: "Helios Delta".to_string(),
            system_name: "Astra Line".to_string(),
        },
        WorldLocationFlavor {
            region_name: "Helios Frontier".to_string(),
            sector_name: "Astra Corridor".to_string(),
            name: "Kite Station".to_string(),
            short_label: "Kite".to_string(),
            lane_name: "Kite Spur".to_string(),
            description:
                "A lean refit spindle perched on the edge of the spur lanes, famous for fast turnarounds."
                    .to_string(),
            cluster_name: "Ravel Spur".to_string(),
            system_name: "Kite Rise".to_string(),
        },
        WorldLocationFlavor {
            region_name: "Helios Frontier".to_string(),
            sector_name: "Astra Corridor".to_string(),
            name: "Ion Anchorage".to_string(),
            short_label: "Ion".to_string(),
            lane_name: "Ion Run".to_string(),
            description:
                "A broad cargo anchor with pressure-docked cranes and a habit of paying well for speed."
                    .to_string(),
            cluster_name: "Ion Expanse".to_string(),
            system_name: "Relay Verge".to_string(),
        },
        WorldLocationFlavor {
            region_name: "Helios Frontier".to_string(),
            sector_name: "Astra Corridor".to_string(),
            name: "Dust Harbor".to_string(),
            short_label: "Dust".to_string(),
            lane_name: "Dust Corridor".to_string(),
            description:
                "A storm-baffled frontier harbor where survey crews and scavengers overlap in the docks."
                    .to_string(),
            cluster_name: "Helios Delta".to_string(),
            system_name: "Astra Line".to_string(),
        },
        WorldLocationFlavor {
            region_name: "Helios Frontier".to_string(),
            sector_name: "Astra Corridor".to_string(),
            name: "Outer Ring Relay".to_string(),
            short_label: "Relay".to_string(),
            lane_name: "Relay Ascent".to_string(),
            description:
                "A signal spine above the lanes where every missed window echoes through the network."
                    .to_string(),
            cluster_name: "Ion Expanse".to_string(),
            system_name: "Relay Verge".to_string(),
        },
        WorldLocationFlavor {
            region_name: "Perihelion Reach".to_string(),
            sector_name: "Vesper March".to_string(),
            name: "Vesper Exchange".to_string(),
            short_label: "Vesper".to_string(),
            lane_name: "March Nexus".to_string(),
            description:
                "A quieter relay capital holding together the far lanes beyond the first frontier ring."
                    .to_string(),
            cluster_name: "Vesper Crown".to_string(),
            system_name: "March Line".to_string(),
        },
        WorldLocationFlavor {
            region_name: "Perihelion Reach".to_string(),
            sector_name: "Vesper March".to_string(),
            name: "Wick Relay".to_string(),
            short_label: "Wick".to_string(),
            lane_name: "Wick Spur".to_string(),
            description:
                "A relay gantry with sparse crews, harsh schedules, and enough lift to reopen dead lanes."
                    .to_string(),
            cluster_name: "Cinder Spur".to_string(),
            system_name: "Wick Rise".to_string(),
        },
        WorldLocationFlavor {
            region_name: "Perihelion Reach".to_string(),
            sector_name: "Vesper March".to_string(),
            name: "Cinder Anchorage".to_string(),
            short_label: "Cinder".to_string(),
            lane_name: "Cinder Run".to_string(),
            description:
                "A furnace-lit anchorage where repair queues and heavy freight stack up around the docks."
                    .to_string(),
            cluster_name: "Cinder Spur".to_string(),
            system_name: "Foundry Verge".to_string(),
        },
        WorldLocationFlavor {
            region_name: "Perihelion Reach".to_string(),
            sector_name: "Vesper March".to_string(),
            name: "Ember Harbor".to_string(),
            short_label: "Ember".to_string(),
            lane_name: "Ember Drift".to_string(),
            description:
                "A weather-beaten harbor clinging to the edge of the march, busy whenever supply lines recover."
                    .to_string(),
            cluster_name: "Vesper Crown".to_string(),
            system_name: "March Line".to_string(),
        },
        WorldLocationFlavor {
            region_name: "Perihelion Reach".to_string(),
            sector_name: "Vesper March".to_string(),
            name: "Far Signal Array".to_string(),
            short_label: "Signal".to_string(),
            lane_name: "Signal Ascent".to_string(),
            description:
                "A remote signal array where jump traffic is stitched back into the wider network."
                    .to_string(),
            cluster_name: "Foundry Verge".to_string(),
            system_name: "Foundry Verge".to_string(),
        },
    ]
}

fn starting_discovered_locations(location_count: usize) -> Vec<bool> {
    let mut discovered = vec![false; location_count];
    if location_count > ASTRA_PRIME {
        discovered[ASTRA_PRIME] = true;
    }
    if location_count > DUST_HARBOR {
        discovered[DUST_HARBOR] = true;
    }
    discovered
}

fn starting_station_fuel(location_count: usize) -> Vec<u16> {
    (0..location_count)
        .map(|index| match index % SECTOR_LOCATION_COUNT {
            0 => 48u16
                .saturating_sub((index / SECTOR_LOCATION_COUNT) as u16 * 10)
                .max(26),
            1 => 20,
            2 => 18,
            3 => 22,
            _ => 10,
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_contact_requires_explicit_exploration_run() {
        let mut standard = GameData::new(Difficulty::Normal);
        standard.fleet[0].state = ShipState::EnRoute {
            origin: ASTRA_PRIME,
            destination: DUST_HARBOR,
            eta_remaining: 1,
            total_eta: 1,
            exploration_run: false,
            segments: vec![(ASTRA_PRIME, DUST_HARBOR)],
            segment_costs: vec![1],
            route: "Astra Prime -> Dust Harbor".to_string(),
            condition_summary: "Dust Corridor: clear lanes".to_string(),
            assigned_contract: None,
            repair_on_arrival: 0,
        };

        standard.tick();
        assert!(!standard.discovered_locations[KITE_STATION]);

        let mut exploration = GameData::new(Difficulty::Normal);
        exploration.fleet[0].state = ShipState::EnRoute {
            origin: ASTRA_PRIME,
            destination: DUST_HARBOR,
            eta_remaining: 1,
            total_eta: 1,
            exploration_run: true,
            segments: vec![(ASTRA_PRIME, DUST_HARBOR)],
            segment_costs: vec![1],
            route: "Astra Prime -> Dust Harbor".to_string(),
            condition_summary: "Dust Corridor: clear lanes".to_string(),
            assigned_contract: None,
            repair_on_arrival: 0,
        };

        exploration.tick();
        assert!(exploration.discovered_locations[KITE_STATION]);
    }
}
