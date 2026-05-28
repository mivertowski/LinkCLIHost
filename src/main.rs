mod app;
mod cli;
mod events;
mod link;
mod logger;
mod ui;

use crate::app::AppState;
use crate::cli::Cli;
use crate::events::Event;
use crate::link::{LinkSession, LinkSnapshot};
use crate::logger::{spawn as spawn_logger, LoggerHandle};
use crate::ui::Snapshot;
use chrono::Utc;
use clap::Parser;
use ratatui::crossterm::{
    event::{self, Event as CtEvent, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, IsTerminal};
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
    let mut link = LinkSession::new(
        args.initial_bpm,
        args.quantum,
        shared.clone(),
        log_sender.clone(),
    );

    if let Some(tx) = &log_sender {
        let _ = tx.send(Event::SessionStart {
            at: Utc::now(),
            quantum: args.quantum,
            peer_name: peer_name.clone(),
        });
    }

    let shutdown = Arc::new(AtomicBool::new(false));
    install_signal_handler(shutdown.clone());

    let stdout_is_tty = io::stdout().is_terminal();
    let use_tui = !args.no_tui && stdout_is_tty;
    if !args.no_tui && !stdout_is_tty {
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
    eprintln!("linkclihost (headless) \u{2014} peer={peer_name} quantum={quantum}");
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
