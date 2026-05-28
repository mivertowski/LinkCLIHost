use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub const HISTORY_CAP: usize = 128;
pub const HISTORY_DISPLAY: usize = 5;

#[derive(Debug, Clone, PartialEq)]
pub struct TempoChange {
    pub at: DateTime<Utc>,
    pub from_bpm: f64,
    pub to_bpm: f64,
}

impl TempoChange {
    pub fn delta(&self) -> f64 {
        self.to_bpm - self.from_bpm
    }
}

pub struct AppState {
    pub current_bpm: f64,
    pub last_tempo_change: Option<TempoChange>,
    pub tempo_history: VecDeque<TempoChange>,
    pub peers: u64,
    pub playing: bool,
    pub started_at: Instant,
    pub last_event_at: Option<Instant>,
}

impl AppState {
    pub fn new(initial_bpm: f64) -> Self {
        Self {
            current_bpm: initial_bpm,
            last_tempo_change: None,
            tempo_history: VecDeque::with_capacity(HISTORY_CAP),
            peers: 0,
            playing: false,
            started_at: Instant::now(),
            last_event_at: None,
        }
    }

    pub fn record_tempo_change(&mut self, new_bpm: f64, at: DateTime<Utc>) -> Option<TempoChange> {
        if (new_bpm - self.current_bpm).abs() < f64::EPSILON {
            return None;
        }
        let change = TempoChange {
            at,
            from_bpm: self.current_bpm,
            to_bpm: new_bpm,
        };
        self.current_bpm = new_bpm;
        self.last_tempo_change = Some(change.clone());
        self.push_history(change.clone());
        self.last_event_at = Some(Instant::now());
        Some(change)
    }

    pub fn set_peers(&mut self, peers: u64) {
        self.peers = peers;
        self.last_event_at = Some(Instant::now());
    }

    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        self.last_event_at = Some(Instant::now());
    }

    pub fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub fn recent_changes(&self) -> Vec<TempoChange> {
        let len = self.tempo_history.len();
        let take = HISTORY_DISPLAY.min(len);
        self.tempo_history
            .iter()
            .rev()
            .take(take)
            .cloned()
            .collect()
    }

    pub fn tempo_stability_bpm(&self) -> f64 {
        let values: Vec<f64> = self.tempo_history.iter().map(|c| c.to_bpm).collect();
        if values.len() < 2 {
            return 0.0;
        }
        let n = values.len() as f64;
        let mean = values.iter().sum::<f64>() / n;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
        variance.sqrt()
    }

    fn push_history(&mut self, change: TempoChange) {
        if self.tempo_history.len() == HISTORY_CAP {
            self.tempo_history.pop_front();
        }
        self.tempo_history.push_back(change);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 28, 12, min, 0).unwrap()
    }

    #[test]
    fn first_tempo_change_records_from_initial() {
        let mut s = AppState::new(120.0);
        let change = s.record_tempo_change(124.0, ts(0)).unwrap();
        assert_eq!(change.from_bpm, 120.0);
        assert_eq!(change.to_bpm, 124.0);
        assert_eq!(s.current_bpm, 124.0);
        assert_eq!(s.tempo_history.len(), 1);
    }

    #[test]
    fn identical_tempo_is_not_recorded() {
        let mut s = AppState::new(120.0);
        assert!(s.record_tempo_change(120.0, ts(0)).is_none());
        assert!(s.tempo_history.is_empty());
    }

    #[test]
    fn delta_matches_to_minus_from() {
        let c = TempoChange {
            at: ts(0),
            from_bpm: 120.0,
            to_bpm: 119.5,
        };
        assert!((c.delta() - -0.5).abs() < 1e-9);
    }

    #[test]
    fn history_buffer_evicts_oldest_past_cap() {
        let mut s = AppState::new(0.0);
        for i in 1..=(HISTORY_CAP + 3) {
            s.record_tempo_change(i as f64, ts(0));
        }
        assert_eq!(s.tempo_history.len(), HISTORY_CAP);
        assert_eq!(s.tempo_history.front().unwrap().to_bpm, 4.0);
        assert_eq!(
            s.tempo_history.back().unwrap().to_bpm,
            (HISTORY_CAP + 3) as f64
        );
    }

    #[test]
    fn recent_changes_returns_newest_first_capped() {
        let mut s = AppState::new(0.0);
        for i in 1..=10 {
            s.record_tempo_change(i as f64, ts(0));
        }
        let r = s.recent_changes();
        assert_eq!(r.len(), HISTORY_DISPLAY);
        assert_eq!(r[0].to_bpm, 10.0);
        assert_eq!(r[1].to_bpm, 9.0);
    }

    #[test]
    fn stability_is_zero_for_empty_history() {
        let s = AppState::new(120.0);
        assert_eq!(s.tempo_stability_bpm(), 0.0);
    }

    #[test]
    fn stability_is_zero_for_single_entry() {
        let mut s = AppState::new(120.0);
        s.record_tempo_change(121.0, ts(0));
        assert_eq!(s.tempo_stability_bpm(), 0.0);
    }

    #[test]
    fn stability_matches_population_stddev() {
        let mut s = AppState::new(0.0);
        s.record_tempo_change(100.0, ts(0));
        s.record_tempo_change(110.0, ts(1));
        s.record_tempo_change(120.0, ts(2));
        assert!((s.tempo_stability_bpm() - 8.164_965_809).abs() < 1e-6);
    }

    #[test]
    fn peers_and_playing_setters_update_state() {
        let mut s = AppState::new(120.0);
        s.set_peers(3);
        s.set_playing(true);
        assert_eq!(s.peers, 3);
        assert!(s.playing);
    }
}
