# LinkCLIHost

A headless [Ableton Link](https://github.com/Ableton/link) peer for Linux, with a live TUI dashboard, optional event log, MIDI clock output, and a built-in drum sequencer — all locked to the joined Link session, so you can stress-test a session host audibly and measurably.

## Build

```bash
sudo apt install build-essential cmake pkg-config libclang-dev libasound2-dev
cargo build --release
```

> `libasound2-dev` provides the ALSA headers used by the MIDI (`midir`) and
> audio (`cpal`) backends.

> **Note on `libclang-dev`:** the bundled Ableton Link bindings use `bindgen`,
> which needs Clang's freestanding C headers (`stdbool.h`, `stddef.h`, etc.).
> On Ubuntu 24.04, install `libclang-dev` (which pulls in
> `libclang-common-XX-dev`). If you can't install system packages, you can
> point `bindgen` at GCC's include directory instead via
> `BINDGEN_EXTRA_CLANG_ARGS`. See `.cargo/config.toml.example` for a template
> (this file is gitignored — copy it to `.cargo/config.toml` locally).

## Run

```bash
./target/release/linkclihost                       # TUI dashboard
./target/release/linkclihost --log run.jsonl       # also log events to JSONL
./target/release/linkclihost --log run.csv         # CSV instead of JSONL
./target/release/linkclihost --no-tui              # plain stdout, useful for piping
./target/release/linkclihost --quantum 8 --name studio

./target/release/linkclihost --list-midi-ports     # enumerate MIDI outputs
./target/release/linkclihost --midi-out "Midi Through"   # 24 PPQN clock locked to Link

./target/release/linkclihost --list-audio-devices  # enumerate audio outputs
./target/release/linkclihost --audio               # drum sequencer on default device
./target/release/linkclihost --audio-out pipewire --preset breaks --gain 0.6
```

### Keys

| Key | Action |
| --- | --- |
| `q` / `Esc` | Quit (`Ctrl-C` also exits cleanly) |
| `Space` | Toggle Link transport (start is phase-quantized to the session) |
| `p` | Cycle sequencer preset |
| `m` | Mute/unmute sequencer audio |
| `+` / `-` | Nudge session tempo by ±1 BPM |

If stdout is not a TTY (piped or redirected), the program automatically falls back to `--no-tui` mode and prints a notice to stderr.

## What the dashboard shows

- **Tempo** — current Link tempo in BPM
- **Beat / Phase** — beat number and position within the quantum (bar)
- **Playing** — Link transport state
- **Peers** — number of other Link-aware apps on the same network
- **Uptime** — since this process started
- **Tempo σ** — standard deviation of every tempo we've seen this session
- **Last Δ** — most recent tempo change and timestamp
- **Sequencer / Sync out** — the 4×16 drum grid with the currently sounding step highlighted, the active preset, the audio device, and MIDI clock jitter (mean / max / last scheduling error vs the Link timeline)
- **Recent tempo changes** — last five tempo deltas with wall-clock timestamps

## MIDI clock output

`--midi-out <port>` (index or name substring from `--list-midi-ports`) starts a
dedicated thread that sends MIDI clock at 24 PPQN. Every tick is scheduled from
the Link timeline (`time_at_beat`), so the clock follows session tempo changes
from any peer. Transport edges send Start (`0xFA`) / Stop (`0xFC`); ticks keep
running while stopped (continuous-clock convention) so receivers stay
tempo-locked.

Each tick's scheduling error against the Link timeline is measured. The TUI
shows mean/max/last jitter live, and a `clock_stats` event is appended to the
log roughly every 480 ticks — useful for long-running stability soaks of a
session host.

## Drum sequencer

`--audio` (default output device) or `--audio-out <device>` enables a 16-step
sequencer with four synthesized drum tracks — kick, snare, closed hat, tom —
rendered by a tiny built-in synth (no samples). Steps are 16th notes derived
from the session *phase*, so every peer that joins the same Link session hears
the same step at the same time; the audio callback maps each buffer (including
reported output latency) to a Link beat range and triggers steps
sample-accurately. Transport stopped = silence; voices ring out naturally.

Presets (`--preset`, cycle live with `p`): `four-floor`, `backbeat`, `breaks`,
`tom-jam`.

## Manual test recipe

1. On the same LAN, open another Link-aware app (Ableton Live, Carabiner, another instance of this binary).
2. Enable Link in that app.
3. `Peers` jumps from 0 to 1. The other app's tempo appears here within ~50 ms.
4. Change the tempo there — the change is logged as a `tempo_changed` event.
5. Run with `--audio --midi-out <port> --log soak.jsonl`, press `Space` — drums and MIDI clock start in phase with the session. Leave it running while you stress the session host (tempo sweeps, peers joining/leaving) and watch the jitter line / `clock_stats` events for drift or spikes. Audible stutter in the drums means the host's timeline jumped.

## Event log format

JSONL (`.jsonl` or `.ndjson` extension):

```json
{"type":"session_start","at":"2026-05-28T12:03:14.812Z","quantum":4,"peer_name":"studio"}
{"type":"peers_changed","at":"2026-05-28T12:03:16.500Z","count":1}
{"type":"tempo_changed","at":"2026-05-28T12:03:17.014Z","from_bpm":120.0,"to_bpm":119.5}
{"type":"transport_changed","at":"2026-05-28T12:03:25.401Z","playing":true}
{"type":"clock_stats","at":"2026-05-28T12:08:11.002Z","port":"Midi Through","ticks":480,"mean_abs_err_us":42.5,"max_abs_err_us":310}
{"type":"session_end","at":"2026-05-28T12:18:02.119Z","reason":"sigint"}
```

CSV (`.csv` extension): columns are `timestamp_utc,event,bpm_from,bpm_to,peer_count,playing,note`.

## CLI options

| Flag | Default | Description |
| --- | --- | --- |
| `-q`, `--quantum <BEATS>` | `4` | Beats per cycle (musical bar). |
| `-n`, `--name <NAME>` | hostname | Peer display name in the header. |
| `--log <PATH>` | none | Append events to file. Format inferred from extension. |
| `--no-tui` | off | Disable TUI; print events to stdout. |
| `--initial-bpm <BPM>` | `120.0` | Tempo announced if we're the first peer in the session. |
| `--midi-out <PORT>` | none | Send MIDI clock (24 PPQN) to this port (index or name substring). |
| `--list-midi-ports` | — | List MIDI output ports and exit. |
| `--audio` | off | Enable the drum sequencer on the default audio device. |
| `--audio-out <DEVICE>` | none | Enable the drum sequencer on this device (name substring). |
| `--list-audio-devices` | — | List audio output devices and exit. |
| `--preset <NAME>` | `four-floor` | Sequencer pattern: `four-floor`, `backbeat`, `breaks`, `tom-jam`. |
| `--gain <0..1>` | `0.8` | Sequencer output gain. |
| `-h`, `--help` | — | Print help. |
| `-V`, `--version` | — | Print version. |

## Tests

```bash
cargo test                                                # unit tests (~69)
cargo test --release --test smoke -- --ignored            # binary smoke test
```

## Design and plan

See `docs/superpowers/specs/` and `docs/superpowers/plans/` for the design spec and implementation plan that produced this code.

## License

Apache-2.0 — see [LICENSE](LICENSE).
