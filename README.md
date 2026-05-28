# LinkCLIHost

A headless [Ableton Link](https://github.com/Ableton/link) peer for Linux, with a live TUI dashboard and optional event log.

## Build

```bash
sudo apt install build-essential cmake pkg-config libclang-dev
cargo build --release
```

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
```

Press `q` (or `Esc`) to quit. `Ctrl-C` also exits cleanly.

If stdout is not a TTY (piped or redirected), the program automatically falls back to `--no-tui` mode and prints a notice to stderr.

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

## CLI options

| Flag | Default | Description |
| --- | --- | --- |
| `-q`, `--quantum <BEATS>` | `4` | Beats per cycle (musical bar). |
| `-n`, `--name <NAME>` | hostname | Peer display name in the header. |
| `--log <PATH>` | none | Append events to file. Format inferred from extension. |
| `--no-tui` | off | Disable TUI; print events to stdout. |
| `--initial-bpm <BPM>` | `120.0` | Tempo announced if we're the first peer in the session. |
| `-h`, `--help` | — | Print help. |
| `-V`, `--version` | — | Print version. |

## Tests

```bash
cargo test                                                # unit tests (~39)
cargo test --release --test smoke -- --ignored            # binary smoke test
```

## Design and plan

See `docs/superpowers/specs/` and `docs/superpowers/plans/` for the design spec and implementation plan that produced this code.

## License

Apache-2.0 — see [LICENSE](LICENSE).
