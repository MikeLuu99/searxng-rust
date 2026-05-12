mod app;
mod ui;

use std::io::{self, Stdout};
use std::sync::Arc;
use std::time::Duration;

use app::{App, Mode};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use metadata_search_engine_rs::{
    aggregator::{aggregate, query_all_engines},
    engines::{BraveEngine, DuckDuckGoEngine, SearchEngine, StartpageEngine, build_http_client},
    models::AggregatedResult,
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// Event loop
// ---------------------------------------------------------------------------

async fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    rx: &mut mpsc::Receiver<Result<Vec<AggregatedResult>, String>>,
    tx: mpsc::Sender<Result<Vec<AggregatedResult>, String>>,
    engines: Vec<Arc<dyn SearchEngine>>,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| ui::ui(f, app))?;

        if let Ok(msg) = rx.try_recv() {
            match msg {
                Ok(results) => app.set_results(results),
                Err(e) => app.mode = Mode::Error(e),
            }
        }

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(());
        }

        if matches!(app.mode, Mode::Input) {
            match key.code {
                KeyCode::Esc => return Ok(()),
                KeyCode::Enter => {
                    let query = app.input.trim().to_string();
                    if !query.is_empty() {
                        app.mode = Mode::Loading;
                        let tx = tx.clone();
                        let engines = engines.clone();
                        tokio::spawn(async move {
                            let (successes, _) =
                                query_all_engines(&engines, &query, 10).await;
                            let result = if successes.is_empty() {
                                Err("All engines failed to respond.".to_string())
                            } else {
                                Ok(aggregate(successes, 10))
                            };
                            let _ = tx.send(result).await;
                        });
                    }
                }
                KeyCode::Char(c) => app.input.push(c),
                KeyCode::Backspace => {
                    app.input.pop();
                }
                _ => {}
            }
        } else if matches!(app.mode, Mode::Browse) {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Down | KeyCode::Char('j') => app.next(),
                KeyCode::Up | KeyCode::Char('k') => app.prev(),
                KeyCode::Char('g') => app.list_state.select(Some(0)),
                KeyCode::Char('G') => {
                    let last = app.results.len().saturating_sub(1);
                    app.list_state.select(Some(last));
                }
                KeyCode::Enter | KeyCode::Char('l') => {
                    if let Some(url) = app.selected_url() {
                        let _ = open::that_detached(url);
                    }
                }
                KeyCode::Char('h') | KeyCode::Char('/') => {
                    app.input.clear();
                    app.mode = Mode::Input;
                }
                _ => {}
            }
        } else if matches!(app.mode, Mode::Error(_)) {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Char('h') | KeyCode::Char('/') => {
                    app.input.clear();
                    app.mode = Mode::Input;
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(build_http_client()?);
    let engines: Vec<Arc<dyn SearchEngine>> = vec![
        Arc::new(DuckDuckGoEngine { client: Arc::clone(&client) }),
        Arc::new(BraveEngine { client: Arc::clone(&client) }),
        Arc::new(StartpageEngine { client: Arc::clone(&client) }),
    ];

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let (tx, mut rx) = mpsc::channel::<Result<Vec<AggregatedResult>, String>>(1);
    let mut app = App::new();

    let initial_query: String = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if !initial_query.trim().is_empty() {
        app.input = initial_query.trim().to_string();
        app.mode = Mode::Loading;
        let tx2 = tx.clone();
        let engines2 = engines.clone();
        let query = app.input.clone();
        tokio::spawn(async move {
            let (successes, _) = query_all_engines(&engines2, &query, 10).await;
            let result = if successes.is_empty() {
                Err("All engines failed to respond.".to_string())
            } else {
                Ok(aggregate(successes, 10))
            };
            let _ = tx2.send(result).await;
        });
    }

    let result = run(&mut terminal, &mut app, &mut rx, tx, engines).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}
