use std::io::{self, Stdout};
use std::sync::Arc;
use std::time::Duration;

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
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::Constraint,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
};
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

enum Mode {
    Input,
    Loading,
    Browse,
    Error(String),
}

struct App {
    mode: Mode,
    input: String,
    results: Vec<AggregatedResult>,
    list_state: ListState,
}

impl App {
    fn new() -> Self {
        Self {
            mode: Mode::Input,
            input: String::new(),
            results: Vec::new(),
            list_state: ListState::default(),
        }
    }

    fn set_results(&mut self, results: Vec<AggregatedResult>) {
        self.results = results;
        self.list_state
            .select(if self.results.is_empty() { None } else { Some(0) });
        self.mode = Mode::Browse;
    }

    fn next(&mut self) {
        if self.results.is_empty() {
            return;
        }
        let next = self
            .list_state
            .selected()
            .map_or(0, |i| (i + 1).min(self.results.len() - 1));
        self.list_state.select(Some(next));
    }

    fn prev(&mut self) {
        let prev = self
            .list_state
            .selected()
            .map_or(0, |i| i.saturating_sub(1));
        self.list_state.select(Some(prev));
    }

    fn selected_url(&self) -> Option<String> {
        self.list_state
            .selected()
            .and_then(|i| self.results.get(i))
            .map(|r| r.url.clone())
    }
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

fn ui(frame: &mut Frame, app: &mut App) {
    // Extract mode info before any mutable borrow of list_state below.
    let is_input = matches!(app.mode, Mode::Input);
    let is_loading = matches!(app.mode, Mode::Loading);
    let error_msg = if let Mode::Error(ref e) = app.mode {
        Some(e.clone())
    } else {
        None
    };
    let help_text = match &app.mode {
        Mode::Input   => "  enter:search  esc:quit",
        Mode::Loading => "  searching…",
        Mode::Browse  => "  jk:move  l/enter:open  h/:search  q:quit",
        Mode::Error(_) => "  h/:search  q:quit",
    };

    let area = frame.area();
    let [search_area, results_area, help_area] =
        ratatui::layout::Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(area);

    // Search bar — yellow border when active
    let border_color = if is_input { Color::Yellow } else { Color::White };
    frame.render_widget(
        Paragraph::new(app.input.as_str()).block(
            Block::bordered()
                .title(" Search ")
                .border_style(Style::default().fg(border_color)),
        ),
        search_area,
    );
    if is_input {
        frame.set_cursor_position((
            search_area.x + app.input.len() as u16 + 1,
            search_area.y + 1,
        ));
    }

    // Results / loading / error panel
    if is_loading {
        frame.render_widget(
            Paragraph::new("  Searching all engines…")
                .style(Style::default().fg(Color::Cyan))
                .block(Block::bordered().title(" Results ")),
            results_area,
        );
    } else if let Some(msg) = error_msg {
        frame.render_widget(
            Paragraph::new(format!("  {msg}"))
                .style(Style::default().fg(Color::Red))
                .block(Block::bordered().title(" Error ")),
            results_area,
        );
    } else {
        let items: Vec<ListItem> = app
            .results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut lines = vec![
                    Line::from(vec![
                        Span::styled(
                            format!(" #{} ", i + 1),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(
                            r.title.clone(),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("  [{}]", r.engines.join(", ")),
                            Style::default().fg(Color::Green),
                        ),
                    ]),
                    Line::from(vec![
                        Span::raw("     "),
                        Span::styled(r.url.clone(), Style::default().fg(Color::Blue)),
                    ]),
                ];
                if let Some(s) = &r.snippet {
                    lines.push(Line::from(vec![
                        Span::raw("     "),
                        Span::styled(
                            s.char_indices().nth(100).map_or(s.as_str(), |(i, _)| &s[..i]).to_string(),
                            Style::default().fg(Color::Gray),
                        ),
                    ]));
                }
                lines.push(Line::from(""));
                ListItem::new(lines)
            })
            .collect();

        let title = if app.results.is_empty() {
            " Results ".to_string()
        } else {
            format!(" Results ({}) ", app.results.len())
        };

        let list = List::new(items)
            .block(Block::bordered().title(title))
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, results_area, &mut app.list_state);
    }

    // Help bar
    frame.render_widget(
        Paragraph::new(help_text).style(Style::default().fg(Color::DarkGray)),
        help_area,
    );
}

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
        terminal.draw(|f| ui(f, app))?;

        // Non-blocking check for a completed search task.
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

        // Ctrl-c quits from any mode.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(());
        }

        // Use matches! so the borrow of app.mode ends before we mutate app below.
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
                // l or Enter: open selected URL in browser
                KeyCode::Enter | KeyCode::Char('l') => {
                    if let Some(url) = app.selected_url() {
                        let _ = open::that_detached(url);
                    }
                }
                // h or /: go back to search input
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
        // Mode::Loading: ignore all keys while search is in flight.
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

    // If the user passed a query as CLI args (e.g. `stx rust vs go`), kick off
    // the search immediately instead of waiting for input.
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

    // Always restore the terminal, even on error.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}
