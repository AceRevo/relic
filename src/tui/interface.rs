use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Terminal,
};
use std::io;
use std::time::Duration;

pub async fn run_interactive_tui() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut progress: u16 = 0;

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints([
                    Constraint::Length(4),
                    Constraint::Length(8),
                    Constraint::Length(5),
                    Constraint::Min(4),
                ])
                .split(f.area());

            // Header Banner
            let header = Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("[ ", Style::default().fg(Color::Cyan)),
                    Span::styled("PROJECT RELIC", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::styled(" // BARE-METAL SYSTEM RECOVERY ENGINE ]", Style::default().fg(Color::Cyan)),
                ]),
                Line::from(Span::styled(
                    "STATUS: ARMED // HOST LINK ESTABLISHED // NVMe STORAGE READY",
                    Style::default().fg(Color::DarkGray),
                )),
            ])
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
            f.render_widget(header, chunks[0]);

            // Operational telemetry
            let telemetry = Paragraph::new(vec![
                Line::from("TARGET DISK:    /dev/nvme0n1 [Samsung 990 PRO 2TB]"),
                Line::from("BACKUP CONTAINER: RELIC_SNAPSHOT_T0.zst (84.2 GB)"),
                Line::from("ACTIVE STREAM:  Zstandard LZ77 (4-threads, Block size: 4MB)"),
                Line::from("CHECKSUM:       BLAKE3 4a8f9c1... (Hardware Accelerated)"),
                Line::from("ESTIMATED TIME: 38 seconds remaining @ 2.4 GB/s"),
            ])
            .block(
                Block::default()
                    .title(" TELEMETRY & SYSTEM STATE ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            );
            f.render_widget(telemetry, chunks[1]);

            // Progress Gauge
            let gauge = Gauge::default()
                .block(
                    Block::default()
                        .title(" RESTORATION PROGRESS ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Green)),
                )
                .gauge_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .bg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
                .percent(progress);
            f.render_widget(gauge, chunks[2]);

            // Controls
            let footer = Paragraph::new("Press [Q] or [Esc] to exit | [SPACE] to pause/resume | [R] to re-scan hardware")
                .style(Style::default().fg(Color::Gray));
            f.render_widget(footer, chunks[3]);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') => progress = 0,
                    _ => {}
                }
            }
        }

        if progress < 100 {
            progress += 1;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    println!("\n[+] TUI session closed safely.");
    Ok(())
}
