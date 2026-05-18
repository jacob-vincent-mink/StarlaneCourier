use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph, Wrap,
        canvas::{Canvas, Line as CanvasLine, Points},
    },
};
use serde::{Deserialize, Serialize};

const PANE_TITLES: [&str; 4] = ["Mission Board", "Sector Map", "Fleet", "Event Log"];
const CONTRACTS_PANE: usize = 0;
const MAP_PANE: usize = 1;
const FLEET_PANE: usize = 2;
const LOG_PANE: usize = 3;

const ASTRA_PRIME: usize = 0;
const KITE_STATION: usize = 1;
const ION_ANCHORAGE: usize = 2;
const DUST_HARBOR: usize = 3;
const OUTER_RING_RELAY: usize = 4;

const GOAL_CREDITS: i32 = 600;
const TICK_SPEEDS: [(&str, u64); 3] = [("Slow", 450), ("Standard", 250), ("Fast", 125)];
const DIFFICULTY_OPTIONS: [Difficulty; 3] =
    [Difficulty::Cozy, Difficulty::Normal, Difficulty::Insane];
const SAVE_DIR: &str = "saves";
const SAVE_SLOT_COUNT: usize = 3;
const SAVE_VERSION: u8 = 1;

fn main() -> io::Result<()> {
    let mut terminal = setup_terminal()?;
    let _guard = TerminalGuard;
    let mut app = App::new();

    run_app(&mut terminal, &mut app)
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;

    Terminal::new(CrosstermBackend::new(stdout))
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;

        if event::poll(app.poll_duration())? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && app.handle_key(key) {
                    let _ = app.save_game();
                    return Ok(());
                }
            }
        } else {
            app.tick();
        }
    }
}

fn draw(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::StartMenu => draw_start_menu(frame, app),
        Screen::LoadGame => draw_load_game(frame, app),
        Screen::Settings => draw_settings(frame, app),
        Screen::HowToPlay => draw_how_to_play(frame, app),
        Screen::InGame => draw_game(frame, app),
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

    let map_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(7)])
        .split(top[1]);

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
    frame.render_stateful_widget(contracts, top[0], &mut contract_state);

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
    frame.render_stateful_widget(fleet, top[2], &mut fleet_state);

    let log = List::new(app.log.iter().map(|entry| ListItem::new(entry.as_str())))
        .block(pane_block(PANE_TITLES[3], app.active_pane == LOG_PANE));
    frame.render_widget(log, bottom[0]);

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
            Constraint::Percentage(32),
            Constraint::Percentage(32),
            Constraint::Percentage(36),
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
    ]))
    .block(Block::default().borders(Borders::ALL).title("Effect"))
    .wrap(Wrap { trim: true });
    frame.render_widget(detail, body[2]);

    let footer = Paragraph::new(Line::from(
        "Left/Right: focus   Up/Down: choose   Enter: apply   Esc: back   q/Ctrl+C: quit",
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
        Line::from("2. Move to Fleet and press Enter on a docked ship."),
        Line::from("3. Focus the map, pick the contract destination, and confirm the route."),
        Line::from("4. Watch the ship move through undocking, cruising, approach, and arrival."),
        Line::from("5. Frontier arrivals reveal new charts and unlock deeper contracts."),
        Line::from(""),
        Line::from("Difficulties:"),
        Line::from("Cozy: fixed rewards, no timeout pressure."),
        Line::from("Normal: rewards decay after acceptance, but contracts do not fail."),
        Line::from(
            "Insane: rewards decay faster and accepted contracts can fail their delivery window.",
        ),
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
            }

            let mut dock_slots = vec![0usize; app.locations.len()];
            for (ship_index, ship) in app.fleet.iter().enumerate() {
                match &ship.state {
                    ShipState::Docked => {
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
            "{} -> {} | {} cr",
            app.location_name(contract.origin),
            app.location_name(contract.destination),
            app.contract_current_reward(index),
        )),
        Line::from(app.contract_pressure_text(index)),
        Line::from(app.contract_hint(index)),
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
            "Run: {} -> {} | reward {} cr",
            app.location_name(contract.origin),
            app.location_name(contract.destination),
            app.contract_current_reward(contract_index),
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
    lines.push(Line::from(
        "Flow: Board -> Enter -> Fleet -> Enter -> charted node -> Enter.",
    ));
    lines.push(Line::from(
        "Transit: route planning -> undocking -> cruising -> approach -> arrived.",
    ));

    match app.mode {
        AppMode::Browse => {
            lines.push(Line::from(
                "Legend: cyan=selected yellow=origin magenta=frontier gray=unknown.",
            ));
        }
        AppMode::SelectingDestination { ship_index } => {
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
    if app.pending_ship() == Some(ship_index) {
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
            "Destination: {}",
            app.location_name(app.selected_location)
        )));

        if let Some(plan) = app.plan_route(ship.current_location, app.selected_location) {
            lines.push(Line::from(format!("Path: {}", plan.path)));
            lines.push(Line::from(format!("ETA: {} ticks", plan.eta)));
            lines.push(Line::from(format!(
                "Conditions: {}",
                plan.condition_summary
            )));

            if let Some(contract_index) =
                app.matching_tracked_contract(ship.current_location, app.selected_location)
            {
                let contract = &app.contracts[contract_index];
                lines.push(Line::from(format!(
                    "Contract match: {} | reward {} cr",
                    contract.title,
                    app.contract_current_reward(contract_index)
                )));
                lines.push(Line::from(app.contract_pressure_text(contract_index)));
            } else if let Some(contract_index) = app.tracked_contract {
                lines.push(Line::from(format!(
                    "Tracked contract still waiting: {}",
                    app.contracts[contract_index].title
                )));
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

            if let Some(plan) = app.plan_route(ship.current_location, app.selected_location) {
                lines.push(Line::from(format!("Preview: {}", plan.path)));
                lines.push(Line::from(format!("ETA: {} ticks", plan.eta)));
                lines.push(Line::from(format!(
                    "Conditions: {}",
                    plan.condition_summary
                )));

                if let Some(contract_index) =
                    app.matching_tracked_contract(ship.current_location, app.selected_location)
                {
                    let contract = &app.contracts[contract_index];
                    lines.push(Line::from(format!(
                        "Contract match: {} | reward {} cr",
                        contract.title,
                        app.contract_current_reward(contract_index)
                    )));
                    lines.push(Line::from(app.contract_pressure_text(contract_index)));
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

fn ship_list_item(
    ship: &Ship,
    locations: &[Location],
    contracts: &[Contract],
    pending: bool,
) -> ListItem<'static> {
    let mut title = ship.name.to_string();
    if pending {
        title.push_str(" [planning]");
    }

    let (status, detail) = match &ship.state {
        ShipState::Docked => (
            format!("Docked at {}", locations[ship.current_location].name),
            "Ready for assignment".to_string(),
        ),
        ShipState::EnRoute {
            origin,
            destination,
            eta_remaining,
            total_eta,
            assigned_contract,
            condition_summary,
            ..
        } => {
            let phase = transit_phase(*eta_remaining, *total_eta);
            let detail = if let Some(contract_index) = assigned_contract {
                format!(
                    "{} | carrying {}",
                    condition_summary, contracts[*contract_index].title
                )
            } else {
                condition_summary.clone()
            };
            (
                phase.status_line(
                    locations[*origin].name,
                    locations[*destination].name,
                    *eta_remaining,
                ),
                detail,
            )
        }
    };

    ListItem::new(vec![
        Line::from(title),
        Line::from(status),
        Line::from(detail),
    ])
}

struct App {
    screen: Screen,
    has_active_game: bool,
    menu_feedback: Option<String>,
    start_menu_selection: usize,
    active_save_slot: usize,
    load_slot_selection: usize,
    settings_selection: usize,
    settings_focus: usize,
    tick_speed_index: usize,
    difficulty: Difficulty,
    difficulty_selection: usize,
    active_pane: usize,
    clock: u64,
    mode: AppMode,
    selected_location: usize,
    selected_ship: usize,
    selected_contract: usize,
    tracked_contract: Option<usize>,
    credits: i32,
    locations: Vec<Location>,
    discovered_locations: Vec<bool>,
    fleet: Vec<Ship>,
    contracts: Vec<Contract>,
    log: Vec<String>,
}

impl App {
    fn new() -> Self {
        let mut app = Self {
            screen: Screen::StartMenu,
            has_active_game: false,
            menu_feedback: None,
            start_menu_selection: 0,
            active_save_slot: 0,
            load_slot_selection: 0,
            settings_selection: 1,
            settings_focus: 0,
            tick_speed_index: 1,
            difficulty: Difficulty::Normal,
            difficulty_selection: 1,
            active_pane: CONTRACTS_PANE,
            clock: 0,
            mode: AppMode::Browse,
            selected_location: DUST_HARBOR,
            selected_ship: 0,
            selected_contract: 0,
            tracked_contract: None,
            credits: 0,
            locations: Vec::new(),
            discovered_locations: Vec::new(),
            fleet: Vec::new(),
            contracts: Vec::new(),
            log: Vec::new(),
        };
        app.reset_game();
        app.has_active_game = false;
        app
    }

    fn reset_game(&mut self) {
        self.active_pane = CONTRACTS_PANE;
        self.clock = 0;
        self.mode = AppMode::Browse;
        self.selected_location = DUST_HARBOR;
        self.selected_ship = 0;
        self.selected_contract = 0;
        self.tracked_contract = None;
        self.credits = 120;
        self.settings_selection = self.tick_speed_index;
        self.difficulty_selection = self.difficulty.index();
        self.locations = vec![
            Location::hub("Astra Prime"),
            Location::new("Kite Station", "Kite Spur", 4, Some(ION_ANCHORAGE)),
            Location::new("Ion Anchorage", "Ion Run", 5, Some(OUTER_RING_RELAY)),
            Location::new("Dust Harbor", "Dust Corridor", 6, Some(KITE_STATION)),
            Location::new("Outer Ring Relay", "Relay Ascent", 7, None),
        ];
        self.discovered_locations = vec![true, false, false, true, false];
        self.fleet = vec![
            Ship::docked("SV Kestrel", ASTRA_PRIME),
            Ship::en_route(
                "CSV Lantern",
                ASTRA_PRIME,
                DUST_HARBOR,
                7,
                7,
                "Astra Prime -> Dust Harbor",
                "Dust Corridor: debris interference",
                None,
            ),
            Ship::docked("HMV Orpheus", ASTRA_PRIME),
        ];
        self.contracts = vec![
            Contract::new(
                "Frontier Survey Drop",
                "Carry survey drones to Dust Harbor and expand the frontier.",
                ASTRA_PRIME,
                DUST_HARBOR,
                160,
                28,
                DUST_HARBOR,
            ),
            Contract::new(
                "Harbor Relief Return",
                "Bring relief crates back from Dust Harbor before local stores spoil.",
                DUST_HARBOR,
                ASTRA_PRIME,
                140,
                34,
                DUST_HARBOR,
            ),
            Contract::new(
                "Medlift to Dust",
                "Rush medical pallets to Dust Harbor on the same shift.",
                ASTRA_PRIME,
                DUST_HARBOR,
                190,
                24,
                DUST_HARBOR,
            ),
            Contract::new(
                "Kite Courier Run",
                "Open commercial traffic with Kite Station once the chart is confirmed.",
                ASTRA_PRIME,
                KITE_STATION,
                230,
                46,
                KITE_STATION,
            ),
            Contract::new(
                "Ion Drydock Refit",
                "Deliver replacement coils to Ion Anchorage for a high-value refit.",
                ASTRA_PRIME,
                ION_ANCHORAGE,
                300,
                60,
                ION_ANCHORAGE,
            ),
            Contract::new(
                "Relay Calibration Window",
                "Reach Outer Ring Relay and stabilize the signal array before the window closes.",
                ION_ANCHORAGE,
                OUTER_RING_RELAY,
                420,
                82,
                OUTER_RING_RELAY,
            ),
        ];
        self.log = vec![
            "[0000] Shift started. Dispatch board synced.".into(),
            "[0000] Primary objective: chart the sector and build credits through contracts."
                .into(),
            "[0000] Accept a contract from the Mission Board, then assign a ship to the route."
                .into(),
            "[0000] Frontier arrivals unlock new charts deeper in the map.".into(),
        ];
    }

    fn poll_duration(&self) -> Duration {
        Duration::from_millis(TICK_SPEEDS[self.tick_speed_index].1)
    }

    fn save_slot_path(slot_index: usize) -> PathBuf {
        Path::new(SAVE_DIR).join(format!("slot-{}.json", slot_index + 1))
    }

    fn save_slot_label(slot_index: usize) -> String {
        format!("Slot {}", slot_index + 1)
    }

    fn selected_slot_summary_text(&self) -> String {
        self.save_slot_summary_text(self.load_slot_selection)
    }

    fn slot_brief(&self, slot_index: usize) -> &'static str {
        match Self::read_save_summary(slot_index) {
            Ok(Some(_)) => "saved",
            Ok(None) => "empty",
            Err(_) => "error",
        }
    }

    fn save_slot_summary_text(&self, slot_index: usize) -> String {
        match Self::read_save_summary(slot_index) {
            Ok(Some(summary)) => summary,
            Ok(None) => format!("{} is empty.", Self::save_slot_label(slot_index)),
            Err(error) => format!("Save file is unreadable: {error}"),
        }
    }

    fn read_save_summary(slot_index: usize) -> Result<Option<String>, String> {
        let Some(save) = Self::read_save_file(slot_index)? else {
            return Ok(None);
        };

        let charted = save
            .discovered_locations
            .iter()
            .filter(|&&seen| seen)
            .count();
        let tracked = save
            .contracts
            .iter()
            .filter(|contract| {
                matches!(
                    contract.state,
                    SavedContractState::Accepted { .. } | SavedContractState::Assigned { .. }
                )
            })
            .count();

        Ok(Some(format!(
            "{}: T+{:04} | {} | {} credits | {}/{} charts | {} active contract(s)",
            Self::save_slot_label(slot_index),
            save.clock,
            save.difficulty.label(),
            save.credits,
            charted,
            save.discovered_locations.len(),
            tracked,
        )))
    }

    fn read_save_file(slot_index: usize) -> Result<Option<SaveGame>, String> {
        Self::read_save_file_from_path(&Self::save_slot_path(slot_index))
    }

    fn read_save_file_from_path(path: &Path) -> Result<Option<SaveGame>, String> {
        if !path.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
        let save: SaveGame = serde_json::from_str(&raw).map_err(|error| error.to_string())?;

        if save.version != SAVE_VERSION {
            return Err(format!(
                "unsupported save version {} (expected {})",
                save.version, SAVE_VERSION
            ));
        }

        Ok(Some(save))
    }

    fn save_game(&self) -> io::Result<()> {
        self.save_game_to_path(&Self::save_slot_path(self.active_save_slot))
    }

    fn save_game_to_path(&self, path: &Path) -> io::Result<()> {
        if !self.has_active_game {
            return Ok(());
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let save = self.snapshot();
        let json = serde_json::to_string_pretty(&save).map_err(io::Error::other)?;
        fs::write(path, json)
    }

    fn snapshot(&self) -> SaveGame {
        SaveGame {
            version: SAVE_VERSION,
            tick_speed_index: self.tick_speed_index,
            active_pane: self.active_pane,
            clock: self.clock,
            mode: match self.mode {
                AppMode::Browse => SavedAppMode::Browse,
                AppMode::SelectingDestination { ship_index } => {
                    SavedAppMode::SelectingDestination { ship_index }
                }
            },
            selected_location: self.selected_location,
            selected_ship: self.selected_ship,
            selected_contract: self.selected_contract,
            tracked_contract: self.tracked_contract,
            credits: self.credits,
            difficulty: self.difficulty,
            discovered_locations: self.discovered_locations.clone(),
            fleet: self
                .fleet
                .iter()
                .map(|ship| SavedShip {
                    current_location: ship.current_location,
                    state: match &ship.state {
                        ShipState::Docked => SavedShipState::Docked,
                        ShipState::EnRoute {
                            origin,
                            destination,
                            eta_remaining,
                            total_eta,
                            route,
                            condition_summary,
                            assigned_contract,
                        } => SavedShipState::EnRoute {
                            origin: *origin,
                            destination: *destination,
                            eta_remaining: *eta_remaining,
                            total_eta: *total_eta,
                            route: route.clone(),
                            condition_summary: condition_summary.clone(),
                            assigned_contract: *assigned_contract,
                        },
                    },
                })
                .collect(),
            contracts: self
                .contracts
                .iter()
                .map(|contract| SavedContract {
                    deadline: contract.deadline,
                    state: match contract.state {
                        ContractState::Available => SavedContractState::Available,
                        ContractState::Accepted { accepted_at } => {
                            SavedContractState::Accepted { accepted_at }
                        }
                        ContractState::Assigned {
                            ship_name,
                            accepted_at,
                        } => SavedContractState::Assigned {
                            ship_index: self
                                .fleet
                                .iter()
                                .position(|ship| ship.name == ship_name)
                                .unwrap_or(0),
                            accepted_at,
                        },
                        ContractState::Completed => SavedContractState::Completed,
                        ContractState::Failed => SavedContractState::Failed,
                    },
                })
                .collect(),
            log: self.log.clone(),
        }
    }

    fn load_game(&mut self, slot_index: usize) -> Result<(), String> {
        let Some(save) = Self::read_save_file(slot_index)? else {
            return Err(format!("{} is empty", Self::save_slot_label(slot_index)));
        };

        self.apply_save(save)?;
        self.active_save_slot = slot_index;
        self.load_slot_selection = slot_index;
        self.has_active_game = true;
        self.screen = Screen::InGame;
        self.menu_feedback = None;
        Ok(())
    }

    fn apply_save(&mut self, save: SaveGame) -> Result<(), String> {
        self.reset_game();

        if save.discovered_locations.len() != self.discovered_locations.len() {
            return Err("save file has incompatible discovery data".to_string());
        }
        if save.fleet.len() != self.fleet.len() {
            return Err("save file has incompatible fleet data".to_string());
        }
        if save.contracts.len() != self.contracts.len() {
            return Err("save file has incompatible contract data".to_string());
        }

        self.tick_speed_index = save.tick_speed_index.min(TICK_SPEEDS.len() - 1);
        self.active_pane = save.active_pane.min(PANE_TITLES.len() - 1);
        self.clock = save.clock;
        self.selected_location = save.selected_location.min(self.locations.len() - 1);
        self.selected_ship = save.selected_ship.min(self.fleet.len() - 1);
        self.selected_contract = save.selected_contract.min(self.contracts.len() - 1);
        self.tracked_contract = save
            .tracked_contract
            .filter(|&index| index < self.contracts.len());
        self.credits = save.credits;
        self.difficulty = save.difficulty;
        self.difficulty_selection = self.difficulty.index();
        self.discovered_locations = save.discovered_locations;
        self.discovered_locations[ASTRA_PRIME] = true;

        for (ship, saved_ship) in self.fleet.iter_mut().zip(save.fleet) {
            ship.current_location = saved_ship.current_location.min(self.locations.len() - 1);
            ship.state = match saved_ship.state {
                SavedShipState::Docked => ShipState::Docked,
                SavedShipState::EnRoute {
                    origin,
                    destination,
                    eta_remaining,
                    total_eta,
                    route,
                    condition_summary,
                    assigned_contract,
                } => ShipState::EnRoute {
                    origin: origin.min(self.locations.len() - 1),
                    destination: destination.min(self.locations.len() - 1),
                    eta_remaining,
                    total_eta: total_eta.max(eta_remaining),
                    route,
                    condition_summary,
                    assigned_contract: assigned_contract
                        .filter(|&index| index < self.contracts.len()),
                },
            };
        }

        for (contract, saved_contract) in self.contracts.iter_mut().zip(save.contracts) {
            contract.deadline = saved_contract.deadline;
            contract.state = match saved_contract.state {
                SavedContractState::Available => ContractState::Available,
                SavedContractState::Accepted { accepted_at } => {
                    ContractState::Accepted { accepted_at }
                }
                SavedContractState::Assigned {
                    ship_index,
                    accepted_at,
                } => ContractState::Assigned {
                    ship_name: self
                        .fleet
                        .get(ship_index)
                        .map(|ship| ship.name)
                        .unwrap_or(self.fleet[0].name),
                    accepted_at,
                },
                SavedContractState::Completed => ContractState::Completed,
                SavedContractState::Failed => ContractState::Failed,
            };

            let unlocked = self.discovered_locations[contract.origin]
                && self.discovered_locations[contract.destination]
                && self.discovered_locations[contract.unlock_location];

            if !unlocked && !matches!(contract.state, ContractState::Completed) {
                contract.state = ContractState::Available;
            }
        }

        self.mode = match save.mode {
            SavedAppMode::Browse => AppMode::Browse,
            SavedAppMode::SelectingDestination { ship_index } => AppMode::SelectingDestination {
                ship_index: ship_index.min(self.fleet.len() - 1),
            },
        };
        self.log = if save.log.is_empty() {
            vec!["[0000] Save loaded.".to_string()]
        } else {
            save.log.into_iter().take(8).collect()
        };

        Ok(())
    }

    fn start_menu_options(&self) -> Vec<StartMenuAction> {
        let mut options = Vec::new();

        if self.has_active_game {
            options.push(StartMenuAction::ResumeShift);
        }

        options.extend([
            StartMenuAction::NewGame,
            StartMenuAction::LoadGame,
            StartMenuAction::Settings,
            StartMenuAction::HowToPlay,
            StartMenuAction::Quit,
        ]);

        options
    }

    fn activate_start_menu_selection(&mut self) -> bool {
        let action = self.start_menu_options()[self.start_menu_selection];

        match action {
            StartMenuAction::ResumeShift => {
                self.menu_feedback = None;
                self.screen = Screen::InGame;
                false
            }
            StartMenuAction::NewGame => {
                self.active_save_slot = self.load_slot_selection;
                self.reset_game();
                self.has_active_game = true;
                self.menu_feedback = self
                    .save_game()
                    .err()
                    .map(|error| format!("Save failed: {error}"));
                self.screen = Screen::InGame;
                false
            }
            StartMenuAction::LoadGame => {
                self.menu_feedback = None;
                self.screen = Screen::LoadGame;
                false
            }
            StartMenuAction::Settings => {
                self.menu_feedback = None;
                self.settings_selection = self.tick_speed_index;
                self.difficulty_selection = self.difficulty.index();
                self.settings_focus = 0;
                self.screen = Screen::Settings;
                false
            }
            StartMenuAction::HowToPlay => {
                self.menu_feedback = None;
                self.screen = Screen::HowToPlay;
                false
            }
            StartMenuAction::Quit => true,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match self.screen {
            Screen::StartMenu => self.handle_start_menu_key(key),
            Screen::LoadGame => self.handle_load_game_key(key),
            Screen::Settings => self.handle_settings_key(key),
            Screen::HowToPlay => self.handle_how_to_key(key),
            Screen::InGame => self.handle_game_key(key),
        }
    }

    fn handle_start_menu_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
            (KeyCode::Up, _) => {
                let len = self.start_menu_options().len();
                self.start_menu_selection = wrap_index(self.start_menu_selection, len, -1);
                false
            }
            (KeyCode::Down, _) => {
                let len = self.start_menu_options().len();
                self.start_menu_selection = wrap_index(self.start_menu_selection, len, 1);
                false
            }
            (KeyCode::Left, _) => {
                self.load_slot_selection =
                    wrap_index(self.load_slot_selection, SAVE_SLOT_COUNT, -1);
                self.menu_feedback = None;
                false
            }
            (KeyCode::Right, _) => {
                self.load_slot_selection = wrap_index(self.load_slot_selection, SAVE_SLOT_COUNT, 1);
                self.menu_feedback = None;
                false
            }
            (KeyCode::Enter, _) => self.activate_start_menu_selection(),
            _ => false,
        }
    }

    fn handle_load_game_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
            (KeyCode::Up, _) => {
                self.load_slot_selection =
                    wrap_index(self.load_slot_selection, SAVE_SLOT_COUNT, -1);
                self.menu_feedback = None;
                false
            }
            (KeyCode::Down, _) => {
                self.load_slot_selection = wrap_index(self.load_slot_selection, SAVE_SLOT_COUNT, 1);
                self.menu_feedback = None;
                false
            }
            (KeyCode::Esc, _) => {
                self.screen = Screen::StartMenu;
                false
            }
            (KeyCode::Enter, _) => match self.load_game(self.load_slot_selection) {
                Ok(()) => false,
                Err(error) => {
                    self.menu_feedback = Some(format!("Load failed: {error}"));
                    false
                }
            },
            _ => false,
        }
    }

    fn handle_settings_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
            (KeyCode::Esc, _) => {
                self.screen = Screen::StartMenu;
                false
            }
            (KeyCode::Left, _) => {
                self.settings_focus = self.settings_focus.saturating_sub(1);
                false
            }
            (KeyCode::Right, _) => {
                self.settings_focus = (self.settings_focus + 1).min(1);
                false
            }
            (KeyCode::Up, _) => {
                if self.settings_focus == 0 {
                    self.settings_selection =
                        wrap_index(self.settings_selection, TICK_SPEEDS.len(), -1);
                } else {
                    self.difficulty_selection =
                        wrap_index(self.difficulty_selection, DIFFICULTY_OPTIONS.len(), -1);
                }
                false
            }
            (KeyCode::Down, _) => {
                if self.settings_focus == 0 {
                    self.settings_selection =
                        wrap_index(self.settings_selection, TICK_SPEEDS.len(), 1);
                } else {
                    self.difficulty_selection =
                        wrap_index(self.difficulty_selection, DIFFICULTY_OPTIONS.len(), 1);
                }
                false
            }
            (KeyCode::Enter, _) => {
                self.tick_speed_index = self.settings_selection;
                self.difficulty = Difficulty::from_index(self.difficulty_selection);
                if self.has_active_game {
                    self.menu_feedback = self
                        .save_game()
                        .err()
                        .map(|error| format!("Save failed: {error}"));
                }
                self.screen = Screen::StartMenu;
                false
            }
            _ => false,
        }
    }

    fn handle_how_to_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
            (KeyCode::Esc | KeyCode::Enter, _) => {
                self.screen = Screen::StartMenu;
                false
            }
            _ => false,
        }
    }

    fn handle_game_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
            (KeyCode::Esc, _) => {
                if self.cancel_dispatch() {
                    false
                } else {
                    self.menu_feedback = self
                        .save_game()
                        .err()
                        .map(|error| format!("Save failed: {error}"));
                    self.screen = Screen::StartMenu;
                    self.start_menu_selection = 0;
                    false
                }
            }
            (KeyCode::Tab | KeyCode::Right, _) => {
                self.active_pane = (self.active_pane + 1) % PANE_TITLES.len();
                false
            }
            (KeyCode::BackTab | KeyCode::Left, _) => {
                self.active_pane = (self.active_pane + PANE_TITLES.len() - 1) % PANE_TITLES.len();
                false
            }
            (KeyCode::Up, _) => {
                self.move_selection(-1);
                false
            }
            (KeyCode::Down, _) => {
                self.move_selection(1);
                false
            }
            (KeyCode::Enter, _) => {
                self.activate_selection();
                false
            }
            _ => false,
        }
    }

    fn tick(&mut self) {
        if self.screen != Screen::InGame {
            return;
        }

        self.clock += 1;
        let mut arrivals = Vec::new();
        let mut events = Vec::new();

        for ship in &mut self.fleet {
            if let ShipState::EnRoute {
                destination,
                eta_remaining,
                total_eta,
                assigned_contract,
                ..
            } = &mut ship.state
            {
                let destination_index = *destination;
                let contract_assignment = *assigned_contract;

                if *eta_remaining == *total_eta {
                    events.push(ShipEvent::Departed {
                        name: ship.name,
                        origin: ship.current_location,
                        destination: destination_index,
                    });
                }

                if *eta_remaining > 0 {
                    *eta_remaining -= 1;
                }

                if *eta_remaining == 1 {
                    events.push(ShipEvent::Approaching {
                        name: ship.name,
                        destination: destination_index,
                    });
                }

                if *eta_remaining == 0 {
                    ship.current_location = destination_index;
                    ship.state = ShipState::Docked;
                    arrivals.push((ship.name, destination_index, contract_assignment));
                }
            }
        }

        for event in events {
            match event {
                ShipEvent::Departed {
                    name,
                    origin,
                    destination,
                } => self.push_log(format!(
                    "[{clock:04}] {name} cleared {origin} and is outbound for {destination}.",
                    clock = self.clock,
                    name = name,
                    origin = self.location_name(origin),
                    destination = self.location_name(destination),
                )),
                ShipEvent::Approaching { name, destination } => self.push_log(format!(
                    "[{clock:04}] {name} is on final approach to {destination}.",
                    clock = self.clock,
                    name = name,
                    destination = self.location_name(destination),
                )),
            }
        }

        for (ship_name, destination_index, contract_assignment) in arrivals {
            self.push_log(format!(
                "[{clock:04}] {name} arrived at {destination}.",
                clock = self.clock,
                name = ship_name,
                destination = self.location_name(destination_index),
            ));

            if let Some(contract_index) = contract_assignment {
                self.resolve_contract_arrival(contract_index, ship_name, destination_index);
            }

            if let Some(discovery) = self.reveal_from_arrival(destination_index) {
                self.push_log(discovery);
            }
        }

        self.update_contract_deadlines();

        if self.clock % 16 == 0 {
            self.push_log(self.ambient_event());
        }

        let _ = self.save_game();
    }

    fn mode_label(&self) -> String {
        match self.mode {
            AppMode::Browse => "Browse".to_string(),
            AppMode::SelectingDestination { ship_index } => {
                format!("Route Planning {}", self.fleet[ship_index].name)
            }
        }
    }

    fn controls_text(&self) -> String {
        match self.mode {
            AppMode::Browse => {
                "Tab/Shift+Tab or Left/Right: focus   Up/Down: select   Enter in Board/Fleet   Esc: menu   q/Ctrl+C: quit"
                    .to_string()
            }
            AppMode::SelectingDestination { .. } => {
                "Up/Down: choose destination   Enter: confirm route   Esc: cancel   q/Ctrl+C: quit"
                    .to_string()
            }
        }
    }

    fn move_selection(&mut self, delta: isize) {
        match self.active_pane {
            CONTRACTS_PANE => {
                self.selected_contract =
                    wrap_index(self.selected_contract, self.contracts.len(), delta);
            }
            MAP_PANE => {
                self.selected_location =
                    self.next_discovered_location(self.selected_location, delta);
            }
            FLEET_PANE => {
                self.selected_ship = wrap_index(self.selected_ship, self.fleet.len(), delta);
            }
            _ => {}
        }
    }

    fn activate_selection(&mut self) {
        match self.mode {
            AppMode::Browse => match self.active_pane {
                CONTRACTS_PANE => self.toggle_contract_tracking(),
                FLEET_PANE => self.begin_dispatch(),
                _ => {}
            },
            AppMode::SelectingDestination { .. } => {
                if self.active_pane == MAP_PANE {
                    self.confirm_dispatch();
                }
            }
        }
    }

    fn begin_dispatch(&mut self) {
        let ship = &self.fleet[self.selected_ship];

        if !ship.is_docked() {
            self.push_log(format!(
                "[{clock:04}] {name} is already in transit and cannot be reassigned.",
                clock = self.clock,
                name = ship.name,
            ));
            return;
        }

        self.mode = AppMode::SelectingDestination {
            ship_index: self.selected_ship,
        };
        self.active_pane = MAP_PANE;

        if let Some(contract_index) = self.tracked_contract {
            let contract = &self.contracts[contract_index];
            if matches!(contract.state, ContractState::Accepted { .. })
                && contract.origin == ship.current_location
            {
                self.selected_location = contract.destination;
                return;
            }
        }

        if !self.is_discovered(self.selected_location)
            || self.selected_location == ship.current_location
        {
            if let Some(destination) = self.first_dispatch_target(ship.current_location) {
                self.selected_location = destination;
            }
        }
    }

    fn confirm_dispatch(&mut self) {
        let AppMode::SelectingDestination { ship_index } = self.mode else {
            return;
        };

        let ship_name = self.fleet[ship_index].name;
        let origin = self.fleet[ship_index].current_location;

        let Some(plan) = self.plan_route(origin, self.selected_location) else {
            self.push_log(format!(
                "[{clock:04}] Pick a different charted destination before dispatching {name}.",
                clock = self.clock,
                name = ship_name,
            ));
            return;
        };

        let destination = self.selected_location;
        let assigned_contract = self.matching_tracked_contract(origin, destination);

        if let Some(contract_index) = assigned_contract {
            let accepted_at = match self.contracts[contract_index].state {
                ContractState::Accepted { accepted_at } => accepted_at,
                _ => self.clock,
            };
            self.contracts[contract_index].state = ContractState::Assigned {
                ship_name: ship_name,
                accepted_at,
            };
        }

        self.fleet[ship_index].state = ShipState::EnRoute {
            origin,
            destination,
            eta_remaining: plan.eta,
            total_eta: plan.eta,
            route: plan.path.clone(),
            condition_summary: plan.condition_summary.clone(),
            assigned_contract,
        };
        self.mode = AppMode::Browse;
        self.active_pane = FLEET_PANE;

        self.push_log(format!(
            "[{clock:04}] {name} dispatched: {path} | ETA {eta} | {conditions}",
            clock = self.clock,
            name = ship_name,
            path = plan.path,
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
        } else if let Some(contract_index) = self.tracked_contract {
            self.push_log(format!(
                "[{clock:04}] Survey run only. Tracked contract still waits: {title}.",
                clock = self.clock,
                title = self.contracts[contract_index].title,
            ));
        }

        let _ = self.save_game();
    }

    fn cancel_dispatch(&mut self) -> bool {
        if matches!(self.mode, AppMode::SelectingDestination { .. }) {
            self.mode = AppMode::Browse;
            self.active_pane = FLEET_PANE;
            return true;
        }

        false
    }

    fn pending_ship(&self) -> Option<usize> {
        match self.mode {
            AppMode::Browse => None,
            AppMode::SelectingDestination { ship_index } => Some(ship_index),
        }
    }

    fn preview_origin(&self) -> Option<usize> {
        if let Some(ship_index) = self.pending_ship() {
            return Some(self.fleet[ship_index].current_location);
        }

        let ship = &self.fleet[self.selected_ship];
        ship.is_docked().then_some(ship.current_location)
    }

    fn ambient_event(&self) -> String {
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

    fn plan_route(&self, from: usize, to: usize) -> Option<RoutePlan> {
        if from == to || !self.is_discovered(from) || !self.is_discovered(to) {
            return None;
        }

        if from == ASTRA_PRIME || to == ASTRA_PRIME {
            let leaf = if from == ASTRA_PRIME { to } else { from };
            let location = &self.locations[leaf];
            let condition = self.lane_condition(leaf);

            return Some(RoutePlan {
                path: format!("{} -> {}", self.location_name(from), self.location_name(to)),
                eta: location.travel_time_from_hub + condition.penalty(),
                condition_summary: format!("{}: {}", location.lane_name, condition.label()),
            });
        }

        let outbound = &self.locations[from];
        let inbound = &self.locations[to];
        let outbound_condition = self.lane_condition(from);
        let inbound_condition = self.lane_condition(to);

        Some(RoutePlan {
            path: format!(
                "{} -> {} -> {}",
                self.location_name(from),
                self.location_name(ASTRA_PRIME),
                self.location_name(to)
            ),
            eta: outbound.travel_time_from_hub
                + outbound_condition.penalty()
                + inbound.travel_time_from_hub
                + inbound_condition.penalty(),
            condition_summary: format!(
                "{}: {} | {}: {}",
                outbound.lane_name,
                outbound_condition.label(),
                inbound.lane_name,
                inbound_condition.label(),
            ),
        })
    }

    fn lane_condition(&self, leaf_index: usize) -> LaneCondition {
        match ((self.clock / 12) + leaf_index as u64) % 4 {
            0 => LaneCondition::Clear,
            1 => LaneCondition::Traffic,
            2 => LaneCondition::Debris,
            _ => LaneCondition::Solar,
        }
    }

    fn location_name(&self, index: usize) -> &'static str {
        self.locations[index].name
    }

    fn is_discovered(&self, index: usize) -> bool {
        self.discovered_locations[index]
    }

    fn discovered_count(&self) -> usize {
        self.discovered_locations
            .iter()
            .filter(|&&seen| seen)
            .count()
    }

    fn undiscovered_count(&self) -> usize {
        self.locations.len() - self.discovered_count()
    }

    fn next_discovered_location(&self, current: usize, delta: isize) -> usize {
        let mut next = current;

        for _ in 0..self.locations.len() {
            next = wrap_index(next, self.locations.len(), delta);
            if self.is_discovered(next) {
                return next;
            }
        }

        current
    }

    fn first_dispatch_target(&self, origin: usize) -> Option<usize> {
        (0..self.locations.len()).find(|&index| self.is_discovered(index) && index != origin)
    }

    fn is_contract_unlocked(&self, index: usize) -> bool {
        let contract = &self.contracts[index];
        self.is_discovered(contract.origin)
            && self.is_discovered(contract.destination)
            && self.is_discovered(contract.unlock_location)
    }

    fn contract_status_label(&self, index: usize) -> &'static str {
        match self.contracts[index].state {
            ContractState::Available => "open",
            ContractState::Accepted { .. } => "tracked",
            ContractState::Assigned { .. } => "assigned",
            ContractState::Completed => "complete",
            ContractState::Failed => "failed",
        }
    }

    fn contract_elapsed_ticks(&self, index: usize) -> Option<u64> {
        match self.contracts[index].state {
            ContractState::Accepted { accepted_at }
            | ContractState::Assigned { accepted_at, .. } => {
                Some(self.clock.saturating_sub(accepted_at))
            }
            _ => None,
        }
    }

    fn contract_current_reward(&self, index: usize) -> i32 {
        let contract = &self.contracts[index];
        let elapsed = self.contract_elapsed_ticks(index).unwrap_or(0);
        self.difficulty.reward_decay(contract.reward, elapsed)
    }

    fn contract_pressure_text(&self, index: usize) -> String {
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

    fn contract_hint(&self, index: usize) -> String {
        let contract = &self.contracts[index];

        match &contract.state {
            ContractState::Available => contract.briefing.to_string(),
            ContractState::Accepted { .. } => {
                format!(
                    "Tracked. Dispatch a ship from the contract origin to the listed destination. {}",
                    self.contract_pressure_text(index)
                )
            }
            ContractState::Assigned { ship_name, .. } => {
                format!(
                    "Assigned to {}. Waiting on delivery. {}",
                    ship_name,
                    self.contract_pressure_text(index)
                )
            }
            ContractState::Completed => {
                "Completed. Reward already banked to the dispatch account.".to_string()
            }
            ContractState::Failed => {
                "Failed. This state is reserved for future hard-mode pressure rules.".to_string()
            }
        }
    }

    fn toggle_contract_tracking(&mut self) {
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

        let _ = self.save_game();
    }

    fn matching_tracked_contract(&self, origin: usize, destination: usize) -> Option<usize> {
        let contract_index = self.tracked_contract?;
        let contract = &self.contracts[contract_index];

        if !matches!(contract.state, ContractState::Accepted { .. }) {
            return None;
        }

        (contract.origin == origin && contract.destination == destination).then_some(contract_index)
    }

    fn next_discovery_target(&self) -> Option<(usize, usize)> {
        (0..self.locations.len()).find_map(|index| {
            self.locations[index]
                .reveal_on_arrival
                .filter(|&target| self.is_discovered(index) && !self.is_discovered(target))
                .map(|target| (index, target))
        })
    }

    fn resolve_contract_arrival(
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

    fn update_contract_deadlines(&mut self) {
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

    fn is_frontier_location(&self, index: usize) -> bool {
        self.is_discovered(index)
            && self.locations[index]
                .reveal_on_arrival
                .is_some_and(|target| !self.is_discovered(target))
    }

    fn highlighted_route(&self) -> Option<(usize, usize)> {
        if let Some(ship_index) = self.pending_ship() {
            let origin = self.fleet[ship_index].current_location;
            return self
                .plan_route(origin, self.selected_location)
                .map(|_| (origin, self.selected_location));
        }

        let ship = &self.fleet[self.selected_ship];
        if !ship.is_docked() {
            return None;
        }

        self.plan_route(ship.current_location, self.selected_location)
            .map(|_| (ship.current_location, self.selected_location))
    }

    fn frontier_locations(&self) -> Vec<usize> {
        (0..self.locations.len())
            .filter(|&index| self.is_frontier_location(index))
            .collect()
    }

    fn discovered_lane_locations(&self) -> Vec<usize> {
        let mut discovered: Vec<usize> = (1..self.locations.len())
            .filter(|&index| self.is_discovered(index))
            .collect();

        if discovered.is_empty() {
            discovered.push(ASTRA_PRIME);
        }

        discovered
    }

    fn reveal_from_arrival(&mut self, location_index: usize) -> Option<String> {
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

    fn in_transit_count(&self) -> usize {
        self.fleet.iter().filter(|ship| !ship.is_docked()).count()
    }

    fn push_log(&mut self, entry: String) {
        self.log.insert(0, entry);
        self.log.truncate(8);
    }
}

#[derive(Serialize, Deserialize)]
struct SaveGame {
    version: u8,
    tick_speed_index: usize,
    active_pane: usize,
    clock: u64,
    mode: SavedAppMode,
    selected_location: usize,
    selected_ship: usize,
    selected_contract: usize,
    tracked_contract: Option<usize>,
    credits: i32,
    difficulty: Difficulty,
    discovered_locations: Vec<bool>,
    fleet: Vec<SavedShip>,
    contracts: Vec<SavedContract>,
    log: Vec<String>,
}

#[derive(Serialize, Deserialize)]
enum SavedAppMode {
    Browse,
    SelectingDestination { ship_index: usize },
}

#[derive(Serialize, Deserialize)]
struct SavedShip {
    current_location: usize,
    state: SavedShipState,
}

#[derive(Serialize, Deserialize)]
enum SavedShipState {
    Docked,
    EnRoute {
        origin: usize,
        destination: usize,
        eta_remaining: u16,
        total_eta: u16,
        route: String,
        condition_summary: String,
        assigned_contract: Option<usize>,
    },
}

#[derive(Serialize, Deserialize)]
struct SavedContract {
    deadline: u64,
    state: SavedContractState,
}

#[derive(Serialize, Deserialize)]
enum SavedContractState {
    Available,
    Accepted { accepted_at: u64 },
    Assigned { ship_index: usize, accepted_at: u64 },
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Difficulty {
    Cozy,
    Normal,
    Insane,
}

impl Difficulty {
    fn label(self) -> &'static str {
        match self {
            Self::Cozy => "Cozy",
            Self::Normal => "Normal",
            Self::Insane => "Insane",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Cozy => "No reward decay and no contract failure pressure.",
            Self::Normal => "Rewards slowly decay after acceptance, but contracts never fail.",
            Self::Insane => {
                "Rewards decay faster and accepted contracts fail if their delivery window expires."
            }
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Cozy => 0,
            Self::Normal => 1,
            Self::Insane => 2,
        }
    }

    fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Cozy,
            1 => Self::Normal,
            _ => Self::Insane,
        }
    }

    fn reward_decay(self, base_reward: i32, elapsed_ticks: u64) -> i32 {
        match self {
            Self::Cozy => base_reward,
            Self::Normal => {
                let decay_steps = (elapsed_ticks / 3) as i32;
                (base_reward - decay_steps * 8).max(base_reward / 2)
            }
            Self::Insane => {
                let decay_steps = (elapsed_ticks / 2) as i32;
                (base_reward - decay_steps * 14).max((base_reward / 4).max(30))
            }
        }
    }

    fn enforces_time_limit(self) -> bool {
        matches!(self, Self::Insane)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    StartMenu,
    LoadGame,
    Settings,
    HowToPlay,
    InGame,
}

#[derive(Clone, Copy)]
enum StartMenuAction {
    ResumeShift,
    NewGame,
    LoadGame,
    Settings,
    HowToPlay,
    Quit,
}

impl StartMenuAction {
    fn label(self) -> &'static str {
        match self {
            Self::ResumeShift => "Resume Shift",
            Self::NewGame => "New Game",
            Self::LoadGame => "Load Game",
            Self::Settings => "Settings",
            Self::HowToPlay => "How To Play",
            Self::Quit => "Quit",
        }
    }

    fn description(self, app: &App) -> String {
        match self {
            Self::ResumeShift => {
                "Return to the current bridge and continue the active dispatch shift.".to_string()
            }
            Self::NewGame => format!(
                "Start a fresh {} shift in {} with 120 credits, two charted locations, and an unopened contract board.",
                app.difficulty.label(),
                App::save_slot_label(app.load_slot_selection)
            ),
            Self::LoadGame => format!(
                "Load {}. {}",
                App::save_slot_label(app.load_slot_selection),
                app.selected_slot_summary_text()
            ),
            Self::Settings => {
                "Adjust simulation speed before launching the live TUI bridge.".to_string()
            }
            Self::HowToPlay => {
                "Read the current goals, contract flow, and frontier discovery rules.".to_string()
            }
            Self::Quit => "Leave Starlane Courier and restore the terminal.".to_string(),
        }
    }
}

#[derive(Clone, Copy)]
enum AppMode {
    Browse,
    SelectingDestination { ship_index: usize },
}

struct Contract {
    title: &'static str,
    briefing: &'static str,
    origin: usize,
    destination: usize,
    reward: i32,
    deadline: u64,
    unlock_location: usize,
    state: ContractState,
}

impl Contract {
    fn new(
        title: &'static str,
        briefing: &'static str,
        origin: usize,
        destination: usize,
        reward: i32,
        deadline: u64,
        unlock_location: usize,
    ) -> Self {
        Self {
            title,
            briefing,
            origin,
            destination,
            reward,
            deadline,
            unlock_location,
            state: ContractState::Available,
        }
    }
}

#[derive(Clone, Copy)]
enum ContractState {
    Available,
    Accepted {
        accepted_at: u64,
    },
    Assigned {
        ship_name: &'static str,
        accepted_at: u64,
    },
    Completed,
    Failed,
}

struct Location {
    name: &'static str,
    lane_name: &'static str,
    travel_time_from_hub: u16,
    reveal_on_arrival: Option<usize>,
}

impl Location {
    fn hub(name: &'static str) -> Self {
        Self {
            name,
            lane_name: "Central Exchange",
            travel_time_from_hub: 0,
            reveal_on_arrival: None,
        }
    }

    fn new(
        name: &'static str,
        lane_name: &'static str,
        travel_time_from_hub: u16,
        reveal_on_arrival: Option<usize>,
    ) -> Self {
        Self {
            name,
            lane_name,
            travel_time_from_hub,
            reveal_on_arrival,
        }
    }
}

struct Ship {
    name: &'static str,
    current_location: usize,
    state: ShipState,
}

impl Ship {
    fn docked(name: &'static str, current_location: usize) -> Self {
        Self {
            name,
            current_location,
            state: ShipState::Docked,
        }
    }

    fn en_route(
        name: &'static str,
        origin: usize,
        destination: usize,
        eta_remaining: u16,
        total_eta: u16,
        route: &'static str,
        condition_summary: &'static str,
        assigned_contract: Option<usize>,
    ) -> Self {
        Self {
            name,
            current_location: origin,
            state: ShipState::EnRoute {
                origin,
                destination,
                eta_remaining,
                total_eta,
                route: route.to_string(),
                condition_summary: condition_summary.to_string(),
                assigned_contract,
            },
        }
    }

    fn is_docked(&self) -> bool {
        matches!(self.state, ShipState::Docked)
    }

    fn map_tag(&self) -> String {
        self.name
            .split_whitespace()
            .last()
            .unwrap_or(self.name)
            .chars()
            .take(2)
            .collect::<String>()
            .to_uppercase()
    }
}

enum ShipState {
    Docked,
    EnRoute {
        origin: usize,
        destination: usize,
        eta_remaining: u16,
        total_eta: u16,
        route: String,
        condition_summary: String,
        assigned_contract: Option<usize>,
    },
}

#[derive(Clone, Copy)]
enum TransitPhase {
    Undocking,
    Cruising,
    Approach,
}

impl TransitPhase {
    fn label(self) -> &'static str {
        match self {
            Self::Undocking => "Undocking",
            Self::Cruising => "Cruising",
            Self::Approach => "Approach",
        }
    }

    fn status_line(
        self,
        origin: &'static str,
        destination: &'static str,
        eta_remaining: u16,
    ) -> String {
        match self {
            Self::Undocking => format!("Undocking from {} | ETA {}", origin, eta_remaining),
            Self::Cruising => format!("Cruising to {} | ETA {}", destination, eta_remaining),
            Self::Approach => format!("Approaching {} | ETA {}", destination, eta_remaining),
        }
    }
}

fn transit_phase(eta_remaining: u16, total_eta: u16) -> TransitPhase {
    if eta_remaining == total_eta {
        TransitPhase::Undocking
    } else if eta_remaining <= 1 {
        TransitPhase::Approach
    } else {
        TransitPhase::Cruising
    }
}

enum ShipEvent {
    Departed {
        name: &'static str,
        origin: usize,
        destination: usize,
    },
    Approaching {
        name: &'static str,
        destination: usize,
    },
}

struct RoutePlan {
    path: String,
    eta: u16,
    condition_summary: String,
}

#[derive(Clone, Copy)]
enum LaneCondition {
    Clear,
    Traffic,
    Debris,
    Solar,
}

impl LaneCondition {
    fn label(self) -> &'static str {
        match self {
            Self::Clear => "clear lanes",
            Self::Traffic => "traffic delay (+1)",
            Self::Debris => "debris interference (+2)",
            Self::Solar => "solar static (+3)",
        }
    }

    fn penalty(self) -> u16 {
        match self {
            Self::Clear => 0,
            Self::Traffic => 1,
            Self::Debris => 2,
            Self::Solar => 3,
        }
    }

    fn report_phrase(self) -> &'static str {
        match self {
            Self::Clear => "clear lanes",
            Self::Traffic => "heavy traffic",
            Self::Debris => "debris interference",
            Self::Solar => "solar static bursts",
        }
    }
}

fn wrap_index(current: usize, len: usize, delta: isize) -> usize {
    ((current as isize + delta).rem_euclid(len as isize)) as usize
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, cursor::Show);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_round_trip_restores_session() {
        let path = Path::new("/tmp/opencode/star-dispatch-save-test.json");
        let _ = fs::remove_file(path);

        let mut app = App::new();
        app.has_active_game = true;
        app.screen = Screen::InGame;
        app.tick_speed_index = 2;
        app.difficulty = Difficulty::Insane;
        app.difficulty_selection = app.difficulty.index();
        app.clock = 42;
        app.selected_location = DUST_HARBOR;
        app.selected_ship = 0;
        app.selected_contract = 0;
        app.tracked_contract = Some(0);
        app.credits = 321;
        app.discovered_locations[KITE_STATION] = true;
        app.contracts[0].state = ContractState::Assigned {
            ship_name: "SV Kestrel",
            accepted_at: 12,
        };
        app.fleet[0].state = ShipState::EnRoute {
            origin: ASTRA_PRIME,
            destination: DUST_HARBOR,
            eta_remaining: 3,
            total_eta: 5,
            route: "Astra Prime -> Dust Harbor".to_string(),
            condition_summary: "Dust Corridor: clear lanes".to_string(),
            assigned_contract: Some(0),
        };
        app.log = vec!["[0042] persistence check".to_string()];

        app.save_game_to_path(path).unwrap();

        let save = App::read_save_file_from_path(path).unwrap().unwrap();
        let mut restored = App::new();
        restored.apply_save(save).unwrap();

        assert_eq!(restored.tick_speed_index, 2);
        assert_eq!(restored.difficulty, Difficulty::Insane);
        assert_eq!(restored.clock, 42);
        assert_eq!(restored.credits, 321);
        assert_eq!(restored.tracked_contract, Some(0));
        assert!(restored.discovered_locations[KITE_STATION]);
        assert_eq!(restored.log[0], "[0042] persistence check");
        assert!(matches!(
            restored.contracts[0].state,
            ContractState::Assigned {
                ship_name: "SV Kestrel",
                accepted_at: 12
            }
        ));
        assert!(matches!(
            restored.fleet[0].state,
            ShipState::EnRoute {
                destination: DUST_HARBOR,
                assigned_contract: Some(0),
                ..
            }
        ));

        let _ = fs::remove_file(path);
    }
}
