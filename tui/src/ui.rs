use ratatui::{
    Frame,
    layout::Constraint,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, Paragraph},
};

use crate::app::{App, Mode};

pub fn ui(frame: &mut Frame, app: &mut App) {
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

    frame.render_widget(
        Paragraph::new(help_text).style(Style::default().fg(Color::DarkGray)),
        help_area,
    );
}
