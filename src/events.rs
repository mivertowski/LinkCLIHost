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
            | Event::SessionEnd { at, .. } => *at,
        }
    }

    pub fn to_jsonl(&self) -> serde_json::Result<String> {
        let mut s = serde_json::to_string(self)?;
        s.push('\n');
        Ok(s)
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
    fn at_accessor() {
        let e = Event::PeersChanged { at: t(), count: 1 };
        assert_eq!(e.at(), t());
    }
}
