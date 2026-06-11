//! MIDI clock output locked to the Link session.
//!
//! A dedicated thread sends 0xF8 ticks at 24 PPQN, each scheduled from the
//! Link timeline via `time_at_beat`, so the clock follows session tempo
//! changes from any peer. Transport edges send Start (0xFA) / Stop (0xFC).
//! Ticks keep running while stopped (continuous-clock convention) so
//! receivers stay tempo-locked.
//!
//! Per-tick scheduling error against the Link timeline is recorded — that
//! jitter is the main stability signal this tool exists to measure.

use crate::events::Event;
use chrono::Utc;
use midir::{MidiOutput, MidiOutputConnection};
use rusty_link::{AblLink, SessionState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

const MIDI_TICK: u8 = 0xF8;
const MIDI_START: u8 = 0xFA;
const MIDI_STOP: u8 = 0xFC;
const PPQN: f64 = 24.0;
/// Emit a clock_stats log event roughly every this many ticks (10 s at 120 BPM).
const STATS_EVERY_TICKS: u64 = 480;

#[derive(Debug, Clone, Default)]
pub struct ClockStats {
    pub ticks: u64,
    pub last_err_us: i64,
    pub max_abs_err_us: i64,
    sum_abs_err_us: u64,
}

impl ClockStats {
    pub fn record(&mut self, err_us: i64) {
        self.ticks += 1;
        self.last_err_us = err_us;
        self.max_abs_err_us = self.max_abs_err_us.max(err_us.abs());
        self.sum_abs_err_us += err_us.unsigned_abs();
    }

    pub fn mean_abs_err_us(&self) -> f64 {
        if self.ticks == 0 {
            0.0
        } else {
            self.sum_abs_err_us as f64 / self.ticks as f64
        }
    }
}

#[derive(Debug, Clone)]
pub struct MidiClockSnapshot {
    pub port: String,
    pub ticks: u64,
    pub last_err_us: i64,
    pub mean_abs_err_us: f64,
    pub max_abs_err_us: i64,
}

pub struct MidiClock {
    port: String,
    stats: Arc<Mutex<ClockStats>>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl MidiClock {
    pub fn snapshot(&self) -> MidiClockSnapshot {
        let s = self.stats.lock().expect("clock stats poisoned");
        MidiClockSnapshot {
            port: self.port.clone(),
            ticks: s.ticks,
            last_err_us: s.last_err_us,
            mean_abs_err_us: s.mean_abs_err_us(),
            max_abs_err_us: s.max_abs_err_us,
        }
    }
}

impl Drop for MidiClock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

pub fn list_ports() -> Result<Vec<String>, String> {
    let out = MidiOutput::new("linkclihost").map_err(|e| format!("midi init: {e}"))?;
    Ok(out
        .ports()
        .iter()
        .map(|p| out.port_name(p).unwrap_or_else(|_| "<unknown>".into()))
        .collect())
}

/// Open the MIDI port selected by index or case-insensitive substring and
/// start the clock thread.
pub fn spawn(
    link: Arc<AblLink>,
    quantum: f64,
    selector: &str,
    events: Option<Sender<Event>>,
) -> Result<MidiClock, String> {
    let out = MidiOutput::new("linkclihost").map_err(|e| format!("midi init: {e}"))?;
    let ports = out.ports();
    if ports.is_empty() {
        return Err("no MIDI output ports found (try --list-midi-ports)".into());
    }

    let names: Vec<String> = ports
        .iter()
        .map(|p| out.port_name(p).unwrap_or_else(|_| "<unknown>".into()))
        .collect();
    let idx = match selector.parse::<usize>() {
        Ok(i) if i < ports.len() => i,
        Ok(i) => {
            return Err(format!(
                "MIDI port index {i} out of range (0..{})",
                ports.len() - 1
            ))
        }
        Err(_) => {
            let needle = selector.to_lowercase();
            names
                .iter()
                .position(|n| n.to_lowercase().contains(&needle))
                .ok_or_else(|| {
                    format!(
                        "no MIDI port matches \"{selector}\"; available: {}",
                        names.join(", ")
                    )
                })?
        }
    };
    let port_name = names[idx].clone();
    let conn = out
        .connect(&ports[idx], "linkclihost-clock")
        .map_err(|e| format!("midi connect: {e}"))?;

    let stats = Arc::new(Mutex::new(ClockStats::default()));
    let stop = Arc::new(AtomicBool::new(false));

    let t_stats = stats.clone();
    let t_stop = stop.clone();
    let t_port = port_name.clone();
    let join = std::thread::Builder::new()
        .name("linkclihost-midiclock".into())
        .spawn(move || run(link, quantum, conn, t_stats, t_stop, events, t_port))
        .map_err(|e| format!("spawn midi thread: {e}"))?;

    Ok(MidiClock {
        port: port_name,
        stats,
        stop,
        join: Some(join),
    })
}

fn run(
    link: Arc<AblLink>,
    quantum: f64,
    mut conn: MidiOutputConnection,
    stats: Arc<Mutex<ClockStats>>,
    stop: Arc<AtomicBool>,
    events: Option<Sender<Event>>,
    port: String,
) {
    let mut state = SessionState::new();
    let mut was_playing = false;
    let mut last_stats_emit: u64 = 0;

    while !stop.load(Ordering::SeqCst) {
        link.capture_app_session_state(&mut state);

        let playing = state.is_playing();
        if playing != was_playing {
            let _ = conn.send(&[if playing { MIDI_START } else { MIDI_STOP }]);
            was_playing = playing;
        }

        // Schedule the next 24-PPQN tick on the Link timeline. Re-capturing
        // the session state each iteration picks up tempo changes mid-wait.
        let now = link.clock_micros();
        let beat = state.beat_at_time(now, quantum);
        let next_tick = (beat * PPQN).floor() + 1.0;
        let target = state.time_at_beat(next_tick / PPQN, quantum);
        let wait = target - now;

        if wait > 2_500 {
            // Coarse sleep until just before the tick, then re-evaluate.
            std::thread::sleep(Duration::from_micros((wait - 2_000) as u64));
            continue;
        }
        // Fine wait: short sleeps until the deadline.
        while link.clock_micros() < target {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(Duration::from_micros(100));
        }

        if conn.send(&[MIDI_TICK]).is_ok() {
            let err = link.clock_micros() - target;
            let ticks = {
                let mut s = stats.lock().expect("clock stats poisoned");
                s.record(err);
                s.ticks
            };
            if let Some(tx) = &events {
                if ticks >= last_stats_emit + STATS_EVERY_TICKS {
                    last_stats_emit = ticks;
                    let s = stats.lock().expect("clock stats poisoned");
                    let _ = tx.send(Event::ClockStats {
                        at: Utc::now(),
                        port: port.clone(),
                        ticks: s.ticks,
                        mean_abs_err_us: s.mean_abs_err_us(),
                        max_abs_err_us: s.max_abs_err_us,
                    });
                }
            }
        }
    }
    // Leave receivers stopped rather than free-running.
    let _ = conn.send(&[MIDI_STOP]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_record_tracks_mean_and_max() {
        let mut s = ClockStats::default();
        s.record(100);
        s.record(-300);
        s.record(200);
        assert_eq!(s.ticks, 3);
        assert_eq!(s.last_err_us, 200);
        assert_eq!(s.max_abs_err_us, 300);
        assert!((s.mean_abs_err_us() - 200.0).abs() < 1e-9);
    }

    #[test]
    fn stats_empty_mean_is_zero() {
        let s = ClockStats::default();
        assert_eq!(s.mean_abs_err_us(), 0.0);
    }
}
