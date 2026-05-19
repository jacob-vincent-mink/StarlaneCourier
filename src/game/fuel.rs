use super::*;

impl GameData {
    pub(crate) fn fuel_alert_threshold(ship: &Ship) -> u16 {
        ship.max_fuel.saturating_div(4).max(1)
    }

    pub(crate) fn sync_low_fuel_alert(&mut self, ship_index: usize) {
        let clock = self.clock;
        let message = {
            let ship = &mut self.fleet[ship_index];
            let threshold = Self::fuel_alert_threshold(ship);
            if ship.current_fuel <= threshold {
                if ship.low_fuel_alerted {
                    None
                } else {
                    ship.low_fuel_alerted = true;
                    Some(format!(
                        "[{clock:04}] Low fuel alert: {name} is down to {fuel}/{max_fuel}.",
                        clock = clock,
                        name = ship.name,
                        fuel = ship.current_fuel,
                        max_fuel = ship.max_fuel,
                    ))
                }
            } else {
                ship.low_fuel_alerted = false;
                None
            }
        };

        if let Some(message) = message {
            self.push_log(message);
        }
    }

    pub(crate) fn refuel_plan_for_ship(&self, ship_index: usize, fuel_required: u16) -> RefuelPlan {
        let ship = &self.fleet[ship_index];

        if !self.difficulty.uses_fuel_economy() {
            return RefuelPlan::NotNeeded;
        }

        if fuel_required > ship.max_fuel {
            return RefuelPlan::ExceedsTank;
        }

        if ship.current_fuel >= fuel_required {
            return RefuelPlan::NotNeeded;
        }

        let needed = fuel_required.saturating_sub(ship.current_fuel);
        let station_available = self.station_fuel[ship.current_location];
        let transferable = self.transferable_fuel_for_ship(ship_index);
        let affordable_units = if self.difficulty.fuel_price_per_unit() == 0 {
            needed
        } else {
            (self.credits / self.difficulty.fuel_price_per_unit()).max(0) as u16
        };
        let purchasable_units = station_available.min(affordable_units).min(needed);
        let remaining_after_purchase = needed.saturating_sub(purchasable_units);

        if remaining_after_purchase == 0 {
            let cost = i32::from(purchasable_units) * self.difficulty.fuel_price_per_unit();
            return RefuelPlan::CanPurchase {
                units: purchasable_units,
                cost,
            };
        }

        if purchasable_units + transferable >= needed {
            return RefuelPlan::NeedTransfer {
                units: remaining_after_purchase,
            };
        }

        let station_short = needed.saturating_sub(station_available + transferable);
        if station_short > 0 {
            return RefuelPlan::BlockedByStation {
                units: station_short,
            };
        }

        let full_cost = i32::from(needed) * self.difficulty.fuel_price_per_unit();
        if self.credits < full_cost {
            return RefuelPlan::BlockedByCredits { cost: full_cost };
        }

        RefuelPlan::BlockedByCredits { cost: full_cost }
    }

    pub(crate) fn transferable_fuel_for_ship(&self, ship_index: usize) -> u16 {
        let ship = &self.fleet[ship_index];

        self.fleet
            .iter()
            .enumerate()
            .filter(|(index, donor)| {
                *index != ship_index
                    && donor.is_docked()
                    && donor.current_location == ship.current_location
            })
            .map(|(_, donor)| donor.current_fuel)
            .sum()
    }

    pub(crate) fn refuel_selected_ship(&mut self) {
        let ship_index = self.selected_ship;
        let ship = &self.fleet[ship_index];

        if !ship.is_docked() {
            self.push_log(format!(
                "[{clock:04}] {} must be docked before refueling.",
                ship.name,
                clock = self.clock,
            ));
            return;
        }

        if !self.difficulty.uses_fuel_economy() {
            self.fleet[ship_index].current_fuel = self.fleet[ship_index].max_fuel;
            self.sync_low_fuel_alert(ship_index);
            self.push_log(format!(
                "[{clock:04}] {} is topped off for free in Cozy mode.",
                self.fleet[ship_index].name,
                clock = self.clock,
            ));
            self.evaluate_run_outcome();
            return;
        }

        let location_index = ship.current_location;
        let needed = ship.max_fuel.saturating_sub(ship.current_fuel);
        if needed == 0 {
            self.push_log(format!(
                "[{clock:04}] {} already has a full tank.",
                ship.name,
                clock = self.clock,
            ));
            return;
        }

        let affordable_units = (self.credits / self.difficulty.fuel_price_per_unit()).max(0) as u16;
        let purchasable_units = needed
            .min(self.station_fuel[location_index])
            .min(affordable_units);
        if purchasable_units == 0 {
            self.push_log(format!(
                "[{clock:04}] No station fuel purchase possible for {} at {}.",
                ship.name,
                self.location_name(location_index),
                clock = self.clock,
            ));
            return;
        }

        let cost = i32::from(purchasable_units) * self.difficulty.fuel_price_per_unit();
        self.credits -= cost;
        self.station_fuel[location_index] =
            self.station_fuel[location_index].saturating_sub(purchasable_units);
        self.fleet[ship_index].current_fuel = self.fleet[ship_index]
            .current_fuel
            .saturating_add(purchasable_units);
        self.sync_low_fuel_alert(ship_index);
        self.push_log(format!(
            "[{clock:04}] Refueled {} at {}: +{} fuel for {} cr.",
            self.fleet[ship_index].name,
            self.location_name(location_index),
            purchasable_units,
            cost,
            clock = self.clock,
        ));
        self.evaluate_run_outcome();
    }

    pub(crate) fn transfer_fuel_to_selected_ship(&mut self) {
        let ship_index = self.selected_ship;
        let ship = &self.fleet[ship_index];

        if !ship.is_docked() {
            self.push_log(format!(
                "[{clock:04}] {} must be docked before receiving transferred fuel.",
                ship.name,
                clock = self.clock,
            ));
            return;
        }

        let needed = ship.max_fuel.saturating_sub(ship.current_fuel);
        if needed == 0 {
            self.push_log(format!(
                "[{clock:04}] {} already has a full tank.",
                ship.name,
                clock = self.clock,
            ));
            return;
        }

        let donor_index = self
            .fleet
            .iter()
            .enumerate()
            .filter(|(index, donor)| {
                *index != ship_index
                    && donor.is_docked()
                    && donor.current_location == ship.current_location
                    && donor.current_fuel > 0
            })
            .max_by_key(|(_, donor)| donor.current_fuel)
            .map(|(index, _)| index);

        let Some(donor_index) = donor_index else {
            self.push_log(format!(
                "[{clock:04}] No docked ship at {} can spare fuel for {}.",
                self.location_name(ship.current_location),
                ship.name,
                clock = self.clock,
            ));
            return;
        };

        let transfer_units = needed.min(self.fleet[donor_index].current_fuel);
        let donor_name = self.fleet[donor_index].name;
        let receiver_name = self.fleet[ship_index].name;
        self.fleet[donor_index].current_fuel = self.fleet[donor_index]
            .current_fuel
            .saturating_sub(transfer_units);
        self.fleet[ship_index].current_fuel = self.fleet[ship_index]
            .current_fuel
            .saturating_add(transfer_units);
        self.sync_low_fuel_alert(donor_index);
        self.sync_low_fuel_alert(ship_index);
        self.push_log(format!(
            "[{clock:04}] Transferred {} fuel from {} to {}.",
            transfer_units,
            donor_name,
            receiver_name,
            clock = self.clock,
        ));
        self.evaluate_run_outcome();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuel_plan_can_require_transfer() {
        let mut game = GameData::new(Difficulty::Normal);
        game.selected_ship = 0;
        game.fleet[0].current_fuel = 0;
        game.station_fuel[ASTRA_PRIME] = 1;
        game.credits = 999;

        let plan = game.refuel_plan_for_ship(0, 4);
        assert!(matches!(plan, RefuelPlan::NeedTransfer { units: 3 }));
    }
}
