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
        .spawn(move || -> std::io::Result<()> { run(rx, writer, format) })?;

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
