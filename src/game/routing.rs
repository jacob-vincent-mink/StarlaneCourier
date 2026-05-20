use super::*;

impl GameData {
    pub(crate) fn pending_ship(&self) -> Option<usize> {
        match self.mode {
            AppMode::Browse => None,
            AppMode::SelectingDestination { ship_index, .. } => Some(ship_index),
        }
    }

    pub(crate) fn pending_dispatch_intent(&self) -> Option<DispatchIntent> {
        match self.mode {
            AppMode::Browse => None,
            AppMode::SelectingDestination { intent, .. } => Some(intent),
        }
    }

    pub(crate) fn preview_origin(&self) -> Option<usize> {
        if let Some(ship_index) = self.pending_ship() {
            return Some(self.fleet[ship_index].current_location);
        }

        let ship = &self.fleet[self.selected_ship];
        ship.is_docked().then_some(ship.current_location)
    }

    pub(crate) fn ambient_event(&self) -> String {
        if self.undiscovered_count() > 0 {
            let frontier = self.frontier_locations();

            if !frontier.is_empty() && self.clock % 32 == 0 {
                let location =
                    &self.locations[frontier[(self.clock / 32) as usize % frontier.len()]];
                return format!(
                    "[{clock:04}] Long-range pings beyond {location} hint at another chartable contact.",
                    clock = self.clock,
                    location = location.name,
                );
            }
        }

        let discovered_lanes = self.discovered_lane_locations();
        let lane_index = discovered_lanes[(self.clock / 16) as usize % discovered_lanes.len()];
        let location = &self.locations[lane_index];
        let condition = self.lane_condition(lane_index);

        format!(
            "[{clock:04}] {lane} now reports {report}.",
            clock = self.clock,
            lane = location.lane_name,
            report = condition.report_phrase(),
        )
    }

    pub(crate) fn begin_dispatch(&mut self) {
        self.begin_route_selection(DispatchIntent::Standard);
    }

    pub(crate) fn begin_exploration(&mut self) {
        self.begin_route_selection(DispatchIntent::Exploration);
    }

    fn begin_route_selection(&mut self, intent: DispatchIntent) {
        if self.player_is_in_transit() {
            self.set_action_feedback(
                "You are in transit and cannot issue local orders until arrival.",
            );
            return;
        }

        let ship_index = self.selected_ship;
        let ship_name = self.fleet[ship_index].name.clone();
        let ship_location = self.fleet[ship_index].current_location;
        let ship_is_docked = self.fleet[ship_index].is_docked();
        let ship_fuel = self.fleet[ship_index].current_fuel;

        if !ship_is_docked {
            self.set_action_feedback(format!(
                "{} is already in transit and cannot be reassigned.",
                ship_name
            ));
            self.push_log(format!(
                "[{clock:04}] {name} is already in transit and cannot be reassigned.",
                clock = self.clock,
                name = ship_name,
            ));
            return;
        }

        if ship_location != self.player_location {
            self.set_action_feedback(format!(
                "You are at {}. Transfer there before operating {} at {}.",
                self.location_name(self.player_location),
                ship_name,
                self.location_name(ship_location)
            ));
            return;
        }

        if self.difficulty.uses_fuel_economy() && ship_fuel == 0 {
            self.set_action_feedback(format!(
                "{} has no fuel. Refuel with `f` or transfer fuel with `t` before plotting a route.",
                ship_name
            ));
            self.push_log(format!(
                "[{clock:04}] {name} has no fuel. Refuel with `f` or transfer fuel with `t` before plotting a route.",
                clock = self.clock,
                name = ship_name,
            ));
            return;
        }

        if matches!(intent, DispatchIntent::Exploration)
            && self.hidden_chartable_locations().is_empty()
        {
            self.set_action_feedback(
                "No unresolved uncharted contacts are currently available for exploration.",
            );
            self.push_log(format!(
                "[{clock:04}] No unresolved uncharted contacts are currently available for exploration.",
                clock = self.clock,
            ));
            return;
        }

        self.mode = AppMode::SelectingDestination { ship_index, intent };
        self.active_pane = MAP_PANE;
        self.map_zoom = MapZoom::Sector;

        if matches!(intent, DispatchIntent::Standard)
            && let Some(contract_index) = self.tracked_contract
        {
            let contract = &self.contracts[contract_index];
            if matches!(contract.state, ContractState::Accepted { .. })
                && contract.origin == ship_location
            {
                self.selected_location = contract.destination;
                self.sync_map_focus_to_selected_location();
                return;
            }
        }

        if matches!(intent, DispatchIntent::Exploration) {
            self.selected_location = ship_location;
            self.sync_map_focus_to_selected_location();
            self.reset_exploration_cursor(ship_index);
            return;
        }

        if !self.is_discovered(self.selected_location) || self.selected_location == ship_location {
            if let Some(destination) = self.first_dispatch_target(ship_location) {
                self.selected_location = destination;
            }
        }
        self.sync_map_focus_to_selected_location();
    }

    pub(crate) fn confirm_dispatch(&mut self) {
        let AppMode::SelectingDestination { ship_index, intent } = self.mode else {
            return;
        };

        if matches!(intent, DispatchIntent::Exploration) {
            self.confirm_exploration_dispatch(ship_index);
            return;
        }

        let ship_name = self.fleet[ship_index].name.clone();
        let origin = self.fleet[ship_index].current_location;
        let destination = self.selected_location;

        let Some(plan) = self.plan_route_for_ship(ship_index, self.selected_location) else {
            self.set_action_feedback(format!(
                "Pick a different charted destination before dispatching {}.",
                ship_name
            ));
            self.push_log(format!(
                "[{clock:04}] Pick a different charted destination before dispatching {name}.",
                clock = self.clock,
                name = ship_name,
            ));
            return;
        };

        let rider_required = self.local_ship_count(self.player_location) == 1
            && self.fleet[ship_index].current_location == self.player_location;
        if self.fleet[ship_index].current_fuel < plan.fuel_required {
            self.set_action_feedback(format!(
                "{} needs fuel before it can make this route.",
                ship_name
            ));
            self.push_log(format!(
                "[{clock:04}] {name} needs fuel before it can make this route.",
                clock = self.clock,
                name = ship_name,
            ));
            return;
        }

        let assigned_contract = if matches!(intent, DispatchIntent::Exploration) {
            None
        } else {
            self.matching_tracked_contract(ship_index, destination, plan.eta)
        };

        if let Some(contract_index) = assigned_contract {
            let accepted_at = match self.contracts[contract_index].state {
                ContractState::Accepted { accepted_at } => accepted_at,
                _ => self.clock,
            };
            self.contracts[contract_index].state = ContractState::Assigned {
                ship_index,
                accepted_at,
            };
        }

        self.fleet[ship_index].state = ShipState::EnRoute {
            origin,
            destination,
            eta_remaining: plan.eta,
            total_eta: plan.eta,
            exploration_run: matches!(intent, DispatchIntent::Exploration),
            exploration_target: None,
            exploration_discoveries: Vec::new(),
            exploration_revealed_count: 0,
            exploration_outcome: None,
            segments: plan.segments.clone(),
            segment_costs: plan.segment_costs.clone(),
            route: plan.path.clone(),
            condition_summary: plan.condition_summary.clone(),
            assigned_contract,
            repair_on_arrival: 0,
        };
        self.mode = AppMode::Browse;
        self.active_pane = FLEET_PANE;

        if rider_required {
            self.player_in_transit_ship = Some(ship_index);
            self.set_action_feedback(format!(
                "Boarded {} for travel. The player marker now rides with this ship until arrival.",
                ship_name
            ));
            self.push_log(format!(
                "[{clock:04}] You board {} because it is the last local ship departing {}.",
                ship_name,
                self.location_name(origin),
                clock = self.clock,
            ));
        }

        let launch_verb = intent.label();
        self.push_log(format!(
            "[{clock:04}] {launch_verb}: {name} -> {destination} | ETA {eta} | {conditions}",
            clock = self.clock,
            launch_verb = launch_verb,
            name = ship_name,
            destination = self.location_name(destination),
            eta = plan.eta,
            conditions = plan.condition_summary,
        ));

        if let Some(contract_index) = assigned_contract {
            self.push_log(format!(
                "[{clock:04}] {name} is now carrying tracked contract: {contract}.",
                clock = self.clock,
                name = ship_name,
                contract = self.contracts[contract_index].title,
            ));
        } else if matches!(intent, DispatchIntent::Exploration) {
            if let Some(heading) = self.exploration_heading_hint(destination) {
                self.push_log(format!(
                    "[{clock:04}] Exploration heading set {} beyond {}.",
                    heading,
                    self.location_name(destination),
                    clock = self.clock,
                ));
            }
        } else if let Some(contract_index) = self.tracked_contract {
            let contract = &self.contracts[contract_index];
            let same_route = contract.origin == origin && contract.destination == destination;
            let message = if same_route && plan.eta > contract.max_eta {
                let max_eta = contract.max_eta;
                let title = contract.title.clone();
                self.set_action_feedback(format!(
                    "Active mission not assigned: ETA {} exceeds required {}.",
                    plan.eta, max_eta
                ));
                format!(
                    "[{clock:04}] Survey run only. {title} requires ETA <= {max_eta}, but this ship needs {eta}.",
                    clock = self.clock,
                    title = title,
                    max_eta = max_eta,
                    eta = plan.eta,
                )
            } else {
                format!(
                    "[{clock:04}] Survey run only. Tracked contract still waits: {title}.",
                    clock = self.clock,
                    title = contract.title,
                )
            };
            self.push_log(message);
        }

        self.fleet[ship_index].current_fuel = self.fleet[ship_index]
            .current_fuel
            .saturating_sub(plan.fuel_required);
        self.sync_low_fuel_alert(ship_index);
        self.evaluate_run_outcome();
    }

    pub(crate) fn cancel_dispatch(&mut self) -> bool {
        if matches!(self.mode, AppMode::SelectingDestination { .. }) {
            self.mode = AppMode::Browse;
            self.active_pane = FLEET_PANE;
            return true;
        }

        false
    }

    pub(crate) fn ship_can_transfer_player_to(
        &self,
        ship_index: usize,
        destination: usize,
    ) -> bool {
        if self.player_is_in_transit() || destination == self.player_location {
            return false;
        }

        let Some(ship) = self.fleet.get(ship_index) else {
            return false;
        };

        if ship.current_location != self.player_location
            || !matches!(&ship.state, ShipState::Docked)
        {
            return false;
        }

        self.plan_route_for_ship(ship_index, destination)
            .is_some_and(|plan| ship.current_fuel >= plan.fuel_required)
    }

    pub(crate) fn player_transfer_candidate_indices(&self, destination: usize) -> Vec<usize> {
        self.fleet
            .iter()
            .enumerate()
            .filter_map(|(ship_index, _)| {
                self.ship_can_transfer_player_to(ship_index, destination)
                    .then_some(ship_index)
            })
            .collect()
    }

    pub(crate) fn transfer_player_with_ship(&mut self, ship_index: usize, destination: usize) {
        if self.player_is_in_transit() {
            self.set_action_feedback("You are already in transit aboard a ship.");
            return;
        }

        if destination == self.player_location {
            self.set_action_feedback("You are already at the selected station.");
            return;
        }

        let Some(ship) = self.fleet.get(ship_index) else {
            self.set_action_feedback("That ship is no longer available.");
            return;
        };

        if ship.current_location != self.player_location {
            self.set_action_feedback(format!(
                "Transfer to {} before boarding {}.",
                self.location_name(ship.current_location),
                ship.name
            ));
            return;
        }

        if !matches!(&ship.state, ShipState::Docked) {
            self.set_action_feedback(format!(
                "{} is not docked and cannot take you there right now.",
                ship.name
            ));
            return;
        }

        let Some(plan) = self.plan_route_for_ship(ship_index, destination) else {
            self.set_action_feedback(format!(
                "{} cannot plot a route to {} right now.",
                ship.name,
                self.location_name(destination)
            ));
            return;
        };

        if ship.current_fuel < plan.fuel_required {
            self.set_action_feedback(format!(
                "{} needs fuel before it can take you to {}.",
                ship.name,
                self.location_name(destination)
            ));
            return;
        }

        let ship_name = ship.name.clone();
        let origin = ship.current_location;
        self.selected_ship = ship_index;
        self.player_in_transit_ship = None;
        self.player_location = destination;
        self.fleet[ship_index].current_location = destination;
        self.fleet[ship_index].state = ShipState::Docked;
        self.fleet[ship_index].current_fuel = self.fleet[ship_index]
            .current_fuel
            .saturating_sub(plan.fuel_required);
        self.active_pane = FLEET_PANE;
        self.selected_shipyard_offer = 0;
        self.set_action_feedback(format!(
            "Transfer complete: you and {} moved to {}.",
            ship_name,
            self.location_name(destination)
        ));
        self.push_log(format!(
            "[{clock:04}] Transfer relocation: you and {ship_name} moved {origin_name} -> {destination} | fuel {fuel} | {conditions}.",
            clock = self.clock,
            ship_name = ship_name,
            origin_name = self.location_name(origin),
            destination = self.location_name(destination),
            fuel = plan.fuel_required,
            conditions = plan.condition_summary,
        ));
        self.sync_low_fuel_alert(ship_index);
        self.evaluate_run_outcome();
    }

    pub(crate) fn transfer_player_to_selected_location(&mut self) {
        let destination = self.selected_location;
        let candidates = self.player_transfer_candidate_indices(destination);
        if let Some(&ship_index) = candidates
            .iter()
            .find(|&&ship_index| ship_index == self.selected_ship)
        {
            self.transfer_player_with_ship(ship_index, destination);
        } else if candidates.len() == 1 {
            self.transfer_player_with_ship(candidates[0], destination);
        } else if candidates.is_empty() {
            let local_ships = self
                .fleet
                .iter()
                .filter(|ship| ship.current_location == self.player_location)
                .count();
            self.set_action_feedback(if local_ships == 0 {
                "No local ship is available to move you from here.".to_string()
            } else {
                format!(
                    "No local docked ship can currently reach {}. Refuel one first.",
                    self.location_name(destination)
                )
            });
        } else {
            self.set_action_feedback(
                "Choose which local ship should take you there from the transfer overlay.",
            );
        }
    }

    pub(crate) fn available_exploration_leads(&self) -> Vec<(usize, usize)> {
        (0..self.locations.len())
            .filter_map(|lead_index| {
                self.locations[lead_index]
                    .reveal_on_arrival
                    .filter(|&target| {
                        self.is_discovered(lead_index)
                            && !self.locations[lead_index].exploration_exhausted
                            && !self.is_charted_empty(target)
                            && !self.is_discovered(target)
                    })
                    .map(|target| (lead_index, target))
            })
            .collect()
    }

    pub(crate) fn hidden_chartable_locations(&self) -> Vec<usize> {
        (0..self.locations.len())
            .filter(|&index| !self.is_discovered(index) && !self.is_charted_empty(index))
            .collect()
    }

    pub(crate) fn exploration_cursor_coords(&self) -> Option<(f64, f64)> {
        matches!(
            self.mode,
            AppMode::SelectingDestination {
                intent: DispatchIntent::Exploration,
                ..
            }
        )
        .then_some(self.exploration_cursor.as_tuple())
    }

    pub(crate) fn exploration_origin_coords(&self) -> Option<(f64, f64)> {
        let AppMode::SelectingDestination {
            ship_index,
            intent: DispatchIntent::Exploration,
        } = self.mode
        else {
            return None;
        };

        Some(
            self.locations[self.fleet[ship_index].current_location]
                .sector_coords
                .as_tuple(),
        )
    }

    pub(crate) fn exploration_ray_heading(&self) -> Option<String> {
        let origin = self.exploration_origin_coords()?;
        Some(
            compass_heading(
                MapPoint {
                    x: origin.0.round() as i16,
                    y: origin.1.round() as i16,
                },
                self.exploration_cursor,
            )
            .to_string(),
        )
    }

    pub(crate) fn exploration_max_range_for_ship(&self, ship_index: usize) -> f64 {
        f64::from(self.fleet[ship_index].current_fuel.max(1))
            * f64::from(self.fleet[ship_index].speed.max(1))
            * 1.5
    }

    pub(crate) fn reset_exploration_cursor(&mut self, ship_index: usize) {
        let origin = self.locations[self.fleet[ship_index].current_location].sector_coords;
        let default_distance = self.exploration_max_range_for_ship(ship_index).min(16.0);
        self.exploration_cursor = self.clamp_exploration_cursor(
            ship_index,
            (f64::from(origin.x) + default_distance, f64::from(origin.y)),
        );
    }

    pub(crate) fn move_exploration_cursor(&mut self, dx: i16, dy: i16) {
        let AppMode::SelectingDestination {
            ship_index,
            intent: DispatchIntent::Exploration,
        } = self.mode
        else {
            return;
        };

        self.exploration_cursor = self.clamp_exploration_cursor(
            ship_index,
            (
                f64::from(self.exploration_cursor.x + dx),
                f64::from(self.exploration_cursor.y + dy),
            ),
        );
    }

    pub(crate) fn exploration_trace_blocked(&self, ship_index: usize) -> bool {
        let origin = self.fleet[ship_index].current_location;
        self.exploration_traces.iter().any(|trace| {
            trace.origin == origin
                && matches!(trace.outcome, ExplorationTraceOutcome::Empty)
                && distance(trace.target.as_tuple(), self.exploration_cursor.as_tuple()) <= 6.0
        })
    }

    pub(crate) fn visible_exploration_traces(&self) -> &[ExplorationTrace] {
        &self.exploration_traces
    }

    pub(crate) fn exploration_preview_exact_targets(&self) -> Vec<usize> {
        let Some(ship_index) = self.active_exploration_ship_index() else {
            return Vec::new();
        };

        self.exploration_ray_hits(
            ship_index,
            self.difficulty.exploration_discovery_width() + f64::from(self.fleet[ship_index].speed),
        )
        .into_iter()
        .map(|hit| hit.target)
        .collect()
    }

    pub(crate) fn exploration_primary_exact_target(&self) -> Option<usize> {
        self.closest_target_to_cursor_within(
            self.exploration_preview_exact_targets(),
            self.difficulty.exploration_discovery_width() * 1.1,
        )
    }

    pub(crate) fn exploration_preview_near_targets(&self) -> Vec<usize> {
        let Some(ship_index) = self.active_exploration_ship_index() else {
            return Vec::new();
        };

        if !self.exploration_preview_exact_targets().is_empty() {
            return Vec::new();
        }

        self.exploration_ray_hits(
            ship_index,
            self.difficulty.exploration_glancing_width()
                + f64::from(self.fleet[ship_index].speed) * 1.5,
        )
        .into_iter()
        .map(|hit| hit.target)
        .collect()
    }

    pub(crate) fn exploration_primary_near_target(&self) -> Option<usize> {
        if self.exploration_primary_exact_target().is_some() {
            return None;
        }

        self.closest_target_to_cursor_within(
            self.exploration_preview_near_targets(),
            self.difficulty.exploration_glancing_width() * 0.9,
        )
    }

    pub(crate) fn exploration_preview_text(&self, ship_index: usize) -> Option<String> {
        let survey = self.preview_exploration_survey(ship_index)?;
        let outcome = match survey.outcome {
            ExplorationTraceOutcome::Discovery => format!(
                "Likely discovery: {}",
                survey
                    .discovered_locations
                    .iter()
                    .map(|&index| self.location_name(index).to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ExplorationTraceOutcome::Miss => {
                "Likely result: miss, but another nearby vector may work".to_string()
            }
            ExplorationTraceOutcome::Empty => {
                "Likely result: empty space; this vector will be charted and exhausted".to_string()
            }
        };

        Some(format!(
            "Survey {} | range {:.0}/{:.0} | lock {:.0} / echo {:.0} | ETA {} | fuel {} | {}",
            survey.heading,
            survey.distance,
            self.exploration_max_range_for_ship(ship_index),
            self.difficulty.exploration_discovery_width() + f64::from(self.fleet[ship_index].speed),
            self.difficulty.exploration_glancing_width()
                + f64::from(self.fleet[ship_index].speed) * 1.5,
            survey.eta,
            survey.fuel_required,
            outcome
        ))
    }

    pub(crate) fn resolve_exploration_arrival(
        &mut self,
        origin: usize,
        target: MapPoint,
        outcome: ExplorationTraceOutcome,
        discoveries: &[usize],
        ship_name: &str,
    ) {
        let trace_target = if matches!(outcome, ExplorationTraceOutcome::Discovery) {
            discoveries
                .last()
                .map(|&index| self.locations[index].sector_coords)
                .unwrap_or(target)
        } else {
            target
        };
        self.exploration_traces.insert(
            0,
            ExplorationTrace {
                origin,
                target: trace_target,
                outcome,
            },
        );
        self.exploration_traces.truncate(12);

        match outcome {
            ExplorationTraceOutcome::Discovery => {
                for &location_index in discoveries {
                    self.discovered_locations[location_index] = true;
                }
                let discovered_names = discoveries
                    .iter()
                    .map(|&index| self.location_name(index).to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                self.set_action_feedback(format!(
                    "Exploration complete: {} charted {}.",
                    ship_name, discovered_names
                ));
                self.push_log(format!(
                    "[{clock:04}] Exploration complete: {ship_name} charted {discovered_names}.",
                    clock = self.clock,
                    ship_name = ship_name,
                    discovered_names = discovered_names,
                ));
            }
            ExplorationTraceOutcome::Miss => {
                let heading = compass_heading(self.locations[origin].sector_coords, target);
                self.set_action_feedback(format!(
                    "Exploration complete: {} found no chartable contact {}. Try another nearby vector.",
                    ship_name, heading
                ));
                self.push_log(format!(
                    "[{clock:04}] Exploration complete: {ship_name} found no chartable contact {heading}.",
                    clock = self.clock,
                    ship_name = ship_name,
                    heading = heading,
                ));
            }
            ExplorationTraceOutcome::Empty => {
                if let Some(location_index) = self.exploration_empty_target(target) {
                    self.locations[location_index].charted_empty = true;
                }
                let heading = compass_heading(self.locations[origin].sector_coords, target);
                self.set_action_feedback(format!(
                    "Exploration complete: {} charted empty space {}. That vector is exhausted.",
                    ship_name, heading
                ));
                self.push_log(format!(
                    "[{clock:04}] Exploration complete: {ship_name} charted empty space {heading}.",
                    clock = self.clock,
                    ship_name = ship_name,
                    heading = heading,
                ));
            }
        }
    }

    fn confirm_exploration_dispatch(&mut self, ship_index: usize) {
        if self.exploration_trace_blocked(ship_index) {
            self.set_action_feedback(
                "That exploration vector is already charted as empty space. Aim elsewhere.",
            );
            return;
        }

        let Some(survey) = self.preview_exploration_survey(ship_index) else {
            self.set_action_feedback(
                "Aim the exploration vector farther from the ship before launching the survey.",
            );
            return;
        };

        let ship_name = self.fleet[ship_index].name.clone();
        let origin = self.fleet[ship_index].current_location;
        let rider_required = self.local_ship_count(self.player_location) == 1
            && self.fleet[ship_index].current_location == self.player_location;

        if self.fleet[ship_index].current_fuel < survey.fuel_required {
            self.set_action_feedback(format!(
                "{} needs fuel before it can sweep that vector.",
                ship_name
            ));
            return;
        }

        let destination = survey.destination;
        let discovered_hidden_destination = !self.is_discovered(destination);
        self.fleet[ship_index].state = ShipState::EnRoute {
            origin,
            destination,
            eta_remaining: survey.eta,
            total_eta: survey.eta,
            exploration_run: true,
            exploration_target: Some(self.exploration_cursor),
            exploration_discoveries: survey.discovered_locations.clone(),
            exploration_revealed_count: 0,
            exploration_outcome: Some(survey.outcome),
            segments: vec![(origin, destination)],
            segment_costs: vec![survey.eta.max(1)],
            route: if discovered_hidden_destination {
                format!("Exploration sweep {}", survey.heading)
            } else {
                format!(
                    "Exploration sweep {} -> {}",
                    survey.heading,
                    self.location_name(destination)
                )
            },
            condition_summary: format!(
                "survey vector {} | {:.0} units",
                survey.heading, survey.distance
            ),
            assigned_contract: None,
            repair_on_arrival: 0,
        };
        self.mode = AppMode::Browse;
        self.active_pane = FLEET_PANE;

        if rider_required {
            self.player_in_transit_ship = Some(ship_index);
            self.push_log(format!(
                "[{clock:04}] You board {} because it is the last local ship departing {}.",
                ship_name,
                self.location_name(origin),
                clock = self.clock,
            ));
        }

        self.push_log(format!(
            "[{clock:04}] Exploration sweep: {ship_name} aims {heading} from {origin_name} | ETA {eta} | fuel {fuel}.",
            clock = self.clock,
            ship_name = ship_name,
            heading = survey.heading,
            origin_name = self.location_name(origin),
            eta = survey.eta,
            fuel = survey.fuel_required,
        ));
        if matches!(survey.outcome, ExplorationTraceOutcome::Discovery) {
            self.push_log(format!(
                "[{clock:04}] Long-range returns lock onto {} strong contact(s).",
                survey.discovered_locations.len(),
                clock = self.clock,
            ));
        }
        let summary = match survey.outcome {
            ExplorationTraceOutcome::Discovery => format!(
                "Exploration sweep launched {} from {}. LOCK: {} strong contact(s). ETA {} | fuel {}.",
                survey.heading,
                self.location_name(origin),
                survey.discovered_locations.len(),
                survey.eta,
                survey.fuel_required,
            ),
            ExplorationTraceOutcome::Miss => format!(
                "Exploration sweep launched {} from {}. Only faint echoes so far. ETA {} | fuel {}.",
                survey.heading,
                self.location_name(origin),
                survey.eta,
                survey.fuel_required,
            ),
            ExplorationTraceOutcome::Empty => format!(
                "Exploration sweep launched {} from {}. No contact lock yet; this may resolve as empty space. ETA {} | fuel {}.",
                survey.heading,
                self.location_name(origin),
                survey.eta,
                survey.fuel_required,
            ),
        };
        self.set_action_feedback(if rider_required {
            format!("{} You are aboard {}.", summary, ship_name)
        } else {
            summary
        });
        self.fleet[ship_index].current_fuel = self.fleet[ship_index]
            .current_fuel
            .saturating_sub(survey.fuel_required);
        self.sync_low_fuel_alert(ship_index);
        self.evaluate_run_outcome();
    }

    fn preview_exploration_survey(&self, ship_index: usize) -> Option<ExplorationSurvey> {
        let origin_point = self.locations[self.fleet[ship_index].current_location]
            .sector_coords
            .as_tuple();
        let target_point = self.exploration_cursor.as_tuple();
        let ray_distance = distance(origin_point, target_point);
        if ray_distance < 2.0 {
            return None;
        }

        let exact_hits = self.exploration_ray_hits(
            ship_index,
            self.difficulty.exploration_discovery_width() + f64::from(self.fleet[ship_index].speed),
        );
        let near_hits = if exact_hits.is_empty() {
            self.exploration_ray_hits(
                ship_index,
                self.difficulty.exploration_glancing_width()
                    + f64::from(self.fleet[ship_index].speed) * 1.5,
            )
        } else {
            Vec::new()
        };

        let discovered_locations = exact_hits.iter().map(|hit| hit.target).collect::<Vec<_>>();
        let outcome = if !discovered_locations.is_empty() {
            ExplorationTraceOutcome::Discovery
        } else if !near_hits.is_empty() {
            ExplorationTraceOutcome::Miss
        } else {
            ExplorationTraceOutcome::Empty
        };

        let destination = discovered_locations
            .last()
            .copied()
            .unwrap_or(self.fleet[ship_index].current_location);
        let destination_point = self.locations[destination].sector_coords.as_tuple();
        let total_distance = ray_distance + distance(target_point, destination_point);
        let divisor = f64::from(self.fleet[ship_index].speed.max(1)) * 3.0;
        let fuel_required = (total_distance / divisor).ceil().max(1.0) as u16;
        let eta = fuel_required.max(1);

        Some(ExplorationSurvey {
            distance: ray_distance,
            fuel_required,
            eta,
            destination,
            heading: compass_heading(
                self.locations[self.fleet[ship_index].current_location].sector_coords,
                self.exploration_cursor,
            )
            .to_string(),
            discovered_locations,
            outcome,
        })
    }

    fn exploration_ray_hits(&self, ship_index: usize, width: f64) -> Vec<ExplorationRayHit> {
        let origin = self.locations[self.fleet[ship_index].current_location]
            .sector_coords
            .as_tuple();
        let target = self.exploration_cursor.as_tuple();
        let ray = (target.0 - origin.0, target.1 - origin.1);
        let ray_length = distance(origin, target);
        if ray_length < 1.0 {
            return Vec::new();
        }

        let ray_unit = (ray.0 / ray_length, ray.1 / ray_length);
        let mut hits = self
            .hidden_chartable_locations()
            .into_iter()
            .filter_map(|hidden| {
                let hidden_point = self.locations[hidden].sector_coords.as_tuple();
                let relative = (hidden_point.0 - origin.0, hidden_point.1 - origin.1);
                let projected_distance = relative.0 * ray_unit.0 + relative.1 * ray_unit.1;
                if projected_distance < 0.0 || projected_distance > ray_length {
                    return None;
                }
                let closest = (
                    origin.0 + ray_unit.0 * projected_distance,
                    origin.1 + ray_unit.1 * projected_distance,
                );
                let offset = distance(hidden_point, closest);
                (offset <= width).then_some(ExplorationRayHit {
                    target: hidden,
                    projected_distance,
                })
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| left.projected_distance.total_cmp(&right.projected_distance));
        hits
    }

    fn exploration_empty_target(&self, target: MapPoint) -> Option<usize> {
        let target_point = target.as_tuple();
        self.hidden_chartable_locations()
            .into_iter()
            .map(|hidden| {
                (
                    hidden,
                    distance(
                        self.locations[hidden].sector_coords.as_tuple(),
                        target_point,
                    ),
                )
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(hidden, _)| hidden)
    }

    fn active_exploration_ship_index(&self) -> Option<usize> {
        match self.mode {
            AppMode::SelectingDestination {
                ship_index,
                intent: DispatchIntent::Exploration,
            } => Some(ship_index),
            _ => None,
        }
    }

    fn closest_target_to_cursor_within(
        &self,
        candidates: Vec<usize>,
        max_distance: f64,
    ) -> Option<usize> {
        let cursor = self.exploration_cursor.as_tuple();
        candidates
            .into_iter()
            .filter_map(|index| {
                let distance_to_cursor =
                    distance(self.locations[index].sector_coords.as_tuple(), cursor);
                (distance_to_cursor <= max_distance).then_some((index, distance_to_cursor))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(index, _)| index)
    }

    fn clamp_exploration_cursor(&self, ship_index: usize, target: (f64, f64)) -> MapPoint {
        let origin = self.locations[self.fleet[ship_index].current_location]
            .sector_coords
            .as_tuple();
        let mut x = target.0.clamp(2.0, 98.0);
        let mut y = target.1.clamp(2.0, 58.0);
        let max_range = self.exploration_max_range_for_ship(ship_index);
        let current_distance = distance(origin, (x, y));
        if current_distance > max_range {
            let scale = max_range / current_distance;
            x = origin.0 + (x - origin.0) * scale;
            y = origin.1 + (y - origin.1) * scale;
        }

        MapPoint {
            x: x.round() as i16,
            y: y.round() as i16,
        }
    }

    pub(crate) fn player_can_operate_ship(&self, ship_index: usize) -> bool {
        !self.player_is_in_transit()
            && self
                .fleet
                .get(ship_index)
                .is_some_and(|ship| ship.current_location == self.player_location)
    }

    pub(crate) fn player_can_reach_station_for_operations(&self, location_index: usize) -> bool {
        (!self.player_is_in_transit() && location_index == self.player_location)
            || !self
                .player_transfer_candidate_indices(location_index)
                .is_empty()
    }

    pub(crate) fn local_ship_count(&self, location_index: usize) -> usize {
        self.fleet
            .iter()
            .filter(|ship| {
                ship.current_location == location_index
                    && matches!(&ship.state, ShipState::Docked | ShipState::Repairing { .. })
            })
            .count()
    }

    pub(crate) fn sector_count(&self) -> usize {
        self.locations.len() / SECTOR_LOCATION_COUNT
    }

    pub(crate) fn sector_index_for_location(&self, location_index: usize) -> usize {
        location_index / SECTOR_LOCATION_COUNT
    }

    pub(crate) fn sector_hub_index(&self, sector_index: usize) -> usize {
        sector_index * SECTOR_LOCATION_COUNT
    }

    pub(crate) fn sector_hub_index_for_location(&self, location_index: usize) -> usize {
        self.sector_hub_index(self.sector_index_for_location(location_index))
    }

    pub(crate) fn primary_hub_index(&self) -> usize {
        ASTRA_PRIME
    }

    pub(crate) fn is_sector_hub(&self, location_index: usize) -> bool {
        location_index % SECTOR_LOCATION_COUNT == 0
    }

    pub(crate) fn plan_route_for_ship(&self, ship_index: usize, to: usize) -> Option<RoutePlan> {
        let ship = &self.fleet[ship_index];
        self.plan_route(ship.current_location, to, ship.speed)
    }

    pub(crate) fn plan_route(&self, from: usize, to: usize, speed: u16) -> Option<RoutePlan> {
        if from == to || !self.is_discovered(from) || !self.is_discovered(to) {
            return None;
        }

        let from_sector = self.sector_index_for_location(from);
        let to_sector = self.sector_index_for_location(to);
        let from_hub = self.sector_hub_index_for_location(from);
        let to_hub = self.sector_hub_index_for_location(to);

        let mut segments = Vec::new();
        let mut segment_costs = Vec::new();
        let mut fuel_required = 0;
        let mut conditions = Vec::new();

        if from_sector == to_sector {
            fuel_required += self.location_leg_cost(
                from,
                from_hub,
                &mut segments,
                &mut segment_costs,
                &mut conditions,
            );
            fuel_required += self.location_leg_cost(
                to_hub,
                to,
                &mut segments,
                &mut segment_costs,
                &mut conditions,
            );
        } else {
            fuel_required += self.location_leg_cost(
                from,
                from_hub,
                &mut segments,
                &mut segment_costs,
                &mut conditions,
            );
            segments.push((from_hub, to_hub));
            let jump_cost = self.inter_sector_jump_cost(from_sector, to_sector);
            segment_costs.push(jump_cost);
            fuel_required += jump_cost;
            conditions.push(format!(
                "Jump corridor: {} -> {}",
                self.location_sector(from_hub),
                self.location_sector(to_hub)
            ));
            fuel_required += self.location_leg_cost(
                to_hub,
                to,
                &mut segments,
                &mut segment_costs,
                &mut conditions,
            );
        }

        Some(RoutePlan {
            path: self.route_path_from_segments(&segments),
            segments,
            segment_costs,
            eta: ceil_div_u16(fuel_required, speed.max(1)),
            fuel_required,
            condition_summary: if conditions.is_empty() {
                "stable lanes".to_string()
            } else {
                conditions.join(" | ")
            },
        })
    }

    fn location_leg_cost(
        &self,
        from: usize,
        to: usize,
        segments: &mut Vec<(usize, usize)>,
        segment_costs: &mut Vec<u16>,
        conditions: &mut Vec<String>,
    ) -> u16 {
        if from == to {
            return 0;
        }

        let leaf = if self.is_sector_hub(from) { to } else { from };
        let location = &self.locations[leaf];
        let condition = self.lane_condition(leaf);
        let cost = location.travel_time_from_hub + condition.penalty();
        segments.push((from, to));
        segment_costs.push(cost);
        conditions.push(format!("{}: {}", location.lane_name, condition.label()));
        cost
    }

    fn route_path_from_segments(&self, segments: &[(usize, usize)]) -> String {
        if segments.is_empty() {
            return "No route".to_string();
        }

        let mut nodes = vec![segments[0].0];
        for &(_, end) in segments {
            if nodes.last().copied() != Some(end) {
                nodes.push(end);
            }
        }

        nodes
            .into_iter()
            .map(|index| self.location_name(index).to_string())
            .collect::<Vec<_>>()
            .join(" -> ")
    }

    fn inter_sector_jump_cost(&self, from_sector: usize, to_sector: usize) -> u16 {
        8 + (from_sector.abs_diff(to_sector) as u16 * 4)
    }

    pub(crate) fn lane_condition(&self, leaf_index: usize) -> LaneCondition {
        match ((self.clock / 12) + leaf_index as u64) % 4 {
            0 => LaneCondition::Clear,
            1 => LaneCondition::Traffic,
            2 => LaneCondition::Debris,
            _ => LaneCondition::Solar,
        }
    }

    pub(crate) fn map_zoom_label(&self) -> &'static str {
        self.map_zoom.label()
    }

    pub(crate) fn location_region(&self, index: usize) -> &str {
        &self.locations[index].region_name
    }

    pub(crate) fn location_sector(&self, index: usize) -> &str {
        &self.locations[index].sector_name
    }

    pub(crate) fn location_name(&self, index: usize) -> &str {
        &self.locations[index].name
    }

    pub(crate) fn location_short_label(&self, index: usize) -> &str {
        &self.locations[index].short_label
    }

    pub(crate) fn location_description(&self, index: usize) -> &str {
        &self.locations[index].description
    }

    pub(crate) fn location_cluster(&self, index: usize) -> &str {
        &self.locations[index].cluster_name
    }

    pub(crate) fn location_system(&self, index: usize) -> &str {
        &self.locations[index].system_name
    }

    pub(crate) fn exploration_target(&self, index: usize) -> Option<usize> {
        self.locations[index].reveal_on_arrival.filter(|&target| {
            self.is_discovered(index)
                && !self.locations[index].exploration_exhausted
                && !self.is_charted_empty(target)
                && !self.is_discovered(target)
        })
    }

    pub(crate) fn exploration_heading_for_lead(&self, index: usize) -> Option<String> {
        let target = self.locations[index].reveal_on_arrival?;
        let from = self.locations[index].sector_coords;
        let to = self.locations[target].sector_coords;
        Some(compass_heading(from, to).to_string())
    }

    pub(crate) fn exploration_heading_hint(&self, index: usize) -> Option<String> {
        self.exploration_target(index)?;
        self.exploration_heading_for_lead(index)
    }

    pub(crate) fn map_focus_location_name(&self) -> &str {
        self.location_name(self.map_focus_location)
    }

    pub(crate) fn player_status_text(&self) -> String {
        if let Some(ship_index) = self.player_in_transit_ship {
            if let Some(ship) = self.fleet.get(ship_index)
                && let ShipState::EnRoute {
                    destination,
                    eta_remaining,
                    ..
                } = &ship.state
            {
                return format!(
                    "Aboard {} -> {} (ETA {})",
                    ship.name,
                    self.location_name(*destination),
                    eta_remaining
                );
            }
            "In transit".to_string()
        } else {
            self.location_name(self.player_location).to_string()
        }
    }

    pub(crate) fn map_scope_label(&self) -> String {
        if matches!(
            self.mode,
            AppMode::SelectingDestination {
                intent: DispatchIntent::Exploration,
                ..
            }
        ) && matches!(self.map_zoom, MapZoom::Sector)
        {
            return "Exploration Chart".to_string();
        }

        match self.map_zoom {
            MapZoom::Region => format!(
                "Region Network: {}",
                self.location_region(self.map_focus_location)
            ),
            MapZoom::Sector => format!("Sector: {}", self.location_sector(self.map_focus_location)),
            MapZoom::Cluster => format!(
                "Cluster: {}",
                self.location_cluster(self.map_focus_location)
            ),
            MapZoom::System => format!("System: {}", self.location_system(self.map_focus_location)),
        }
    }

    pub(crate) fn map_scope_locations(&self) -> Vec<usize> {
        if matches!(
            self.mode,
            AppMode::SelectingDestination {
                intent: DispatchIntent::Exploration,
                ..
            }
        ) && matches!(self.map_zoom, MapZoom::Sector)
        {
            return (0..self.locations.len()).collect();
        }

        match self.map_zoom {
            MapZoom::Region => (0..self.sector_count())
                .map(|sector| self.sector_hub_index(sector))
                .collect(),
            MapZoom::Sector => {
                let sector = self.sector_index_for_location(self.map_focus_location);
                let start = self.sector_hub_index(sector);
                (start..start + SECTOR_LOCATION_COUNT).collect()
            }
            MapZoom::Cluster => {
                let sector = self.sector_index_for_location(self.map_focus_location);
                let cluster = self.location_cluster(self.map_focus_location);
                let start = self.sector_hub_index(sector);
                (start..start + SECTOR_LOCATION_COUNT)
                    .filter(|&index| self.location_cluster(index) == cluster)
                    .collect()
            }
            MapZoom::System => {
                let sector = self.sector_index_for_location(self.map_focus_location);
                let system = self.location_system(self.map_focus_location);
                let start = self.sector_hub_index(sector);
                (start..start + SECTOR_LOCATION_COUNT)
                    .filter(|&index| self.location_system(index) == system)
                    .collect()
            }
        }
    }

    pub(crate) fn map_visible_locations(&self) -> Vec<usize> {
        self.map_scope_locations()
            .into_iter()
            .filter(|&index| self.is_discovered(index))
            .collect()
    }

    pub(crate) fn location_visible_in_map(&self, index: usize) -> bool {
        self.map_scope_locations().contains(&index)
    }

    pub(crate) fn map_coords(&self, index: usize) -> (f64, f64) {
        match self.map_zoom {
            MapZoom::Region => self.locations[index].region_coords.as_tuple(),
            MapZoom::Sector => self.locations[index].sector_coords.as_tuple(),
            MapZoom::Cluster => self.locations[index].cluster_coords.as_tuple(),
            MapZoom::System => self.locations[index].system_coords.as_tuple(),
        }
    }

    pub(crate) fn sync_map_focus_to_selected_location(&mut self) {
        self.map_focus_location = self.selected_location.min(self.locations.len() - 1);
        self.normalize_map_view();
    }

    pub(crate) fn auto_focus_map(&mut self) {
        let anchor = if let Some(ship_index) = self.pending_ship() {
            self.fleet[ship_index].current_location
        } else {
            self.fleet
                .get(self.selected_ship)
                .map(|ship| ship.current_location)
                .unwrap_or(self.selected_location)
        };

        self.selected_location = anchor.min(self.locations.len() - 1);
        self.map_focus_location = self.selected_location;
        self.normalize_map_view();
    }

    pub(crate) fn zoom_in_map(&mut self) {
        self.map_zoom = self.map_zoom.zoom_in();
        self.normalize_map_view();
    }

    pub(crate) fn zoom_out_map(&mut self) {
        self.map_zoom = self.map_zoom.zoom_out();
        self.normalize_map_view();
    }

    fn normalize_map_view(&mut self) {
        match self.map_zoom {
            MapZoom::Region => {
                self.selected_location = self.sector_hub_index_for_location(self.selected_location);
            }
            MapZoom::Sector => {
                let sector = self.sector_index_for_location(self.map_focus_location);
                if self.sector_index_for_location(self.selected_location) != sector {
                    self.selected_location = self.sector_hub_index(sector);
                }
            }
            _ => {}
        }

        let mut visible = self.map_visible_locations();
        while visible.is_empty() && self.map_zoom != MapZoom::Region {
            self.map_zoom = self.map_zoom.zoom_out();
            if self.map_zoom == MapZoom::Region {
                self.selected_location = self.sector_hub_index_for_location(self.selected_location);
            }
            visible = self.map_visible_locations();
        }

        if visible.is_empty() {
            return;
        }

        if !visible.contains(&self.selected_location) {
            self.selected_location = visible[0];
        }
        self.map_focus_location = self.selected_location;
    }

    pub(crate) fn is_discovered(&self, index: usize) -> bool {
        self.discovered_locations[index]
    }

    pub(crate) fn is_charted_empty(&self, index: usize) -> bool {
        self.locations[index].charted_empty
    }

    pub(crate) fn is_charted(&self, index: usize) -> bool {
        self.is_discovered(index) || self.is_charted_empty(index)
    }

    pub(crate) fn discovered_count(&self) -> usize {
        (0..self.locations.len())
            .filter(|&index| self.is_charted(index))
            .count()
    }

    pub(crate) fn undiscovered_count(&self) -> usize {
        self.locations.len() - self.discovered_count()
    }

    pub(crate) fn next_discovered_location(&self, current: usize, delta: isize) -> usize {
        let visible = self.map_visible_locations();
        if visible.is_empty() {
            return current;
        }

        if let Some(position) = visible.iter().position(|&index| index == current) {
            visible[wrap_index(position, visible.len(), delta)]
        } else {
            visible[0]
        }
    }

    pub(crate) fn first_dispatch_target(&self, origin: usize) -> Option<usize> {
        (0..self.locations.len()).find(|&index| self.is_discovered(index) && index != origin)
    }

    pub(crate) fn visible_map_links(&self) -> Vec<(usize, usize)> {
        if matches!(
            self.mode,
            AppMode::SelectingDestination {
                intent: DispatchIntent::Exploration,
                ..
            }
        ) && matches!(self.map_zoom, MapZoom::Sector)
        {
            let mut links = Vec::new();
            for sector in 0..self.sector_count() {
                let hub = self.sector_hub_index(sector);
                for location in hub + 1..(hub + SECTOR_LOCATION_COUNT).min(self.locations.len()) {
                    links.push((hub, location));
                }
            }
            return links;
        }

        match self.map_zoom {
            MapZoom::Region => {
                let hubs = self.map_scope_locations();
                hubs.windows(2)
                    .map(|window| (window[0], window[1]))
                    .collect()
            }
            _ => {
                let hub = self.sector_hub_index_for_location(self.map_focus_location);
                self.map_scope_locations()
                    .into_iter()
                    .filter(|&location| location != hub)
                    .map(|location| (hub, location))
                    .collect()
            }
        }
    }

    pub(crate) fn highlighted_route_segments(&self) -> Option<Vec<(usize, usize)>> {
        if let Some(ship_index) = self.pending_ship() {
            if matches!(
                self.pending_dispatch_intent(),
                Some(DispatchIntent::Exploration)
            ) {
                return None;
            }
            let origin = self.fleet[ship_index].current_location;
            let plan =
                self.plan_route(origin, self.selected_location, self.fleet[ship_index].speed)?;
            return Some(
                plan.segments
                    .into_iter()
                    .filter(|(start, end)| {
                        self.location_visible_in_map(*start) && self.location_visible_in_map(*end)
                    })
                    .collect(),
            );
        }

        let ship = &self.fleet[self.selected_ship];
        if !ship.is_docked() {
            return None;
        }

        let plan = self.plan_route(ship.current_location, self.selected_location, ship.speed)?;
        Some(
            plan.segments
                .into_iter()
                .filter(|(start, end)| {
                    self.location_visible_in_map(*start) && self.location_visible_in_map(*end)
                })
                .collect(),
        )
    }
}

struct ExplorationRayHit {
    target: usize,
    projected_distance: f64,
}

struct ExplorationSurvey {
    distance: f64,
    fuel_required: u16,
    eta: u16,
    destination: usize,
    heading: String,
    discovered_locations: Vec<usize>,
    outcome: ExplorationTraceOutcome,
}

fn distance(from: (f64, f64), to: (f64, f64)) -> f64 {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    (dx * dx + dy * dy).sqrt()
}

fn compass_heading(from: MapPoint, to: MapPoint) -> &'static str {
    let dx = f64::from(to.x - from.x);
    let dy = f64::from(to.y - from.y);
    let horizontal = if dx > 3.0 {
        "east"
    } else if dx < -3.0 {
        "west"
    } else {
        ""
    };
    let vertical = if dy > 3.0 {
        "south"
    } else if dy < -3.0 {
        "north"
    } else {
        ""
    };

    match (vertical, horizontal) {
        ("north", "east") => "to the northeast",
        ("north", "west") => "to the northwest",
        ("south", "east") => "to the southeast",
        ("south", "west") => "to the southwest",
        ("north", "") => "to the north",
        ("south", "") => "to the south",
        ("", "east") => "to the east",
        ("", "west") => "to the west",
        _ => "nearby",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variable_world_flavor(sector_count: usize) -> WorldFlavor {
        let mut locations = Vec::new();
        for sector in 0..sector_count {
            let region_name = if sector < 2 {
                "Helios Frontier"
            } else {
                "Perihelion Reach"
            };
            let sector_name = format!("Sector {}", sector + 1);
            for (role, suffix) in ["Prime", "Relay", "Anchorage", "Harbor", "Signal"]
                .into_iter()
                .enumerate()
            {
                locations.push(WorldLocationFlavor {
                    region_name: region_name.to_string(),
                    sector_name: sector_name.clone(),
                    name: format!("{} {}", sector_name, suffix),
                    short_label: format!("S{}{}", sector + 1, role + 1),
                    lane_name: format!("{} Lane", suffix),
                    description: format!("Generated {} location {}.", suffix, role + 1),
                    cluster_name: format!("Cluster {}", role % 2 + 1),
                    system_name: format!("System {}", role % 3 + 1),
                });
            }
        }

        WorldFlavor {
            environment_name: "Variable Frontier".to_string(),
            environment_summary: "A test environment with variable sector count.".to_string(),
            locations,
            starter_ships: Vec::new(),
            shipyard_offers: Vec::new(),
        }
    }

    #[test]
    fn ship_speed_changes_contract_eligibility() {
        let mut game = GameData::new(Difficulty::Normal);
        game.discovered_locations[KITE_STATION] = true;
        game.tracked_contract = Some(3);
        game.contracts[3].state = ContractState::Accepted { accepted_at: 0 };

        game.fleet[1].state = ShipState::Docked;
        game.fleet[1].current_location = ASTRA_PRIME;
        game.fleet[1].current_fuel = 16;

        let fast_eta = game.plan_route_for_ship(0, KITE_STATION).unwrap().eta;
        let slow_eta = game.plan_route_for_ship(1, KITE_STATION).unwrap().eta;

        assert!(fast_eta <= game.contracts[3].max_eta);
        assert!(slow_eta > game.contracts[3].max_eta);
        assert_eq!(
            game.matching_tracked_contract(0, KITE_STATION, fast_eta),
            Some(3)
        );
        assert_eq!(
            game.matching_tracked_contract(1, KITE_STATION, slow_eta),
            None
        );
    }

    #[test]
    fn zooming_cluster_filters_location_selection_to_scope() {
        let mut game = GameData::new(Difficulty::Normal);
        game.discovered_locations[KITE_STATION] = true;
        game.discovered_locations[ION_ANCHORAGE] = true;
        game.selected_location = DUST_HARBOR;
        game.sync_map_focus_to_selected_location();

        game.zoom_in_map();

        assert_eq!(game.map_zoom, MapZoom::Cluster);
        assert_eq!(game.next_discovered_location(DUST_HARBOR, 1), ASTRA_PRIME);
        assert!(
            !game
                .map_visible_locations()
                .contains(&(SECTOR_LOCATION_COUNT + 2))
        );
    }

    #[test]
    fn inter_sector_route_uses_jump_corridor() {
        let mut game = GameData::new(Difficulty::Normal);
        game.discovered_locations[SECTOR_LOCATION_COUNT] = true;
        let plan = game
            .plan_route(ASTRA_PRIME, SECTOR_LOCATION_COUNT, 3)
            .unwrap();
        assert!(plan.path.contains("Astra Prime"));
        assert!(plan.path.contains("Vesper Exchange"));
        assert_eq!(plan.segments, vec![(ASTRA_PRIME, SECTOR_LOCATION_COUNT)]);
        assert!(plan.fuel_required >= 8);
    }

    #[test]
    fn qualifying_dispatch_assigns_active_mission_and_keeps_it_active() {
        let mut game = GameData::new(Difficulty::Normal);
        game.discovered_locations[KITE_STATION] = true;
        game.selected_ship = 0;
        game.tracked_contract = Some(3);
        game.contracts[3].state = ContractState::Accepted { accepted_at: 0 };

        game.begin_dispatch();
        game.confirm_dispatch();

        assert!(matches!(
            game.contracts[3].state,
            ContractState::Assigned { ship_index: 0, .. }
        ));
        assert_eq!(game.tracked_contract, Some(3));
        assert!(matches!(
            game.fleet[0].state,
            ShipState::EnRoute {
                assigned_contract: Some(3),
                ..
            }
        ));
    }

    #[test]
    fn dispatching_last_local_ship_carries_player_with_it() {
        let mut game = GameData::new(Difficulty::Normal);
        game.fleet[1].current_location = DUST_HARBOR;
        game.fleet[1].state = ShipState::Docked;
        game.fleet[2].current_location = KITE_STATION;
        game.selected_ship = 0;
        game.selected_location = DUST_HARBOR;

        game.begin_dispatch();
        game.confirm_dispatch();

        assert_eq!(game.player_in_transit_ship, Some(0));
    }

    #[test]
    fn transfer_player_to_selected_location_uses_selected_local_ship() {
        let mut game = GameData::new(Difficulty::Normal);
        game.selected_location = DUST_HARBOR;
        game.selected_ship = 0;

        game.transfer_player_to_selected_location();

        assert_eq!(game.player_in_transit_ship, None);
        assert_eq!(game.player_location, DUST_HARBOR);
        assert_eq!(game.fleet[0].current_location, DUST_HARBOR);
        assert!(
            game.take_action_feedback()
                .is_some_and(|message| { message.contains("Transfer complete") })
        );
    }

    #[test]
    fn boarded_ship_arrival_moves_player_with_the_ship() {
        let mut game = GameData::new(Difficulty::Normal);
        game.fleet[1].current_location = DUST_HARBOR;
        game.fleet[1].state = ShipState::Docked;
        game.fleet[2].current_location = KITE_STATION;
        game.selected_ship = 0;
        game.selected_location = DUST_HARBOR;

        game.begin_dispatch();
        game.confirm_dispatch();

        assert_eq!(game.player_in_transit_ship, Some(0));

        for _ in 0..12 {
            if !game.player_is_in_transit() {
                break;
            }
            game.tick();
        }

        assert_eq!(game.player_in_transit_ship, None);
        assert_eq!(game.player_location, DUST_HARBOR);
        assert_eq!(game.fleet[0].current_location, DUST_HARBOR);
        assert!(matches!(game.fleet[0].state, ShipState::Docked));
    }

    #[test]
    fn exploration_ray_discovers_contact_and_empty_vector_blocks_repeat() {
        let mut game = GameData::new(Difficulty::Normal);
        game.discovered_locations[KITE_STATION] = true;
        game.fleet[0].current_location = KITE_STATION;
        game.fleet[0].state = ShipState::Docked;
        game.player_location = KITE_STATION;
        game.selected_ship = 0;

        game.begin_exploration();
        assert_eq!(game.selected_location, KITE_STATION);
        game.exploration_cursor = game.locations[ION_ANCHORAGE].sector_coords;
        game.confirm_dispatch();

        assert!(matches!(game.fleet[0].state, ShipState::EnRoute { .. }));

        for _ in 0..12 {
            if matches!(game.fleet[0].state, ShipState::Docked) {
                break;
            }
            game.tick();
        }

        assert_eq!(
            game.visible_exploration_traces()[0].target.as_tuple(),
            game.locations[ION_ANCHORAGE].sector_coords.as_tuple()
        );
        assert!(game.discovered_locations[ION_ANCHORAGE]);
    }

    #[test]
    fn exploration_launch_feedback_mentions_locked_contacts() {
        let mut game = GameData::new(Difficulty::Normal);
        game.discovered_locations[KITE_STATION] = true;
        game.fleet[0].current_location = KITE_STATION;
        game.fleet[0].state = ShipState::Docked;
        game.player_location = KITE_STATION;
        game.selected_ship = 0;

        game.begin_exploration();
        game.exploration_cursor = game.locations[ION_ANCHORAGE].sector_coords;
        game.confirm_dispatch();

        assert!(
            game.take_action_feedback()
                .is_some_and(|message| message.contains("LOCK: 1 strong contact"))
        );
    }

    #[test]
    fn exploration_empty_trace_blocks_repeat_vector() {
        let mut game = GameData::new(Difficulty::Normal);
        game.discovered_locations[KITE_STATION] = true;
        game.fleet[0].current_location = KITE_STATION;
        game.fleet[0].state = ShipState::Docked;
        game.player_location = KITE_STATION;
        game.selected_ship = 0;

        let origin = game.locations[KITE_STATION].sector_coords;
        let blocked_target = crate::game::MapPoint {
            x: origin.x + 10,
            y: origin.y - 12,
        };
        game.exploration_traces.push(ExplorationTrace {
            origin: KITE_STATION,
            target: blocked_target,
            outcome: ExplorationTraceOutcome::Empty,
        });

        game.begin_exploration();
        game.exploration_cursor = blocked_target;
        game.confirm_dispatch();

        assert!(
            game.take_action_feedback()
                .is_some_and(|message| message.contains("already charted as empty space"))
        );
    }

    #[test]
    fn empty_exploration_charts_hidden_region_as_empty() {
        let mut game = GameData::new(Difficulty::Normal);
        game.discovered_locations[KITE_STATION] = true;

        game.resolve_exploration_arrival(
            KITE_STATION,
            game.locations[ION_ANCHORAGE].sector_coords,
            ExplorationTraceOutcome::Empty,
            &[],
            "SV Kestrel",
        );

        assert!(game.is_charted_empty(ION_ANCHORAGE));
        assert!(
            !game
                .available_exploration_leads()
                .iter()
                .any(|&(lead, target)| lead == KITE_STATION && target == ION_ANCHORAGE)
        );
        assert!(!game.is_charted_empty(KITE_STATION));
    }

    #[test]
    fn empty_exploration_from_other_origin_still_charts_hidden_region_empty() {
        let mut game = GameData::new(Difficulty::Normal);
        game.discovered_locations[KITE_STATION] = true;

        game.resolve_exploration_arrival(
            ASTRA_PRIME,
            game.locations[ION_ANCHORAGE].sector_coords,
            ExplorationTraceOutcome::Empty,
            &[],
            "SV Kestrel",
        );

        assert!(game.is_charted_empty(ION_ANCHORAGE));
    }

    #[test]
    fn cozy_exploration_has_wider_detection_field_than_insane() {
        let mut cozy = GameData::new(Difficulty::Cozy);
        cozy.discovered_locations[KITE_STATION] = true;
        cozy.fleet[0].current_location = KITE_STATION;
        cozy.fleet[0].state = ShipState::Docked;
        cozy.selected_ship = 0;

        let mut insane = GameData::new(Difficulty::Insane);
        insane.discovered_locations[KITE_STATION] = true;
        insane.fleet[0].current_location = KITE_STATION;
        insane.fleet[0].state = ShipState::Docked;
        insane.selected_ship = 0;

        let mut found = None;
        for x in 5..95 {
            for y in 5..55 {
                let cursor = MapPoint { x, y };
                cozy.exploration_cursor = cursor;
                insane.exploration_cursor = cursor;
                let cozy_outcome = cozy
                    .preview_exploration_survey(0)
                    .map(|survey| survey.outcome);
                let insane_outcome = insane
                    .preview_exploration_survey(0)
                    .map(|survey| survey.outcome);
                if matches!(cozy_outcome, Some(ExplorationTraceOutcome::Discovery))
                    && !matches!(insane_outcome, Some(ExplorationTraceOutcome::Discovery))
                {
                    found = Some(cursor);
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }

        assert!(
            found.is_some(),
            "expected at least one cursor where Cozy detects and Insane does not"
        );
    }

    #[test]
    fn variable_world_size_builds_additional_sector_chain() {
        let game = GameData::new_seeded(Difficulty::Normal, 42, Some(variable_world_flavor(3)));

        assert_eq!(game.locations.len(), 15);
        assert_eq!(game.sector_count(), 3);
        assert_eq!(
            game.locations[SECTOR_LOCATION_COUNT - 1].reveal_on_arrival,
            Some(SECTOR_LOCATION_COUNT)
        );
        assert_eq!(
            game.locations[SECTOR_LOCATION_COUNT * 2 - 1].reveal_on_arrival,
            Some(SECTOR_LOCATION_COUNT * 2)
        );
        assert_eq!(
            game.locations[SECTOR_LOCATION_COUNT * 3 - 1].reveal_on_arrival,
            None
        );
    }
}
