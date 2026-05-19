use super::*;

impl GameData {
    pub(crate) fn pending_ship(&self) -> Option<usize> {
        match self.mode {
            AppMode::Browse => None,
            AppMode::SelectingDestination { ship_index } => Some(ship_index),
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
        let ship_index = self.selected_ship;
        let ship_name = self.fleet[ship_index].name;
        let ship_location = self.fleet[ship_index].current_location;
        let ship_is_docked = self.fleet[ship_index].is_docked();
        let ship_fuel = self.fleet[ship_index].current_fuel;

        if !ship_is_docked {
            self.push_log(format!(
                "[{clock:04}] {name} is already in transit and cannot be reassigned.",
                clock = self.clock,
                name = ship_name,
            ));
            return;
        }

        if self.difficulty.uses_fuel_economy() && ship_fuel == 0 {
            self.push_log(format!(
                "[{clock:04}] {name} has no fuel. Refuel with `f` or transfer fuel with `t` before plotting a route.",
                clock = self.clock,
                name = ship_name,
            ));
            return;
        }

        self.mode = AppMode::SelectingDestination { ship_index };
        self.active_pane = MAP_PANE;

        if let Some(contract_index) = self.tracked_contract {
            let contract = &self.contracts[contract_index];
            if matches!(contract.state, ContractState::Accepted { .. })
                && contract.origin == ship_location
            {
                self.selected_location = contract.destination;
                return;
            }
        }

        if !self.is_discovered(self.selected_location) || self.selected_location == ship_location {
            if let Some(destination) = self.first_dispatch_target(ship_location) {
                self.selected_location = destination;
            }
        }
    }

    pub(crate) fn confirm_dispatch(&mut self) {
        let AppMode::SelectingDestination { ship_index } = self.mode else {
            return;
        };

        let ship_name = self.fleet[ship_index].name;
        let origin = self.fleet[ship_index].current_location;

        let Some(plan) = self.plan_route_for_ship(ship_index, self.selected_location) else {
            self.push_log(format!(
                "[{clock:04}] Pick a different charted destination before dispatching {name}.",
                clock = self.clock,
                name = ship_name,
            ));
            return;
        };

        let destination = self.selected_location;
        if self.fleet[ship_index].current_fuel < plan.fuel_required {
            self.push_log(format!(
                "[{clock:04}] {name} needs fuel before it can make this route.",
                clock = self.clock,
                name = ship_name,
            ));
            return;
        }

        let assigned_contract = self.matching_tracked_contract(ship_index, destination, plan.eta);

        if let Some(contract_index) = assigned_contract {
            let accepted_at = match self.contracts[contract_index].state {
                ContractState::Accepted { accepted_at } => accepted_at,
                _ => self.clock,
            };
            self.contracts[contract_index].state = ContractState::Assigned {
                ship_name,
                accepted_at,
            };
        }

        self.fleet[ship_index].state = ShipState::EnRoute {
            origin,
            destination,
            eta_remaining: plan.eta,
            total_eta: plan.eta,
            route: plan.path.clone(),
            condition_summary: plan.condition_summary.clone(),
            assigned_contract,
        };
        self.mode = AppMode::Browse;
        self.active_pane = FLEET_PANE;

        self.push_log(format!(
            "[{clock:04}] {name} dispatched: {path} | ETA {eta} | {conditions}",
            clock = self.clock,
            name = ship_name,
            path = plan.path,
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
        } else if let Some(contract_index) = self.tracked_contract {
            let contract = &self.contracts[contract_index];
            let same_route = contract.origin == origin && contract.destination == destination;
            let message = if same_route && plan.eta > contract.max_eta {
                format!(
                    "[{clock:04}] Survey run only. {title} requires ETA <= {max_eta}, but this ship needs {eta}.",
                    clock = self.clock,
                    title = contract.title,
                    max_eta = contract.max_eta,
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

    pub(crate) fn plan_route_for_ship(&self, ship_index: usize, to: usize) -> Option<RoutePlan> {
        let ship = &self.fleet[ship_index];
        self.plan_route(ship.current_location, to, ship.speed)
    }

    pub(crate) fn plan_route(&self, from: usize, to: usize, speed: u16) -> Option<RoutePlan> {
        if from == to || !self.is_discovered(from) || !self.is_discovered(to) {
            return None;
        }

        if from == ASTRA_PRIME || to == ASTRA_PRIME {
            let leaf = if from == ASTRA_PRIME { to } else { from };
            let location = &self.locations[leaf];
            let condition = self.lane_condition(leaf);
            let fuel_required = location.travel_time_from_hub + condition.penalty();

            return Some(RoutePlan {
                path: format!("{} -> {}", self.location_name(from), self.location_name(to)),
                eta: ceil_div_u16(fuel_required, speed.max(1)),
                fuel_required,
                condition_summary: format!("{}: {}", location.lane_name, condition.label()),
            });
        }

        let outbound = &self.locations[from];
        let inbound = &self.locations[to];
        let outbound_condition = self.lane_condition(from);
        let inbound_condition = self.lane_condition(to);
        let fuel_required = outbound.travel_time_from_hub
            + outbound_condition.penalty()
            + inbound.travel_time_from_hub
            + inbound_condition.penalty();

        Some(RoutePlan {
            path: format!(
                "{} -> {} -> {}",
                self.location_name(from),
                self.location_name(ASTRA_PRIME),
                self.location_name(to)
            ),
            eta: ceil_div_u16(fuel_required, speed.max(1)),
            fuel_required,
            condition_summary: format!(
                "{}: {} | {}: {}",
                outbound.lane_name,
                outbound_condition.label(),
                inbound.lane_name,
                inbound_condition.label(),
            ),
        })
    }

    pub(crate) fn lane_condition(&self, leaf_index: usize) -> LaneCondition {
        match ((self.clock / 12) + leaf_index as u64) % 4 {
            0 => LaneCondition::Clear,
            1 => LaneCondition::Traffic,
            2 => LaneCondition::Debris,
            _ => LaneCondition::Solar,
        }
    }

    pub(crate) fn location_name(&self, index: usize) -> &'static str {
        self.locations[index].name
    }

    pub(crate) fn is_discovered(&self, index: usize) -> bool {
        self.discovered_locations[index]
    }

    pub(crate) fn discovered_count(&self) -> usize {
        self.discovered_locations
            .iter()
            .filter(|&&seen| seen)
            .count()
    }

    pub(crate) fn undiscovered_count(&self) -> usize {
        self.locations.len() - self.discovered_count()
    }

    pub(crate) fn next_discovered_location(&self, current: usize, delta: isize) -> usize {
        let mut next = current;

        for _ in 0..self.locations.len() {
            next = wrap_index(next, self.locations.len(), delta);
            if self.is_discovered(next) {
                return next;
            }
        }

        current
    }

    pub(crate) fn first_dispatch_target(&self, origin: usize) -> Option<usize> {
        (0..self.locations.len()).find(|&index| self.is_discovered(index) && index != origin)
    }

    pub(crate) fn highlighted_route(&self) -> Option<(usize, usize)> {
        if let Some(ship_index) = self.pending_ship() {
            let origin = self.fleet[ship_index].current_location;
            return self
                .plan_route(origin, self.selected_location, self.fleet[ship_index].speed)
                .map(|_| (origin, self.selected_location));
        }

        let ship = &self.fleet[self.selected_ship];
        if !ship.is_docked() {
            return None;
        }

        self.plan_route(ship.current_location, self.selected_location, ship.speed)
            .map(|_| (ship.current_location, self.selected_location))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
