use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    SessionStart {
        at: DateTime<Utc>,
        quantum: f64,
        peer_name: String,
    },
    TempoChanged {
        at: DateTime<Utc>,
        from_bpm: f64,
        to_bpm: f64,
    },
    PeersChanged {
        at: DateTime<Utc>,
        count: u64,
    },
    TransportChanged {
        at: DateTime<Utc>,
        playing: bool,
    },
    /// Periodic MIDI clock jitter report (scheduling error vs the Link timeline).
    ClockStats {
        at: DateTime<Utc>,
        port: String,
        ticks: u64,
        mean_abs_err_us: f64,
        max_abs_err_us: i64,
    },
    SessionEnd {
        at: DateTime<Utc>,
        reason: String,
    },
}

impl Event {
    pub fn at(&self) -> DateTime<Utc> {
        match self {
            Event::SessionStart { at, .. }
            | Event::TempoChanged { at, .. }
            | Event::PeersChanged { at, .. }
            | Event::TransportChanged { at, .. }
            | Event::ClockStats { at, .. }
            | Event::SessionEnd { at, .. } => *at,
        }
    }

    pub fn to_jsonl(&self) -> serde_json::Result<String> {
        let mut s = serde_json::to_string(self)?;
        s.push('\n');
        Ok(s)
    }
}

/// Fixed column order for CSV output:
/// timestamp_utc, event, bpm_from, bpm_to, peer_count, playing, note
pub const CSV_HEADERS: [&str; 7] = [
    "timestamp_utc",
    "event",
    "bpm_from",
    "bpm_to",
    "peer_count",
    "playing",
    "note",
];

impl Event {
    pub fn to_csv_row(&self) -> [String; 7] {
        let ts = |at: &DateTime<Utc>| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        match self {
            Event::SessionStart {
                at,
                quantum,
                peer_name,
            } => [
                ts(at),
                "session_start".into(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                format!("quantum={quantum} peer={peer_name}"),
            ],
            Event::TempoChanged {
                at,
                from_bpm,
                to_bpm,
            } => [
                ts(at),
                "tempo_changed".into(),
                from_bpm.to_string(),
                to_bpm.to_string(),
                String::new(),
                String::new(),
                String::new(),
            ],
            Event::PeersChanged { at, count } => [
                ts(at),
                "peers_changed".into(),
                String::new(),
                String::new(),
                count.to_string(),
                String::new(),
                String::new(),
            ],
            Event::TransportChanged { at, playing } => [
                ts(at),
                "transport_changed".into(),
                String::new(),
                String::new(),
                String::new(),
                playing.to_string(),
                String::new(),
            ],
            Event::ClockStats {
                at,
                port,
                ticks,
                mean_abs_err_us,
                max_abs_err_us,
            } => [
                ts(at),
                "clock_stats".into(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                format!(
                    "port={port} ticks={ticks} mean_abs_us={mean_abs_err_us:.1} max_abs_us={max_abs_err_us}"
                ),
            ],
            Event::SessionEnd { at, reason } => [
                ts(at),
                "session_end".into(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                reason.clone(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 28, 12, 3, 14).unwrap()
    }

    #[test]
    fn session_start_jsonl() {
        let e = Event::SessionStart {
            at: t(),
            quantum: 4.0,
            peer_name: "rig".into(),
        };
        let line = e.to_jsonl().unwrap();
        assert!(line.ends_with('\n'));
        let v: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v["type"], "session_start");
        assert_eq!(v["peer_name"], "rig");
        assert_eq!(v["quantum"], 4.0);
    }

    #[test]
    fn tempo_changed_jsonl() {
        let e = Event::TempoChanged {
            at: t(),
            from_bpm: 120.0,
            to_bpm: 119.5,
        };
        let v: serde_json::Value = serde_json::from_str(e.to_jsonl().unwrap().trim()).unwrap();
        assert_eq!(v["type"], "tempo_changed");
        assert_eq!(v["from_bpm"], 120.0);
        assert_eq!(v["to_bpm"], 119.5);
    }

    #[test]
    fn peers_changed_jsonl() {
        let e = Event::PeersChanged { at: t(), count: 3 };
        let v: serde_json::Value = serde_json::from_str(e.to_jsonl().unwrap().trim()).unwrap();
        assert_eq!(v["type"], "peers_changed");
        assert_eq!(v["count"], 3);
    }

    #[test]
    fn transport_changed_jsonl() {
        let e = Event::TransportChanged {
            at: t(),
            playing: true,
        };
        let v: serde_json::Value = serde_json::from_str(e.to_jsonl().unwrap().trim()).unwrap();
        assert_eq!(v["type"], "transport_changed");
        assert_eq!(v["playing"], true);
    }

    #[test]
    fn session_end_jsonl() {
        let e = Event::SessionEnd {
            at: t(),
            reason: "sigint".into(),
        };
        let v: serde_json::Value = serde_json::from_str(e.to_jsonl().unwrap().trim()).unwrap();
        assert_eq!(v["type"], "session_end");
        assert_eq!(v["reason"], "sigint");
    }

    #[test]
    fn clock_stats_jsonl() {
        let e = Event::ClockStats {
            at: t(),
            port: "Midi Through".into(),
            ticks: 480,
            mean_abs_err_us: 42.5,
            max_abs_err_us: 310,
        };
        let v: serde_json::Value = serde_json::from_str(e.to_jsonl().unwrap().trim()).unwrap();
        assert_eq!(v["type"], "clock_stats");
        assert_eq!(v["port"], "Midi Through");
        assert_eq!(v["ticks"], 480);
        assert_eq!(v["max_abs_err_us"], 310);
    }

    #[test]
    fn clock_stats_csv_row_carries_metrics_in_note() {
        let r = Event::ClockStats {
            at: t(),
            port: "Midi Through".into(),
            ticks: 480,
            mean_abs_err_us: 42.5,
            max_abs_err_us: 310,
        }
        .to_csv_row();
        assert_eq!(r[1], "clock_stats");
        assert!(r[6].contains("port=Midi Through"));
        assert!(r[6].contains("ticks=480"));
        assert!(r[6].contains("mean_abs_us=42.5"));
        assert!(r[6].contains("max_abs_us=310"));
    }

    #[test]
    fn at_accessor() {
        let e = Event::PeersChanged { at: t(), count: 1 };
        assert_eq!(e.at(), t());
    }

    #[test]
    fn csv_headers_have_seven_columns() {
        assert_eq!(CSV_HEADERS.len(), 7);
    }

    #[test]
    fn tempo_changed_csv_row() {
        let r = Event::TempoChanged {
            at: t(),
            from_bpm: 120.0,
            to_bpm: 119.5,
        }
        .to_csv_row();
        assert_eq!(r[1], "tempo_changed");
        assert_eq!(r[2], "120");
        assert_eq!(r[3], "119.5");
        assert!(r[4].is_empty());
        assert!(r[5].is_empty());
    }

    #[test]
    fn session_start_csv_row_carries_metadata_in_note() {
        let r = Event::SessionStart {
            at: t(),
            quantum: 4.0,
            peer_name: "rig".into(),
        }
        .to_csv_row();
        assert_eq!(r[1], "session_start");
        assert!(r[6].contains("quantum=4"));
        assert!(r[6].contains("peer=rig"));
    }

    #[test]
    fn transport_changed_csv_row_records_playing() {
        let r = Event::TransportChanged {
            at: t(),
            playing: true,
        }
        .to_csv_row();
        assert_eq!(r[5], "true");
    }
}
