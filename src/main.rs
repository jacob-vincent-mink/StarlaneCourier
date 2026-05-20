mod app;
mod game;
mod llm;
mod save;
mod settings;
mod ui;

use std::io;

use app::App;
use crossterm::{
    cursor,
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

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
        app.sync_background_work();
        terminal.draw(|frame| ui::draw(frame, app))?;

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

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, cursor::Show);
    }
}
