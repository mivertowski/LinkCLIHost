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
