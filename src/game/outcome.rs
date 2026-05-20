use super::*;

impl GameData {
    pub(crate) fn next_discovery_target(&self) -> Option<(usize, usize)> {
        (0..self.locations.len()).find_map(|index| {
            self.locations[index]
                .reveal_on_arrival
                .filter(|&target| self.is_discovered(index) && !self.is_discovered(target))
                .map(|target| (index, target))
        })
    }

    pub(crate) fn is_frontier_location(&self, index: usize) -> bool {
        self.is_discovered(index)
            && self.locations[index]
                .reveal_on_arrival
                .is_some_and(|target| !self.is_discovered(target))
    }

    pub(crate) fn frontier_locations(&self) -> Vec<usize> {
        (0..self.locations.len())
            .filter(|&index| self.is_frontier_location(index))
            .collect()
    }

    pub(crate) fn discovered_lane_locations(&self) -> Vec<usize> {
        let mut discovered: Vec<usize> = (0..self.locations.len())
            .filter(|&index| self.is_discovered(index) && !self.is_sector_hub(index))
            .collect();

        if discovered.is_empty() {
            discovered.push(self.primary_hub_index());
        }

        discovered
    }

    pub(crate) fn reveal_from_arrival(&mut self, location_index: usize) -> Option<String> {
        let target = self.locations[location_index].reveal_on_arrival?;

        if self.is_discovered(target) {
            return None;
        }

        self.discovered_locations[target] = true;

        Some(format!(
            "[{clock:04}] Survey traffic from {origin} charted a new destination: {destination}.",
            clock = self.clock,
            origin = self.location_name(location_index),
            destination = self.location_name(target),
        ))
    }

    pub(crate) fn in_transit_count(&self) -> usize {
        self.fleet.iter().filter(|ship| !ship.is_docked()).count()
    }

    pub(crate) fn has_viable_progress_path(&self) -> bool {
        if self.fleet.iter().any(|ship| !ship.is_docked()) {
            return true;
        }

        for (index, contract) in self.contracts.iter().enumerate() {
            if !self.is_contract_unlocked(index) {
                continue;
            }

            match contract.state {
                ContractState::Available | ContractState::Accepted { .. } => {
                    for (ship_index, ship) in self.fleet.iter().enumerate() {
                        if ship.is_docked()
                            && ship.current_location == contract.origin
                            && self.player_can_reach_station_for_operations(ship.current_location)
                            && self
                                .plan_route_for_ship(ship_index, contract.destination)
                                .is_some_and(|plan| {
                                    plan.eta <= contract.max_eta
                                        && self.can_stage_route(ship_index, plan.fuel_required)
                                })
                        {
                            return true;
                        }
                    }
                }
                ContractState::Assigned { .. } => return true,
                ContractState::Completed | ContractState::Failed => {}
            }
        }

        for frontier in self.frontier_locations() {
            for ship_index in 0..self.fleet.len() {
                if self.can_complete_route_from(ship_index, frontier)
                    && self.player_can_reach_station_for_operations(
                        self.fleet[ship_index].current_location,
                    )
                {
                    return true;
                }
            }
        }

        for contract in &self.contracts {
            if matches!(
                contract.state,
                ContractState::Completed | ContractState::Failed
            ) {
                continue;
            }

            for ship_index in 0..self.fleet.len() {
                if self.can_complete_route_from(ship_index, contract.origin)
                    && self.player_can_reach_station_for_operations(
                        self.fleet[ship_index].current_location,
                    )
                {
                    return true;
                }
            }
        }

        if self.can_buy_ship_that_unlocks_progress() {
            return true;
        }

        false
    }

    pub(crate) fn evaluate_run_outcome(&mut self) {
        if self.run_outcome.is_some() || matches!(self.difficulty, Difficulty::Cozy) {
            return;
        }

        if self.discovered_count() == self.locations.len() && self.credits >= GOAL_CREDITS {
            self.run_outcome = Some(RunOutcome::Won);
            self.push_log(format!(
                "[{clock:04}] Shift complete. You charted the full environment and reached {} credits.",
                GOAL_CREDITS,
                clock = self.clock,
            ));
            return;
        }

        if !self.has_viable_progress_path() {
            let reason = if self.difficulty.uses_fuel_economy() {
                "No viable contract or frontier route remains. You cannot afford enough fuel to make more meaningful runs."
                    .to_string()
            } else {
                "No viable contract or frontier route remains.".to_string()
            };
            self.run_outcome = Some(RunOutcome::Lost {
                reason: reason.clone(),
            });
            self.push_log(format!(
                "[{clock:04}] Shift lost. {reason}",
                clock = self.clock,
                reason = reason,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_viable_progress_when_every_ship_is_unfueled_and_broke() {
        let mut game = GameData::new(Difficulty::Normal);
        game.credits = 0;
        game.station_fuel = vec![0; game.station_fuel.len()];
        for ship in &mut game.fleet {
            ship.state = ShipState::Docked;
            ship.current_location = ASTRA_PRIME;
            ship.current_fuel = 0;
        }

        assert!(!game.has_viable_progress_path());
    }
}
