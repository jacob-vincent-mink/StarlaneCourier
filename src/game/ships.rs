use super::*;

const SHOP_RESTOCK_INTERVAL: u64 = 36;
const OFFERS_PER_SHIPYARD: usize = 2;
const NAME_SUFFIXES: [&str; 12] = [
    "Kestrel", "Lantern", "Orpheus", "Venture", "Halcyon", "Drift", "Aster", "Pioneer", "Comet",
    "Nomad", "Beacon", "Zephyr",
];

struct ShipBlueprint {
    prefix: &'static str,
    class_name: &'static str,
    description: &'static str,
    speed: u16,
    max_fuel: u16,
    price: i32,
}

const SHIP_BLUEPRINTS: [ShipBlueprint; 6] = [
    ShipBlueprint {
        prefix: "SV",
        class_name: "Courier Cutter",
        description: "A fast dispatch hull tuned for urgent packets and short-hop contracts.",
        speed: 3,
        max_fuel: 12,
        price: 190,
    },
    ShipBlueprint {
        prefix: "CSV",
        class_name: "Utility Freighter",
        description: "A dependable work barge with broad tanks and enough room for frontier cargo.",
        speed: 1,
        max_fuel: 18,
        price: 210,
    },
    ShipBlueprint {
        prefix: "HMV",
        class_name: "Survey Tender",
        description: "A balanced exploration ship fitted for chart work and steady courier lanes.",
        speed: 2,
        max_fuel: 16,
        price: 225,
    },
    ShipBlueprint {
        prefix: "RSV",
        class_name: "Relay Skiff",
        description: "A compact runner built to snap between stations before the traffic turns heavy.",
        speed: 4,
        max_fuel: 10,
        price: 250,
    },
    ShipBlueprint {
        prefix: "TSS",
        class_name: "Tank Sloop",
        description: "A lane tanker with oversized fuel cells for long shunts and rough conditions.",
        speed: 2,
        max_fuel: 22,
        price: 280,
    },
    ShipBlueprint {
        prefix: "MVS",
        class_name: "Maintenance Sloop",
        description: "A resilient service hull that trades peak speed for safe, repeatable station work.",
        speed: 2,
        max_fuel: 20,
        price: 240,
    },
];

pub(super) fn starting_fleet(world_flavor: Option<&WorldFlavor>) -> Vec<Ship> {
    vec![
        build_docked_ship(
            0,
            0,
            ASTRA_PRIME,
            9,
            world_flavor.and_then(|f| f.starter_ships.first()),
        ),
        build_en_route_ship(
            1,
            1,
            ASTRA_PRIME,
            DUST_HARBOR,
            7,
            7,
            "Astra Prime -> Dust Harbor",
            "Dust Corridor: debris interference",
            2,
            world_flavor.and_then(|f| f.starter_ships.get(1)),
        ),
        build_docked_ship(
            2,
            2,
            ASTRA_PRIME,
            13,
            world_flavor.and_then(|f| f.starter_ships.get(2)),
        ),
    ]
}

pub(super) fn starting_ship_shops(
    location_count: usize,
    world_flavor: Option<&WorldFlavor>,
) -> Vec<Option<ShipShop>> {
    let mut shops = vec![None; location_count];
    let locations = starting_shipyard_locations(location_count);
    for (offer_index, location_index) in locations.into_iter().enumerate() {
        shops[location_index] = Some(ShipShop {
            offers: generate_shop_offers(location_index, 0, offer_index, world_flavor),
            last_refresh: 0,
        });
    }
    shops
}

fn starting_shipyard_locations(location_count: usize) -> Vec<usize> {
    (0..location_count)
        .filter(|&index| {
            let role = index % SECTOR_LOCATION_COUNT;
            role == 0 || role == 2
        })
        .collect()
}

fn build_docked_ship(
    seed: u64,
    blueprint_index: usize,
    location: usize,
    current_fuel: u16,
    generated: Option<&WorldShipFlavor>,
) -> Ship {
    let blueprint = &SHIP_BLUEPRINTS[blueprint_index % SHIP_BLUEPRINTS.len()];
    let (name, class_name, description) = flavored_ship_text(seed, blueprint, generated);
    Ship::docked(
        name,
        class_name,
        description,
        location,
        current_fuel.min(blueprint.max_fuel),
        blueprint.max_fuel,
        blueprint.speed,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_en_route_ship(
    seed: u64,
    blueprint_index: usize,
    origin: usize,
    destination: usize,
    eta_remaining: u16,
    total_eta: u16,
    route: &str,
    condition_summary: &str,
    current_fuel: u16,
    generated: Option<&WorldShipFlavor>,
) -> Ship {
    let blueprint = &SHIP_BLUEPRINTS[blueprint_index % SHIP_BLUEPRINTS.len()];
    let (name, class_name, description) = flavored_ship_text(seed, blueprint, generated);
    let segments = vec![(origin, destination)];
    let segment_costs = vec![total_eta.max(1)];
    Ship::en_route(
        name,
        class_name,
        description,
        origin,
        destination,
        eta_remaining,
        total_eta,
        false,
        segments,
        segment_costs,
        route,
        condition_summary,
        None,
        current_fuel.min(blueprint.max_fuel),
        blueprint.max_fuel,
        blueprint.speed,
        0,
    )
}

fn flavored_ship_text(
    seed: u64,
    blueprint: &ShipBlueprint,
    generated: Option<&WorldShipFlavor>,
) -> (String, String, String) {
    if let Some(generated) = generated {
        (
            generated.name.clone(),
            generated.class_name.clone(),
            generated.description.clone(),
        )
    } else {
        (
            generated_ship_name(seed, blueprint.prefix),
            blueprint.class_name.to_string(),
            blueprint.description.to_string(),
        )
    }
}

fn generated_ship_name(seed: u64, prefix: &str) -> String {
    let suffix = NAME_SUFFIXES[seed as usize % NAME_SUFFIXES.len()];
    format!("{prefix} {suffix}")
}

fn generate_shop_offer(
    location_index: usize,
    cycle: u64,
    offer_offset: usize,
    generated: Option<&WorldShipFlavor>,
) -> ShipShopOffer {
    let seed = cycle
        .wrapping_mul(11)
        .wrapping_add((location_index as u64 + 1) * 7)
        .wrapping_add((offer_offset as u64 + 1) * 13)
        .wrapping_add(3);
    let blueprint = &SHIP_BLUEPRINTS[seed as usize % SHIP_BLUEPRINTS.len()];
    let price_variation = ((seed % 4) as i32 - 1) * 10;
    let (name, class_name, description) =
        flavored_ship_text(seed + location_index as u64, blueprint, generated);

    ShipShopOffer {
        name,
        class_name,
        description,
        speed: blueprint.speed,
        max_fuel: blueprint.max_fuel,
        price: (blueprint.price + price_variation).max(150),
    }
}

fn generate_shop_offers(
    location_index: usize,
    cycle: u64,
    shipyard_index: usize,
    world_flavor: Option<&WorldFlavor>,
) -> Vec<ShipShopOffer> {
    (0..OFFERS_PER_SHIPYARD)
        .map(|offer_offset| {
            generate_shop_offer(
                location_index,
                cycle,
                offer_offset,
                world_flavor.and_then(|f| {
                    if f.shipyard_offers.is_empty() {
                        None
                    } else {
                        let flavor_index = (shipyard_index * OFFERS_PER_SHIPYARD + offer_offset)
                            % f.shipyard_offers.len();
                        f.shipyard_offers.get(flavor_index)
                    }
                }),
            )
        })
        .collect()
}

impl GameData {
    pub(crate) fn hull_status(ship: &Ship) -> &'static str {
        match ship.hull {
            80..=100 => "Solid",
            50..=79 => "Worn",
            25..=49 => "Strained",
            _ => "Critical",
        }
    }

    pub(crate) fn shipyard_offers(&self, location_index: usize) -> &[ShipShopOffer] {
        self.station_ship_shops
            .get(location_index)
            .and_then(|shop| shop.as_ref())
            .map(|shop| shop.offers.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn shipyard_offer_count(&self, location_index: usize) -> usize {
        self.shipyard_offers(location_index).len()
    }

    pub(crate) fn shipyard_offer(&self, location_index: usize) -> Option<&ShipShopOffer> {
        let offers = self.shipyard_offers(location_index);
        if offers.is_empty() {
            None
        } else {
            Some(&offers[self.selected_shipyard_offer.min(offers.len() - 1)])
        }
    }

    pub(crate) fn has_shipyard(&self, location_index: usize) -> bool {
        self.station_ship_shops
            .get(location_index)
            .is_some_and(|shop| shop.is_some())
    }

    pub(crate) fn purchase_ship_at_selected_location(&mut self) {
        let location_index = self.selected_location;
        if self.player_is_in_transit() {
            self.set_action_feedback("You are in transit and cannot buy ships until arrival.");
            return;
        }
        if location_index != self.player_location {
            self.set_action_feedback(format!(
                "Transfer to {} before buying a ship there.",
                self.location_name(location_index)
            ));
            return;
        }
        if !self.is_discovered(location_index) {
            self.set_action_feedback(format!(
                "Shipyard records are unavailable until {} is charted.",
                self.location_name(location_index)
            ));
            self.push_log(format!(
                "[{clock:04}] Shipyard records are unavailable until {} is charted.",
                self.location_name(location_index),
                clock = self.clock,
            ));
            return;
        }

        if !self.has_shipyard(location_index) {
            self.set_action_feedback(format!(
                "No shipyard is currently operating at {}.",
                self.location_name(location_index)
            ));
            self.push_log(format!(
                "[{clock:04}] No shipyard is currently operating at {}.",
                self.location_name(location_index),
                clock = self.clock,
            ));
            return;
        }

        let offers = self.shipyard_offers(location_index);
        let Some(offer_index) =
            (!offers.is_empty()).then_some(self.selected_shipyard_offer.min(offers.len() - 1))
        else {
            self.set_action_feedback(format!(
                "The featured hull at {} has already been sold.",
                self.location_name(location_index)
            ));
            self.push_log(format!(
                "[{clock:04}] The shipyard at {} is sold out. Wait for the next yard refresh.",
                self.location_name(location_index),
                clock = self.clock,
            ));
            return;
        };
        let offer = offers[offer_index].clone();

        if self.credits < offer.price {
            self.set_action_feedback(format!(
                "Need {} cr to acquire {} at {}.",
                offer.price,
                offer.name,
                self.location_name(location_index)
            ));
            self.push_log(format!(
                "[{clock:04}] Need {} cr to acquire {} at {}.",
                offer.price,
                offer.name,
                self.location_name(location_index),
                clock = self.clock,
            ));
            return;
        }

        self.credits -= offer.price;
        let ship = offer.to_ship(location_index);
        let ship_name = ship.name.clone();
        let class_name = ship.class_name.clone();
        self.fleet.push(ship);
        self.selected_ship = self.fleet.len() - 1;
        self.active_pane = FLEET_PANE;

        if let Some(shop) = self.station_ship_shops[location_index].as_mut() {
            shop.offers.remove(offer_index);
        }
        self.selected_shipyard_offer = self.selected_shipyard_offer.saturating_sub(1);

        self.push_log(format!(
            "[{clock:04}] Acquired {} ({}) from {} for {} cr.",
            ship_name,
            class_name,
            self.location_name(location_index),
            offer.price,
            clock = self.clock,
        ));
        self.evaluate_run_outcome();
    }

    pub(crate) fn can_buy_ship_that_unlocks_progress(&self) -> bool {
        for location_index in 0..self.locations.len() {
            if !self.is_discovered(location_index) {
                continue;
            }

            for offer in self.shipyard_offers(location_index) {
                if offer.price > self.credits {
                    continue;
                }

                if self.frontier_locations().into_iter().any(|frontier| {
                    self.offer_can_reach_destination(location_index, offer, frontier)
                }) {
                    return true;
                }

                for (contract_index, contract) in self.contracts.iter().enumerate() {
                    if !self.is_contract_unlocked(contract_index) {
                        continue;
                    }

                    if matches!(
                        contract.state,
                        ContractState::Completed | ContractState::Failed
                    ) {
                        continue;
                    }

                    if self.offer_can_reach_destination(location_index, offer, contract.origin) {
                        return true;
                    }
                }
            }
        }

        false
    }

    fn offer_can_reach_destination(
        &self,
        origin: usize,
        offer: &ShipShopOffer,
        destination: usize,
    ) -> bool {
        self.plan_route(origin, destination, offer.speed)
            .is_some_and(|plan| plan.fuel_required <= offer.max_fuel)
    }

    pub(crate) fn next_upgrade_offer(&self, ship_index: usize) -> Option<UpgradeOffer> {
        let ship = &self.fleet[ship_index];

        if !matches!(&ship.state, ShipState::Docked) {
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
        if self.player_is_in_transit() {
            self.set_action_feedback("You are in transit and cannot upgrade ships until arrival.");
            return;
        }

        let ship_index = self.selected_ship;
        let ship = &self.fleet[ship_index];
        let ship_name = ship.name.clone();
        let ship_location = ship.current_location;

        if !matches!(&ship.state, ShipState::Docked) {
            self.set_action_feedback(format!("{} must be docked before upgrading.", ship_name));
            self.push_log(format!(
                "[{clock:04}] {} must be docked before upgrading.",
                ship_name,
                clock = self.clock,
            ));
            return;
        }

        if ship_location != self.player_location {
            self.set_action_feedback(format!(
                "Transfer to {} before upgrading {}.",
                self.location_name(ship_location),
                ship_name
            ));
            return;
        }

        let Some(offer) = self.next_upgrade_offer(ship_index) else {
            self.set_action_feedback(format!("{} has no upgrade currently available.", ship_name));
            self.push_log(format!(
                "[{clock:04}] {} has no upgrade currently available.",
                ship_name,
                clock = self.clock,
            ));
            return;
        };

        if self.credits < offer.cost {
            self.set_action_feedback(format!("Need {} cr to upgrade {}.", offer.cost, ship_name));
            self.push_log(format!(
                "[{clock:04}] Need {} cr to upgrade {}.",
                offer.cost,
                ship_name,
                clock = self.clock,
            ));
            return;
        }

        self.credits -= offer.cost;
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

    pub(crate) fn restock_ship_shops(&mut self) {
        if self.clock == 0 || !self.clock.is_multiple_of(SHOP_RESTOCK_INTERVAL) {
            return;
        }

        let cycle = self.clock / SHOP_RESTOCK_INTERVAL;
        let mut refreshed = Vec::new();
        let shipyard_locations = starting_shipyard_locations(self.station_ship_shops.len());
        for location_index in 0..self.station_ship_shops.len() {
            let Some(shop) = self.station_ship_shops[location_index].as_mut() else {
                continue;
            };

            let shipyard_index = shipyard_locations
                .iter()
                .position(|&index| index == location_index)
                .unwrap_or(0);
            shop.offers = generate_shop_offers(location_index, cycle, shipyard_index, None);
            shop.last_refresh = self.clock;
            if self.is_discovered(location_index) {
                refreshed.push(self.location_name(location_index).to_string());
            }
        }

        if !refreshed.is_empty() {
            self.push_log(format!(
                "[{clock:04}] Shipyards rotated featured hulls at {}.",
                refreshed.join(", "),
                clock = self.clock,
            ));
        }
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

    #[test]
    fn shipyard_purchase_adds_new_ship_to_fleet() {
        let mut game = GameData::new(Difficulty::Normal);
        game.selected_location = ASTRA_PRIME;
        game.credits = 400;
        let fleet_before = game.fleet.len();
        let offers_before = game.shipyard_offer_count(ASTRA_PRIME);

        game.purchase_ship_at_selected_location();

        assert_eq!(game.fleet.len(), fleet_before + 1);
        assert_eq!(game.shipyard_offer_count(ASTRA_PRIME), offers_before - 1);
        assert_eq!(game.selected_ship, game.fleet.len() - 1);
    }

    #[test]
    fn shipyards_restock_featured_hulls_on_schedule() {
        let mut game = GameData::new(Difficulty::Normal);
        let before = game
            .shipyard_offer(ASTRA_PRIME)
            .expect("starting shipyard offer")
            .name
            .clone();

        game.clock = SHOP_RESTOCK_INTERVAL;
        game.restock_ship_shops();

        let after = game
            .shipyard_offer(ASTRA_PRIME)
            .expect("restocked shipyard offer")
            .name
            .clone();
        assert_ne!(before, after);
        assert_eq!(game.shipyard_offer_count(ASTRA_PRIME), OFFERS_PER_SHIPYARD);
    }
}
