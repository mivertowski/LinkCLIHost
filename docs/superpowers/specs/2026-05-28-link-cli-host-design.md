# LinkCLIHost — Design

**Status:** Draft for review
**Date:** 2026-05-28
**Author:** Brainstorm session w/ Claude (Opus 4.7)

## 1. Goal

A small, single-binary Linux CLI that joins an Ableton Link session as a passive peer and renders a live TUI dashboard showing the current tempo, recent tempo changes, beat/phase, peer count, and a handful of supporting metrics. Optionally writes every observed event to a JSONL or CSV file.

Non-goals (out of scope): changing tempo, starting/stopping transport, audio output, MIDI/OSC bridging, non-Linux targets, multi-session capture, web UI.

## 2. Stack

- **Language:** Rust (2021 edition, MSRV 1.85)
- **Link bindings:** [`rusty_link`](https://crates.io/crates/rusty_link) `0.4` — wraps Ableton's official `abl_link` C-11 wrapper
- **TUI:** [`ratatui`](https://crates.io/crates/ratatui) `0.29` (use its re-exported `crossterm` backend, no separate crossterm dep)
- **CLI parsing:** [`clap`](https://crates.io/crates/clap) `4` with derive
- **Serialization:** [`serde`](https://crates.io/crates/serde) + [`serde_json`](https://crates.io/crates/serde_json), [`csv`](https://crates.io/crates/csv)
- **Time:** [`chrono`](https://crates.io/crates/chrono) for wall-clock timestamps in event logs
- **Channels:** `std::sync::mpsc` (no async runtime needed)

System packages required on Ubuntu 24.04:

```
sudo apt install build-essential cmake pkg-config libclang-dev
```

(`rusty_link` invokes `cmake` to build the bundled Ableton Link C++ sources and uses `bindgen` for the abl_link C wrapper.)

## 3. Architecture

```
                   ┌────────────────────────────────────────────┐
                   │              main thread                   │
                   │  • parse CLI                               │
                   │  • build AppState, channels                │
                   │  • configure AblLink + callbacks           │
                   │  • run TUI draw loop (~30 Hz)              │
                   │  • read keyboard for q / Ctrl-C            │
                   └─────────┬──────────────────────────────────┘
                             │ installs callbacks │ captures session state
                             ▼                    │
              ┌──────────────────────────┐        │
              │ rusty_link::AblLink      │        │
              │  (internal Link thread)  │        │
              │  emits:                  │        │
              │   • tempo callback       │        │
              │   • num_peers callback   │        │
              │   • start_stop callback  │        │
              └─────────┬────────────────┘        │
                        │ Event                   │ Snapshot
                        ▼                         ▼
                ┌─────────────────────────────────────────┐
                │ AppState                                │
                │  • current bpm / playing / peers        │
                │  • tempo history ring buffer (N=128)    │
                │  • session uptime, last-event clocks    │
                │  • event channel sender (Option<…>)     │
                └────────┬────────────────────────────────┘
                         │ Event (mpsc)
                         ▼
                ┌─────────────────────────────┐
                │ logger thread (only if      │
                │ --log <PATH> given)         │
                │  • writes JSONL or CSV      │
                │  • fsyncs on shutdown       │
                └─────────────────────────────┘
```

### Threads at runtime

1. **main** — TUI render loop + input.
2. **Link internal** — owned by `rusty_link`; invokes our callback closures.
3. **logger** (optional) — drains the event channel into a file.

Three threads, all communication is either through an `Arc<Mutex<AppState>>` or an `mpsc::Sender<Event>` — no `async`, no `tokio`.

## 4. File layout

```
LinkCLIHost/
├── Cargo.toml
├── Cargo.lock
├── README.md          # short usage doc
├── LICENSE            # already present (Apache-2.0 per initial commit)
├── src/
│   ├── main.rs        # entry, wires components, runs render loop
│   ├── cli.rs         # clap struct
│   ├── app.rs         # AppState, Snapshot, TempoChange ring buffer
│   ├── events.rs      # Event enum + serde impls + Csv row
│   ├── link.rs        # thin wrapper over AblLink: install callbacks, snapshot
│   ├── logger.rs      # mpsc → file writer (JSONL or CSV by extension)
│   └── ui/
│       ├── mod.rs     # draw(frame, &Snapshot)
│       └── widgets.rs # header, phase bar, tempo history list, footer
└── docs/
    └── superpowers/
        └── specs/
            └── 2026-05-28-link-cli-host-design.md  # this file
```

Eight `.rs` files, each under ~200 lines. No file does more than one thing.

## 5. Data model

### `AppState`
Lives behind `Arc<Mutex<…>>`. Updated by Link callbacks; read by the render loop.

```rust
const HISTORY_CAP: usize = 128;
const HISTORY_DISPLAY: usize = 5;

pub struct AppState {
    pub current_bpm: f64,
    pub last_tempo_change: Option<TempoChange>,
    pub tempo_history: VecDeque<TempoChange>,  // newest at the back, len ≤ HISTORY_CAP
    pub peers: u64,
    pub playing: bool,
    pub started_at: Instant,
    pub last_event_at: Option<Instant>,
}

pub struct TempoChange {
    pub at: DateTime<Utc>,
    pub from_bpm: f64,
    pub to_bpm: f64,
}

pub struct Snapshot {  // captured once per render tick, owned by the renderer
    pub bpm: f64,
    pub beat: f64,
    pub phase: f64,
    pub quantum: f64,
    pub playing: bool,
    pub peers: u64,
    pub link_clock_micros: i64,
    pub uptime: Duration,
    pub last_tempo_change: Option<TempoChange>,
    pub recent_tempo_changes: Vec<TempoChange>,  // up to HISTORY_DISPLAY, newest first
    pub tempo_stability_bpm: f64,                // see definition below
}
```

`tempo_stability_bpm` is the population standard deviation of the `to_bpm` field across every `TempoChange` currently in `tempo_history`. When fewer than two entries exist it is `0.0`. Inserts past `HISTORY_CAP` evict the oldest entry (FIFO).

### `Event`
What gets logged.

```rust
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    SessionStart { at: DateTime<Utc>, quantum: f64, peer_name: String },
    TempoChanged { at: DateTime<Utc>, from_bpm: f64, to_bpm: f64 },
    PeersChanged { at: DateTime<Utc>, count: u64 },
    TransportChanged { at: DateTime<Utc>, playing: bool },
    SessionEnd   { at: DateTime<Utc>, reason: String },
}
```

CSV columns: `timestamp_utc,event,bpm_from,bpm_to,peer_count,playing,note`. Unused cells empty.

## 6. UI layout

Width grows to fit terminal; below assumes ≥ 60 cols. Refresh ~30 Hz.

```
╔═ LinkCLIHost ═════════════════ peer: my-laptop ════════════╗
║  Tempo        120.00 BPM        Quantum     4              ║
║  Beat         142.37            Phase       2.37 / 4       ║
║  Playing      yes               Peers       3              ║
║  Uptime       00:14:22          Link clock  874 632 199 µs ║
║  Tempo σ      0.12 BPM          Last Δ      −0.50 @ 12:03  ║
╠═ Phase ════════════════════════════════════════════════════╣
║  ████████████████████████████████░░░░░░░░░░░░░░░░░░░░░░░░  ║
╠═ Recent tempo changes ═════════════════════════════════════╣
║  12:03:17  120.50 → 120.00   Δ −0.50                       ║
║  12:02:50  119.00 → 120.50   Δ +1.50                       ║
║  12:01:11  120.00 → 119.00   Δ −1.00                       ║
║                                                            ║
╠════════════════════════════════════════════════════════════╣
║  q  quit                       log: ./events.jsonl         ║
╚════════════════════════════════════════════════════════════╝
```

Below 50 cols the UI falls back to a single-column listing with no box drawing.

### Widget breakdown
- **Header block** — 5-row two-column key/value grid.
- **Phase bar** — single-row gauge: filled portion = `phase / quantum`, characters `█` and `░`. Refreshes every tick.
- **Recent tempo changes** — list widget bound to the latest 5 entries in the history ring buffer.
- **Footer** — keybindings on left, log file path (or `log: none`) on right.

If `--no-tui` is passed, the program instead prints one line per `Event` to stdout (timestamped, human-readable) and never enters raw mode. Useful for piping into `tee` / journals or for CI smoke tests.

## 7. CLI surface

```
linkclihost [OPTIONS]

Options:
  -q, --quantum <BEATS>     Beats per cycle (musical bar). Default: 4
  -n, --name <NAME>         Peer display name in the header.
                            Default: hostname
      --log <PATH>          Append events to file. Format is inferred from
                            extension: .jsonl / .ndjson → JSON Lines,
                            .csv → CSV. Default: no log.
      --no-tui              Disable TUI; print events to stdout instead.
      --initial-bpm <BPM>   Tempo announced if we're the first peer in the
                            session. Default: 120.0
  -h, --help                Print help
  -V, --version             Print version
```

Examples:

```
linkclihost
linkclihost --log run.jsonl
linkclihost --quantum 8 --no-tui | tee bands-soundcheck.log
```

## 8. Logging format

JSONL (one JSON object per line):

```json
{"type":"session_start","at":"2026-05-28T12:03:14.812Z","quantum":4,"peer_name":"my-laptop"}
{"type":"peers_changed","at":"2026-05-28T12:03:16.500Z","count":1}
{"type":"tempo_changed","at":"2026-05-28T12:03:17.014Z","from_bpm":120.0,"to_bpm":119.5}
{"type":"transport_changed","at":"2026-05-28T12:03:25.401Z","playing":true}
{"type":"session_end","at":"2026-05-28T12:18:02.119Z","reason":"sigint"}
```

CSV (header row written first):

```
timestamp_utc,event,bpm_from,bpm_to,peer_count,playing,note
2026-05-28T12:03:14.812Z,session_start,,,,,quantum=4 peer=my-laptop
2026-05-28T12:03:16.500Z,peers_changed,,,1,,
2026-05-28T12:03:17.014Z,tempo_changed,120.0,119.5,,,
2026-05-28T12:03:25.401Z,transport_changed,,,,true,
2026-05-28T12:18:02.119Z,session_end,,,,,sigint
```

Choice of format is purely from the file extension. Anything else → error before TUI starts.

## 9. Error handling

| Situation | Behaviour |
|---|---|
| `--log` path's parent directory doesn't exist | Print error to stderr, exit code 2 before entering TUI. |
| `--log` extension unrecognized | Same as above. |
| Terminal isn't a TTY and `--no-tui` not set | Auto-fallback to `--no-tui` mode, print one-line notice on stderr. |
| Terminal too small (< 30 cols or < 12 rows) | Render a "terminal too small" message; keep polling for resize. |
| SIGINT / SIGTERM | Restore terminal, flush logger, write `session_end`, exit code 0. |
| Panic on render or callback thread | Catch in `main`, restore terminal, print panic info to stderr, exit code 101 (standard Rust). |
| `AblLink::enable(true)` failure (network init) | Show "Link offline" badge in header; keep TUI running, retry every second. |

No retries inside Link callbacks — they must not panic and must not block; they just lock the mutex, update state, and send the event.

## 10. Concurrency contracts

- **Link callbacks** run on Link's internal thread. They acquire the `AppState` mutex briefly, append to history, push to the event channel, release. No I/O, no allocation beyond `Vec::push` into the ring buffer (preallocated).
- **Render loop** acquires the mutex, clones a `Snapshot` (cheap — small struct + a `Vec<TempoChange>` with ≤ 5 entries), releases, then draws. Lock held for microseconds, never across `terminal.draw()`.
- **Logger thread** owns the file handle; it's never touched from anywhere else. The channel is unbounded — capped indirectly by Link's emission rate (a handful of events per minute in practice).

## 11. Testing strategy

### Unit tests (no Link required)
- `events.rs`: round-trip JSONL and CSV serialization for each variant.
- `app.rs`: `RingBuffer` insertion order, capacity overflow, `tempo_stability_bpm` calculation on synthetic histories.
- `cli.rs`: parsing of valid and invalid `--log` extensions; defaults.
- `ui/widgets.rs`: phase bar character count given (phase, quantum, width).

### Integration tests
- **Headless smoke test** (`--no-tui`, exits after a fixed duration): spin up two `linkclihost` processes against loopback multicast, observe that each one logs `peers_changed` with `count=1`. Gated behind a `--features integration` feature flag because it depends on multicast working on the runner.
- **Snapshot test for TUI**: use `ratatui::backend::TestBackend` to render the UI against a fixed `Snapshot` and compare the resulting buffer to a golden file.

### Manual test recipe (documented in README)
1. Open Ableton Live (or another Link-aware app) on the same network.
2. Enable Link.
3. Run `linkclihost --log run.jsonl`.
4. Confirm `peers` becomes 1, tempo matches Live's tempo, changes in Live appear within ~50 ms.

Link's actual networking behaviour cannot be unit-tested without integration — be honest about that.

## 12. Build & run

```bash
git clone <repo>
cd LinkCLIHost
cargo build --release
./target/release/linkclihost --log demo.jsonl
```

Release binary is ~2-3 MB statically linked against the bundled Link C++. First build is slow (Link compile) — subsequent builds are incremental.

Optional `Justfile` for the common verbs (`just run`, `just test`, `just lint`).

## 13. Open questions

None. All design choices have been made:

- Quantum default: 4 (configurable via `--quantum`).
- Start/stop sync: always enabled (no flag — the spec says "no transport control" but listening is fine).
- Peer name: hostname by default, overridable via `--name`.
- Refresh rate: 30 Hz.
- History size: 128 entries, 5 shown.

## 14. Future work (deliberately deferred)

- `--no-tui` JSON-stream mode is included; a full structured-log mode that prints one JSON object per tick would be a small follow-up.
- A `--summary` flag that reads a previously-recorded JSONL/CSV and prints aggregate statistics (tempo histogram, total downtime, peer churn).
- A `--listen-port` flag if/when Ableton Link exposes one; today it's discovery-only.
- macOS / Windows support — should "just work" given `rusty_link`'s cross-platform story, but not in initial scope.

---

**End of design.**
