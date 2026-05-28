# LinkCLIHost Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Linux CLI that joins an Ableton Link session as a passive peer and renders a live TUI dashboard of tempo, beat/phase, peers, and recent tempo changes, with optional JSONL or CSV event logging.

**Architecture:** Single Rust binary. Three threads: `main` runs a 30 Hz ratatui render loop and reads keyboard input; the internal Link thread fires our `tempo`/`peers`/`start_stop` closures, which lock an `Arc<Mutex<AppState>>` and push an `Event` onto an `mpsc` channel; an optional logger thread drains that channel into a file. No async runtime.

**Tech Stack:** Rust 2021, `rusty_link` 0.4 (wraps Ableton's official C++ Link), `ratatui` 0.29 with bundled crossterm, `clap` 4 (derive), `serde`/`serde_json`/`csv`, `chrono`, `ctrlc`, `gethostname`. Build prereqs on Ubuntu 24.04: `build-essential cmake pkg-config libclang-dev`.

**Reference spec:** `docs/superpowers/specs/2026-05-28-link-cli-host-design.md`

---

## Prerequisites (do once, not part of any task)

```bash
sudo apt install build-essential cmake pkg-config libclang-dev
```

Verify Rust:

```bash
rustc --version   # expect 1.85+
cargo --version
```

---

## Task 1: Bootstrap Cargo project

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `.gitignore`

- [ ] **Step 1.1: Initialize the Cargo manifest**

Create `Cargo.toml`:

```toml
[package]
name = "linkclihost"
version = "0.1.0"
edition = "2021"
rust-version = "1.85"
license = "Apache-2.0"
description = "Headless Ableton Link peer with live TUI dashboard"
repository = "https://github.com/mivertowski/LinkCLIHost"

[dependencies]
rusty_link = "0.4"
ratatui = "0.29"
clap = { version = "4.5", features = ["derive"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
csv = "1.3"
chrono = { version = "0.4", features = ["serde"] }
ctrlc = { version = "3.4", features = ["termination"] }
gethostname = "0.4"

[dev-dependencies]
tempfile = "3"

[profile.release]
lto = "thin"
codegen-units = 1
strip = "debuginfo"
```

- [ ] **Step 1.2: Create minimal `src/main.rs`**

```rust
fn main() {
    println!("linkclihost starting up");
}
```

- [ ] **Step 1.3: Create `.gitignore`**

```
/target
*.swp
.DS_Store
```

- [ ] **Step 1.4: First build (fetches and compiles Ableton Link — slow on first run)**

```bash
cargo build
```

Expected: succeeds. First build pulls `rusty_link` which then `cmake`s the bundled Link C++ — can take 1–3 minutes. Run `./target/debug/linkclihost` to confirm it prints "linkclihost starting up".

- [ ] **Step 1.5: Verify tests run (none yet, but the harness must be live)**

```bash
cargo test
```

Expected: `0 passed; 0 failed`.

- [ ] **Step 1.6: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs .gitignore
git commit -m "chore: bootstrap Cargo project with dependencies"
```

---

## Task 2: CLI parsing (`src/cli.rs`)

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`

- [ ] **Step 2.1: Add the module declaration and a smoke call in `src/main.rs`**

Replace `src/main.rs` with:

```rust
mod cli;

fn main() {
    let args = cli::Cli::parse_from_env();
    println!("{:?}", args);
}
```

- [ ] **Step 2.2: Write the failing test in `src/cli.rs`**

```rust
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
```

- [ ] **Step 2.3: Run tests, verify they pass**

```bash
cargo test --lib cli
```

Expected: 8 tests pass. (We wrote the implementation alongside the tests because clap derive is mostly declarative — running them once confirms the schema and the helper logic.)

- [ ] **Step 2.4: Smoke-check the binary**

```bash
cargo run -- --help
```

Expected: clap prints the help with all the flags listed.

- [ ] **Step 2.5: Commit**

```bash
git add Cargo.toml src/cli.rs src/main.rs
git commit -m "feat: add clap-based CLI parser"
```

---

## Task 3: Event types and JSONL serialization (`src/events.rs`)

**Files:**
- Create: `src/events.rs`
- Modify: `src/main.rs` (add module decl only)

- [ ] **Step 3.1: Add `mod events;` to `src/main.rs`**

```rust
mod cli;
mod events;

fn main() {
    let args = cli::Cli::parse_from_env();
    println!("{:?}", args);
}
```

- [ ] **Step 3.2: Write the events module with tests**

Create `src/events.rs`:

```rust
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
```

- [ ] **Step 3.3: Run tests**

```bash
cargo test --lib events
```

Expected: 6 tests pass.

- [ ] **Step 3.4: Commit**

```bash
git add src/events.rs src/main.rs
git commit -m "feat: add Event enum with JSONL serialization"
```

---

## Task 4: CSV row encoding for events

**Files:**
- Modify: `src/events.rs`

- [ ] **Step 4.1: Append a `CsvRow` representation and a `write_csv_header` helper**

At the bottom of `src/events.rs` (before the existing `#[cfg(test)]`):

```rust
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
```

- [ ] **Step 4.2: Add CSV tests inside the existing `tests` module**

Append inside the `tests` module:

```rust
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
```

- [ ] **Step 4.3: Run tests**

```bash
cargo test --lib events
```

Expected: 10 tests pass (4 new).

- [ ] **Step 4.4: Commit**

```bash
git add src/events.rs
git commit -m "feat: add CSV row representation for events"
```

---

## Task 5: File logger (`src/logger.rs`)

**Files:**
- Create: `src/logger.rs`
- Modify: `src/main.rs` (module declaration)

- [ ] **Step 5.1: Add `mod logger;` to `src/main.rs`**

```rust
mod cli;
mod events;
mod logger;

fn main() {
    let args = cli::Cli::parse_from_env();
    println!("{:?}", args);
}
```

- [ ] **Step 5.2: Write the logger with tests**

Create `src/logger.rs`:

```rust
use crate::cli::LogFormat;
use crate::events::{Event, CSV_HEADERS};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::{self, JoinHandle};

pub struct LoggerHandle {
    sender: Sender<Event>,
    join: Option<JoinHandle<std::io::Result<()>>>,
}

impl LoggerHandle {
    pub fn sender(&self) -> Sender<Event> {
        self.sender.clone()
    }

    /// Drop the public sender so the worker drains and exits, then join.
    pub fn shutdown(mut self) -> std::io::Result<()> {
        drop(self.sender);
        if let Some(j) = self.join.take() {
            j.join().expect("logger thread panicked")?;
        }
        Ok(())
    }
}

pub fn spawn(path: PathBuf, format: LogFormat) -> std::io::Result<LoggerHandle> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("log directory does not exist: {}", parent.display()),
            ));
        }
    }
    let file = File::create(&path)?;
    let mut writer = BufWriter::new(file);

    if matches!(format, LogFormat::Csv) {
        writeln!(writer, "{}", CSV_HEADERS.join(","))?;
        writer.flush()?;
    }

    let (tx, rx): (Sender<Event>, Receiver<Event>) = std::sync::mpsc::channel();
    let join = thread::Builder::new()
        .name("linkclihost-logger".into())
        .spawn(move || -> std::io::Result<()> {
            run(rx, writer, format)
        })?;

    Ok(LoggerHandle {
        sender: tx,
        join: Some(join),
    })
}

fn run<W: Write>(rx: Receiver<Event>, mut writer: W, format: LogFormat) -> std::io::Result<()> {
    while let Ok(event) = rx.recv() {
        match format {
            LogFormat::Jsonl => {
                let line = event
                    .to_jsonl()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                writer.write_all(line.as_bytes())?;
            }
            LogFormat::Csv => {
                let row = event.to_csv_row();
                let line = row
                    .iter()
                    .map(|c| csv_escape(c))
                    .collect::<Vec<_>>()
                    .join(",");
                writer.write_all(line.as_bytes())?;
                writer.write_all(b"\n")?;
            }
        }
        writer.flush()?;
    }
    writer.flush()
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Event;
    use chrono::TimeZone;
    use std::fs;

    fn t() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(2026, 5, 28, 12, 0, 0).unwrap()
    }

    #[test]
    fn jsonl_writes_one_line_per_event() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.jsonl");
        let handle = spawn(path.clone(), LogFormat::Jsonl).unwrap();
        let tx = handle.sender();
        tx.send(Event::PeersChanged { at: t(), count: 1 }).unwrap();
        tx.send(Event::PeersChanged { at: t(), count: 2 }).unwrap();
        drop(tx);
        handle.shutdown().unwrap();

        let body = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 2);
        for line in lines {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(v["type"], "peers_changed");
        }
    }

    #[test]
    fn csv_writes_header_then_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.csv");
        let handle = spawn(path.clone(), LogFormat::Csv).unwrap();
        let tx = handle.sender();
        tx.send(Event::TempoChanged {
            at: t(),
            from_bpm: 120.0,
            to_bpm: 119.5,
        })
        .unwrap();
        drop(tx);
        handle.shutdown().unwrap();

        let body = fs::read_to_string(&path).unwrap();
        let mut lines = body.lines();
        assert_eq!(lines.next().unwrap(), CSV_HEADERS.join(","));
        let row = lines.next().unwrap();
        assert!(row.contains("tempo_changed"));
        assert!(row.contains("120"));
        assert!(row.contains("119.5"));
    }

    #[test]
    fn csv_escapes_commas_in_notes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("escape.csv");
        let handle = spawn(path.clone(), LogFormat::Csv).unwrap();
        let tx = handle.sender();
        tx.send(Event::SessionEnd {
            at: t(),
            reason: "a, b".into(),
        })
        .unwrap();
        drop(tx);
        handle.shutdown().unwrap();

        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("\"a, b\""));
    }

    #[test]
    fn missing_parent_dir_is_error() {
        let bogus = PathBuf::from("/nonexistent/path/x.jsonl");
        assert!(spawn(bogus, LogFormat::Jsonl).is_err());
    }
}
```

- [ ] **Step 5.3: Run tests**

```bash
cargo test --lib logger
```

Expected: 4 tests pass.

- [ ] **Step 5.4: Commit**

```bash
git add src/logger.rs src/main.rs
git commit -m "feat: add channel-driven JSONL/CSV logger"
```

---

## Task 6: App state, tempo history, stability (`src/app.rs`)

**Files:**
- Create: `src/app.rs`
- Modify: `src/main.rs` (module declaration)

- [ ] **Step 6.1: Add `mod app;` to `src/main.rs`**

```rust
mod app;
mod cli;
mod events;
mod logger;

fn main() {
    let args = cli::Cli::parse_from_env();
    println!("{:?}", args);
}
```

- [ ] **Step 6.2: Write the app state module with tests**

Create `src/app.rs`:

```rust
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
        // Filter spurious "changes" where Link re-fires with the same value.
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
        // values [100,110,120], mean=110, var=(100+0+100)/3=66.66..., stddev≈8.165
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
```

- [ ] **Step 6.3: Run tests**

```bash
cargo test --lib app
```

Expected: 9 tests pass.

- [ ] **Step 6.4: Commit**

```bash
git add src/app.rs src/main.rs
git commit -m "feat: add AppState with tempo history and stability metric"
```

---

## Task 7: TUI widgets (`src/ui/widgets.rs`)

**Files:**
- Create: `src/ui/mod.rs` (skeleton)
- Create: `src/ui/widgets.rs`
- Modify: `src/main.rs` (module declaration)

- [ ] **Step 7.1: Wire up the `ui` module**

Modify `src/main.rs` to add `mod ui;`:

```rust
mod app;
mod cli;
mod events;
mod logger;
mod ui;

fn main() {
    let args = cli::Cli::parse_from_env();
    println!("{:?}", args);
}
```

Create the skeleton `src/ui/mod.rs`:

```rust
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
```

- [ ] **Step 7.2: Write `src/ui/widgets.rs` with widget builders and tests**

```rust
use crate::app::TempoChange;
use crate::ui::Snapshot;
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
};
use std::time::Duration;

pub fn header(snap: &Snapshot) -> Paragraph<'static> {
    let lines = vec![
        kv("Tempo", format!("{:.2} BPM", snap.bpm), "Quantum", format!("{}", snap.quantum as u64)),
        kv("Beat", format!("{:.2}", snap.beat), "Phase", format!("{:.2} / {}", snap.phase, snap.quantum as u64)),
        kv("Playing", yes_no(snap.playing), "Peers", snap.peers.to_string()),
        kv("Uptime", fmt_duration(snap.uptime), "Link clock", format!("{} \u{00B5}s", snap.link_clock_micros)),
        kv(
            "Tempo \u{03C3}",
            format!("{:.2} BPM", snap.tempo_stability_bpm),
            "Last \u{0394}",
            last_delta(&snap.last_tempo_change),
        ),
    ];
    let title = format!(" LinkCLIHost \u{2014} peer: {} ", snap.peer_name);
    Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title))
}

pub fn phase_bar(snap: &Snapshot) -> Gauge<'static> {
    let ratio = if snap.quantum > 0.0 {
        (snap.phase / snap.quantum).clamp(0.0, 1.0)
    } else {
        0.0
    };
    Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" Phase "))
        .gauge_style(Style::default().fg(if snap.link_online { Color::Green } else { Color::DarkGray }))
        .ratio(ratio)
        .label(format!("{:.2}/{:.0}", snap.phase, snap.quantum))
}

pub fn history(changes: &[TempoChange]) -> List<'static> {
    let items: Vec<ListItem> = if changes.is_empty() {
        vec![ListItem::new("  (no tempo changes yet)")]
    } else {
        changes
            .iter()
            .map(|c| {
                let delta = c.delta();
                let arrow = if delta >= 0.0 { "+" } else { "" };
                let ts = c.at.format("%H:%M:%S");
                ListItem::new(Line::from(vec![
                    Span::raw(format!(
                        "  {}  {:.2} \u{2192} {:.2}   \u{0394} {arrow}{:.2}",
                        ts, c.from_bpm, c.to_bpm, delta
                    )),
                ]))
            })
            .collect()
    };
    List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Recent tempo changes "),
    )
}

pub fn footer(snap: &Snapshot) -> Paragraph<'static> {
    let log = match &snap.log_path {
        Some(p) => format!("log: {p}"),
        None => "log: none".into(),
    };
    let line = Line::from(vec![
        Span::styled(" q ", Style::default().add_modifier(Modifier::REVERSED)),
        Span::raw(" quit"),
        Span::raw("   "),
        if snap.link_online {
            Span::styled("link: online", Style::default().fg(Color::Green))
        } else {
            Span::styled("link: offline", Style::default().fg(Color::Red))
        },
        Span::raw("   "),
        Span::raw(log),
    ]);
    Paragraph::new(line).block(Block::default().borders(Borders::ALL))
}

pub fn split(area: Rect) -> Vec<Rect> {
    Layout::vertical([
        Constraint::Length(7),  // header (5 rows + borders)
        Constraint::Length(3),  // phase bar (1 row + borders)
        Constraint::Min(4),     // history list
        Constraint::Length(3),  // footer
    ])
    .split(area)
    .to_vec()
}

fn kv(k1: &str, v1: String, k2: &str, v2: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {k1:<10}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("{v1:<20}")),
        Span::styled(
            format!("{k2:<12}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(v2),
    ])
    .alignment(Alignment::Left)
}

fn yes_no(b: bool) -> String {
    if b { "yes".into() } else { "no".into() }
}

fn last_delta(change: &Option<TempoChange>) -> String {
    match change {
        Some(c) => {
            let d = c.delta();
            let sign = if d >= 0.0 { "+" } else { "" };
            format!("{sign}{d:.2} @ {}", c.at.format("%H:%M"))
        }
        None => "\u{2014}".into(),
    }
}

fn fmt_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    fn sample_snapshot() -> Snapshot {
        Snapshot {
            peer_name: "rig".into(),
            bpm: 120.00,
            beat: 142.37,
            phase: 2.37,
            quantum: 4.0,
            playing: true,
            peers: 3,
            link_clock_micros: 874_632_199,
            uptime: Duration::from_secs(862),
            last_tempo_change: Some(TempoChange {
                at: Utc.with_ymd_and_hms(2026, 5, 28, 12, 3, 17).unwrap(),
                from_bpm: 120.50,
                to_bpm: 120.00,
            }),
            recent_tempo_changes: vec![TempoChange {
                at: Utc.with_ymd_and_hms(2026, 5, 28, 12, 3, 17).unwrap(),
                from_bpm: 120.50,
                to_bpm: 120.00,
            }],
            tempo_stability_bpm: 0.12,
            log_path: Some("./events.jsonl".into()),
            link_online: true,
        }
    }

    fn render_to_buffer(snap: &Snapshot, width: u16, height: u16) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let chunks = split(f.area());
                f.render_widget(header(snap), chunks[0]);
                f.render_widget(phase_bar(snap), chunks[1]);
                f.render_widget(history(&snap.recent_tempo_changes), chunks[2]);
                f.render_widget(footer(snap), chunks[3]);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn buffer_contains(buf: &Buffer, needle: &str) -> bool {
        let mut joined = String::new();
        let area = buf.area;
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(""));
            }
            joined.push('\n');
        }
        joined.contains(needle)
    }

    #[test]
    fn header_renders_tempo_and_peer() {
        let buf = render_to_buffer(&sample_snapshot(), 70, 18);
        assert!(buffer_contains(&buf, "120.00"));
        assert!(buffer_contains(&buf, "rig"));
    }

    #[test]
    fn history_renders_tempo_change_row() {
        let buf = render_to_buffer(&sample_snapshot(), 70, 18);
        assert!(buffer_contains(&buf, "120.50"));
        assert!(buffer_contains(&buf, "120.00"));
    }

    #[test]
    fn empty_history_shows_placeholder() {
        let mut snap = sample_snapshot();
        snap.recent_tempo_changes.clear();
        let buf = render_to_buffer(&snap, 70, 18);
        assert!(buffer_contains(&buf, "no tempo changes yet"));
    }

    #[test]
    fn footer_shows_log_path() {
        let buf = render_to_buffer(&sample_snapshot(), 70, 18);
        assert!(buffer_contains(&buf, "events.jsonl"));
    }

    #[test]
    fn footer_shows_log_none() {
        let mut snap = sample_snapshot();
        snap.log_path = None;
        let buf = render_to_buffer(&snap, 70, 18);
        assert!(buffer_contains(&buf, "log: none"));
    }

    #[test]
    fn duration_format_pads_to_hhmmss() {
        assert_eq!(fmt_duration(Duration::from_secs(0)), "00:00:00");
        assert_eq!(fmt_duration(Duration::from_secs(65)), "00:01:05");
        assert_eq!(fmt_duration(Duration::from_secs(3661)), "01:01:01");
    }
}
```

- [ ] **Step 7.3: Run tests**

```bash
cargo test --lib ui
```

Expected: 6 tests pass.

- [ ] **Step 7.4: Commit**

```bash
git add src/ui/ src/main.rs
git commit -m "feat: add ratatui dashboard widgets with TestBackend coverage"
```

---

## Task 8: UI top-level draw + too-small fallback (`src/ui/mod.rs`)

**Files:**
- Modify: `src/ui/mod.rs`

- [ ] **Step 8.1: Add a `draw` function and a "terminal too small" path**

Append to `src/ui/mod.rs` (after the `Snapshot` struct):

```rust
use ratatui::Frame;

pub const MIN_WIDTH: u16 = 50;
pub const MIN_HEIGHT: u16 = 18;

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
    use crate::app::TempoChange;
    use chrono::{TimeZone, Utc};
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
```

- [ ] **Step 8.2: Run tests**

```bash
cargo test --lib ui
```

Expected: 8 tests pass (6 from widgets + 2 here).

- [ ] **Step 8.3: Commit**

```bash
git add src/ui/mod.rs
git commit -m "feat: add top-level UI draw with min-size fallback"
```

---

## Task 9: Link wrapper (`src/link.rs`)

**Files:**
- Create: `src/link.rs`
- Modify: `src/main.rs` (module declaration)

- [ ] **Step 9.1: Add `mod link;` in `src/main.rs`**

```rust
mod app;
mod cli;
mod events;
mod link;
mod logger;
mod ui;

fn main() {
    let args = cli::Cli::parse_from_env();
    println!("{:?}", args);
}
```

- [ ] **Step 9.2: Write the Link wrapper**

Create `src/link.rs`:

```rust
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
    /// Build the Link instance, install the three callbacks, then enable
    /// networking and start/stop sync. All callbacks run on Link's internal
    /// thread; they lock the AppState briefly and then push an Event.
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
        // `rusty_link` does not expose a direct "enabled" status, but if
        // enable(true) is called and the network stack is up, the instance
        // is considered online from our perspective.
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
```

- [ ] **Step 9.3: Verify the wrapper compiles**

```bash
cargo build
```

Expected: succeeds. If the build fails because a rusty_link method has a different name than assumed, run `cargo doc --no-deps -p rusty_link --open` to see the actual API and adjust the four calls (`set_tempo_callback`, `set_num_peers_callback`, `set_start_stop_callback`, `capture_app_session_state`, `clock_micros`, `enable`, `enable_start_stop_sync`). The signatures we depend on are documented in the design spec, Section 2 and Section 10.

- [ ] **Step 9.4: Smoke-run (no UI yet, just verify Link starts)**

Temporarily add a tiny probe to the bottom of `main()` to confirm the wrapper boots without panicking. After confirmation, the next task replaces `main()` wholesale, so we don't keep this probe.

Edit `src/main.rs`:

```rust
mod app;
mod cli;
mod events;
mod link;
mod logger;
mod ui;

use crate::app::AppState;
use crate::link::LinkSession;
use std::sync::{Arc, Mutex};

fn main() {
    let args = cli::Cli::parse_from_env();
    let shared = Arc::new(Mutex::new(AppState::new(args.initial_bpm)));
    let mut link = LinkSession::new(args.initial_bpm, args.quantum, shared.clone(), None);
    let snap = link.snapshot();
    println!("link snapshot: {:?} bpm at clock {} us", snap.bpm, snap.clock_micros);
}
```

```bash
cargo run --release
```

Expected: prints something like `link snapshot: 120.0 bpm at clock 12345678 us`. The release build is recommended here because it cuts Link's compile time on subsequent rebuilds and avoids debug-build noise.

- [ ] **Step 9.5: Commit**

```bash
git add src/link.rs src/main.rs
git commit -m "feat: wrap rusty_link with callback installation and snapshot"
```

---

## Task 10: Main loop and TUI wiring (`src/main.rs`)

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 10.1: Replace `src/main.rs` with the full wiring**

```rust
mod app;
mod cli;
mod events;
mod link;
mod logger;
mod ui;

use crate::app::AppState;
use crate::cli::{Cli, LogFormat};
use crate::events::Event;
use crate::link::{LinkSession, LinkSnapshot};
use crate::logger::{spawn as spawn_logger, LoggerHandle};
use crate::ui::Snapshot;
use chrono::Utc;
use clap::Parser;
use ratatui::crossterm::{
    event::{self, Event as CtEvent, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(33);

fn main() {
    let args = Cli::parse();
    if let Err(e) = run(args) {
        eprintln!("error: {e}");
        std::process::exit(2);
    }
}

fn run(args: Cli) -> Result<(), String> {
    let log_format = match args.log_format() {
        Some(Ok(f)) => Some(f),
        Some(Err(msg)) => return Err(msg),
        None => None,
    };

    let log_path = args.log.clone();
    let logger = match (&log_path, log_format) {
        (Some(p), Some(fmt)) => Some(
            spawn_logger(p.clone(), fmt)
                .map_err(|e| format!("failed to open log file {}: {e}", p.display()))?,
        ),
        _ => None,
    };
    let log_sender: Option<Sender<Event>> = logger.as_ref().map(LoggerHandle::sender);
    let log_display = log_path.as_ref().map(|p| p.display().to_string());

    let peer_name = args.resolved_name();
    let shared = Arc::new(Mutex::new(AppState::new(args.initial_bpm)));
    let mut link = LinkSession::new(args.initial_bpm, args.quantum, shared.clone(), log_sender.clone());

    if let Some(tx) = &log_sender {
        let _ = tx.send(Event::SessionStart {
            at: Utc::now(),
            quantum: args.quantum,
            peer_name: peer_name.clone(),
        });
    }

    let shutdown = Arc::new(AtomicBool::new(false));
    install_signal_handler(shutdown.clone());

    let use_tui = !args.no_tui && io::stdout().is_terminal();
    if !use_tui && args.no_tui == false {
        eprintln!("notice: stdout is not a TTY, falling back to --no-tui mode");
    }

    let result = if use_tui {
        run_tui(
            &mut link,
            shared.clone(),
            shutdown.clone(),
            peer_name.clone(),
            args.quantum,
            log_display,
        )
    } else {
        run_headless(&mut link, shutdown.clone(), args.quantum, peer_name.clone())
    };

    if let Some(tx) = &log_sender {
        let _ = tx.send(Event::SessionEnd {
            at: Utc::now(),
            reason: shutdown_reason(&shutdown),
        });
    }

    // Order matters: the Link callbacks (owned by `link`) each hold a clone
    // of the event Sender. We must drop both `log_sender` and `link` before
    // joining the logger thread, otherwise its `rx.recv()` blocks forever.
    drop(log_sender);
    drop(link);
    if let Some(h) = logger {
        h.shutdown().map_err(|e| format!("logger flush failed: {e}"))?;
    }

    result
}

fn shutdown_reason(flag: &AtomicBool) -> String {
    if flag.load(Ordering::SeqCst) {
        "sigint".into()
    } else {
        "normal".into()
    }
}

fn install_signal_handler(flag: Arc<AtomicBool>) {
    let h = flag.clone();
    let _ = ctrlc::set_handler(move || {
        h.store(true, Ordering::SeqCst);
    });
}

fn run_tui(
    link: &mut LinkSession,
    shared: Arc<Mutex<AppState>>,
    shutdown: Arc<AtomicBool>,
    peer_name: String,
    quantum: f64,
    log_path: Option<String>,
) -> Result<(), String> {
    enable_raw_mode().map_err(|e| format!("enable raw mode: {e}"))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| format!("alt screen: {e}"))?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend).map_err(|e| format!("terminal: {e}"))?;

    let res = tui_loop(
        &mut terminal,
        link,
        shared,
        shutdown,
        peer_name,
        quantum,
        log_path,
    );

    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = disable_raw_mode();
    res
}

fn tui_loop<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    link: &mut LinkSession,
    shared: Arc<Mutex<AppState>>,
    shutdown: Arc<AtomicBool>,
    peer_name: String,
    quantum: f64,
    log_path: Option<String>,
) -> Result<(), String> {
    let mut next_tick = Instant::now();
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return Ok(());
        }
        let now = Instant::now();
        if now < next_tick {
            let remaining = next_tick - now;
            poll_input(remaining, &shutdown)?;
            continue;
        }
        next_tick = now + TICK;

        let link_snap: LinkSnapshot = link.snapshot();
        let snapshot = build_snapshot(
            &shared,
            &link_snap,
            peer_name.clone(),
            quantum,
            log_path.clone(),
            link.online(),
        );
        terminal
            .draw(|f| ui::draw(f, &snapshot))
            .map_err(|e| format!("draw: {e}"))?;
    }
}

fn poll_input(timeout: Duration, shutdown: &AtomicBool) -> Result<(), String> {
    if event::poll(timeout).map_err(|e| format!("poll: {e}"))? {
        if let CtEvent::Key(k) = event::read().map_err(|e| format!("read: {e}"))? {
            if k.kind == KeyEventKind::Press
                && matches!(k.code, KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc)
            {
                shutdown.store(true, Ordering::SeqCst);
            }
        }
    }
    Ok(())
}

fn build_snapshot(
    shared: &Arc<Mutex<AppState>>,
    link_snap: &LinkSnapshot,
    peer_name: String,
    quantum: f64,
    log_path: Option<String>,
    link_online: bool,
) -> Snapshot {
    let st = shared.lock().expect("app state poisoned");
    Snapshot {
        peer_name,
        bpm: link_snap.bpm,
        beat: link_snap.beat,
        phase: link_snap.phase,
        quantum,
        playing: link_snap.playing,
        peers: st.peers,
        link_clock_micros: link_snap.clock_micros,
        uptime: st.uptime(),
        last_tempo_change: st.last_tempo_change.clone(),
        recent_tempo_changes: st.recent_changes(),
        tempo_stability_bpm: st.tempo_stability_bpm(),
        log_path,
        link_online,
    }
}

fn run_headless(
    link: &mut LinkSession,
    shutdown: Arc<AtomicBool>,
    quantum: f64,
    peer_name: String,
) -> Result<(), String> {
    eprintln!("linkclihost (headless) — peer={peer_name} quantum={quantum}");
    let mut next_tick = Instant::now();
    while !shutdown.load(Ordering::SeqCst) {
        let now = Instant::now();
        if now < next_tick {
            std::thread::sleep(next_tick - now);
            continue;
        }
        next_tick = now + Duration::from_secs(1);
        let s = link.snapshot();
        println!(
            "{ts}  bpm={bpm:.2}  beat={beat:.2}  phase={phase:.2}  playing={playing}",
            ts = Utc::now().format("%H:%M:%S%.3f"),
            bpm = s.bpm,
            beat = s.beat,
            phase = s.phase,
            playing = s.playing,
        );
    }
    Ok(())
}
```

- [ ] **Step 10.2: Compile**

```bash
cargo build --release
```

Expected: succeeds. If `event::Event` import conflicts with `events::Event`, the alias in the `use` statement (`Event as CtEvent`) resolves it.

- [ ] **Step 10.3: Hand-run the binary in a real terminal**

```bash
./target/release/linkclihost --log /tmp/lch.jsonl
```

Expected behavior:
- TUI appears with "Tempo 120.00 BPM", "Peers 0" (until a Link peer joins), zeroed beat/phase.
- Pressing `q` exits cleanly, restoring the terminal.
- `/tmp/lch.jsonl` exists and contains at least a `session_start` and a `session_end` line.

- [ ] **Step 10.4: Verify the log was written**

```bash
head -n 5 /tmp/lch.jsonl
```

Expected: two JSON lines (or more, if a peer joined/left during the run).

- [ ] **Step 10.5: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire main loop, TUI render, signal handling, headless mode"
```

---

## Task 11: Verify `--no-tui` path manually

**Files:**
- (none — verification only)

- [ ] **Step 11.1: Run headless mode**

```bash
./target/release/linkclihost --no-tui --log /tmp/lch-headless.jsonl &
LCH=$!
sleep 3
kill -INT $LCH
wait $LCH 2>/dev/null
```

Expected stdout: a few "bpm=120.00 beat=… phase=… playing=false" lines, then exit.

- [ ] **Step 11.2: Verify the log captured session lifecycle**

```bash
grep -E '"(session_start|session_end)"' /tmp/lch-headless.jsonl
```

Expected: at least one `session_start` and one `session_end` line.

- [ ] **Step 11.3: Verify TTY-detection fallback**

```bash
./target/release/linkclihost > /tmp/lch-piped.log 2>&1 &
LCH=$!
sleep 2
kill -INT $LCH
wait $LCH 2>/dev/null
head /tmp/lch-piped.log
```

Expected: starts with `notice: stdout is not a TTY, falling back to --no-tui mode`, followed by the headless tick lines. (No TUI escape codes in the output.)

- [ ] **Step 11.4: No commit needed — this task is verification only.**

---

## Task 12: README and integration smoke test

**Files:**
- Modify: `README.md`
- Create: `tests/smoke.rs`

- [ ] **Step 12.1: Replace `README.md`**

```markdown
# LinkCLIHost

A headless [Ableton Link](https://github.com/Ableton/link) peer for Linux, with a live TUI dashboard and optional event log.

## Build

```bash
sudo apt install build-essential cmake pkg-config libclang-dev
cargo build --release
```

## Run

```bash
./target/release/linkclihost                       # TUI dashboard
./target/release/linkclihost --log run.jsonl       # also log events to JSONL
./target/release/linkclihost --log run.csv         # CSV instead of JSONL
./target/release/linkclihost --no-tui              # plain stdout, useful for piping
./target/release/linkclihost --quantum 8 --name studio
```

Press `q` (or `Esc`) to quit. `Ctrl-C` also exits cleanly.

## What the dashboard shows

- **Tempo** — current Link tempo in BPM
- **Beat / Phase** — beat number and position within the quantum (bar)
- **Playing** — Link transport state
- **Peers** — number of other Link-aware apps on the same network
- **Uptime** — since this process started
- **Tempo σ** — standard deviation of every tempo we've seen this session
- **Last Δ** — most recent tempo change and timestamp
- **Recent tempo changes** — last five tempo deltas with wall-clock timestamps

## Manual test recipe

1. On the same LAN, open another Link-aware app (Ableton Live, Carabiner, another instance of this binary).
2. Enable Link in that app.
3. `Peers` jumps from 0 to 1. The other app's tempo appears here within ~50 ms.
4. Change the tempo there — the change is logged as a `tempo_changed` event.

## Event log format

JSONL (`.jsonl` or `.ndjson` extension):

```json
{"type":"session_start","at":"2026-05-28T12:03:14.812Z","quantum":4,"peer_name":"studio"}
{"type":"peers_changed","at":"2026-05-28T12:03:16.500Z","count":1}
{"type":"tempo_changed","at":"2026-05-28T12:03:17.014Z","from_bpm":120.0,"to_bpm":119.5}
{"type":"transport_changed","at":"2026-05-28T12:03:25.401Z","playing":true}
{"type":"session_end","at":"2026-05-28T12:18:02.119Z","reason":"sigint"}
```

CSV (`.csv` extension): columns are `timestamp_utc,event,bpm_from,bpm_to,peer_count,playing,note`.

## License

Apache-2.0 — see [LICENSE](LICENSE).
```

- [ ] **Step 12.2: Add a smoke integration test**

Create `tests/smoke.rs`:

```rust
use std::process::{Command, Stdio};
use std::time::Duration;

/// Spawns the release binary, lets it run briefly, then sends SIGINT.
/// Confirms the process exits with status 0 and writes a non-empty log.
#[test]
#[ignore] // run with `cargo test --release -- --ignored` because it needs the release binary built
fn binary_starts_logs_and_exits_cleanly() {
    let bin = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .map(|p| p.join("linkclihost"))
        .expect("locate binary");
    assert!(bin.exists(), "build the binary first: cargo build --release");

    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("smoke.jsonl");

    let mut child = Command::new(&bin)
        .arg("--no-tui")
        .arg("--log")
        .arg(&log)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn linkclihost");

    std::thread::sleep(Duration::from_secs(2));
    unsafe {
        libc::kill(child.id() as i32, libc::SIGINT);
    }
    let status = child.wait().expect("wait");
    assert!(status.success(), "child exited with {status}");

    let body = std::fs::read_to_string(&log).expect("read log");
    assert!(body.contains("session_start"), "log missing session_start");
    assert!(body.contains("session_end"), "log missing session_end");
}
```

Add `libc` to dev-deps in `Cargo.toml` under `[dev-dependencies]`:

```toml
libc = "0.2"
```

- [ ] **Step 12.3: Build the binary, then run the smoke test**

```bash
cargo build --release
cargo test --release --test smoke -- --ignored
```

Expected: 1 passed.

- [ ] **Step 12.4: Run the entire suite as a sanity check**

```bash
cargo test
```

Expected: all unit tests still pass (CLI 8, events 10, logger 4, app 9, ui widgets 6, ui mod 2 — 39 total). Integration test `smoke.rs` doesn't run without `--ignored`, which is intentional.

- [ ] **Step 12.5: Commit**

```bash
git add README.md tests/smoke.rs Cargo.toml
git commit -m "docs: write README and add binary smoke test"
```

---

## Verification checklist (run at the end)

- [ ] `cargo build --release` — succeeds, produces `./target/release/linkclihost`
- [ ] `cargo test` — 39 unit tests pass
- [ ] `cargo test --release --test smoke -- --ignored` — smoke test passes
- [ ] `./target/release/linkclihost --help` — clap help renders
- [ ] `./target/release/linkclihost` in a real TTY — TUI dashboard appears, `q` exits cleanly
- [ ] `./target/release/linkclihost --no-tui` — prints headless tick lines
- [ ] `./target/release/linkclihost --log /tmp/x.jsonl` — writes valid JSONL on shutdown
- [ ] `./target/release/linkclihost --log /tmp/x.csv` — writes CSV with the expected header

If a second Link-aware app is available on the LAN, also verify peer count, tempo sync, and that a tempo change in the other app shows up in the recent-changes list within ~100 ms.

---

## Recovery guidance

- **Build fails on first run with "missing libclang"**: install `libclang-dev` (Ubuntu) or the equivalent.
- **Build fails on `rusty_link` because cmake is too old**: install cmake ≥ 3.14.
- **rusty_link method names differ from the plan**: open `cargo doc --no-deps -p rusty_link --open` to see the real API and rename the calls in `src/link.rs`. The shape is the same (constructor, three callback setters, capture/commit pair, `clock_micros`, `enable`, `enable_start_stop_sync`).
- **ratatui 0.29 API drift**: if `frame.area()` is named `frame.size()` in your patch version, replace accordingly. The other APIs (`Layout::vertical`, `TestBackend`) have been stable since 0.27.
- **Terminal stays in raw mode after a crash**: `reset` or `stty sane` restores it. The Drop on `LinkSession` plus the `LeaveAlternateScreen` calls in `run_tui` handle the normal path; for hard crashes the OS will release the terminal on process exit, but the visible state may be garbled until you `reset`.
