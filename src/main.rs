mod app;
mod audio;
mod cli;
mod events;
mod link;
mod logger;
mod midi_clock;
mod sequencer;
mod synth;
mod ui;

use crate::app::AppState;
use crate::audio::{AudioEngine, AudioShared};
use crate::cli::Cli;
use crate::events::Event;
use crate::link::{LinkSession, LinkSnapshot};
use crate::logger::{spawn as spawn_logger, LoggerHandle};
use crate::midi_clock::MidiClock;
use crate::sequencer::{PRESETS, STEPS, TRACKS};
use crate::ui::{SeqDisplay, Snapshot};
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

/// Optional sync/sound outputs driven by the Link session.
struct Outputs {
    midi: Option<MidiClock>,
    audio_shared: Option<Arc<AudioShared>>,
    audio: Option<AudioEngine>,
}

/// Key commands available while the dashboard runs.
enum Action {
    Quit,
    TogglePlay,
    NextPreset,
    ToggleMute,
    TempoUp,
    TempoDown,
}

fn main() {
    let args = Cli::parse();
    if let Err(e) = run(args) {
        eprintln!("error: {e}");
        std::process::exit(2);
    }
}

fn run(args: Cli) -> Result<(), String> {
    if args.list_midi_ports {
        return list_and_print("MIDI output ports", midi_clock::list_ports()?);
    }
    if args.list_audio_devices {
        return list_and_print("audio output devices", audio::list_devices()?);
    }

    let preset_idx = args.preset_idx()?;

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

    let midi = match &args.midi_out {
        Some(sel) => Some(midi_clock::spawn(
            link.handle(),
            args.quantum,
            sel,
            log_sender.clone(),
        )?),
        None => None,
    };

    let (audio_shared, audio_engine) = if args.audio_enabled() {
        let s = Arc::new(AudioShared::new(preset_idx));
        let engine = audio::start(
            link.handle(),
            args.quantum,
            s.clone(),
            args.audio_out.as_deref(),
            args.gain.clamp(0.0, 1.0),
        )?;
        (Some(s), Some(engine))
    } else {
        (None, None)
    };
    let outputs = Outputs {
        midi,
        audio_shared,
        audio: audio_engine,
    };

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
            &outputs,
        )
    } else {
        run_headless(
            &mut link,
            shutdown.clone(),
            args.quantum,
            peer_name.clone(),
            &outputs,
        )
    };

    if let Some(tx) = &log_sender {
        let _ = tx.send(Event::SessionEnd {
            at: Utc::now(),
            reason: shutdown_reason(&shutdown),
        });
    }

    // Order matters: the Link callbacks (owned by `link`) and the MIDI clock
    // thread each hold a clone of the event Sender. We must drop them all
    // before joining the logger thread, otherwise its `rx.recv()` blocks
    // forever. The audio stream holds a Link handle, so it goes first.
    drop(outputs);
    drop(log_sender);
    drop(link);
    if let Some(h) = logger {
        h.shutdown()
            .map_err(|e| format!("logger flush failed: {e}"))?;
    }

    result
}

fn list_and_print(what: &str, items: Vec<String>) -> Result<(), String> {
    if items.is_empty() {
        println!("no {what} found");
    } else {
        println!("{what}:");
        for (i, name) in items.iter().enumerate() {
            println!("  [{i}] {name}");
        }
    }
    Ok(())
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

#[allow(clippy::too_many_arguments)]
fn run_tui(
    link: &mut LinkSession,
    shared: Arc<Mutex<AppState>>,
    shutdown: Arc<AtomicBool>,
    peer_name: String,
    quantum: f64,
    log_path: Option<String>,
    outputs: &Outputs,
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
        outputs,
    );

    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = disable_raw_mode();
    res
}

#[allow(clippy::too_many_arguments)]
fn tui_loop<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    link: &mut LinkSession,
    shared: Arc<Mutex<AppState>>,
    shutdown: Arc<AtomicBool>,
    peer_name: String,
    quantum: f64,
    log_path: Option<String>,
    outputs: &Outputs,
) -> Result<(), String> {
    let mut next_tick = Instant::now();
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return Ok(());
        }
        let now = Instant::now();
        if now < next_tick {
            let remaining = next_tick - now;
            if let Some(action) = poll_input(remaining)? {
                handle_action(action, link, outputs, &shutdown);
            }
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
            outputs,
        );
        terminal
            .draw(|f| ui::draw(f, &snapshot))
            .map_err(|e| format!("draw: {e}"))?;
    }
}

fn handle_action(action: Action, link: &mut LinkSession, outputs: &Outputs, shutdown: &AtomicBool) {
    match action {
        Action::Quit => shutdown.store(true, Ordering::SeqCst),
        Action::TogglePlay => link.toggle_playing(),
        Action::TempoUp => link.nudge_tempo(1.0),
        Action::TempoDown => link.nudge_tempo(-1.0),
        Action::NextPreset => {
            if let Some(s) = &outputs.audio_shared {
                s.next_preset();
            }
        }
        Action::ToggleMute => {
            if let Some(s) = &outputs.audio_shared {
                s.toggle_mute();
            }
        }
    }
}

fn poll_input(timeout: Duration) -> Result<Option<Action>, String> {
    if event::poll(timeout).map_err(|e| format!("poll: {e}"))? {
        if let CtEvent::Key(k) = event::read().map_err(|e| format!("read: {e}"))? {
            if k.kind == KeyEventKind::Press {
                return Ok(match k.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => Some(Action::Quit),
                    KeyCode::Char(' ') => Some(Action::TogglePlay),
                    KeyCode::Char('p') | KeyCode::Char('P') => Some(Action::NextPreset),
                    KeyCode::Char('m') | KeyCode::Char('M') => Some(Action::ToggleMute),
                    KeyCode::Char('+') | KeyCode::Char('=') => Some(Action::TempoUp),
                    KeyCode::Char('-') => Some(Action::TempoDown),
                    _ => None,
                });
            }
        }
    }
    Ok(None)
}

fn seq_display(outputs: &Outputs) -> Option<SeqDisplay> {
    let shared = outputs.audio_shared.as_ref()?;
    let engine = outputs.audio.as_ref()?;
    let preset = &PRESETS[shared.preset_idx.load(Ordering::Relaxed) % PRESETS.len()];
    let mut pattern = [[false; STEPS]; TRACKS];
    for (t, row) in pattern.iter_mut().enumerate() {
        for (s, cell) in row.iter_mut().enumerate() {
            *cell = preset.step(t, s);
        }
    }
    Some(SeqDisplay {
        preset_name: preset.name,
        pattern,
        current_step: shared.current_step.load(Ordering::Relaxed),
        muted: shared.muted.load(Ordering::Relaxed),
        device: engine.device_name.clone(),
        sample_rate: engine.sample_rate,
        stream_errors: shared.stream_errors.load(Ordering::Relaxed),
    })
}

#[allow(clippy::too_many_arguments)]
fn build_snapshot(
    shared: &Arc<Mutex<AppState>>,
    link_snap: &LinkSnapshot,
    peer_name: String,
    quantum: f64,
    log_path: Option<String>,
    link_online: bool,
    outputs: &Outputs,
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
        seq: seq_display(outputs),
        midi: outputs.midi.as_ref().map(MidiClock::snapshot),
    }
}

fn run_headless(
    link: &mut LinkSession,
    shutdown: Arc<AtomicBool>,
    quantum: f64,
    peer_name: String,
    outputs: &Outputs,
) -> Result<(), String> {
    eprintln!("linkclihost (headless) \u{2014} peer={peer_name} quantum={quantum}");
    if let Some(m) = &outputs.midi {
        eprintln!("midi clock -> {}", m.snapshot().port);
    }
    if let Some(a) = &outputs.audio {
        eprintln!(
            "sequencer audio -> {} @ {} Hz",
            a.device_name, a.sample_rate
        );
    }
    let mut next_tick = Instant::now();
    while !shutdown.load(Ordering::SeqCst) {
        let now = Instant::now();
        if now < next_tick {
            std::thread::sleep(next_tick - now);
            continue;
        }
        next_tick = now + Duration::from_secs(1);
        let s = link.snapshot();
        let midi_part = match &outputs.midi {
            Some(m) => {
                let snap = m.snapshot();
                format!(
                    "  midi_jitter_us={:.0}/{}",
                    snap.mean_abs_err_us, snap.max_abs_err_us
                )
            }
            None => String::new(),
        };
        println!(
            "{ts}  bpm={bpm:.2}  beat={beat:.2}  phase={phase:.2}  playing={playing}{midi_part}",
            ts = Utc::now().format("%H:%M:%S%.3f"),
            bpm = s.bpm,
            beat = s.beat,
            phase = s.phase,
            playing = s.playing,
        );
    }
    Ok(())
}
