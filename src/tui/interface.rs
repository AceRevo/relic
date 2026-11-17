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
    widgets::{Block, Borders, Gauge, Paragraph, Row, Table, Tabs},
    Terminal,
};
use sysinfo::Disks;
use std::io;
use std::time::Duration;

pub async fn run_interactive_tui() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut current_tab: usize = 0;
    let mut selected_disk_idx: usize = 0;
    let mut disks = Disks::new_with_refreshed_list();
    let mut status_message = String::from("System armed. Select a block device to inspect or capture.");

    loop {
        terminal.draw(|f| {
            let area = f.area();
            let main_chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),  // Banner Header
                    Constraint::Length(3),  // Nav Tabs
                    Constraint::Min(10),   // Content View
                    Constraint::Length(3),  // Footer Controls
                ])
                .split(area);

            // Banner Header
            let header = Paragraph::new(Line::from(vec![
                Span::styled(" [ ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    "PROJECT RELIC",
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " // AUTONOMOUS BARE-METAL DISASTER RECOVERY ENGINE ]",
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    "  [ STATUS: READY ]",
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            );
            f.render_widget(header, main_chunks[0]);

            // Tabs
            let tab_titles = vec!["[1] DISK TOPOLOGY", "[2] REANIMATION HUD", "[3] ARCHITECTURE & SPECS"];
            let tabs = Tabs::new(tab_titles)
                .select(current_tab)
                .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)))
                .style(Style::default().fg(Color::Gray))
                .highlight_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                );
            f.render_widget(tabs, main_chunks[1]);

            // Tab Content
            match current_tab {
                0 => {
                    // Disk Topology Inspector
                    let rows: Vec<Row> = disks
                        .iter()
                        .enumerate()
                        .map(|(i, d)| {
                            let total_gb = d.total_space() as f64 / (1024.0 * 1024.0 * 1024.0);
                            let avail_gb = d.available_space() as f64 / (1024.0 * 1024.0 * 1024.0);
                            let style = if i == selected_disk_idx {
                                Style::default()
                                    .fg(Color::Yellow)
                                    .bg(Color::DarkGray)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(Color::White)
                            };

                            Row::new(vec![
                                format!("#{}", i),
                                format!("{:?}", d.name()),
                                format!("{:?}", d.mount_point()),
                                format!("{:?}", d.file_system()),
                                format!("{:.1} GB", total_gb),
                                format!("{:.1} GB", avail_gb),
                                format!("{}", if d.is_removable() { "YES" } else { "NO" }),
                            ])
                            .style(style)
                        })
                        .collect();

                    let table = Table::new(
                        rows,
                        [
                            Constraint::Length(4),
                            Constraint::Length(18),
                            Constraint::Length(18),
                            Constraint::Length(12),
                            Constraint::Length(12),
                            Constraint::Length(12),
                            Constraint::Length(10),
                        ],
                    )
                    .header(
                        Row::new(vec![
                            "ID",
                            "DEVICE NAME",
                            "MOUNT POINT",
                            "FILESYSTEM",
                            "CAPACITY",
                            "FREE SPACE",
                            "REMOVABLE",
                        ])
                        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    )
                    .block(
                        Block::default()
                            .title(" DETECTED PHYSICAL & LOGICAL BLOCK DEVICES [Use UP/DOWN to select] ")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Cyan)),
                    );
                    f.render_widget(table, main_chunks[2]);
                }
                1 => {
                    // Reanimation HUD
                    let hud_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(7),
                            Constraint::Length(5),
                            Constraint::Min(4),
                        ])
                        .split(main_chunks[2]);

                    let active_disk_name = if !disks.is_empty() {
                        format!("{:?}", disks[selected_disk_idx.min(disks.len() - 1)].name())
                    } else {
                        "NONE".to_string()
                    };

                    let telemetry_info = vec![
                        Line::from(vec![
                            Span::styled("ACTIVE TARGET / SOURCE:  ", Style::default().fg(Color::Cyan)),
                            Span::styled(active_disk_name, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        ]),
                        Line::from(vec![
                            Span::styled("COMPRESSION PROTOCOL:    ", Style::default().fg(Color::Cyan)),
                            Span::styled("Zstandard LZ77 Multi-Threaded (4-64 MB Sparse-Aware)", Style::default().fg(Color::White)),
                        ]),
                        Line::from(vec![
                            Span::styled("CRYPTOGRAPHIC VERIFIER:  ", Style::default().fg(Color::Cyan)),
                            Span::styled("BLAKE3 Tree Hashing (AVX-512 / NEON Hardware Accelerated)", Style::default().fg(Color::White)),
                        ]),
                        Line::from(vec![
                            Span::styled("SYSTEM BUS INTERFACE:    ", Style::default().fg(Color::Cyan)),
                            Span::styled("USB4 40 Gbps / NVMe PCIe Gen4 Direct DMA", Style::default().fg(Color::Green)),
                        ]),
                    ];

                    let p = Paragraph::new(telemetry_info).block(
                        Block::default()
                            .title(" TELEMETRY & HARDWARE STATE ")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Yellow)),
                    );
                    f.render_widget(p, hud_chunks[0]);

                    let gauge = Gauge::default()
                        .block(
                            Block::default()
                                .title(" LIVE STREAM PROGRESS ")
                                .borders(Borders::ALL)
                                .border_style(Style::default().fg(Color::Green)),
                        )
                        .gauge_style(
                            Style::default()
                                .fg(Color::Yellow)
                                .bg(Color::Black)
                                .add_modifier(Modifier::BOLD),
                        )
                        .percent(100);
                    f.render_widget(gauge, hud_chunks[1]);

                    let status_p = Paragraph::new(vec![
                        Line::from(vec![
                            Span::styled("CONSOLE: ", Style::default().fg(Color::Cyan)),
                            Span::styled(&status_message, Style::default().fg(Color::LightYellow)),
                        ]),
                    ])
                    .block(
                        Block::default()
                            .title(" OPERATION LOG ")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::DarkGray)),
                    );
                    f.render_widget(status_p, hud_chunks[2]);
                }
                _ => {
                    // Architecture & Specs
                    let arch_info = vec![
                        Line::from(Span::styled("PROJECT RELIC SPECIFICATIONS", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
                        Line::from(""),
                        Line::from("- Phase 1: Host Flatline condition detected (BSOD loop, ransomware, bootloader failure)."),
                        Line::from("- Phase 2: High-speed NVMe USB4 enclosure connected (<2s UEFI boot into RAM)."),
                        Line::from("- Phase 3: Block streaming at raw sector layer bypassing file system and inode traversal."),
                        Line::from("- Phase 4: BLAKE3 cryptographic verification ensures zero corruption bitrot tolerance."),
                        Line::from(""),
                        Line::from(Span::styled("Hardware Bill of Materials (BOM):", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
                        Line::from("  * Samsung 990 Pro / Crucial T500 (M.2 2280 NVMe PCIe 4.0 x4)"),
                        Line::from("  * Acasis TBU405 / Yottamaster ASM2464PD 40Gbps enclosure"),
                    ];
                    let arch_p = Paragraph::new(arch_info).block(
                        Block::default()
                            .title(" ARCHITECTURE & HARDWARE SPECIFICATIONS ")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Cyan)),
                    );
                    f.render_widget(arch_p, main_chunks[2]);
                }
            }

            // Footer
            let footer = Paragraph::new(
                " [TAB] Switch View | [UP/DOWN] Select Disk | [R] Refresh Storage | [Q/ESC] Exit",
            )
            .style(Style::default().fg(Color::Gray))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
            f.render_widget(footer, main_chunks[3]);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Tab => {
                        current_tab = (current_tab + 1) % 3;
                    }
                    KeyCode::Char('1') => current_tab = 0,
                    KeyCode::Char('2') => current_tab = 1,
                    KeyCode::Char('3') => current_tab = 2,
                    KeyCode::Up => {
                        if selected_disk_idx > 0 {
                            selected_disk_idx -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if !disks.is_empty() && selected_disk_idx < disks.len() - 1 {
                            selected_disk_idx += 1;
                        }
                    }
                    KeyCode::Char('r') => {
                        disks = Disks::new_with_refreshed_list();
                        status_message = format!("Storage devices rescanned. Total detected: {}", disks.len());
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    println!("\n[+] TUI session closed safely.");
    Ok(())
}
