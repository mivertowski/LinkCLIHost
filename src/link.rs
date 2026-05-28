use crate::app::AppState;
use crate::events::Event;
use chrono::Utc;
use rusty_link::{AblLink, SessionState};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub struct LinkSession {
    link: AblLink,
    state: SessionState,
    quantum: f64,
}

impl LinkSession {
    pub fn new(
        initial_bpm: f64,
        quantum: f64,
        shared: Arc<Mutex<AppState>>,
        events: Option<Sender<Event>>,
    ) -> Self {
        let link = AblLink::new(initial_bpm);

        let s_tempo = shared.clone();
        let e_tempo = events.clone();
        link.set_tempo_callback(move |bpm: f64| {
            let now = Utc::now();
            let recorded = {
                let mut st = s_tempo.lock().expect("app state poisoned");
                st.record_tempo_change(bpm, now)
            };
            if let (Some(c), Some(tx)) = (recorded, e_tempo.as_ref()) {
                let _ = tx.send(Event::TempoChanged {
                    at: c.at,
                    from_bpm: c.from_bpm,
                    to_bpm: c.to_bpm,
                });
            }
        });

        let s_peers = shared.clone();
        let e_peers = events.clone();
        link.set_num_peers_callback(move |count: u64| {
            let now = Utc::now();
            {
                let mut st = s_peers.lock().expect("app state poisoned");
                st.set_peers(count);
            }
            if let Some(tx) = &e_peers {
                let _ = tx.send(Event::PeersChanged { at: now, count });
            }
        });

        let s_play = shared.clone();
        let e_play = events.clone();
        link.set_start_stop_callback(move |playing: bool| {
            let now = Utc::now();
            {
                let mut st = s_play.lock().expect("app state poisoned");
                st.set_playing(playing);
            }
            if let Some(tx) = &e_play {
                let _ = tx.send(Event::TransportChanged { at: now, playing });
            }
        });

        link.enable_start_stop_sync(true);
        link.enable(true);

        Self {
            link,
            state: SessionState::new(),
            quantum,
        }
    }

    pub fn snapshot(&mut self) -> LinkSnapshot {
        self.link.capture_app_session_state(&mut self.state);
        let micros = self.link.clock_micros();
        LinkSnapshot {
            bpm: self.state.tempo(),
            beat: self.state.beat_at_time(micros, self.quantum),
            phase: self.state.phase_at_time(micros, self.quantum),
            playing: self.state.is_playing(),
            clock_micros: micros,
        }
    }

    pub fn online(&self) -> bool {
        true
    }
}

impl Drop for LinkSession {
    fn drop(&mut self) {
        self.link.enable(false);
    }
}

pub struct LinkSnapshot {
    pub bpm: f64,
    pub beat: f64,
    pub phase: f64,
    pub playing: bool,
    pub clock_micros: i64,
}
