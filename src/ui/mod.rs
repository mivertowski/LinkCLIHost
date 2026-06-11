pub mod widgets;

use crate::app::TempoChange;
use crate::midi_clock::MidiClockSnapshot;
use crate::sequencer::{MAX_STEPS, TRACKS};
use ratatui::Frame;
use std::time::Duration;

pub const MIN_WIDTH: u16 = 50;
pub const MIN_HEIGHT: u16 = 26;

/// Sequencer state for display: pattern grid plus audio device info.
#[derive(Debug, Clone)]
pub struct SeqDisplay {
    pub preset_name: &'static str,
    pub pattern: [[bool; MAX_STEPS]; TRACKS],
    /// Sixteenths per bar; only the first `steps` columns of `pattern` are live.
    pub steps: usize,
    /// Step currently sounding, or -1 while transport is stopped.
    pub current_step: i64,
    pub muted: bool,
    pub device: String,
    pub sample_rate: u32,
    pub stream_errors: u64,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub peer_name: String,
    pub bpm: f64,
    pub beat: f64,
    pub phase: f64,
    pub quantum: f64,
    /// Time-signature label, e.g. "4/4", "7/8".
    pub meter_label: String,
    pub playing: bool,
    pub peers: u64,
    pub link_clock_micros: i64,
    pub uptime: Duration,
    pub last_tempo_change: Option<TempoChange>,
    pub recent_tempo_changes: Vec<TempoChange>,
    pub tempo_stability_bpm: f64,
    pub log_path: Option<String>,
    pub link_online: bool,
    pub seq: Option<SeqDisplay>,
    pub midi: Option<MidiClockSnapshot>,
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
    frame.render_widget(widgets::sequencer(snap), chunks[2]);
    frame.render_widget(widgets::history(&snap.recent_tempo_changes), chunks[3]);
    frame.render_widget(widgets::footer(snap), chunks[4]);
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
            meter_label: "4/4".into(),
            playing: false,
            peers: 0,
            link_clock_micros: 0,
            uptime: Duration::ZERO,
            last_tempo_change: None,
            recent_tempo_changes: vec![],
            tempo_stability_bpm: 0.0,
            log_path: None,
            link_online: false,
            seq: None,
            midi: None,
        }
    }

    #[test]
    fn draw_with_full_size_does_not_panic() {
        let mut t = Terminal::new(TestBackend::new(80, 32)).unwrap();
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
