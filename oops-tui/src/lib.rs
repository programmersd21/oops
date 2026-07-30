use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use oops_core::{DiffResult, SnapshotSummary};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::Constraint,
    style::{Color, Modifier, Style},
    text::Text,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
};
use std::io;

struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn term<T>(
    f: impl FnOnce(&mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<T>,
) -> io::Result<T> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let _g = Guard;
    let mut term = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    f(&mut term)
}

fn truncated(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

fn bytes_fmt(n: i64) -> String {
    if n < 1024 {
        format!("{n}B")
    } else if n < 1024 * 1024 {
        format!("{}K", n / 1024)
    } else {
        format!("{}M", n / (1024 * 1024))
    }
}

fn wait_q() -> io::Result<()> {
    loop {
        if let Event::Key(k) = event::read()?
            && matches!(k.code, KeyCode::Char('q') | KeyCode::Esc)
        {
            return Ok(());
        }
    }
}

pub fn list(items: Vec<SnapshotSummary>) -> io::Result<()> {
    let mut state = TableState::default();
    state.select(Some(0));
    term(|term| {
        loop {
            term.draw(|f| {
                let cmd_max = (f.area().width as usize).saturating_sub(48).max(10);
                let selected = state.selected().unwrap_or(0);
                let rows: Vec<Row> = items
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let style = if i == selected {
                            Style::new().bg(Color::White).fg(Color::Black)
                        } else {
                            Style::default()
                        };
                        Row::new(vec![
                            Cell::from(s.id.to_string()).style(style),
                            Cell::from(truncated(&s.command, cmd_max)).style(style),
                            Cell::from(s.file_count.to_string()).style(style),
                            Cell::from(bytes_fmt(s.total_bytes)).style(style),
                            Cell::from(s.method.clone()).style(style),
                            Cell::from(if s.restorable { "ready" } else { "partial" }).style(style),
                        ])
                    })
                    .collect();
                let header_style = Style::new().add_modifier(Modifier::BOLD);
                let t = Table::new(
                    rows,
                    [
                        Constraint::Length(6),
                        Constraint::Fill(1),
                        Constraint::Length(6),
                        Constraint::Length(8),
                        Constraint::Length(8),
                        Constraint::Length(8),
                    ],
                )
                .header(Row::new(vec![
                    Cell::from(Text::styled("ID", header_style)),
                    Cell::from(Text::styled("Command", header_style)),
                    Cell::from(Text::styled("Files", header_style)),
                    Cell::from(Text::styled("Size", header_style)),
                    Cell::from(Text::styled("Method", header_style)),
                    Cell::from(Text::styled("Status", header_style)),
                ]))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" oops · snapshot history · ↑↓/jk navigate · q quit "),
                );
                f.render_stateful_widget(t, f.area(), &mut state);
            })?;

            if let Event::Key(k) = event::read()? {
                match k.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Up | KeyCode::Char('k') => {
                        let sel = state.selected().unwrap_or(0);
                        state.select(Some(sel.saturating_sub(1)));
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let sel = state.selected().unwrap_or(0);
                        state.select(Some((sel + 1).min(items.len().saturating_sub(1))));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })
}

pub fn diff(data: DiffResult) -> io::Result<()> {
    term(|term| {
        term.draw(|f| {
            let text = data
                .files
                .iter()
                .map(|x| {
                    format!(
                        "{} {} ({}, {} bytes, {:o})",
                        if x.recoverable { "-" } else { "!" },
                        x.original_path,
                        x.op,
                        x.size_bytes,
                        x.mode
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let p =
                Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(format!(
                    " snapshot #{} · {} · q quit ",
                    data.snapshot.id,
                    truncated(&data.snapshot.command, 60),
                )));
            f.render_widget(p, f.area());
        })?;
        wait_q()
    })
}
