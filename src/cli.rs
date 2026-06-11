use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone, PartialEq)]
#[command(name = "linkclihost", version, about = "Ableton Link monitor")]
pub struct Cli {
    /// Beats per cycle (musical bar).
    #[arg(short = 'q', long, default_value_t = 4.0)]
    pub quantum: f64,

    /// Peer display name in the header (defaults to hostname).
    #[arg(short = 'n', long)]
    pub name: Option<String>,

    /// Append events to file (.jsonl / .ndjson / .csv).
    #[arg(long)]
    pub log: Option<PathBuf>,

    /// Disable TUI; print events to stdout instead.
    #[arg(long)]
    pub no_tui: bool,

    /// Initial tempo if we're the first peer in the session.
    #[arg(long, default_value_t = 120.0)]
    pub initial_bpm: f64,

    /// Send MIDI clock (24 PPQN) to this output port (index or name substring).
    #[arg(long, value_name = "PORT")]
    pub midi_out: Option<String>,

    /// List available MIDI output ports and exit.
    #[arg(long)]
    pub list_midi_ports: bool,

    /// Enable the drum sequencer on the default audio output device.
    #[arg(long)]
    pub audio: bool,

    /// Enable the drum sequencer on this audio output device (name substring).
    #[arg(long, value_name = "DEVICE")]
    pub audio_out: Option<String>,

    /// List available audio output devices and exit.
    #[arg(long)]
    pub list_audio_devices: bool,

    /// Sequencer pattern preset.
    #[arg(long, default_value = "four-floor")]
    pub preset: String,

    /// Sequencer output gain (0.0 - 1.0).
    #[arg(long, default_value_t = 0.8)]
    pub gain: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Jsonl,
    Csv,
}

impl Cli {
    pub fn parse_from_env() -> Self {
        Self::parse()
    }

    pub fn log_format(&self) -> Option<Result<LogFormat, String>> {
        self.log.as_ref().map(|p| {
            let ext = p
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase());
            match ext.as_deref() {
                Some("jsonl") | Some("ndjson") => Ok(LogFormat::Jsonl),
                Some("csv") => Ok(LogFormat::Csv),
                Some(other) => Err(format!("unsupported log extension: .{other}")),
                None => Err("log path needs an extension (.jsonl, .ndjson, or .csv)".into()),
            }
        })
    }

    pub fn audio_enabled(&self) -> bool {
        self.audio || self.audio_out.is_some()
    }

    pub fn preset_idx(&self) -> Result<usize, String> {
        crate::sequencer::preset_index(&self.preset).ok_or_else(|| {
            format!(
                "unknown preset \"{}\"; available: {}",
                self.preset,
                crate::sequencer::preset_names().join(", ")
            )
        })
    }

    pub fn resolved_name(&self) -> String {
        self.name.clone().unwrap_or_else(|| {
            gethostname::gethostname()
                .into_string()
                .unwrap_or_else(|_| "linkclihost".into())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn parse(args: &[&str]) -> Cli {
        let mut v = vec!["linkclihost"];
        v.extend_from_slice(args);
        Cli::try_parse_from(v).expect("parse")
    }

    #[test]
    fn defaults() {
        let c = parse(&[]);
        assert_eq!(c.quantum, 4.0);
        assert_eq!(c.initial_bpm, 120.0);
        assert!(c.name.is_none());
        assert!(c.log.is_none());
        assert!(!c.no_tui);
    }

    #[test]
    fn overrides() {
        let c = parse(&["--quantum", "8", "--name", "foo", "--no-tui"]);
        assert_eq!(c.quantum, 8.0);
        assert_eq!(c.name.as_deref(), Some("foo"));
        assert!(c.no_tui);
    }

    #[test]
    fn audio_defaults_off() {
        let c = parse(&[]);
        assert!(!c.audio_enabled());
        assert!(c.midi_out.is_none());
        assert_eq!(c.preset, "four-floor");
        assert_eq!(c.gain, 0.8);
    }

    #[test]
    fn audio_out_implies_audio_enabled() {
        let c = parse(&["--audio-out", "pulse"]);
        assert!(c.audio_enabled());
        assert_eq!(c.audio_out.as_deref(), Some("pulse"));
    }

    #[test]
    fn audio_flag_enables_default_device() {
        let c = parse(&["--audio"]);
        assert!(c.audio_enabled());
        assert!(c.audio_out.is_none());
    }

    #[test]
    fn known_preset_resolves() {
        let c = parse(&["--preset", "breaks"]);
        assert_eq!(c.preset_idx(), Ok(2));
    }

    #[test]
    fn unknown_preset_is_error() {
        let c = parse(&["--preset", "polka"]);
        let err = c.preset_idx().unwrap_err();
        assert!(err.contains("polka"));
        assert!(err.contains("four-floor"));
    }

    #[test]
    fn midi_out_parses() {
        let c = parse(&["--midi-out", "Midi Through"]);
        assert_eq!(c.midi_out.as_deref(), Some("Midi Through"));
    }

    #[test]
    fn jsonl_log_format() {
        let c = parse(&["--log", "events.jsonl"]);
        assert_eq!(c.log, Some(PathBuf::from("events.jsonl")));
        assert!(matches!(c.log_format(), Some(Ok(LogFormat::Jsonl))));
    }

    #[test]
    fn ndjson_is_jsonl() {
        let c = parse(&["--log", "x.ndjson"]);
        assert!(matches!(c.log_format(), Some(Ok(LogFormat::Jsonl))));
    }

    #[test]
    fn csv_log_format() {
        let c = parse(&["--log", "events.csv"]);
        assert!(matches!(c.log_format(), Some(Ok(LogFormat::Csv))));
    }

    #[test]
    fn unsupported_extension_is_error() {
        let c = parse(&["--log", "events.txt"]);
        assert!(matches!(c.log_format(), Some(Err(_))));
    }

    #[test]
    fn no_log_yields_none() {
        let c = parse(&[]);
        assert!(c.log_format().is_none());
    }

    #[test]
    fn resolved_name_uses_override() {
        let c = parse(&["--name", "studio-rig"]);
        assert_eq!(c.resolved_name(), "studio-rig");
    }
}
