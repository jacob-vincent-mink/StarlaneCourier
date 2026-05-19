use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph, Wrap,
        canvas::{Canvas, Line as CanvasLine, Points},
    },
};

use crate::{
    app::{App, DIFFICULTY_OPTIONS, LLM_FIELDS, Screen, TICK_SPEEDS},
    game::{
        ASTRA_PRIME, CONTRACTS_PANE, DUST_HARBOR, Difficulty, FLEET_PANE, GOAL_CREDITS,
        ION_ANCHORAGE, KITE_STATION, LOG_PANE, Location, MAP_PANE, OUTER_RING_RELAY, PANE_TITLES,
        RefuelPlan, RunOutcome, Ship, ShipState, transit_phase,
    },
    save::SAVE_SLOT_COUNT,
};

pub(crate) fn draw(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::StartMenu => draw_start_menu(frame, app),
        Screen::LoadGame => draw_load_game(frame, app),
        Screen::Settings => draw_settings(frame, app),
        Screen::HowToPlay => draw_how_to_play(frame, app),
        Screen::InGame => draw_game(frame, app),
        Screen::EndGame => draw_end_game(frame, app),
    }
}

fn draw_game(frame: &mut Frame, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(16),
            Constraint::Length(9),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(28),
            Constraint::Percentage(40),
            Constraint::Percentage(28),
        ])
        .split(areas[1]);

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(areas[2]);
    let alert_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
        .split(bottom[0]);

    let map_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(7)])
        .split(top[1]);
    let board_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(top[0]);
    let fleet_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(top[2]);

    let header = Paragraph::new(Line::from(vec![
        "Starlane Courier".into(),
        "  ".into(),
        format!("Shift window: T+{:04}", app.clock).into(),
        "  ".into(),
        format!("Mode: {}", app.mode_label()).into(),
        "  ".into(),
        format!(
            "Survey: {}/{} charts",
            app.discovered_count(),
            app.locations.len()
        )
        .into(),
        "  ".into(),
        format!("Credits: {} cr", app.credits).into(),
        "  ".into(),
        format!("Difficulty: {}", app.difficulty.label()).into(),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Bridge"));
    frame.render_widget(header, areas[0]);

    let contracts = List::new(
        app.contracts
            .iter()
            .enumerate()
            .map(|(index, _)| contract_list_item(app, index)),
    )
    .highlight_style(selection_style())
    .highlight_symbol(">> ")
    .block(pane_block(
        PANE_TITLES[0],
        app.active_pane == CONTRACTS_PANE,
    ));
    let mut contract_state = ListState::default();
    contract_state.select(Some(app.selected_contract));
    frame.render_stateful_widget(contracts, board_sections[0], &mut contract_state);

    let contract_detail = Paragraph::new(Text::from(contract_detail_lines(app)))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Contract Detail"),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(contract_detail, board_sections[1]);

    render_sector_map(frame, map_sections[0], app, app.active_pane == MAP_PANE);

    let route_intel = Paragraph::new(Text::from(route_preview_lines(app)))
        .block(Block::default().borders(Borders::ALL).title("Route Intel"))
        .wrap(Wrap { trim: true });
    frame.render_widget(route_intel, map_sections[1]);

    let fleet = List::new(app.fleet.iter().enumerate().map(|(index, ship)| {
        ship_list_item(
            ship,
            &app.locations,
            &app.contracts,
            app.pending_ship() == Some(index),
        )
    }))
    .highlight_style(selection_style())
    .highlight_symbol(">> ")
    .block(pane_block(PANE_TITLES[2], app.active_pane == FLEET_PANE));
    let mut fleet_state = ListState::default();
    fleet_state.select(Some(app.selected_ship));
    frame.render_stateful_widget(fleet, fleet_sections[0], &mut fleet_state);

    let ship_detail = Paragraph::new(Text::from(ship_detail_lines(app)))
        .block(Block::default().borders(Borders::ALL).title("Ship Detail"))
        .wrap(Wrap { trim: true });
    frame.render_widget(ship_detail, fleet_sections[1]);

    let alerts_data = app.current_alerts();
    let alerts = List::new(alerts_data.iter().map(|alert| {
        let prefix = match alert.severity {
            crate::game::AlertSeverity::Info => "Info: ",
            crate::game::AlertSeverity::Warning => "Warn: ",
            crate::game::AlertSeverity::Critical => "Critical: ",
        };
        ListItem::new(format!("{}{}", prefix, alert.summary))
    }))
    .highlight_style(selection_style())
    .highlight_symbol(">> ")
    .block(pane_block("Alerts", app.active_pane == LOG_PANE));
    let mut alerts_state = ListState::default();
    alerts_state.select(Some(
        app.selected_alert.min(alerts_data.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(alerts, alert_sections[0], &mut alerts_state);

    let log = List::new(app.log.iter().map(|entry| ListItem::new(entry.as_str())))
        .block(pane_block(PANE_TITLES[3], app.active_pane == LOG_PANE));
    frame.render_widget(log, alert_sections[1]);

    let mission = Paragraph::new(Text::from(build_mission_text(app)))
        .block(Block::default().borders(Borders::ALL).title("Mission"))
        .wrap(Wrap { trim: true });
    frame.render_widget(mission, bottom[1]);

    let footer = Paragraph::new(Line::from(app.controls_text()))
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, areas[3]);
}

fn draw_start_menu(frame: &mut Frame, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(12),
            Constraint::Length(3),
        ])
        .split(frame.area());
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(34), Constraint::Percentage(66)])
        .split(areas[1]);

    let header = Paragraph::new(Text::from(vec![
        Line::from(Span::styled(
            "Starlane Courier",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(
            "Run a frontier dispatch shift: chart the sector, take contracts, and grow credits.",
        ),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Launch Bay"))
    .wrap(Wrap { trim: true });
    frame.render_widget(header, areas[0]);

    let options = app.start_menu_options();
    let menu = List::new(options.iter().map(|option| ListItem::new(option.label())))
        .highlight_style(selection_style())
        .highlight_symbol(">> ")
        .block(Block::default().borders(Borders::ALL).title("Menu"));
    let mut menu_state = ListState::default();
    menu_state.select(Some(app.start_menu_selection));
    frame.render_stateful_widget(menu, body[0], &mut menu_state);

    let selected = options[app.start_menu_selection];
    let detail = Paragraph::new(Text::from(vec![
        Line::from(format!("{}", selected.label())),
        Line::from(""),
        Line::from(selected.description(app)),
        Line::from(""),
        Line::from(format!(
            "Selected archive slot: {}",
            App::save_slot_label(app.load_slot_selection)
        )),
        Line::from(if app.has_active_game {
            format!(
                "Current shift slot: {}",
                App::save_slot_label(app.active_save_slot)
            )
        } else {
            "Current shift slot: none".to_string()
        }),
        Line::from(format!("Difficulty: {}", app.difficulty.label())),
        Line::from(format!(
            "Current settings: {} speed ({} ms/tick)",
            TICK_SPEEDS[app.tick_speed_index].0, TICK_SPEEDS[app.tick_speed_index].1,
        )),
        Line::from(app.llm_summary()),
        Line::from("Contracts reward credits; frontier arrivals reveal deeper routes."),
        Line::from(""),
        Line::from(
            app.menu_feedback
                .clone()
                .unwrap_or_else(|| app.selected_slot_summary_text()),
        ),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Briefing"))
    .wrap(Wrap { trim: true });
    frame.render_widget(detail, body[1]);

    let footer = Paragraph::new(Line::from(
        "Up/Down: menu   Left/Right: save slot   Enter: confirm   q/Ctrl+C: quit",
    ))
    .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, areas[2]);
}

fn draw_load_game(frame: &mut Frame, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let header = Paragraph::new("Load Game")
        .block(Block::default().borders(Borders::ALL).title("Archive Bay"));
    frame.render_widget(header, areas[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(areas[1]);

    let list = List::new((0..SAVE_SLOT_COUNT).map(|index| {
        ListItem::new(format!(
            "{} [{}]",
            App::save_slot_label(index),
            app.slot_brief(index)
        ))
    }))
    .highlight_style(selection_style())
    .highlight_symbol(">> ")
    .block(Block::default().borders(Borders::ALL).title("Slots"));
    let mut slot_state = ListState::default();
    slot_state.select(Some(app.load_slot_selection));
    frame.render_stateful_widget(list, body[0], &mut slot_state);

    let detail = Paragraph::new(Text::from(vec![
        Line::from(format!(
            "Selected: {}",
            App::save_slot_label(app.load_slot_selection)
        )),
        Line::from(""),
        Line::from(app.save_slot_summary_text(app.load_slot_selection)),
        Line::from(""),
        Line::from(app.menu_feedback.clone().unwrap_or_else(|| {
            "Enter loads the selected slot. Esc returns to the start menu and keeps this slot active for New Game autosaves.".to_string()
        })),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Status"))
    .wrap(Wrap { trim: true });
    frame.render_widget(detail, body[1]);

    let footer = Paragraph::new(Line::from(
        "Up/Down: choose slot   Enter: load slot   Esc: back   q/Ctrl+C: quit",
    ))
    .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, areas[2]);
}

fn draw_settings(frame: &mut Frame, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(frame.area());
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(22),
            Constraint::Percentage(22),
            Constraint::Percentage(28),
            Constraint::Percentage(28),
        ])
        .split(areas[1]);

    let header = Paragraph::new("Settings")
        .block(Block::default().borders(Borders::ALL).title("Control Room"));
    frame.render_widget(header, areas[0]);

    let list = List::new(
        TICK_SPEEDS
            .iter()
            .enumerate()
            .map(|(index, (label, millis))| {
                let current = if index == app.tick_speed_index {
                    " [current]"
                } else {
                    ""
                };
                ListItem::new(format!("{}{} - {} ms/tick", label, current, millis))
            }),
    )
    .highlight_style(selection_style())
    .highlight_symbol(">> ")
    .block(pane_block("Tick Speed", app.settings_focus == 0));
    let mut state = ListState::default();
    state.select(Some(app.settings_selection));
    frame.render_stateful_widget(list, body[0], &mut state);

    let difficulty_list = List::new(DIFFICULTY_OPTIONS.iter().enumerate().map(
        |(index, difficulty)| {
            let current = if index == app.difficulty.index() {
                " [current]"
            } else {
                ""
            };
            ListItem::new(format!("{}{}", difficulty.label(), current))
        },
    ))
    .highlight_style(selection_style())
    .highlight_symbol(">> ")
    .block(pane_block("Difficulty", app.settings_focus == 1));
    let mut difficulty_state = ListState::default();
    difficulty_state.select(Some(app.difficulty_selection));
    frame.render_stateful_widget(difficulty_list, body[1], &mut difficulty_state);

    let llm_list = List::new(LLM_FIELDS.iter().map(|field| {
        let value = if app
            .settings_edit
            .as_ref()
            .is_some_and(|edit| edit.field == *field)
        {
            let edit = app.settings_edit.as_ref().unwrap();
            if edit.secret {
                format!(
                    "{}: {}",
                    app.llm_field_label(*field),
                    "*".repeat(edit.buffer.len().max(1))
                )
            } else {
                format!("{}: {}", app.llm_field_label(*field), edit.buffer)
            }
        } else {
            format!(
                "{}: {}",
                app.llm_field_label(*field),
                app.llm_field_value(*field)
            )
        };
        ListItem::new(value)
    }))
    .highlight_style(selection_style())
    .highlight_symbol(">> ")
    .block(pane_block("LLM", app.settings_focus == 2));
    let mut llm_state = ListState::default();
    llm_state.select(Some(
        app.llm_field_selection
            .min(LLM_FIELDS.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(llm_list, body[2], &mut llm_state);

    let selected_field = app.selected_llm_field();
    let detail = Paragraph::new(Text::from(vec![
        Line::from("Simulation speed controls how fast route ETAs and ambient events advance."),
        Line::from(
            "Difficulty controls reward decay and whether hard delivery windows are enforced.",
        ),
        Line::from(""),
        Line::from(format!("Speed: {}", TICK_SPEEDS[app.settings_selection].0)),
        Line::from(format!(
            "Difficulty: {}",
            DIFFICULTY_OPTIONS[app.difficulty_selection].label()
        )),
        Line::from(""),
        Line::from(DIFFICULTY_OPTIONS[app.difficulty_selection].description()),
        Line::from(""),
        Line::from(format!(
            "LLM field: {}",
            app.llm_field_label(selected_field)
        )),
        Line::from(app.llm_field_description(selected_field)),
        Line::from(""),
        Line::from(app.llm_summary()),
        Line::from(if app.settings_edit.is_some() {
            "Editing: type text, Backspace, Enter to save, Esc to cancel".to_string()
        } else {
            "Settings: Enter edits/toggles the selected field; Delete clears API key".to_string()
        }),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Effect"))
    .wrap(Wrap { trim: true });
    frame.render_widget(detail, body[3]);

    let footer = Paragraph::new(Line::from(
        "Left/Right: focus   Up/Down: choose   Enter: apply/edit   Delete: clear API key   Esc: back   q/Ctrl+C: quit",
    ))
    .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, areas[2]);
}

fn draw_how_to_play(frame: &mut Frame, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(12),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let header = Paragraph::new("How To Play").block(
        Block::default()
            .borders(Borders::ALL)
            .title("Flight School"),
    );
    frame.render_widget(header, areas[0]);

    let body = Paragraph::new(Text::from(vec![
        Line::from(format!(
            "Primary goals: chart the full sector and reach {} credits.",
            GOAL_CREDITS
        )),
        Line::from("1. Accept a contract from the Mission Board."),
        Line::from("2. Refuel with `f` or transfer dockside fuel with `t` if needed."),
        Line::from("3. Upgrade a docked ship with `u` if you need more speed or range."),
        Line::from("4. Move to Fleet and press Enter on a docked ship."),
        Line::from("5. Focus the map, pick the contract destination, and confirm the route."),
        Line::from("6. Watch the ship move through undocking, cruising, incidents, and arrival."),
        Line::from("7. Use Alerts with Enter to jump to urgent ships, stations, or contracts."),
        Line::from("8. Frontier arrivals reveal new charts and unlock deeper contracts."),
        Line::from(""),
        Line::from("Difficulties:"),
        Line::from("Cozy: fixed rewards, no timeout pressure, no fuel economy."),
        Line::from("Normal: fuel costs matter and rewards decay after acceptance."),
        Line::from("Insane: faster reward decay plus hard delivery-window failures."),
        Line::from(""),
        Line::from(format!(
            "Current start: {} charted locations, {} credits, {} ships.",
            app.discovered_count(),
            app.credits,
            app.fleet.len()
        )),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Briefing"))
    .wrap(Wrap { trim: true });
    frame.render_widget(body, areas[1]);

    let footer = Paragraph::new(Line::from("Enter or Esc: back   q/Ctrl+C: quit"))
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, areas[2]);
}

fn draw_end_game(frame: &mut Frame, app: &App) {
    let outcome = app
        .run_outcome
        .as_ref()
        .expect("end screen requires outcome");
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let header = Paragraph::new(Text::from(vec![
        Line::from(Span::styled(
            outcome.title(),
            Style::default()
                .fg(if matches!(outcome, RunOutcome::Won) {
                    Color::Green
                } else {
                    Color::Red
                })
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("Starlane Courier shift summary"),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Debrief"));
    frame.render_widget(header, areas[0]);

    let body = Paragraph::new(Text::from(vec![
        Line::from(outcome.message()),
        Line::from(""),
        Line::from(format!("Difficulty: {}", app.difficulty.label())),
        Line::from(format!("Credits: {} / {}", app.credits, GOAL_CREDITS)),
        Line::from(format!(
            "Charts: {}/{}",
            app.discovered_count(),
            app.locations.len()
        )),
        Line::from(format!(
            "Save slot: {}",
            App::save_slot_label(app.active_save_slot)
        )),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Outcome"))
    .wrap(Wrap { trim: true });
    frame.render_widget(body, areas[1]);

    let footer = Paragraph::new(Line::from("Enter or Esc: return to menu   q/Ctrl+C: quit"))
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, areas[2]);
}

fn pane_block(title: &'static str, active: bool) -> Block<'static> {
    let border_style = if active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style)
}

fn selection_style() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn render_sector_map(frame: &mut Frame, area: Rect, app: &App, active: bool) {
    let origin = app.preview_origin();

    let canvas = Canvas::default()
        .block(pane_block(PANE_TITLES[1], active))
        .background_color(Color::Black)
        .marker(Marker::Braille)
        .x_bounds([0.0, 100.0])
        .y_bounds([0.0, 60.0])
        .paint(|ctx| {
            for index in 0..app.locations.len() {
                if app.is_discovered(index) {
                    continue;
                }

                let cloud = hidden_cloud_points(index);
                let (label_x, label_y) = location_label_coords(index);
                ctx.draw(&Points {
                    coords: &cloud,
                    color: Color::DarkGray,
                });
                ctx.print(
                    label_x,
                    label_y,
                    Span::styled(
                        "???",
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::BOLD),
                    ),
                );
            }

            let (hub_x, hub_y) = location_coords(ASTRA_PRIME);
            for leaf in 1..app.locations.len() {
                if !app.is_discovered(leaf) {
                    continue;
                }

                let (x, y) = location_coords(leaf);
                ctx.draw(&CanvasLine::new(hub_x, hub_y, x, y, Color::DarkGray));
            }

            if let Some((from, to)) = app.highlighted_route() {
                for (start, end) in route_segments(from, to) {
                    let (x1, y1) = location_coords(start);
                    let (x2, y2) = location_coords(end);
                    ctx.draw(&CanvasLine::new(x1, y1, x2, y2, Color::Cyan));
                }
            }

            for index in 0..app.locations.len() {
                if !app.is_discovered(index) {
                    continue;
                }

                let color = location_color(app, index, origin);
                let label = if app.is_frontier_location(index) {
                    format!("{}*", map_label(index))
                } else {
                    map_label(index).to_string()
                };
                let (x, y) = location_coords(index);
                let (label_x, label_y) = location_label_coords(index);
                let points = node_points(x, y);
                ctx.draw(&Points {
                    coords: &points,
                    color,
                });
                ctx.print(
                    label_x,
                    label_y,
                    Span::styled(label, location_label_style(app, index, origin)),
                );
                ctx.print(
                    label_x,
                    label_y - 3.0,
                    Span::styled(
                        format!("F{}", app.station_fuel[index]),
                        station_fuel_style(app, index),
                    ),
                );
            }

            let mut dock_slots = vec![0usize; app.locations.len()];
            for (ship_index, ship) in app.fleet.iter().enumerate() {
                match &ship.state {
                    ShipState::Docked | ShipState::Repairing { .. } => {
                        if !app.is_discovered(ship.current_location) {
                            continue;
                        }

                        let slot = dock_slots[ship.current_location];
                        dock_slots[ship.current_location] += 1;
                        let (base_x, base_y) = location_coords(ship.current_location);
                        let (offset_x, offset_y) = docked_ship_offset(slot);
                        ctx.print(
                            base_x + offset_x,
                            base_y + offset_y,
                            Span::styled(ship.map_tag(), ship_style(app, ship_index)),
                        );
                    }
                    ShipState::EnRoute {
                        origin,
                        destination,
                        eta_remaining,
                        total_eta,
                        ..
                    } => {
                        let position = interpolate(
                            location_coords(*origin),
                            location_coords(*destination),
                            transit_progress(*eta_remaining, *total_eta),
                        );
                        let point = [position];
                        ctx.draw(&Points {
                            coords: &point,
                            color: ship_color(app, ship_index),
                        });
                        ctx.print(
                            position.0 + 1.5,
                            position.1 + 1.5,
                            Span::styled(ship.map_tag(), ship_style(app, ship_index)),
                        );
                    }
                }
            }
        });

    frame.render_widget(canvas, area);
}

fn contract_list_item(app: &App, index: usize) -> ListItem<'static> {
    let contract = &app.contracts[index];

    if !app.is_contract_unlocked(index) {
        return ListItem::new(vec![
            Line::from(Span::styled(
                "Locked Opportunity",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                format!(
                    "Chart {} to unlock",
                    app.location_name(contract.unlock_location)
                ),
                Style::default().fg(Color::DarkGray),
            )),
        ]);
    }

    ListItem::new(vec![
        Line::from(format!(
            "{} [{}]",
            contract.title,
            app.contract_status_label(index)
        )),
        Line::from(format!(
            "{} -> {} | {} cr | ETA <= {}",
            app.location_name(contract.origin),
            app.location_name(contract.destination),
            app.contract_current_reward(index),
            contract.max_eta,
        )),
        Line::from(contract_list_summary(app, index)),
    ])
}

fn build_mission_text(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled(
            "Goal ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "Chart the full sector ({}/{}) and reach {} credits",
            app.discovered_count(),
            app.locations.len(),
            GOAL_CREDITS,
        )),
    ])];

    if let Some((frontier, reveal)) = app.next_discovery_target() {
        lines.push(Line::from(format!(
            "Next lead: send a ship to {}",
            app.location_name(frontier)
        )));
        lines.push(Line::from(format!(
            "That unlocks: {}",
            app.location_name(reveal)
        )));
    } else {
        lines.push(Line::from(
            "Sector charted: all known contacts are visible.",
        ));
        lines.push(Line::from(
            "Focus on clearing the remaining contract board for credits.",
        ));
    }

    if let Some(contract_index) = app.tracked_contract {
        let contract = &app.contracts[contract_index];
        lines.push(Line::from(format!(
            "Tracked: {} [{}]",
            contract.title,
            app.contract_status_label(contract_index)
        )));
        lines.push(Line::from(format!(
            "Run: {} -> {} | reward {} cr | ETA <= {}",
            app.location_name(contract.origin),
            app.location_name(contract.destination),
            app.contract_current_reward(contract_index),
            contract.max_eta,
        )));
        lines.push(Line::from(app.contract_pressure_text(contract_index)));
    } else {
        lines.push(Line::from(
            "Tracked contract: none. Accept one from the Mission Board.",
        ));
    }

    lines.push(Line::from(format!(
        "Credits: {} / {} | Fleet: {} in transit | {} docked",
        app.credits,
        GOAL_CREDITS,
        app.in_transit_count(),
        app.fleet.len() - app.in_transit_count()
    )));
    lines.push(Line::from(format!(
        "Difficulty: {}",
        app.difficulty.label()
    )));
    let low_fuel = app.low_fuel_ship_names();
    if !low_fuel.is_empty() {
        lines.push(Line::from(format!("Low fuel: {}", low_fuel.join(", "))));
    }
    lines.push(Line::from(
        "Flow: Board -> Enter -> Fleet -> Enter -> charted node -> Enter.",
    ));
    lines.push(Line::from(
        "Transit: route planning -> undocking -> cruising -> approach -> arrived.",
    ));

    match app.mode {
        crate::game::AppMode::Browse => {
            lines.push(Line::from(
                "Legend: cyan=selected yellow=origin magenta=frontier gray=unknown.",
            ));
        }
        crate::game::AppMode::SelectingDestination { ship_index } => {
            lines.push(Line::from(format!(
                "Dispatch armed: choose a destination for {}.",
                app.fleet[ship_index].name
            )));
        }
    }

    lines
}

fn location_color(app: &App, index: usize, origin: Option<usize>) -> Color {
    match (index == app.selected_location, Some(index) == origin) {
        (true, true) => Color::Green,
        (true, false) => Color::Cyan,
        (false, true) => Color::Yellow,
        (false, false) if app.is_frontier_location(index) => Color::Magenta,
        _ => Color::White,
    }
}

fn location_label_style(app: &App, index: usize, origin: Option<usize>) -> Style {
    let style = Style::default().fg(location_color(app, index, origin));

    if index == app.selected_location || Some(index) == origin || app.is_frontier_location(index) {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

fn station_fuel_style(app: &App, index: usize) -> Style {
    let fuel = app.station_fuel[index];
    let color = if fuel <= 4 {
        Color::Red
    } else if fuel <= 10 {
        Color::Yellow
    } else {
        Color::Gray
    };

    Style::default().fg(color)
}

fn ship_color(app: &App, ship_index: usize) -> Color {
    if matches!(app.fleet[ship_index].state, ShipState::Repairing { .. }) {
        Color::Red
    } else if app.pending_ship() == Some(ship_index) {
        Color::Green
    } else if app.selected_ship == ship_index {
        Color::Yellow
    } else {
        Color::LightCyan
    }
}

fn ship_style(app: &App, ship_index: usize) -> Style {
    Style::default()
        .fg(ship_color(app, ship_index))
        .add_modifier(Modifier::BOLD)
}

fn location_coords(index: usize) -> (f64, f64) {
    match index {
        ASTRA_PRIME => (50.0, 30.0),
        KITE_STATION => (18.0, 30.0),
        ION_ANCHORAGE => (82.0, 30.0),
        DUST_HARBOR => (50.0, 10.0),
        OUTER_RING_RELAY => (50.0, 52.0),
        _ => (50.0, 30.0),
    }
}

fn location_label_coords(index: usize) -> (f64, f64) {
    match index {
        ASTRA_PRIME => (43.0, 25.0),
        KITE_STATION => (7.0, 35.0),
        ION_ANCHORAGE => (75.0, 35.0),
        DUST_HARBOR => (44.0, 4.0),
        OUTER_RING_RELAY => (43.0, 56.0),
        _ => (43.0, 25.0),
    }
}

fn map_label(index: usize) -> &'static str {
    match index {
        ASTRA_PRIME => "Astra",
        KITE_STATION => "Kite",
        ION_ANCHORAGE => "Ion",
        DUST_HARBOR => "Dust",
        OUTER_RING_RELAY => "Relay",
        _ => "Unknown",
    }
}

fn node_points(x: f64, y: f64) -> [(f64, f64); 5] {
    [
        (x, y),
        (x - 1.0, y),
        (x + 1.0, y),
        (x, y - 1.0),
        (x, y + 1.0),
    ]
}

fn hidden_cloud_points(index: usize) -> Vec<(f64, f64)> {
    let (x, y) = location_coords(index);
    vec![
        (x - 5.0, y + 1.0),
        (x - 3.0, y + 3.0),
        (x - 1.0, y),
        (x + 1.0, y + 2.0),
        (x + 3.0, y - 1.0),
        (x + 5.0, y + 1.0),
        (x - 2.0, y - 2.0),
        (x + 2.0, y - 3.0),
    ]
}

fn docked_ship_offset(slot: usize) -> (f64, f64) {
    match slot {
        0 => (-5.0, -4.0),
        1 => (3.0, -4.0),
        2 => (-5.0, 4.0),
        _ => (3.0, 4.0),
    }
}

fn transit_progress(eta_remaining: u16, total_eta: u16) -> f64 {
    let completed = total_eta.saturating_sub(eta_remaining);
    (f64::from(completed) / f64::from(total_eta.max(1))).clamp(0.05, 0.95)
}

fn interpolate(start: (f64, f64), end: (f64, f64), t: f64) -> (f64, f64) {
    (
        start.0 + (end.0 - start.0) * t,
        start.1 + (end.1 - start.1) * t,
    )
}

fn route_segments(from: usize, to: usize) -> Vec<(usize, usize)> {
    if from == ASTRA_PRIME || to == ASTRA_PRIME {
        vec![(from, to)]
    } else {
        vec![(from, ASTRA_PRIME), (ASTRA_PRIME, to)]
    }
}

fn route_preview_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    if let Some(ship_index) = app.pending_ship() {
        let ship = &app.fleet[ship_index];
        lines.push(Line::from(vec![
            Span::raw("Planning route for: "),
            Span::styled(
                ship.name,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(format!(
            "Origin: {}",
            app.location_name(ship.current_location)
        )));
        lines.push(Line::from(format!(
            "Ship stats: speed {} | fuel {}/{}",
            ship.speed, ship.current_fuel, ship.max_fuel
        )));
        lines.push(Line::from(format!(
            "Destination: {}",
            app.location_name(app.selected_location)
        )));

        if let Some(plan) = app.plan_route_for_ship(ship_index, app.selected_location) {
            lines.push(Line::from(format!("Path: {}", plan.path)));
            lines.push(Line::from(format!("ETA: {} ticks", plan.eta)));
            lines.push(Line::from(format!("Fuel needed: {}", plan.fuel_required)));
            lines.push(Line::from(format!(
                "Conditions: {}",
                plan.condition_summary
            )));
            lines.extend(route_fuel_lines(app, ship_index, plan.fuel_required));

            if let Some(contract_index) = app.tracked_contract {
                let contract = &app.contracts[contract_index];
                if contract.origin == ship.current_location
                    && contract.destination == app.selected_location
                {
                    lines.push(Line::from(format!(
                        "Tracked contract: {} | reward {} cr",
                        contract.title,
                        app.contract_current_reward(contract_index)
                    )));
                    lines.push(Line::from(app.contract_pressure_text(contract_index)));
                    if plan.eta > contract.max_eta {
                        lines.push(Line::from(format!(
                            "Too slow for contract: needs ETA <= {}",
                            contract.max_eta
                        )));
                    } else {
                        lines.push(Line::from("This route can complete the tracked contract."));
                    }
                } else {
                    lines.push(Line::from(format!(
                        "Tracked contract still waiting: {}",
                        contract.title
                    )));
                }
            }

            if app.is_frontier_location(app.selected_location) {
                lines.push(Line::from(
                    "Survey lead: arrival may reveal new coordinates.",
                ));
            }
        } else {
            lines.push(Line::from(
                "Choose a destination other than the current dock to dispatch.",
            ));
        }

        lines.push(Line::from("Enter: confirm route   Esc: cancel"));
        return lines;
    }

    let ship = &app.fleet[app.selected_ship];
    lines.push(Line::from(vec![
        Span::raw("Selected ship: "),
        Span::styled(
            ship.name,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    match &ship.state {
        ShipState::Docked => {
            lines.push(Line::from(format!(
                "Docked at {}",
                app.location_name(ship.current_location)
            )));
            lines.push(Line::from(format!(
                "Ship stats: speed {} | fuel {}/{}",
                ship.speed, ship.current_fuel, ship.max_fuel
            )));

            if let Some(plan) = app.plan_route_for_ship(app.selected_ship, app.selected_location) {
                lines.push(Line::from(format!("Preview: {}", plan.path)));
                lines.push(Line::from(format!("ETA: {} ticks", plan.eta)));
                lines.push(Line::from(format!("Fuel needed: {}", plan.fuel_required)));
                lines.push(Line::from(format!(
                    "Conditions: {}",
                    plan.condition_summary
                )));
                lines.extend(route_fuel_lines(app, app.selected_ship, plan.fuel_required));

                if let Some(contract_index) = app.tracked_contract {
                    let contract = &app.contracts[contract_index];
                    if contract.origin == ship.current_location
                        && contract.destination == app.selected_location
                    {
                        lines.push(Line::from(format!(
                            "Tracked contract: {} | reward {} cr",
                            contract.title,
                            app.contract_current_reward(contract_index)
                        )));
                        lines.push(Line::from(app.contract_pressure_text(contract_index)));
                        if plan.eta > contract.max_eta {
                            lines.push(Line::from(format!(
                                "Too slow for contract: needs ETA <= {}",
                                contract.max_eta
                            )));
                        } else {
                            lines.push(Line::from("This route can complete the tracked contract."));
                        }
                    }
                }

                if app.is_frontier_location(app.selected_location) {
                    lines.push(Line::from(
                        "Survey lead: arrival may reveal new coordinates.",
                    ));
                }
            } else {
                lines.push(Line::from(
                    "Select a different charted destination to preview a route.",
                ));
            }

            lines.push(Line::from("Press Enter in Fleet to dispatch this ship."));
        }
        ShipState::Repairing { ticks_remaining } => {
            lines.push(Line::from(format!(
                "Repairing at {}",
                app.location_name(ship.current_location)
            )));
            lines.push(Line::from(format!(
                "Ship stats: speed {} | fuel {}/{}",
                ship.speed, ship.current_fuel, ship.max_fuel
            )));
            lines.push(Line::from(format!(
                "Repairs remaining: {} ticks",
                ticks_remaining
            )));
            lines.push(Line::from(
                "This ship cannot launch until repairs complete.",
            ));
        }
        ShipState::EnRoute {
            origin,
            destination,
            eta_remaining,
            total_eta,
            route,
            condition_summary,
            assigned_contract,
            ..
        } => {
            let phase = transit_phase(*eta_remaining, *total_eta);
            lines.push(Line::from(format!("Phase: {}", phase.label())));
            lines.push(Line::from(format!("Current route: {}", route)));
            lines.push(Line::from(format!(
                "{} -> {} | ETA {}/{}",
                app.location_name(*origin),
                app.location_name(*destination),
                eta_remaining,
                total_eta,
            )));
            lines.push(Line::from(format!(
                "Locked conditions: {}",
                condition_summary
            )));
            if let Some(contract_index) = assigned_contract {
                lines.push(Line::from(format!(
                    "Assigned contract: {}",
                    app.contracts[*contract_index].title
                )));
            }
            lines.push(Line::from(
                "This ship cannot accept a new dispatch while in transit.",
            ));
        }
    }

    lines
}

fn route_fuel_lines(app: &App, ship_index: usize, fuel_required: u16) -> Vec<Line<'static>> {
    let ship = &app.fleet[ship_index];
    let station_index = ship.current_location;

    if !app.difficulty.uses_fuel_economy() {
        return vec![
            Line::from("Fuel: relaxed in Cozy mode"),
            Line::from(format!(
                "{} fuel stock: {} units",
                app.location_name(station_index),
                app.station_fuel[station_index]
            )),
        ];
    }

    let mut lines = vec![Line::from(format!(
        "Fuel aboard: {}/{}",
        ship.current_fuel, ship.max_fuel
    ))];
    lines.push(Line::from(format!(
        "{} fuel stock: {} units",
        app.location_name(station_index),
        app.station_fuel[station_index]
    )));

    if ship.current_fuel >= fuel_required {
        lines.push(Line::from("Fuel ready: this ship can depart now"));
    } else {
        lines.push(Line::from(format!(
            "Needs {} more fuel before launch",
            fuel_required - ship.current_fuel
        )));

        match app.refuel_plan_for_ship(ship_index, fuel_required) {
            RefuelPlan::NotNeeded => {}
            RefuelPlan::CanPurchase { units, cost } => {
                lines.push(Line::from(format!(
                    "Refuel with `f`: +{} fuel for {} cr",
                    units, cost
                )));
            }
            RefuelPlan::NeedTransfer { units } => {
                lines.push(Line::from(format!(
                    "Need {} transferred fuel with `t` or more station stock",
                    units
                )));
            }
            RefuelPlan::ExceedsTank => {
                lines.push(Line::from("This route exceeds the ship's fuel tank."));
            }
            RefuelPlan::BlockedByCredits { cost } => {
                lines.push(Line::from(format!(
                    "Cannot buy enough fuel: need {} cr",
                    cost
                )));
            }
            RefuelPlan::BlockedByStation { units } => {
                lines.push(Line::from(format!("Station short by {} fuel units", units)));
            }
        }
    }

    lines
}

fn contract_list_summary(app: &App, index: usize) -> String {
    let contract = &app.contracts[index];

    match contract.state {
        crate::game::ContractState::Available => match app.difficulty {
            Difficulty::Cozy => "Open | no expiry".to_string(),
            Difficulty::Normal => "Open | reward decays after accept".to_string(),
            Difficulty::Insane => format!("Open | {}t window after accept", contract.deadline),
        },
        crate::game::ContractState::Accepted { .. } => app.contract_pressure_text(index),
        crate::game::ContractState::Assigned { ship_name, .. } => {
            format!("Assigned to {}", ship_name)
        }
        crate::game::ContractState::Completed => "Completed; slot will refresh".to_string(),
        crate::game::ContractState::Failed => "Failed; slot will refresh".to_string(),
    }
}

fn contract_detail_lines(app: &App) -> Vec<Line<'static>> {
    let index = app.selected_contract;
    let contract = &app.contracts[index];

    if !app.is_contract_unlocked(index) {
        return vec![
            Line::from("Locked Opportunity"),
            Line::from(format!(
                "Unlock by charting {}.",
                app.location_name(contract.unlock_location)
            )),
            Line::from(contract.briefing.clone()),
        ];
    }

    let mut lines = vec![
        Line::from(format!(
            "{} [{}]",
            contract.title,
            app.contract_status_label(index)
        )),
        Line::from(format!(
            "Route: {} -> {}",
            app.location_name(contract.origin),
            app.location_name(contract.destination)
        )),
        Line::from(format!(
            "Reward now: {} cr | Target ETA <= {}",
            app.contract_current_reward(index),
            contract.max_eta
        )),
        Line::from(format!("Archetype: {}", contract.archetype.title())),
        Line::from(contract.archetype.effect_summary()),
        Line::from(app.contract_pressure_text(index)),
        Line::from(contract.briefing.clone()),
    ];

    if matches!(app.difficulty, Difficulty::Cozy) {
        if let Some((ship_index, eta)) = best_ship_hint(app, index) {
            lines.push(Line::from(format!(
                "Cozy hint: best docked fit is {} (ETA {}).",
                app.fleet[ship_index].name, eta
            )));
        } else {
            lines.push(Line::from(
                "Cozy hint: no docked ship can complete this contract right now.",
            ));
        }
    }

    if app.tracked_contract == Some(index) {
        lines.push(Line::from(
            "Tracked contract: this is the current assignment target.",
        ));
    }

    lines.push(Line::from(
        "Enter on Mission Board: accept or release this contract.",
    ));
    lines
}

fn ship_detail_lines(app: &App) -> Vec<Line<'static>> {
    let ship_index = app.selected_ship;
    let ship = &app.fleet[ship_index];
    let station = app.location_name(ship.current_location);
    let mut lines = vec![
        Line::from(ship.name),
        Line::from(format!("Station: {}", station)),
        Line::from(format!(
            "Speed: {} | Fuel: {}/{}",
            ship.speed, ship.current_fuel, ship.max_fuel
        )),
        Line::from(format!(
            "Hull: {}% | Condition: {}",
            ship.hull,
            crate::game::GameData::hull_status(ship)
        )),
        Line::from(format!(
            "Station fuel stock: {}",
            app.station_fuel[ship.current_location]
        )),
    ];

    if ship.current_fuel <= ship.max_fuel.saturating_div(4).max(1) {
        lines.push(Line::from(
            "Low fuel: refuel with `f` or transfer with `t`.",
        ));
    }

    match &ship.state {
        ShipState::Docked => {
            lines.push(Line::from("Status: docked and available."));
            if let Some(contract_index) = app.tracked_contract {
                let contract = &app.contracts[contract_index];
                if contract.origin != ship.current_location {
                    lines.push(Line::from(format!(
                        "Tracked contract starts elsewhere: {}.",
                        app.location_name(contract.origin)
                    )));
                } else if let Some(plan) = app.plan_route_for_ship(ship_index, contract.destination)
                {
                    if plan.eta > contract.max_eta {
                        lines.push(Line::from(format!(
                            "Too slow for tracked contract: ETA {} > {}.",
                            plan.eta, contract.max_eta
                        )));
                    } else if ship.current_fuel < plan.fuel_required {
                        lines.push(Line::from(format!(
                            "Needs {} fuel before it can run the tracked contract.",
                            plan.fuel_required - ship.current_fuel
                        )));
                    } else {
                        lines.push(Line::from("Ready for tracked contract with current fuel."));
                    }
                }
            }
            lines.push(Line::from(
                "Actions: `f` buy station fuel, `t` transfer dockside fuel.",
            ));
            if let Some(offer) = app.next_upgrade_offer(ship_index) {
                lines.push(Line::from(format!(
                    "Upgrade with `u`: {}",
                    offer.description
                )));
            } else {
                lines.push(Line::from(
                    "Upgrade with `u`: no dockside upgrade currently offered.",
                ));
            }
        }
        ShipState::Repairing { ticks_remaining } => {
            lines.push(Line::from(format!(
                "Status: repairing in port ({} ticks remaining).",
                ticks_remaining
            )));
            lines.push(Line::from("Actions locked until repairs complete."));
        }
        ShipState::EnRoute {
            destination,
            eta_remaining,
            total_eta,
            assigned_contract,
            ..
        } => {
            lines.push(Line::from(format!(
                "Status: en route to {} ({}/{})",
                app.location_name(*destination),
                eta_remaining,
                total_eta
            )));
            if let Some(contract_index) = assigned_contract {
                lines.push(Line::from(format!(
                    "Carrying: {}",
                    app.contracts[*contract_index].title
                )));
            }
            lines.push(Line::from("Fuel actions unavailable while in transit."));
        }
    }

    lines
}

fn ship_list_item(
    ship: &Ship,
    locations: &[Location],
    contracts: &[crate::game::Contract],
    pending: bool,
) -> ListItem<'static> {
    let mut title = ship.name.to_string();
    if pending {
        title.push_str(" [planning]");
    }

    let (status, detail) = match &ship.state {
        ShipState::Docked => (
            format!("Docked at {}", locations[ship.current_location].name),
            format!(
                "Fuel {}/{} | Speed {} | Hull {}%{}",
                ship.current_fuel,
                ship.max_fuel,
                ship.speed,
                ship.hull,
                if ship.current_fuel * 4 <= ship.max_fuel {
                    " | LOW FUEL"
                } else {
                    ""
                }
            ),
        ),
        ShipState::Repairing { ticks_remaining } => (
            format!("Repairing at {}", locations[ship.current_location].name),
            format!(
                "Fuel {}/{} | Speed {} | Hull {}% | {} ticks remaining",
                ship.current_fuel, ship.max_fuel, ship.speed, ship.hull, ticks_remaining
            ),
        ),
        ShipState::EnRoute {
            origin,
            destination,
            eta_remaining,
            total_eta,
            assigned_contract,
            ..
        } => {
            let phase = transit_phase(*eta_remaining, *total_eta);
            let detail = if let Some(contract_index) = assigned_contract {
                format!(
                    "carrying {} | fuel {}/{} | speed {} | hull {}%",
                    contracts[*contract_index].title,
                    ship.current_fuel,
                    ship.max_fuel,
                    ship.speed,
                    ship.hull
                )
            } else {
                format!(
                    "fuel {}/{} | speed {} | hull {}% | underway",
                    ship.current_fuel, ship.max_fuel, ship.speed, ship.hull
                )
            };
            let status = if let Some(contract_index) = assigned_contract {
                format!(
                    "{} -> {} | {} | {}",
                    locations[*origin].name,
                    locations[*destination].name,
                    phase.label(),
                    contracts[*contract_index].title
                )
            } else {
                format!(
                    "{} -> {} | {} | ETA {}",
                    locations[*origin].name,
                    locations[*destination].name,
                    phase.label(),
                    eta_remaining
                )
            };
            (status, detail)
        }
    };

    ListItem::new(vec![
        Line::from(title),
        Line::from(status),
        Line::from(detail),
    ])
}

fn best_ship_hint(app: &App, contract_index: usize) -> Option<(usize, u16)> {
    let contract = &app.contracts[contract_index];

    app.fleet
        .iter()
        .enumerate()
        .filter(|(_, ship)| {
            matches!(ship.state, ShipState::Docked) && ship.current_location == contract.origin
        })
        .filter_map(|(ship_index, ship)| {
            app.plan_route_for_ship(ship_index, contract.destination)
                .and_then(|plan| {
                    (plan.eta <= contract.max_eta && ship.current_fuel >= plan.fuel_required)
                        .then_some((ship_index, plan.eta))
                })
        })
        .min_by_key(|(_, eta)| *eta)
}
