pub mod widgets;

use crate::app::TempoChange;
use ratatui::Frame;
use std::time::Duration;

pub const MIN_WIDTH: u16 = 50;
pub const MIN_HEIGHT: u16 = 18;

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub peer_name: String,
    pub bpm: f64,
    pub beat: f64,
    pub phase: f64,
    pub quantum: f64,
    pub playing: bool,
    pub peers: u64,
    pub link_clock_micros: i64,
    pub uptime: Duration,
    pub last_tempo_change: Option<TempoChange>,
    pub recent_tempo_changes: Vec<TempoChange>,
    pub tempo_stability_bpm: f64,
    pub log_path: Option<String>,
    pub link_online: bool,
}

pub fn draw(frame: &mut Frame, snap: &Snapshot) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        draw_too_small(frame, area);
        return;
    }
    let chunks = widgets::split(area);
    frame.render_widget(widgets::header(snap), chunks[0]);
    frame.render_widget(widgets::phase_bar(snap), chunks[1]);
    frame.render_widget(widgets::history(&snap.recent_tempo_changes), chunks[2]);
    frame.render_widget(widgets::footer(snap), chunks[3]);
}

fn draw_too_small(frame: &mut Frame, area: ratatui::layout::Rect) {
    use ratatui::widgets::Paragraph;
    let msg = format!(
        "terminal too small\nneed at least {MIN_WIDTH}x{MIN_HEIGHT}, got {}x{}",
        area.width, area.height
    );
    frame.render_widget(Paragraph::new(msg), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};
    use std::time::Duration;

    fn snap() -> Snapshot {
        Snapshot {
            peer_name: "rig".into(),
            bpm: 120.0,
            beat: 0.0,
            phase: 0.0,
            quantum: 4.0,
            playing: false,
            peers: 0,
            link_clock_micros: 0,
            uptime: Duration::ZERO,
            last_tempo_change: None,
            recent_tempo_changes: vec![],
            tempo_stability_bpm: 0.0,
            log_path: None,
            link_online: false,
        }
    }

    #[test]
    fn draw_with_full_size_does_not_panic() {
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
        t.draw(|f| draw(f, &snap())).unwrap();
    }

    #[test]
    fn draw_with_tiny_size_renders_too_small_notice() {
        let mut t = Terminal::new(TestBackend::new(20, 5)).unwrap();
        t.draw(|f| draw(f, &snap())).unwrap();
        let buf = t.backend().buffer().clone();
        let mut joined = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                joined.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(""));
            }
        }
        assert!(joined.contains("terminal too small"));
    }
}
