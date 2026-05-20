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
            && self
                .frontier_locations()
                .into_iter()
                .filter(|&location| location != ship_location)
                .next()
                .is_none()
        {
            self.set_action_feedback(
                "No frontier nodes are currently available for exploration. Reach a charted frontier first.",
            );
            self.push_log(format!(
                "[{clock:04}] No frontier nodes are currently available for exploration.",
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

        if matches!(intent, DispatchIntent::Exploration)
            && let Some(frontier) = self
                .frontier_locations()
                .into_iter()
                .find(|&location| location != ship_location)
        {
            self.selected_location = frontier;
            self.sync_map_focus_to_selected_location();
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

        let ship_name = self.fleet[ship_index].name.clone();
        let origin = self.fleet[ship_index].current_location;

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

        let destination = self.selected_location;
        let rider_required = self.local_ship_count(self.player_location) == 1
            && self.fleet[ship_index].current_location == self.player_location;
        if matches!(intent, DispatchIntent::Exploration) && !self.is_frontier_location(destination)
        {
            self.set_action_feedback(
                "Exploration runs must target a frontier node with an uncharted contact beyond it.",
            );
            self.push_log(format!(
                "[{clock:04}] Exploration runs must target a frontier node.",
                clock = self.clock,
            ));
            return;
        }

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
                    "[{clock:04}] Exploration heading set {} of {}.",
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

    pub(crate) fn player_transfer_cost(&self, destination: usize) -> Option<i32> {
        if destination == self.player_location {
            return Some(0);
        }
        if !self.is_discovered(destination) {
            return None;
        }

        let plan = self.plan_route(self.player_location, destination, 3)?;
        Some(10 + i32::from(plan.fuel_required) * 2)
    }

    pub(crate) fn transfer_player_to_selected_location(&mut self) {
        if self.player_is_in_transit() {
            self.set_action_feedback("You are already in transit aboard a ship.");
            return;
        }

        let destination = self.selected_location;
        if destination == self.player_location {
            self.set_action_feedback("You are already at the selected station.");
            return;
        }

        let Some(cost) = self.player_transfer_cost(destination) else {
            self.set_action_feedback(
                "Passenger transfer is only available to discovered stations.",
            );
            return;
        };

        if self.credits < cost {
            self.set_action_feedback(format!(
                "Need {} more cr for passenger transfer to {}.",
                cost - self.credits,
                self.location_name(destination)
            ));
            return;
        }

        self.credits -= cost;
        let origin_name = self.location_name(self.player_location).to_string();
        let destination_name = self.location_name(destination).to_string();
        self.player_location = destination;
        self.selected_shipyard_offer = 0;
        self.push_log(format!(
            "[{clock:04}] Passenger transfer: {origin} -> {destination} for {} cr.",
            cost,
            clock = self.clock,
            origin = origin_name,
            destination = destination_name,
        ));
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
            || self
                .player_transfer_cost(location_index)
                .is_some_and(|cost| self.credits >= cost)
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
        self.locations[index]
            .reveal_on_arrival
            .filter(|&target| self.is_discovered(index) && !self.is_discovered(target))
    }

    pub(crate) fn exploration_heading_hint(&self, index: usize) -> Option<String> {
        let target = self.exploration_target(index)?;
        let from = self.locations[index].sector_coords;
        let to = self.locations[target].sector_coords;
        Some(compass_heading(from, to).to_string())
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
