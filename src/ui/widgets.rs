use crate::app::TempoChange;
use crate::ui::Snapshot;
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
};
use std::time::Duration;

pub fn header(snap: &Snapshot) -> Paragraph<'static> {
    let lines = vec![
        kv("Tempo", format!("{:.2} BPM", snap.bpm), "Quantum", format!("{}", snap.quantum as u64)),
        kv("Beat", format!("{:.2}", snap.beat), "Phase", format!("{:.2} / {}", snap.phase, snap.quantum as u64)),
        kv("Playing", yes_no(snap.playing), "Peers", snap.peers.to_string()),
        kv("Uptime", fmt_duration(snap.uptime), "Link clock", format!("{} \u{00B5}s", snap.link_clock_micros)),
        kv(
            "Tempo \u{03C3}",
            format!("{:.2} BPM", snap.tempo_stability_bpm),
            "Last \u{0394}",
            last_delta(&snap.last_tempo_change),
        ),
    ];
    let title = format!(" LinkCLIHost \u{2014} peer: {} ", snap.peer_name);
    Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title))
}

pub fn phase_bar(snap: &Snapshot) -> Gauge<'static> {
    let ratio = if snap.quantum > 0.0 {
        (snap.phase / snap.quantum).clamp(0.0, 1.0)
    } else {
        0.0
    };
    Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" Phase "))
        .gauge_style(Style::default().fg(if snap.link_online { Color::Green } else { Color::DarkGray }))
        .ratio(ratio)
        .label(format!("{:.2}/{:.0}", snap.phase, snap.quantum))
}

pub fn history(changes: &[TempoChange]) -> List<'static> {
    let items: Vec<ListItem> = if changes.is_empty() {
        vec![ListItem::new("  (no tempo changes yet)")]
    } else {
        changes
            .iter()
            .map(|c| {
                let delta = c.delta();
                let arrow = if delta >= 0.0 { "+" } else { "" };
                let ts = c.at.format("%H:%M:%S");
                ListItem::new(Line::from(vec![Span::raw(format!(
                    "  {}  {:.2} \u{2192} {:.2}   \u{0394} {arrow}{:.2}",
                    ts, c.from_bpm, c.to_bpm, delta
                ))]))
            })
            .collect()
    };
    List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Recent tempo changes "),
    )
}

pub fn footer(snap: &Snapshot) -> Paragraph<'static> {
    let log = match &snap.log_path {
        Some(p) => format!("log: {p}"),
        None => "log: none".into(),
    };
    let line = Line::from(vec![
        Span::styled(" q ", Style::default().add_modifier(Modifier::REVERSED)),
        Span::raw(" quit"),
        Span::raw("   "),
        if snap.link_online {
            Span::styled("link: online", Style::default().fg(Color::Green))
        } else {
            Span::styled("link: offline", Style::default().fg(Color::Red))
        },
        Span::raw("   "),
        Span::raw(log),
    ]);
    Paragraph::new(line).block(Block::default().borders(Borders::ALL))
}

pub fn split(area: Rect) -> Vec<Rect> {
    Layout::vertical([
        Constraint::Length(7), // header (5 rows + borders)
        Constraint::Length(3), // phase bar
        Constraint::Min(4),    // history list
        Constraint::Length(3), // footer
    ])
    .split(area)
    .to_vec()
}

fn kv(k1: &str, v1: String, k2: &str, v2: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {k1:<10}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("{v1:<20}")),
        Span::styled(
            format!("{k2:<12}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(v2),
    ])
    .alignment(Alignment::Left)
}

fn yes_no(b: bool) -> String {
    if b {
        "yes".into()
    } else {
        "no".into()
    }
}

fn last_delta(change: &Option<TempoChange>) -> String {
    match change {
        Some(c) => {
            let d = c.delta();
            let sign = if d >= 0.0 { "+" } else { "" };
            format!("{sign}{d:.2} @ {}", c.at.format("%H:%M"))
        }
        None => "\u{2014}".into(),
    }
}

fn fmt_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    fn sample_snapshot() -> Snapshot {
        Snapshot {
            peer_name: "rig".into(),
            bpm: 120.00,
            beat: 142.37,
            phase: 2.37,
            quantum: 4.0,
            playing: true,
            peers: 3,
            link_clock_micros: 874_632_199,
            uptime: Duration::from_secs(862),
            last_tempo_change: Some(TempoChange {
                at: Utc.with_ymd_and_hms(2026, 5, 28, 12, 3, 17).unwrap(),
                from_bpm: 120.50,
                to_bpm: 120.00,
            }),
            recent_tempo_changes: vec![TempoChange {
                at: Utc.with_ymd_and_hms(2026, 5, 28, 12, 3, 17).unwrap(),
                from_bpm: 120.50,
                to_bpm: 120.00,
            }],
            tempo_stability_bpm: 0.12,
            log_path: Some("./events.jsonl".into()),
            link_online: true,
        }
    }

    fn render_to_buffer(snap: &Snapshot, width: u16, height: u16) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let chunks = split(f.area());
                f.render_widget(header(snap), chunks[0]);
                f.render_widget(phase_bar(snap), chunks[1]);
                f.render_widget(history(&snap.recent_tempo_changes), chunks[2]);
                f.render_widget(footer(snap), chunks[3]);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn buffer_contains(buf: &Buffer, needle: &str) -> bool {
        let mut joined = String::new();
        let area = buf.area;
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(""));
            }
            joined.push('\n');
        }
        joined.contains(needle)
    }

    #[test]
    fn header_renders_tempo_and_peer() {
        let buf = render_to_buffer(&sample_snapshot(), 70, 18);
        assert!(buffer_contains(&buf, "120.00"));
        assert!(buffer_contains(&buf, "rig"));
    }

    #[test]
    fn history_renders_tempo_change_row() {
        let buf = render_to_buffer(&sample_snapshot(), 70, 18);
        assert!(buffer_contains(&buf, "120.50"));
        assert!(buffer_contains(&buf, "120.00"));
    }

    #[test]
    fn empty_history_shows_placeholder() {
        let mut snap = sample_snapshot();
        snap.recent_tempo_changes.clear();
        let buf = render_to_buffer(&snap, 70, 18);
        assert!(buffer_contains(&buf, "no tempo changes yet"));
    }

    #[test]
    fn footer_shows_log_path() {
        let buf = render_to_buffer(&sample_snapshot(), 70, 18);
        assert!(buffer_contains(&buf, "events.jsonl"));
    }

    #[test]
    fn footer_shows_log_none() {
        let mut snap = sample_snapshot();
        snap.log_path = None;
        let buf = render_to_buffer(&snap, 70, 18);
        assert!(buffer_contains(&buf, "log: none"));
    }

    #[test]
    fn duration_format_pads_to_hhmmss() {
        assert_eq!(fmt_duration(Duration::from_secs(0)), "00:00:00");
        assert_eq!(fmt_duration(Duration::from_secs(65)), "00:01:05");
        assert_eq!(fmt_duration(Duration::from_secs(3661)), "01:01:01");
    }
}
