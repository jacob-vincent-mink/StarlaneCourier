use super::*;

impl GameData {
    pub(crate) fn hull_status(ship: &Ship) -> &'static str {
        match ship.hull {
            80..=100 => "Solid",
            50..=79 => "Worn",
            25..=49 => "Strained",
            _ => "Critical",
        }
    }

    pub(crate) fn next_upgrade_offer(&self, ship_index: usize) -> Option<UpgradeOffer> {
        let ship = &self.fleet[ship_index];

        if !matches!(ship.state, ShipState::Docked) {
            return None;
        }

        if let Some(contract_index) = self.tracked_contract {
            let contract = &self.contracts[contract_index];
            if contract.origin == ship.current_location {
                if let Some(plan) = self.plan_route_for_ship(ship_index, contract.destination) {
                    if plan.eta > contract.max_eta && ship.speed < 6 {
                        let cost = 90 + i32::from(ship.speed) * 35;
                        return Some(UpgradeOffer {
                            kind: UpgradeKind::Engine,
                            cost,
                            description: format!(
                                "+1 speed for {} cr to chase ETA {} contracts",
                                cost, contract.max_eta
                            ),
                        });
                    }
                    if plan.fuel_required > ship.max_fuel && ship.max_fuel < 26 {
                        let cost = 70 + i32::from(ship.max_fuel) * 8;
                        return Some(UpgradeOffer {
                            kind: UpgradeKind::FuelTank,
                            cost,
                            description: format!(
                                "+2 max fuel for {} cr to extend route reach",
                                cost
                            ),
                        });
                    }
                }
            }
        }

        if ship.max_fuel < 26 {
            let cost = 70 + i32::from(ship.max_fuel) * 8;
            return Some(UpgradeOffer {
                kind: UpgradeKind::FuelTank,
                cost,
                description: format!("+2 max fuel for {} cr", cost),
            });
        }

        if ship.speed < 6 {
            let cost = 90 + i32::from(ship.speed) * 35;
            return Some(UpgradeOffer {
                kind: UpgradeKind::Engine,
                cost,
                description: format!("+1 speed for {} cr", cost),
            });
        }

        None
    }

    pub(crate) fn upgrade_selected_ship(&mut self) {
        let ship_index = self.selected_ship;
        let ship = &self.fleet[ship_index];

        if !matches!(ship.state, ShipState::Docked) {
            self.push_log(format!(
                "[{clock:04}] {} must be docked before upgrading.",
                ship.name,
                clock = self.clock,
            ));
            return;
        }

        let Some(offer) = self.next_upgrade_offer(ship_index) else {
            self.push_log(format!(
                "[{clock:04}] {} has no upgrade currently available.",
                ship.name,
                clock = self.clock,
            ));
            return;
        };

        if self.credits < offer.cost {
            self.push_log(format!(
                "[{clock:04}] Need {} cr to upgrade {}.",
                offer.cost,
                ship.name,
                clock = self.clock,
            ));
            return;
        }

        self.credits -= offer.cost;
        let ship_name = self.fleet[ship_index].name;
        match offer.kind {
            UpgradeKind::Engine => {
                self.fleet[ship_index].speed += 1;
                self.push_log(format!(
                    "[{clock:04}] Upgraded {} engines: speed now {}.",
                    ship_name,
                    self.fleet[ship_index].speed,
                    clock = self.clock,
                ));
            }
            UpgradeKind::FuelTank => {
                self.fleet[ship_index].max_fuel += 2;
                self.fleet[ship_index].current_fuel += 2;
                self.push_log(format!(
                    "[{clock:04}] Expanded {} tanks: fuel capacity now {}.",
                    ship_name,
                    self.fleet[ship_index].max_fuel,
                    clock = self.clock,
                ));
            }
        }
        self.evaluate_run_outcome();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_offer_targets_engine_for_too_slow_contract() {
        let mut game = GameData::new(Difficulty::Normal);
        game.discovered_locations[KITE_STATION] = true;
        game.selected_ship = 1;
        game.tracked_contract = Some(3);
        game.contracts[3].state = ContractState::Accepted { accepted_at: 0 };
        game.fleet[1].state = ShipState::Docked;
        game.fleet[1].current_location = ASTRA_PRIME;

        let offer = game.next_upgrade_offer(1).unwrap();
        assert!(matches!(offer.kind, UpgradeKind::Engine));
    }
}
