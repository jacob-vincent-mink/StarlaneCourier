use super::*;

impl GameData {
    pub(crate) fn next_discovery_target(&self) -> Option<(usize, usize)> {
        (0..self.locations.len()).find_map(|index| {
            self.locations[index]
                .reveal_on_arrival
                .filter(|&target| {
                    self.is_discovered(index)
                        && !self.locations[index].exploration_exhausted
                        && !self.is_charted_empty(target)
                        && !self.is_discovered(target)
                })
                .map(|target| (index, target))
        })
    }

    pub(crate) fn is_frontier_location(&self, index: usize) -> bool {
        self.is_discovered(index)
            && self.locations[index]
                .reveal_on_arrival
                .is_some_and(|target| {
                    !self.locations[index].exploration_exhausted
                        && !self.is_charted_empty(target)
                        && !self.is_discovered(target)
                })
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

        if self.is_discovered(target)
            || self.is_charted_empty(target)
            || self.locations[location_index].exploration_exhausted
        {
            return None;
        }

        let attempt = self.locations[location_index].exploration_attempts;
        self.locations[location_index].exploration_attempts = attempt.saturating_add(1);
        let heading = self
            .exploration_heading_hint(location_index)
            .unwrap_or_else(|| "into the uncharted reach".to_string());

        match self.exploration_survey_outcome(location_index, target, attempt) {
            ExplorationSurveyOutcome::Discovery => {
                self.discovered_locations[target] = true;
                Some(format!(
                    "[{clock:04}] Exploration sweep from {origin} charted a new destination {heading}: {destination}.",
                    clock = self.clock,
                    origin = self.location_name(location_index),
                    heading = heading,
                    destination = self.location_name(target),
                ))
            }
            ExplorationSurveyOutcome::Miss => Some(format!(
                "[{clock:04}] Exploration sweep from {origin} searched {heading} but found no chartable contact yet.",
                clock = self.clock,
                origin = self.location_name(location_index),
                heading = heading,
            )),
            ExplorationSurveyOutcome::Exhausted => {
                self.locations[location_index].exploration_exhausted = true;
                Some(format!(
                    "[{clock:04}] Exploration sweep from {origin} mapped {heading} and found only empty space. No further leads remain there.",
                    clock = self.clock,
                    origin = self.location_name(location_index),
                    heading = heading,
                ))
            }
        }
    }

    fn exploration_survey_outcome(
        &self,
        location_index: usize,
        target: usize,
        attempt: u8,
    ) -> ExplorationSurveyOutcome {
        match (target + self.sector_index_for_location(location_index)) % 4 {
            0 | 1 => ExplorationSurveyOutcome::Discovery,
            2 => {
                if attempt == 0 {
                    ExplorationSurveyOutcome::Miss
                } else {
                    ExplorationSurveyOutcome::Discovery
                }
            }
            _ => {
                if attempt == 0 {
                    ExplorationSurveyOutcome::Miss
                } else {
                    ExplorationSurveyOutcome::Exhausted
                }
            }
        }
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

enum ExplorationSurveyOutcome {
    Discovery,
    Miss,
    Exhausted,
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

    #[test]
    fn exploration_sweep_can_miss_before_discovery() {
        let mut game = GameData::new(Difficulty::Normal);
        game.discovered_locations[KITE_STATION] = true;
        game.discovered_locations[ION_ANCHORAGE] = false;

        let first = game.reveal_from_arrival(KITE_STATION).unwrap();
        assert!(first.contains("found no chartable contact yet"));
        assert!(!game.discovered_locations[ION_ANCHORAGE]);
        assert_eq!(game.locations[KITE_STATION].exploration_attempts, 1);

        let second = game.reveal_from_arrival(KITE_STATION).unwrap();
        assert!(second.contains(game.location_name(ION_ANCHORAGE)));
        assert!(game.discovered_locations[ION_ANCHORAGE]);
    }

    #[test]
    fn exploration_sweep_can_mark_a_reach_empty() {
        let mut game = GameData::new(Difficulty::Normal);
        game.locations[ASTRA_PRIME].reveal_on_arrival = Some(DUST_HARBOR);
        game.locations[ASTRA_PRIME].exploration_attempts = 0;
        game.locations[ASTRA_PRIME].exploration_exhausted = false;
        game.discovered_locations[DUST_HARBOR] = false;

        let first = game.reveal_from_arrival(ASTRA_PRIME).unwrap();
        assert!(first.contains("found no chartable contact yet"));
        assert!(game.is_frontier_location(ASTRA_PRIME));

        let second = game.reveal_from_arrival(ASTRA_PRIME).unwrap();
        assert!(second.contains("found only empty space"));
        assert!(game.locations[ASTRA_PRIME].exploration_exhausted);
        assert!(!game.discovered_locations[DUST_HARBOR]);
        assert!(!game.is_frontier_location(ASTRA_PRIME));
    }
}
