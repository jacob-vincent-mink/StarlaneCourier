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

    pub(crate) fn contract_hint(&self, index: usize) -> String {
        let contract = &self.contracts[index];

        match &contract.state {
            ContractState::Available => contract.briefing.to_string(),
            ContractState::Accepted { .. } => format!(
                "Tracked. Dispatch a ship from the contract origin to the listed destination. {}",
                self.contract_pressure_text(index)
            ),
            ContractState::Assigned { ship_name, .. } => format!(
                "Assigned to {}. Waiting on delivery. {}",
                ship_name,
                self.contract_pressure_text(index)
            ),
            ContractState::Completed => {
                "Completed. Reward already banked to the dispatch account.".to_string()
            }
            ContractState::Failed => {
                "Failed. The delivery window was missed on a high-pressure run.".to_string()
            }
        }
    }

    pub(crate) fn toggle_contract_tracking(&mut self) {
        let index = self.selected_contract;

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
        ship_name: &'static str,
        destination_index: usize,
    ) {
        let title = self.contracts[contract_index].title;

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
    }
}
