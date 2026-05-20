use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
        canvas::{Canvas, Line as CanvasLine, Points},
    },
};

use crate::{
    app::{App, DIFFICULTY_OPTIONS, InspectOverlay, LLM_FIELDS, Screen, TICK_SPEEDS},
    game::{
        CONTRACTS_PANE, Difficulty, FLEET_PANE, GOAL_CREDITS, LOG_PANE, Location, MAP_PANE,
        PANE_TITLES, RefuelPlan, RunOutcome, SECTOR_LOCATION_COUNT, SHIPYARD_PANE, Ship,
        ShipShopOffer, ShipState, transit_phase,
    },
    llm::PROMPT_CATALOG,
    save::SAVE_SLOT_COUNT,
};

pub(crate) fn draw(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::LlmGate => draw_llm_gate(frame, app),
        Screen::StartMenu => draw_start_menu(frame, app),
        Screen::LoadGame => draw_load_game(frame, app),
        Screen::InitializingWorld => draw_initializing_world(frame, app),
        Screen::Settings => draw_settings(frame, app),
        Screen::HowToPlay => draw_how_to_play(frame, app),
        Screen::InGame => draw_game(frame, app),
        Screen::EndGame => draw_end_game(frame, app),
    }
}

fn draw_initializing_world(frame: &mut Frame, app: &App) {
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
            "Initializing Your Environment",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("Seeding the first playable environment before opening the bridge."),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Bootstrap"))
    .wrap(Wrap { trim: true });
    frame.render_widget(header, areas[0]);

    let seed = app.pending_world_seed().unwrap_or(app.world_seed);
    let body = Paragraph::new(Text::from(vec![
        Line::from(format!("Seed: {}", seed)),
        Line::from(format!("Target environment: {}", app.sector_name)),
        Line::from(""),
        Line::from(app.llm_summary()),
        Line::from(format!("Last LLM status: {}", app.last_llm_status)),
        Line::from(""),
        Line::from(app.sector_summary.clone()),
        Line::from(""),
        Line::from("Boot sequence:"),
        Line::from("1. Resolve seeded sector identity and station names."),
        Line::from("2. Prepare the first charted frontier environment."),
        Line::from("3. Open the bridge once the environment is ready."),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Status"))
    .wrap(Wrap { trim: true });
    frame.render_widget(body, areas[1]);

    let footer = Paragraph::new(Line::from("Esc: cancel to menu   q/Ctrl+C: quit"))
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, areas[2]);
}

fn draw_llm_gate(frame: &mut Frame, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(frame.area());
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(34), Constraint::Percentage(66)])
        .split(areas[1]);

    let header = Paragraph::new(Text::from(vec![
        Line::from(Span::styled(
            "LLM Connection Required",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from("LLM mode is enabled, but the configured OpenAI-compatible endpoint is not currently available."),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Connection Gate"))
    .wrap(Wrap { trim: true });
    frame.render_widget(header, areas[0]);

    let options = app.llm_gate_options();
    let menu = List::new(options.iter().map(|option| ListItem::new(option.label())))
        .highlight_style(selection_style())
        .highlight_symbol(">> ")
        .block(Block::default().borders(Borders::ALL).title("Choices"));
    let mut state = ListState::default();
    state.select(Some(app.llm_gate_selection));
    frame.render_stateful_widget(menu, body[0], &mut state);

    let detail = Paragraph::new(Text::from(vec![
        Line::from("Detected issue:"),
        Line::from(app.menu_feedback.clone().unwrap_or_else(|| "LLM connection not available.".to_string())),
        Line::from(""),
        Line::from(app.llm_summary()),
        Line::from(format!("Last LLM status: {}", app.last_llm_status)),
        Line::from(""),
        Line::from("Disable LLM to continue with the deterministic storyline, retry the connection, or open Settings to fix the endpoint/model/key."),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Status"))
    .wrap(Wrap { trim: true });
    frame.render_widget(detail, body[1]);

    let footer = Paragraph::new(Line::from(
        "Up/Down: choose   Enter: confirm   q/Ctrl+C: quit",
    ))
    .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, areas[2]);
}

fn draw_game(frame: &mut Frame, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(16),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(areas[2]);

    let map_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(12), Constraint::Length(7)])
        .split(body[0]);
    let focus_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(10)])
        .split(body[1]);

    let header = Paragraph::new(Line::from(vec![
        "Starlane Courier".into(),
        "  ".into(),
        format!("Sector: {}", app.sector_name).into(),
        "  ".into(),
        format!("Player: {}", app.player_status_text()).into(),
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
    ]))
    .block(Block::default().borders(Borders::ALL).title("Bridge"));
    frame.render_widget(header, areas[0]);

    let status = Paragraph::new(Text::from(bridge_status_lines(app)))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Operations Deck"),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(status, areas[1]);

    render_sector_map(frame, map_sections[0], app, app.active_pane == MAP_PANE);

    let station = Paragraph::new(Text::from(station_brief_lines(app)))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Selected Station"),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(station, map_sections[1]);

    draw_focus_tabs(frame, focus_sections[0], app);
    render_active_focus_panel(frame, focus_sections[1], app);

    let footer = Paragraph::new(Line::from(app.controls_text()))
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, areas[3]);

    if let Some(message) = &app.popup_message {
        draw_popup(
            frame,
            "Notification",
            message,
            "Any key: dismiss   q/Ctrl+C: quit",
        );
    } else if let Some(overlay) = app.current_inspect_overlay() {
        draw_inspect_overlay(frame, app, overlay);
    }
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
            "Run a frontier dispatch shift: chart the environment, take contracts, and grow credits.",
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
        Line::from(format!("Last LLM status: {}", app.last_llm_status)),
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
    let primary_prompt = PROMPT_CATALOG[0];
    let prompt_names = PROMPT_CATALOG
        .iter()
        .map(|prompt| prompt.name)
        .collect::<Vec<_>>()
        .join(", ");
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
        Line::from(format!("Last status: {}", app.last_llm_status)),
        Line::from(format!(
            "Prompt catalog: {} template(s) loaded.",
            PROMPT_CATALOG.len()
        )),
        Line::from(format!("Active prompts: {}", prompt_names)),
        Line::from(format!(
            "Primary prompt: {} -> {}",
            primary_prompt.name, primary_prompt.output
        )),
        Line::from(if app.settings_edit.is_some() {
            "Editing: type text, Backspace, Enter to save, Esc to cancel".to_string()
        } else {
            "Settings: Enter edits/toggles the selected field; `c` tests the LLM connection; Delete clears the API key".to_string()
        }),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Effect"))
    .wrap(Wrap { trim: true });
    frame.render_widget(detail, body[3]);

    let footer = Paragraph::new(Line::from(
        "Left/Right: focus   Up/Down: choose   Enter: apply/edit   c: test LLM   Delete: clear API key   Esc: back   q/Ctrl+C: quit",
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
            "Primary goals: chart the full environment and reach {} credits.",
            GOAL_CREDITS
        )),
        Line::from("1. Accept a contract from the Mission Board."),
        Line::from("2. Refuel with `f` or transfer dockside fuel with `t` if needed."),
        Line::from("3. Upgrade a docked ship with `u` if you need more speed or range."),
        Line::from("4. Move to Fleet and press Enter on a docked ship for a normal dispatch."),
        Line::from(
            "5. Press `e` on a docked ship to arm an exploration run toward a frontier node.",
        ),
        Line::from("6. Focus the map, pick the destination, and confirm the route."),
        Line::from(
            "7. Exploration runs uncover hidden contacts only when you arrive at frontier nodes.",
        ),
        Line::from("8. You can only operate docked ships at your current location."),
        Line::from("9. Use `m` on the map to take a paid passenger transfer to another discovered station."),
        Line::from("10. Some hubs and anchorages host shipyards with generated hull offers."),
        Line::from("11. Select a charted station in the map pane and press `b` to buy a ship if you can afford it."),
        Line::from("12. Use Alerts with Enter to jump to urgent ships, stations, or contracts."),
        Line::from("13. Contracts, exploration, and shipyards all compete for fuel and credits."),
        Line::from("14. Watch ships move through undocking, cruising, incidents, and arrival."),
        Line::from(""),
        Line::from("Difficulties:"),
        Line::from("Cozy: fixed rewards, no timeout pressure, light fuel costs with emergency reserve top-offs."),
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

fn draw_popup(frame: &mut Frame, title: &'static str, message: &str, footer: &str) {
    let area = centered_rect(64, 22, frame.area());
    frame.render_widget(Clear, area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(3)])
        .split(area);

    let body = Paragraph::new(Text::from(vec![Line::from(message.to_string())]))
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: true });
    frame.render_widget(body, sections[0]);

    let controls = Paragraph::new(Line::from(footer))
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(controls, sections[1]);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn selection_style() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn pane_short_title(index: usize) -> &'static str {
    match index {
        CONTRACTS_PANE => "Board",
        MAP_PANE => "Map",
        FLEET_PANE => "Fleet",
        SHIPYARD_PANE => "Yard",
        LOG_PANE => "Signals",
        _ => "Focus",
    }
}

fn bridge_status_lines(app: &App) -> Vec<Line<'static>> {
    let docked_count = app.fleet.len().saturating_sub(app.in_transit_count());
    let mut lines = vec![Line::from(vec![
        Span::styled(
            "Player ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(app.player_status_text()),
        Span::raw("   "),
        Span::styled(
            "Fleet ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "{} docked / {} in transit",
            docked_count,
            app.in_transit_count()
        )),
        Span::raw("   "),
        Span::styled(
            "Difficulty ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(app.difficulty.label()),
    ])];

    match app.mode {
        crate::game::AppMode::SelectingDestination { ship_index, intent } => {
            lines.push(Line::from(format!(
                "Planning {} for {} toward {}.",
                intent.label().to_lowercase(),
                app.fleet[ship_index].name,
                app.location_name(app.selected_location)
            )));
        }
        crate::game::AppMode::Browse => {
            if let Some(contract_index) = app.tracked_contract {
                let contract = &app.contracts[contract_index];
                lines.push(Line::from(format!(
                    "Tracked mission: {} | {} -> {} | {} cr | ETA <= {}",
                    contract.title,
                    app.location_name(contract.origin),
                    app.location_name(contract.destination),
                    app.contract_current_reward(contract_index),
                    contract.max_eta,
                )));
            } else if let Some((frontier, _)) = app.next_discovery_target() {
                lines.push(Line::from(format!(
                    "Next lead: send an exploration run to {}.",
                    app.location_name(frontier)
                )));
            } else {
                lines.push(Line::from(
                    "Environment fully charted. Push contracts and ship growth to reach the credit goal.",
                ));
            }
        }
    }

    let signal = app
        .current_alerts()
        .first()
        .map(|alert| alert.summary.clone())
        .unwrap_or_else(|| "No urgent incidents.".to_string());
    let suffix = if app.pending_contract_flavor_count() > 0 {
        format!(" | LLM jobs {}", app.pending_contract_flavor_count())
    } else {
        let low_fuel = app.low_fuel_ship_names();
        if low_fuel.is_empty() {
            String::new()
        } else {
            format!(" | Low fuel: {}", low_fuel.join(", "))
        }
    };
    lines.push(Line::from(format!(
        "Focus: {} | Signal: {}{}",
        pane_short_title(app.active_pane),
        signal,
        suffix
    )));

    lines
}

fn station_brief_lines(app: &App) -> Vec<Line<'static>> {
    let index = app.selected_location;
    let ship = &app.fleet[app.selected_ship];
    let mut lines = vec![
        Line::from(format!(
            "{} | {} | {}",
            app.location_name(index),
            app.location_sector(index),
            app.location_region(index)
        )),
        Line::from(app.location_description(index).to_string()),
    ];

    if let Some(plan) = app.plan_route_for_ship(app.selected_ship, index) {
        lines.push(Line::from(format!(
            "{} preview: ETA {} | fuel {} | {}",
            ship.name, plan.eta, plan.fuel_required, plan.path
        )));
        if let Some(note) = app.active_mission_assignment_note(app.selected_ship, index, plan.eta) {
            lines.push(Line::from(note));
        }
    } else if ship.is_docked() {
        lines.push(Line::from(format!(
            "{} preview: choose another charted station to plan a route.",
            ship.name
        )));
    } else {
        lines.push(Line::from(format!(
            "{} is already in transit and cannot take new orders yet.",
            ship.name
        )));
    }

    lines.push(Line::from(if app.player_is_in_transit() {
        format!("Player: {}", app.player_status_text())
    } else if index == app.player_location {
        "Player: present at this station".to_string()
    } else {
        match app.player_transfer_cost(index) {
            Some(cost) => format!("Player transfer: {} cr with `m`", cost),
            None => "Player transfer: unavailable right now".to_string(),
        }
    }));

    let shipyard = if let Some(offer) = app.shipyard_offer(index) {
        format!(
            "Shipyard: {} offers | selected {} for {} cr",
            app.shipyard_offer_count(index),
            offer.name,
            offer.price
        )
    } else if app.has_shipyard(index) {
        "Shipyard: sold out until the next refresh".to_string()
    } else {
        "Shipyard: none active here".to_string()
    };
    let lead = app
        .exploration_heading_hint(index)
        .map(|heading| format!("Lead {}", heading))
        .unwrap_or_else(|| format!("Docked ships {}", docked_ship_count_at(app, index)));
    lines.push(Line::from(format!("{} | {}", shipyard, lead)));
    lines.push(Line::from(
        "Press `i` for full station, route, and marketplace detail.",
    ));

    lines
}

fn draw_focus_tabs(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans = Vec::new();
    for index in 0..PANE_TITLES.len() {
        let style = if app.active_pane == index {
            selection_style()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(
            format!(" {} ", pane_short_title(index)),
            style,
        ));
        if index < PANE_TITLES.len() - 1 {
            spans.push(Span::raw(" "));
        }
    }

    let tabs = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL).title("Focus"))
        .wrap(Wrap { trim: true });
    frame.render_widget(tabs, area);
}

fn render_active_focus_panel(frame: &mut Frame, area: Rect, app: &App) {
    match app.active_pane {
        CONTRACTS_PANE => {
            let contracts = List::new(
                app.contracts
                    .iter()
                    .enumerate()
                    .map(|(index, _)| contract_list_item(app, index)),
            )
            .highlight_style(selection_style())
            .highlight_symbol(">> ")
            .block(pane_block(PANE_TITLES[CONTRACTS_PANE], true));
            let mut contract_state = ListState::default();
            contract_state.select(Some(app.selected_contract));
            frame.render_stateful_widget(contracts, area, &mut contract_state);
        }
        MAP_PANE => {
            let title = if matches!(app.mode, crate::game::AppMode::SelectingDestination { .. }) {
                "Route Planner"
            } else {
                "Station & Route"
            };
            let panel = Paragraph::new(Text::from(focused_map_lines(app)))
                .block(Block::default().borders(Borders::ALL).title(title))
                .wrap(Wrap { trim: true });
            frame.render_widget(panel, area);
        }
        FLEET_PANE => {
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
            .block(pane_block(PANE_TITLES[FLEET_PANE], true));
            let mut fleet_state = ListState::default();
            fleet_state.select(Some(app.selected_ship));
            frame.render_stateful_widget(fleet, area, &mut fleet_state);
        }
        SHIPYARD_PANE => {
            let shipyard = List::new(shipyard_list_items(app))
                .highlight_style(selection_style())
                .highlight_symbol("$$ ")
                .block(pane_block(PANE_TITLES[SHIPYARD_PANE], true));
            let mut shipyard_state = ListState::default();
            let shipyard_count = app.shipyard_offer_count(app.selected_location);
            shipyard_state.select(if shipyard_count > 0 {
                Some(app.selected_shipyard_offer.min(shipyard_count - 1))
            } else {
                None
            });
            frame.render_stateful_widget(shipyard, area, &mut shipyard_state);
        }
        LOG_PANE => {
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
            .block(pane_block("Signals", true));
            let mut alerts_state = ListState::default();
            alerts_state.select(Some(
                app.selected_alert.min(alerts_data.len().saturating_sub(1)),
            ));
            frame.render_stateful_widget(alerts, area, &mut alerts_state);
        }
        _ => {}
    }
}

fn focused_map_lines(app: &App) -> Vec<Line<'static>> {
    let index = app.selected_location;
    let ship = &app.fleet[app.selected_ship];
    let mut lines = vec![
        Line::from(format!(
            "View: {} | {}",
            app.map_zoom_label(),
            app.map_scope_label()
        )),
        Line::from(format!("Focus: {}", app.map_focus_location_name())),
        Line::from(format!(
            "Selected destination: {}",
            app.location_name(index)
        )),
        Line::from(app.location_description(index).to_string()),
        Line::from(format!(
            "Selected ship: {} ({})",
            ship.name, ship.class_name
        )),
    ];

    if let Some(intent) = app.pending_dispatch_intent() {
        lines.push(Line::from(format!("Intent: {}", intent.label())));
    }

    if let Some(plan) = app.plan_route_for_ship(app.selected_ship, index) {
        lines.push(Line::from(format!(
            "Preview: ETA {} | fuel {} | {}",
            plan.eta, plan.fuel_required, plan.path
        )));
        lines.push(Line::from(format!(
            "Conditions: {}",
            plan.condition_summary
        )));
        if let Some(note) = app.active_mission_assignment_note(app.selected_ship, index, plan.eta) {
            lines.push(Line::from(note));
        }
    } else if ship.is_docked() {
        lines.push(Line::from(
            "Preview unavailable until you choose another charted station.",
        ));
    } else {
        lines.push(Line::from(
            "Preview unavailable while the selected ship is in transit.",
        ));
    }

    lines.push(Line::from(
        if app.player_can_operate_ship(app.selected_ship) {
            "Local control: selected ship is operable here".to_string()
        } else {
            format!(
                "Local control: transfer to {} before operating {}",
                app.location_name(ship.current_location),
                ship.name
            )
        },
    ));

    lines.push(Line::from(if let Some(offer) = app.shipyard_offer(index) {
        format!(
            "Shipyard: {} offers | selected {} ({}) for {} cr",
            app.shipyard_offer_count(index),
            offer.name,
            offer.class_name,
            offer.price
        )
    } else if app.has_shipyard(index) {
        "Shipyard: sold out until next rotation".to_string()
    } else {
        "Shipyard: none at the selected station".to_string()
    }));

    lines.push(Line::from(
        "`i` opens full route detail. `m` transfers the player. `b` buys the selected hull.",
    ));

    lines
}

fn draw_inspect_overlay(frame: &mut Frame, app: &App, overlay: InspectOverlay) {
    match overlay {
        InspectOverlay::MissionBoard => draw_mission_board_overlay(frame, app),
        InspectOverlay::Station => draw_station_overlay(frame, app),
        InspectOverlay::Fleet => draw_fleet_overlay(frame, app),
        InspectOverlay::Shipyard => draw_shipyard_overlay(frame, app),
        InspectOverlay::Signals => draw_signals_overlay(frame, app),
    }
}

fn draw_mission_board_overlay(frame: &mut Frame, app: &App) {
    let area = centered_rect(84, 82, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Mission Board Detail");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(3)])
        .split(inner);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(sections[0]);
    let detail_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(body[1]);

    let contracts = List::new(
        app.contracts
            .iter()
            .enumerate()
            .map(|(index, _)| contract_list_item(app, index)),
    )
    .highlight_style(selection_style())
    .highlight_symbol(">> ")
    .block(Block::default().borders(Borders::ALL).title("Contracts"));
    let mut contract_state = ListState::default();
    contract_state.select(Some(app.selected_contract));
    frame.render_stateful_widget(contracts, body[0], &mut contract_state);

    let detail = Paragraph::new(Text::from(contract_detail_lines(app)))
        .block(Block::default().borders(Borders::ALL).title("Detail"))
        .wrap(Wrap { trim: true });
    frame.render_widget(detail, detail_sections[0]);

    let shift = Paragraph::new(Text::from(build_mission_text(app)))
        .block(Block::default().borders(Borders::ALL).title("Shift Status"))
        .wrap(Wrap { trim: true });
    frame.render_widget(shift, detail_sections[1]);

    let footer = Paragraph::new(Line::from(
        "Up/Down: choose contract   Enter: accept/release   r: regenerate flavor   i/Esc: close",
    ))
    .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, sections[1]);
}

fn draw_station_overlay(frame: &mut Frame, app: &App) {
    let area = centered_rect(86, 84, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Station Detail");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(56),
            Constraint::Percentage(44),
            Constraint::Length(3),
        ])
        .split(inner);

    let route = Paragraph::new(Text::from(route_preview_lines(app)))
        .block(Block::default().borders(Borders::ALL).title("Route Intel"))
        .wrap(Wrap { trim: true });
    frame.render_widget(route, sections[0]);

    let detail_lines = station_info_lines(app)
        .into_iter()
        .chain(std::iter::once(Line::from("")))
        .chain(shipyard_lines(app))
        .collect::<Vec<_>>();
    let detail = Paragraph::new(Text::from(detail_lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Station & Shipyard"),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(detail, sections[1]);

    let footer_text = if matches!(app.mode, crate::game::AppMode::SelectingDestination { .. }) {
        "Up/Down: choose destination   Enter: confirm dispatch   z/x/g: map view   i/Esc: close"
    } else {
        "Up/Down: choose destination   m: move player   b: buy hull   z/x/g: map view   i/Esc: close"
    };
    let footer = Paragraph::new(Line::from(footer_text))
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, sections[2]);
}

fn draw_fleet_overlay(frame: &mut Frame, app: &App) {
    let area = centered_rect(84, 82, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default().borders(Borders::ALL).title("Fleet Detail");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(3)])
        .split(inner);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(sections[0]);

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
    .block(Block::default().borders(Borders::ALL).title("Fleet"));
    let mut fleet_state = ListState::default();
    fleet_state.select(Some(app.selected_ship));
    frame.render_stateful_widget(fleet, body[0], &mut fleet_state);

    let detail = Paragraph::new(Text::from(ship_detail_lines(app)))
        .block(Block::default().borders(Borders::ALL).title("Ship Detail"))
        .wrap(Wrap { trim: true });
    frame.render_widget(detail, body[1]);

    let footer = Paragraph::new(Line::from(
        "Up/Down: choose ship   Enter: dispatch   e: exploration   f/t/u: dockside ops   i/Esc: close",
    ))
    .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, sections[1]);
}

fn draw_shipyard_overlay(frame: &mut Frame, app: &App) {
    let area = centered_rect(82, 78, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Shipyard Detail");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(3)])
        .split(inner);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(sections[0]);

    let shipyard = List::new(shipyard_list_items(app))
        .highlight_style(selection_style())
        .highlight_symbol("$$ ")
        .block(Block::default().borders(Borders::ALL).title("Offers"));
    let mut shipyard_state = ListState::default();
    let shipyard_count = app.shipyard_offer_count(app.selected_location);
    shipyard_state.select(if shipyard_count > 0 {
        Some(app.selected_shipyard_offer.min(shipyard_count - 1))
    } else {
        None
    });
    frame.render_stateful_widget(shipyard, body[0], &mut shipyard_state);

    let detail = Paragraph::new(Text::from(shipyard_lines(app)))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Selected Offer"),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(detail, body[1]);

    let footer = Paragraph::new(Line::from(
        "Up/Down: choose hull   Enter or b: buy selected hull   i/Esc: close",
    ))
    .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, sections[1]);
}

fn draw_signals_overlay(frame: &mut Frame, app: &App) {
    let area = centered_rect(84, 82, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Signals Detail");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(3)])
        .split(inner);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
        .split(sections[0]);

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
    .block(Block::default().borders(Borders::ALL).title("Alerts"));
    let mut alerts_state = ListState::default();
    alerts_state.select(Some(
        app.selected_alert.min(alerts_data.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(alerts, body[0], &mut alerts_state);

    let log = List::new(app.log.iter().map(|entry| ListItem::new(entry.as_str())))
        .block(Block::default().borders(Borders::ALL).title("Event Log"));
    frame.render_widget(log, body[1]);

    let footer = Paragraph::new(Line::from(
        "Up/Down: choose alert   Enter: focus selected alert   i/Esc: close",
    ))
    .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, sections[1]);
}

fn render_sector_map(frame: &mut Frame, area: Rect, app: &App, active: bool) {
    let origin = app.preview_origin();
    let scope = app.map_scope_locations();

    let canvas = Canvas::default()
        .block(pane_block(PANE_TITLES[1], active))
        .background_color(Color::Black)
        .marker(Marker::Braille)
        .x_bounds([0.0, 100.0])
        .y_bounds([0.0, 60.0])
        .paint(|ctx| {
            for &index in &scope {
                if app.is_discovered(index) {
                    continue;
                }

                let cloud = hidden_cloud_points(app.map_coords(index));
                let (label_x, label_y) = location_label_coords(app, index);
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

            for (start, end) in app.visible_map_links() {
                if !app.is_discovered(start) || !app.is_discovered(end) {
                    continue;
                }

                let (x1, y1) = app.map_coords(start);
                let (x2, y2) = app.map_coords(end);
                ctx.draw(&CanvasLine::new(x1, y1, x2, y2, Color::DarkGray));
            }

            if let Some(segments) = app.highlighted_route_segments() {
                for (start, end) in segments {
                    let (x1, y1) = app.map_coords(start);
                    let (x2, y2) = app.map_coords(end);
                    ctx.draw(&CanvasLine::new(x1, y1, x2, y2, Color::Cyan));
                }
            }

            for &index in &scope {
                if !app.is_discovered(index) {
                    continue;
                }

                let label = if app.is_frontier_location(index) {
                    format!("{}*", map_label(app, index))
                } else {
                    map_label(app, index).to_string()
                };
                let (x, y) = app.map_coords(index);
                let (label_x, label_y) = location_label_coords(app, index);
                ctx.print(
                    x,
                    y,
                    Span::styled(
                        node_glyph(app, index, origin),
                        node_style(app, index, origin),
                    ),
                );
                ctx.print(
                    label_x,
                    label_y,
                    Span::styled(label, location_label_style(app, index, origin)),
                );
                if index == app.player_location && !app.player_is_in_transit() {
                    ctx.print(
                        x - 2.0,
                        y - 2.0,
                        Span::styled("P", player_marker_style(app, index)),
                    );
                }
                if app.has_shipyard(index) {
                    ctx.print(
                        x + 2.0,
                        y - 2.0,
                        Span::styled("$", shipyard_marker_style(app, index)),
                    );
                }
            }

            let mut dock_slots = vec![0usize; app.locations.len()];
            for (ship_index, ship) in app.fleet.iter().enumerate() {
                match &ship.state {
                    ShipState::Docked | ShipState::Repairing { .. } => {
                        if !app.is_discovered(ship.current_location)
                            || !app.location_visible_in_map(ship.current_location)
                        {
                            continue;
                        }

                        let (base_x, base_y) = app.map_coords(ship.current_location);
                        if show_individual_ship_tags(app) {
                            let slot = dock_slots[ship.current_location];
                            dock_slots[ship.current_location] += 1;
                            let (offset_x, offset_y) = docked_ship_offset(slot);
                            ctx.print(
                                base_x + offset_x,
                                base_y + offset_y,
                                Span::styled(ship.map_tag(), ship_style(app, ship_index)),
                            );
                        }
                    }
                    ShipState::EnRoute {
                        eta_remaining,
                        total_eta,
                        segments,
                        segment_costs,
                        ..
                    } => {
                        let Some(((start, end), position)) = route_position(
                            app,
                            segments,
                            segment_costs,
                            *eta_remaining,
                            *total_eta,
                        ) else {
                            continue;
                        };
                        if !app.location_visible_in_map(start) || !app.location_visible_in_map(end)
                        {
                            continue;
                        }

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
            "Chart the full environment ({}/{}) and reach {} credits",
            app.discovered_count(),
            app.locations.len(),
            GOAL_CREDITS,
        )),
    ])];
    lines.push(Line::from(format!(
        "Sector {} | seed {}",
        app.sector_name, app.world_seed
    )));
    lines.push(Line::from(app.sector_summary.clone()));

    if let Some((frontier, _reveal)) = app.next_discovery_target() {
        lines.push(Line::from(format!(
            "Next lead: send an exploration run to {}",
            app.location_name(frontier)
        )));
        let heading = app
            .exploration_heading_hint(frontier)
            .unwrap_or_else(|| "into the unknown".to_string());
        lines.push(Line::from(format!("Hidden contact heading: {}", heading)));
    } else {
        lines.push(Line::from(
            "Environment charted: all known contacts are visible.",
        ));
        lines.push(Line::from(
            "Focus on clearing the remaining contract board for credits.",
        ));
    }

    if let Some(contract_index) = app.tracked_contract {
        let contract = &app.contracts[contract_index];
        lines.push(Line::from(format!(
            "Active mission: {} [{}]",
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
        if let crate::game::ContractState::Assigned { ship_index, .. } = contract.state {
            lines.push(Line::from(format!(
                "Assigned carrier: {}",
                app.fleet[ship_index].name
            )));
        }
        lines.push(Line::from(app.contract_pressure_text(contract_index)));
    } else {
        let assigned: Vec<usize> = app
            .contracts
            .iter()
            .enumerate()
            .filter_map(|(index, contract)| {
                matches!(contract.state, crate::game::ContractState::Assigned { .. })
                    .then_some(index)
            })
            .collect();

        if let Some(&contract_index) = assigned.first() {
            let contract = &app.contracts[contract_index];
            lines.push(Line::from(format!(
                "Assigned: {} [{}]",
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
            if assigned.len() > 1 {
                lines.push(Line::from(format!(
                    "Assigned runs in transit: {}",
                    assigned.len()
                )));
            }
        } else {
            lines.push(Line::from(
                "Tracked contract: none. Accept one from the Mission Board.",
            ));
        }
    }

    lines.push(Line::from(format!(
        "Credits: {} / {} | Fleet: {} in transit | {} docked",
        app.credits,
        GOAL_CREDITS,
        app.in_transit_count(),
        app.fleet.len() - app.in_transit_count()
    )));
    if !app.mission_history.is_empty() {
        lines.push(Line::from("Recent missions:"));
        for entry in app.mission_history.iter().take(3) {
            lines.push(Line::from(entry.clone()));
        }
    }
    lines.push(Line::from(format!(
        "Difficulty: {}",
        app.difficulty.label()
    )));
    if app.pending_contract_flavor_count() > 0 {
        lines.push(Line::from(format!(
            "LLM: {} contract flavor job(s) running in background.",
            app.pending_contract_flavor_count()
        )));
    }
    let low_fuel = app.low_fuel_ship_names();
    if !low_fuel.is_empty() {
        lines.push(Line::from(format!("Low fuel: {}", low_fuel.join(", "))));
    }
    lines.push(Line::from(
        "Flow: Board -> Enter or Fleet -> Enter dispatch / e exploration -> charted node -> Enter.",
    ));
    lines.push(Line::from(
        "Transit: route planning -> undocking -> cruising -> approach -> arrived.",
    ));
    lines.push(Line::from(
        "Map hierarchy: Region -> Sector -> Cluster -> System with z/x zoom and g auto-focus.",
    ));

    match app.mode {
        crate::game::AppMode::Browse => {
            lines.push(Line::from(
                "Legend: cyan=selected yellow=origin magenta=frontier gray=unknown.",
            ));
        }
        crate::game::AppMode::SelectingDestination { ship_index, .. } => {
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

fn ship_color(app: &App, ship_index: usize) -> Color {
    if matches!(&app.fleet[ship_index].state, ShipState::Repairing { .. }) {
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

fn location_label_coords(app: &App, index: usize) -> (f64, f64) {
    let (x, y) = app.map_coords(index);
    let role = location_role(index);
    match app.map_zoom {
        crate::game::MapZoom::Region => (x - 6.0, y + 3.0),
        crate::game::MapZoom::Sector => match role {
            0 => (x - 2.0, y - 3.0),
            1 => (x + 2.0, y - 2.0),
            2 => (x - 5.0, y + 2.0),
            3 => (x - 3.0, y + 3.0),
            _ => (x + 2.0, y + 2.0),
        },
        crate::game::MapZoom::Cluster => match role {
            0 => (x - 2.0, y - 3.0),
            1 => (x + 2.0, y - 2.0),
            2 => (x - 5.0, y + 2.0),
            3 => (x - 3.0, y + 3.0),
            _ => (x + 2.0, y + 2.0),
        },
        crate::game::MapZoom::System => match role {
            0 => (x - 2.0, y - 3.0),
            1 => (x + 2.0, y - 2.0),
            2 => (x - 5.0, y + 2.0),
            3 => (x - 3.0, y + 3.0),
            _ => (x + 2.0, y + 2.0),
        },
    }
}

fn map_label(app: &App, index: usize) -> &str {
    match app.map_zoom {
        crate::game::MapZoom::Region => app.location_sector(index),
        crate::game::MapZoom::Sector => app.location_short_label(index),
        crate::game::MapZoom::Cluster => app.location_short_label(index),
        crate::game::MapZoom::System => app.location_name(index),
    }
}

fn node_glyph(app: &App, index: usize, origin: Option<usize>) -> &'static str {
    if Some(index) == origin {
        "@"
    } else if index == app.selected_location {
        "O"
    } else if app.is_frontier_location(index) {
        "*"
    } else if app.is_sector_hub(index) {
        "#"
    } else {
        "o"
    }
}

fn node_style(app: &App, index: usize, origin: Option<usize>) -> Style {
    let mut style = Style::default().fg(location_color(app, index, origin));
    if index == app.selected_location || Some(index) == origin || app.is_frontier_location(index) {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

fn hidden_cloud_points((x, y): (f64, f64)) -> Vec<(f64, f64)> {
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
        0 => (-8.0, -6.0),
        1 => (7.0, -6.0),
        2 => (-8.0, 6.0),
        _ => (7.0, 6.0),
    }
}

fn location_role(index: usize) -> usize {
    index % SECTOR_LOCATION_COUNT
}

fn show_individual_ship_tags(app: &App) -> bool {
    matches!(
        app.map_zoom,
        crate::game::MapZoom::Cluster | crate::game::MapZoom::System
    )
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

fn route_position(
    app: &App,
    segments: &[(usize, usize)],
    segment_costs: &[u16],
    eta_remaining: u16,
    total_eta: u16,
) -> Option<((usize, usize), (f64, f64))> {
    if segments.is_empty() {
        return None;
    }

    if segments.len() == 1 {
        let (start, end) = segments[0];
        return Some((
            (start, end),
            interpolate(
                app.map_coords(start),
                app.map_coords(end),
                transit_progress(eta_remaining, total_eta),
            ),
        ));
    }

    let total_cost: u16 = segment_costs.iter().copied().sum::<u16>().max(1);
    let completed_progress = transit_progress(eta_remaining, total_eta);
    let mut distance_along = completed_progress.clamp(0.05, 0.95) * f64::from(total_cost);

    for (segment_index, &(start, end)) in segments.iter().enumerate() {
        let segment_cost = f64::from(
            segment_costs
                .get(segment_index)
                .copied()
                .unwrap_or(1)
                .max(1),
        );
        if distance_along <= segment_cost || segment_index == segments.len() - 1 {
            let local_t = (distance_along / segment_cost).clamp(0.05, 0.95);
            return Some((
                (start, end),
                interpolate(app.map_coords(start), app.map_coords(end), local_t),
            ));
        }
        distance_along -= segment_cost;
    }

    let (start, end) = segments[segments.len() - 1];
    Some((
        (start, end),
        interpolate(app.map_coords(start), app.map_coords(end), 0.95),
    ))
}

fn route_preview_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(format!(
            "Map view: {} | {}",
            app.map_zoom_label(),
            app.map_scope_label()
        )),
        Line::from(format!("Focus: {}", app.map_focus_location_name())),
        Line::from(format!(
            "Selected destination: {}",
            app.location_name(app.selected_location)
        )),
        Line::from(app.location_description(app.selected_location).to_string()),
        Line::from("z: zoom in   x: zoom out   g: auto-focus on active ship"),
        Line::from(""),
    ];

    let selected_ship = &app.fleet[app.selected_ship];
    lines.push(Line::from(format!(
        "Selected ship: {} ({})",
        selected_ship.name, selected_ship.class_name
    )));
    lines.push(Line::from(format!(
        "Player status: {}",
        app.player_status_text()
    )));
    if let Some(intent) = app.pending_dispatch_intent() {
        lines.push(Line::from(format!("Pending intent: {}", intent.label())));
    }
    if selected_ship.is_docked() {
        if let Some(plan) = app.plan_route_for_ship(app.selected_ship, app.selected_location) {
            lines.push(Line::from(format!(
                "Selected ship preview to {}: ETA {} | fuel {} | path {}",
                app.location_name(app.selected_location),
                plan.eta,
                plan.fuel_required,
                plan.path
            )));
            if app.player_can_operate_ship(app.selected_ship) {
                lines.push(Line::from("Local control: YES"));
            } else {
                lines.push(Line::from(format!(
                    "Local control: NO - ship is at {}",
                    app.location_name(selected_ship.current_location)
                )));
            }
        } else {
            lines.push(Line::from(
                "Selected ship preview: unavailable for the current destination.",
            ));
        }
    } else {
        lines.push(Line::from(
            "Selected ship preview: unavailable while this ship is in transit.",
        ));
    }
    if let Some(_target) = app.locations[app.selected_location].reveal_on_arrival
        && !app.is_discovered(_target)
    {
        let heading = app
            .exploration_heading_hint(app.selected_location)
            .unwrap_or_else(|| "into the unknown".to_string());
        lines.push(Line::from(format!(
            "Exploration lead: hidden contact {}.",
            heading
        )));
    }
    lines.push(Line::from(""));

    if let Some(ship_index) = app.pending_ship() {
        let ship = &app.fleet[ship_index];
        lines.push(Line::from(vec![
            Span::raw("Planning route for: "),
            Span::styled(
                ship.name.clone(),
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
            "Dockside traffic: {} ship(s) at {}",
            docked_ship_count_at(app, ship.current_location),
            app.location_name(ship.current_location)
        )));
        lines.push(Line::from(format!(
            "Destination: {}",
            app.location_name(app.selected_location)
        )));
        if let Some(intent) = app.pending_dispatch_intent() {
            lines.push(Line::from(format!("Intent: {}", intent.label())));
        }

        if let Some(plan) = app.plan_route_for_ship(ship_index, app.selected_location) {
            lines.push(Line::from(format!("Path: {}", plan.path)));
            lines.push(Line::from(format!("ETA: {} ticks", plan.eta)));
            lines.push(Line::from(format!("Fuel needed: {}", plan.fuel_required)));
            lines.push(Line::from(format!(
                "Conditions: {}",
                plan.condition_summary
            )));
            if let Some(note) =
                app.active_mission_assignment_note(ship_index, app.selected_location, plan.eta)
            {
                lines.push(Line::from(note));
            }
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
                let heading = app
                    .exploration_heading_hint(app.selected_location)
                    .unwrap_or_else(|| "into the unknown".to_string());
                lines.push(Line::from(format!(
                    "Exploration lead: hidden contact {}.",
                    heading
                )));
            }
            if matches!(
                app.pending_dispatch_intent(),
                Some(crate::game::DispatchIntent::Exploration)
            ) && !app.is_frontier_location(app.selected_location)
            {
                lines.push(Line::from(
                    "Exploration requires a frontier node with an uncharted contact.",
                ));
            }
            if app.sector_index_for_location(ship.current_location)
                != app.sector_index_for_location(app.selected_location)
            {
                lines.push(Line::from(
                    "Inter-sector route: jump corridor traversal required.",
                ));
            }
        } else {
            lines.push(Line::from(
                "Choose a destination other than the current dock to dispatch.",
            ));
        }

        lines.push(Line::from(
            "Enter: confirm dispatch/exploration   Esc: cancel   z/x/g: navigate map view",
        ));
        return lines;
    }

    let ship = &app.fleet[app.selected_ship];
    lines.push(Line::from(vec![
        Span::raw("Selected ship: "),
        Span::styled(
            ship.name.clone(),
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
            lines.push(Line::from(format!(
                "Dockside traffic: {} ship(s) at {}",
                docked_ship_count_at(app, ship.current_location),
                app.location_name(ship.current_location)
            )));

            if let Some(plan) = app.plan_route_for_ship(app.selected_ship, app.selected_location) {
                lines.push(Line::from(format!("Preview: {}", plan.path)));
                lines.push(Line::from(format!("ETA: {} ticks", plan.eta)));
                lines.push(Line::from(format!("Fuel needed: {}", plan.fuel_required)));
                lines.push(Line::from(format!(
                    "Conditions: {}",
                    plan.condition_summary
                )));
                if let Some(note) = app.active_mission_assignment_note(
                    app.selected_ship,
                    app.selected_location,
                    plan.eta,
                ) {
                    lines.push(Line::from(note));
                }
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
                    let heading = app
                        .exploration_heading_hint(app.selected_location)
                        .unwrap_or_else(|| "into the unknown".to_string());
                    lines.push(Line::from(format!(
                        "Exploration lead: hidden contact {}.",
                        heading
                    )));
                }
                if app.sector_index_for_location(ship.current_location)
                    != app.sector_index_for_location(app.selected_location)
                {
                    lines.push(Line::from(
                        "Inter-sector route preview: this dispatch crosses a jump corridor.",
                    ));
                }
            } else {
                lines.push(Line::from(
                    "Select a different charted destination to preview a route.",
                ));
            }

            lines.push(Line::from(""));
            lines.extend(shipyard_lines(app));

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

fn station_info_lines(app: &App) -> Vec<Line<'static>> {
    let index = app.selected_location;
    let mut lines = vec![
        Line::from(format!("{}", app.location_name(index))),
        Line::from(format!(
            "Fuel reserves: {} | {} | {}",
            app.station_fuel[index],
            app.location_sector(index),
            app.location_region(index)
        )),
        Line::from(if app.player_is_in_transit() {
            format!("Player status: {}", app.player_status_text())
        } else if index == app.player_location {
            "You are here.".to_string()
        } else {
            match app.player_transfer_cost(index) {
                Some(cost) => format!("Passenger transfer with `m`: {} cr", cost),
                None => "Passenger transfer unavailable".to_string(),
            }
        }),
    ];

    if app.is_discovered(index) {
        lines.push(Line::from(app.location_description(index).to_string()));
    } else {
        lines.push(Line::from(
            "Uncharted location. Details unlock after discovery.",
        ));
    }

    if app.has_shipyard(index) {
        lines.push(Line::from(format!(
            "Shipyard offers: {}",
            app.shipyard_offer_count(index)
        )));
        if let Some(offer) = app.shipyard_offer(index) {
            lines.push(Line::from(format!(
                "Selected hull: {} ({}) for {} cr",
                offer.name, offer.class_name, offer.price
            )));
            if app.credits >= offer.price {
                lines.push(Line::from(
                    "Purchase available now: press `b` or Enter in Shipyards.",
                ));
            } else {
                lines.push(Line::from(format!(
                    "Need {} more cr to buy this hull.",
                    offer.price - app.credits
                )));
            }
        } else {
            lines.push(Line::from("Shipyard: sold out until next rotation"));
        }
    } else {
        lines.push(Line::from("Shipyard: none active"));
    }

    if let Some(_target) = app.locations[index].reveal_on_arrival
        && !app.is_discovered(_target)
    {
        let heading = app
            .exploration_heading_hint(index)
            .unwrap_or_else(|| "into the unknown".to_string());
        lines.push(Line::from(format!(
            "Exploration lead: hidden contact {}.",
            heading
        )));
    }

    lines.push(Line::from(format!(
        "Docked ships here: {}",
        docked_ship_count_at(app, index)
    )));

    lines
}

fn docked_ship_count_at(app: &App, location_index: usize) -> usize {
    app.fleet
        .iter()
        .filter(|ship| {
            ship.current_location == location_index
                && matches!(&ship.state, ShipState::Docked | ShipState::Repairing { .. })
        })
        .count()
}

fn shipyard_list_items(app: &App) -> Vec<ListItem<'static>> {
    let location_index = app.selected_location;
    let offers = app.shipyard_offers(location_index);
    if !app.has_shipyard(location_index) {
        return vec![ListItem::new("No shipyard at the selected station.")];
    }
    if offers.is_empty() {
        return vec![ListItem::new("Shipyard sold out until next rotation.")];
    }

    offers.iter().map(shipyard_offer_item).collect()
}

fn shipyard_offer_item(offer: &ShipShopOffer) -> ListItem<'static> {
    ListItem::new(vec![
        Line::from(format!("{} [{}]", offer.name, offer.class_name)),
        Line::from(format!(
            "{} cr | Speed {} | Fuel {}",
            offer.price, offer.speed, offer.max_fuel
        )),
    ])
}

fn shipyard_marker_style(app: &App, index: usize) -> Style {
    let color = if app.shipyard_offer_count(index) > 0 {
        Color::Green
    } else {
        Color::DarkGray
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn player_marker_style(app: &App, index: usize) -> Style {
    let color = if index == app.selected_location {
        Color::Yellow
    } else {
        Color::LightGreen
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
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
            RefuelPlan::EmergencyReserve {
                purchased_units,
                reserve_units,
                cost,
            } => {
                if purchased_units > 0 {
                    lines.push(Line::from(format!(
                        "Refuel with `f`: +{} paid fuel for {} cr, emergency reserve +{}",
                        purchased_units, cost, reserve_units
                    )));
                } else {
                    lines.push(Line::from(format!(
                        "Refuel with `f`: emergency reserve +{} fuel in Cozy",
                        reserve_units
                    )));
                }
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

fn shipyard_lines(app: &App) -> Vec<Line<'static>> {
    let location_index = app.selected_location;
    let mut lines = vec![Line::from(format!(
        "Selected station: {}",
        app.location_name(location_index)
    ))];

    if !app.is_discovered(location_index) {
        lines.push(Line::from(
            "Shipyard intel unavailable until this station is charted.",
        ));
        return lines;
    }

    lines.push(Line::from(format!(
        "Cluster: {} | System: {}",
        app.location_cluster(location_index),
        app.location_system(location_index)
    )));
    lines.push(Line::from(
        app.location_description(location_index).to_string(),
    ));

    if !app.has_shipyard(location_index) {
        lines.push(Line::from("Shipyard: none active at this station."));
        return lines;
    }

    lines.push(Line::from(format!(
        "Available hulls: {}",
        app.shipyard_offer_count(location_index)
    )));

    match app.shipyard_offer(location_index) {
        Some(offer) => {
            lines.push(Line::from(format!(
                "Selected hull: {} ({}) for {} cr",
                offer.name, offer.class_name, offer.price
            )));
            lines.push(Line::from(format!(
                "Offer stats: speed {} | fuel {}",
                offer.speed, offer.max_fuel
            )));
            lines.push(Line::from(offer.description.clone()));
            lines.push(Line::from(
                "Press `b` in the map pane or Enter in Shipyards to acquire this hull.",
            ));
        }
        None => {
            lines.push(Line::from(
                "Shipyard: featured hull sold out; wait for the next yard rotation.",
            ));
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
        crate::game::ContractState::Assigned { ship_index, .. } => {
            format!("Assigned to {}", app.fleet[ship_index].name)
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
    ];

    if contract.pending_llm_flavor {
        lines.push(Line::from(
            "Flavor: background generation pending; deterministic fallback is shown until it finishes.",
        ));
    }

    lines.push(Line::from(contract.briefing.clone()));

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
        Line::from(ship.name.clone()),
        Line::from(format!("Class: {}", ship.class_name)),
        Line::from(format!("Station: {}", station)),
        Line::from(if ship.current_location == app.player_location {
            "Local control: available here".to_string()
        } else {
            format!(
                "Remote ship: transfer to {} to issue new orders",
                app.location_name(ship.current_location)
            )
        }),
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
        Line::from(ship.description.clone()),
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
    let mut title = format!("{} [{}]", ship.name, ship.class_name);
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
            matches!(&ship.state, ShipState::Docked) && ship.current_location == contract.origin
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
