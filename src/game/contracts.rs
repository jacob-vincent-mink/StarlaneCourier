use super::*;

impl GameData {
    pub(crate) fn is_contract_unlocked(&self, index: usize) -> bool {
        let contract = &self.contracts[index];
        self.is_discovered(contract.origin)
            && self.is_discovered(contract.destination)
            && self.is_discovered(contract.unlock_location)
    }

    pub(crate) fn contract_status_label(&self, index: usize) -> &'static str {
        match self.contracts[index].state {
            ContractState::Available => "open",
            ContractState::Accepted { .. } => "tracked",
            ContractState::Assigned { .. } => "assigned",
            ContractState::Completed => "complete",
            ContractState::Failed => "failed",
        }
    }

    pub(crate) fn contract_elapsed_ticks(&self, index: usize) -> Option<u64> {
        match self.contracts[index].state {
            ContractState::Accepted { accepted_at }
            | ContractState::Assigned { accepted_at, .. } => {
                Some(self.clock.saturating_sub(accepted_at))
            }
            _ => None,
        }
    }

    pub(crate) fn contract_current_reward(&self, index: usize) -> i32 {
        let contract = &self.contracts[index];
        let elapsed = self.contract_elapsed_ticks(index).unwrap_or(0);
        self.difficulty.reward_decay(contract.reward, elapsed)
    }

    pub(crate) fn contract_pressure_text(&self, index: usize) -> String {
        let contract = &self.contracts[index];

        if let Some(elapsed) = self.contract_elapsed_ticks(index) {
            let payout = self.contract_current_reward(index);

            if self.difficulty.enforces_time_limit() {
                let remaining = contract.deadline.saturating_sub(elapsed);
                return format!(
                    "Payout {} cr | window remaining {} ticks",
                    payout, remaining
                );
            }

            if matches!(self.difficulty, Difficulty::Normal) {
                return format!("Payout now {} cr | reward decays while tracked", payout);
            }
        }

        match self.difficulty {
            Difficulty::Cozy => format!("Payout {} cr | no expiry", contract.reward),
            Difficulty::Normal => {
                format!("Payout {} cr | decays after acceptance", contract.reward)
            }
            Difficulty::Insane => format!(
                "Payout {} cr | {}-tick delivery window after acceptance",
                contract.reward, contract.deadline
            ),
        }
    }

    pub(crate) fn toggle_contract_tracking(&mut self) {
        let index = self.selected_contract;

        if self.needs_contract_refresh(index) {
            self.refresh_contract_slot(index);
        }

        if !self.is_contract_unlocked(index) {
            self.push_log(format!(
                "[{clock:04}] That contract stays locked until {location} is charted.",
                clock = self.clock,
                location = self.location_name(self.contracts[index].unlock_location),
            ));
            return;
        }

        match self.contracts[index].state {
            ContractState::Available => {
                if self.tracked_contract.is_some() {
                    self.push_log(format!(
                        "[{clock:04}] Track or finish the current contract before accepting another.",
                        clock = self.clock,
                    ));
                    return;
                }

                self.contracts[index].state = ContractState::Accepted {
                    accepted_at: self.clock,
                };
                self.tracked_contract = Some(index);
                self.active_pane = FLEET_PANE;
                self.push_log(format!(
                    "[{clock:04}] Accepted contract: {title} for {reward} credits.",
                    clock = self.clock,
                    title = self.contracts[index].title,
                    reward = self.contract_current_reward(index),
                ));
            }
            ContractState::Accepted { .. } => {
                self.contracts[index].state = ContractState::Available;
                self.tracked_contract = None;
                self.push_log(format!(
                    "[{clock:04}] Released tracked contract: {title}.",
                    clock = self.clock,
                    title = self.contracts[index].title,
                ));
            }
            ContractState::Assigned { ship_name, .. } => {
                self.push_log(format!(
                    "[{clock:04}] {title} is already assigned to {ship_name}.",
                    clock = self.clock,
                    title = self.contracts[index].title,
                    ship_name = ship_name,
                ));
            }
            ContractState::Completed => {
                self.push_log(format!(
                    "[{clock:04}] {title} is already complete.",
                    clock = self.clock,
                    title = self.contracts[index].title,
                ));
            }
            ContractState::Failed => {
                self.push_log(format!(
                    "[{clock:04}] {title} has already failed and cannot be reassigned.",
                    clock = self.clock,
                    title = self.contracts[index].title,
                ));
            }
        }

        self.evaluate_run_outcome();
    }

    pub(crate) fn matching_tracked_contract(
        &self,
        ship_index: usize,
        destination: usize,
        eta: u16,
    ) -> Option<usize> {
        let contract_index = self.tracked_contract?;
        let contract = &self.contracts[contract_index];
        let ship = &self.fleet[ship_index];

        if !matches!(contract.state, ContractState::Accepted { .. }) {
            return None;
        }

        (contract.origin == ship.current_location
            && contract.destination == destination
            && eta <= contract.max_eta)
            .then_some(contract_index)
    }

    pub(crate) fn resolve_contract_arrival(
        &mut self,
        contract_index: usize,
        ship_index: usize,
        ship_name: &'static str,
        destination_index: usize,
    ) {
        let title = self.contracts[contract_index].title.clone();
        let archetype = self.contracts[contract_index].archetype;

        match self.contracts[contract_index].state {
            ContractState::Assigned { accepted_at, .. } => {
                let payout = self.difficulty.reward_decay(
                    self.contracts[contract_index].reward,
                    self.clock.saturating_sub(accepted_at),
                );
                self.contracts[contract_index].state = ContractState::Completed;
                self.tracked_contract = None;
                self.credits += payout;
                self.push_log(format!(
                    "[{clock:04}] Contract complete: {title} via {ship_name} at {destination}. +{reward} cr.",
                    clock = self.clock,
                    title = title,
                    ship_name = ship_name,
                    destination = self.location_name(destination_index),
                    reward = payout,
                ));
                self.apply_contract_archetype_effect(archetype, ship_index, destination_index);
                self.refresh_contract_slot(contract_index);
            }
            ContractState::Failed => {
                self.push_log(format!(
                    "[{clock:04}] {ship_name} reached {destination} too late for {title}.",
                    clock = self.clock,
                    ship_name = ship_name,
                    destination = self.location_name(destination_index),
                    title = title,
                ));
            }
            _ => {}
        }
    }

    pub(crate) fn update_contract_deadlines(&mut self) {
        if !self.difficulty.enforces_time_limit() {
            return;
        }

        let mut notices = Vec::new();

        for index in 0..self.contracts.len() {
            let elapsed = match self.contracts[index].state {
                ContractState::Accepted { accepted_at }
                | ContractState::Assigned { accepted_at, .. } => {
                    self.clock.saturating_sub(accepted_at)
                }
                _ => continue,
            };

            if elapsed <= self.contracts[index].deadline {
                continue;
            }

            match self.contracts[index].state {
                ContractState::Accepted { .. } => {
                    self.contracts[index].state = ContractState::Failed;
                    self.tracked_contract = None;
                    notices.push(format!(
                        "[{clock:04}] Contract failed: {title} expired before launch.",
                        clock = self.clock,
                        title = self.contracts[index].title,
                    ));
                }
                ContractState::Assigned { ship_name, .. } => {
                    self.contracts[index].state = ContractState::Failed;
                    self.tracked_contract = None;
                    notices.push(format!(
                        "[{clock:04}] Contract failed: {title} missed its delivery window with {ship_name}.",
                        clock = self.clock,
                        title = self.contracts[index].title,
                        ship_name = ship_name,
                    ));
                }
                _ => {}
            }
        }

        for notice in notices {
            self.push_log(notice);
        }

        for index in 0..self.contracts.len() {
            if self.needs_contract_refresh(index) {
                self.refresh_contract_slot(index);
            }
        }
    }

    fn needs_contract_refresh(&self, index: usize) -> bool {
        matches!(
            self.contracts[index].state,
            ContractState::Completed | ContractState::Failed
        )
    }

    fn refresh_contract_slot(&mut self, index: usize) {
        let old_title = self.contracts[index].title.clone();
        let replacement = self.generate_contract_for_slot(index);
        self.contracts[index] = replacement;
        if self.tracked_contract == Some(index) {
            self.tracked_contract = None;
        }
        self.push_log(format!(
            "[{clock:04}] Mission board refreshed: {old} -> {new}.",
            clock = self.clock,
            old = old_title,
            new = self.contracts[index].title,
        ));
    }

    fn generate_contract_for_slot(&self, index: usize) -> Contract {
        let discovered: Vec<usize> = (0..self.locations.len())
            .filter(|&location| self.is_discovered(location))
            .collect();
        let reachable: Vec<usize> = discovered
            .iter()
            .copied()
            .filter(|&location| location != ASTRA_PRIME)
            .collect();

        let cycle = (self.clock as usize / 8).saturating_add(index);
        let destination = if reachable.is_empty() {
            DUST_HARBOR
        } else {
            reachable[cycle % reachable.len()]
        };
        let origin = if destination == ASTRA_PRIME {
            discovered[(cycle + 1) % discovered.len()]
        } else if cycle.is_multiple_of(2) {
            ASTRA_PRIME
        } else {
            destination
        };

        let destination = if origin == destination {
            ASTRA_PRIME
        } else {
            destination
        };
        let destination = if origin == destination {
            DUST_HARBOR
        } else {
            destination
        };

        let archetype = refresh_archetype(index, origin, destination);
        let eta_budget = self.estimate_eta_budget(origin, destination);
        let reward = 120 + i32::from(eta_budget) * 30 + (index as i32 * 15);
        let deadline = self.clock + u64::from(eta_budget) * 8 + 24;
        let unlock_location = destination.max(origin);

        Contract::new(
            archetype,
            origin,
            destination,
            reward,
            eta_budget,
            deadline,
            unlock_location,
        )
    }

    fn estimate_eta_budget(&self, origin: usize, destination: usize) -> u16 {
        let best_speed = self.fleet.iter().map(|ship| ship.speed).max().unwrap_or(1);
        self.plan_route(origin, destination, best_speed)
            .map(|plan| plan.eta.saturating_add(1))
            .unwrap_or(4)
    }

    fn apply_contract_archetype_effect(
        &mut self,
        archetype: ContractArchetype,
        ship_index: usize,
        destination_index: usize,
    ) {
        match archetype {
            ContractArchetype::SurveyDrop | ContractArchetype::FrontierSupply => {
                self.station_fuel[destination_index] = self.station_fuel[destination_index]
                    .saturating_add(6)
                    .min(60);
                self.push_log(format!(
                    "[{clock:04}] {} receives a frontier supply bonus: +6 station fuel.",
                    self.location_name(destination_index),
                    clock = self.clock,
                ));
            }
            ContractArchetype::ReliefReturn | ContractArchetype::ReturnFreight => {
                self.station_fuel[ASTRA_PRIME] =
                    self.station_fuel[ASTRA_PRIME].saturating_add(5).min(60);
                self.push_log(format!(
                    "[{clock:04}] Astra Prime docks receive a return-freight fuel bonus.",
                    clock = self.clock,
                ));
            }
            ContractArchetype::Medlift | ContractArchetype::PriorityRelay => {
                self.credits += 35;
                self.push_log(format!(
                    "[{clock:04}] Priority handling bonus: +35 cr.",
                    clock = self.clock,
                ));
            }
            ContractArchetype::CourierRun | ContractArchetype::OutboundCourier => {
                if self.fleet[ship_index].speed >= 3 {
                    self.credits += 25;
                    self.push_log(format!(
                        "[{clock:04}] Fast-lane courier bonus: +25 cr.",
                        clock = self.clock,
                    ));
                }
            }
            ContractArchetype::DrydockRefit => {
                self.fleet[ship_index].hull = 100;
                self.fleet[ship_index].state = ShipState::Docked;
                self.push_log(format!(
                    "[{clock:04}] Drydock refit complete: {} leaves port fully repaired.",
                    self.fleet[ship_index].name,
                    clock = self.clock,
                ));
            }
            ContractArchetype::RelayCalibration => {
                for fuel in &mut self.station_fuel {
                    *fuel = fuel.saturating_add(4).min(60);
                }
                self.push_log(format!(
                    "[{clock:04}] Relay calibration stabilizes convoy timing: all stations gain fuel.",
                    clock = self.clock,
                ));
            }
        }
    }
}

fn refresh_archetype(index: usize, origin: usize, destination: usize) -> ContractArchetype {
    match (index % 4, origin == ASTRA_PRIME, destination == ASTRA_PRIME) {
        (0, true, false) => ContractArchetype::OutboundCourier,
        (1, false, true) => ContractArchetype::ReturnFreight,
        (2, _, _) => ContractArchetype::PriorityRelay,
        _ => ContractArchetype::FrontierSupply,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_contract_slot_gets_replaced() {
        let mut game = GameData::new(Difficulty::Normal);
        let old_title = game.contracts[0].title.clone();
        game.contracts[0].state = ContractState::Completed;

        game.refresh_contract_slot(0);

        assert!(matches!(game.contracts[0].state, ContractState::Available));
        assert_ne!(game.contracts[0].title, old_title);
    }

    #[test]
    fn survey_drop_completion_boosts_destination_fuel() {
        let mut game = GameData::new(Difficulty::Normal);
        let base_fuel = game.station_fuel[DUST_HARBOR];
        game.contracts[0].state = ContractState::Assigned {
            ship_name: "SV Kestrel",
            accepted_at: 0,
        };

        game.resolve_contract_arrival(0, 0, "SV Kestrel", DUST_HARBOR);

        assert!(game.station_fuel[DUST_HARBOR] > base_fuel);
    }
}
