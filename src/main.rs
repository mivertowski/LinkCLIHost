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
    println!(
        "link snapshot: {:?} bpm at clock {} us",
        snap.bpm, snap.clock_micros
    );
}
