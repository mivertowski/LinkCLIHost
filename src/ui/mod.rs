pub mod widgets;

use crate::app::TempoChange;
use std::time::Duration;

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
