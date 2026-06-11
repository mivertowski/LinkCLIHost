//! Step-sequencer pattern data and Link-phase-to-step mapping.
//!
//! Patterns are 16 steps of 16th notes (4 steps per Link beat) across four
//! drum tracks. Steps are derived from the session *phase*, so every peer
//! that joins the same Link session hears the same step at the same time.

pub const TRACKS: usize = 4;
pub const STEPS: usize = 16;
pub const STEPS_PER_BEAT: f64 = 4.0;

pub const TRACK_NAMES: [&str; TRACKS] = ["BD", "SD", "HH", "TOM"];

/// One pattern preset. Each row is a 16-bit mask, bit 0 = step 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    pub name: &'static str,
    rows: [u16; TRACKS],
}

impl Preset {
    pub const fn new(name: &'static str, bd: u16, sd: u16, hh: u16, tom: u16) -> Self {
        Self {
            name,
            rows: [bd, sd, hh, tom],
        }
    }

    pub fn step(&self, track: usize, step: usize) -> bool {
        (self.rows[track] >> step) & 1 == 1
    }

    /// Tracks that fire on the given step.
    pub fn hits(&self, step: usize) -> [bool; TRACKS] {
        let mut out = [false; TRACKS];
        for (t, slot) in out.iter_mut().enumerate() {
            *slot = self.step(t, step);
        }
        out
    }
}

/// Build a row mask from a human-readable pattern string, e.g. "x...x...x...x...".
/// Any char other than '.' or ' ' marks an active step.
const fn row(s: &str) -> u16 {
    let bytes = s.as_bytes();
    let mut mask: u16 = 0;
    let mut i = 0;
    while i < bytes.len() && i < STEPS {
        if bytes[i] != b'.' && bytes[i] != b' ' {
            mask |= 1 << i;
        }
        i += 1;
    }
    mask
}

pub const PRESETS: [Preset; 4] = [
    Preset::new(
        "four-floor",
        row("x...x...x...x..."), // BD
        row("....x.......x..."), // SD
        row("..x...x...x...x."), // HH
        row("..............x."), // TOM
    ),
    Preset::new(
        "backbeat",
        row("x......x..x....."),
        row("....x.......x..."),
        row("x.x.x.x.x.x.x.x."),
        row(".............x.."),
    ),
    Preset::new(
        "breaks",
        row("x.x.......x..x.."),
        row("....x..x.x..x..x"),
        row("x.x.x.x.x.x.x.x."),
        row("................"),
    ),
    Preset::new(
        "tom-jam",
        row("x.......x......."),
        row("....x.......x..."),
        row("x..x..x..x..x..x"),
        row("..x...x...x.xx.."),
    ),
];

pub fn preset_names() -> Vec<&'static str> {
    PRESETS.iter().map(|p| p.name).collect()
}

pub fn preset_index(name: &str) -> Option<usize> {
    PRESETS.iter().position(|p| p.name == name)
}

/// Map a Link phase (0..quantum, in beats) to a step index.
/// Steps are 16th notes; for quantum 4 the 16-step pattern spans one bar,
/// for other quanta it simply repeats every 4 beats, staying phase-locked.
pub fn step_at_phase(phase: f64) -> usize {
    let step = (phase * STEPS_PER_BEAT).floor() as i64;
    step.rem_euclid(STEPS as i64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_parses_pattern_string() {
        let m = row("x...x...x...x...");
        assert_eq!(m, 0x1111);
        assert!(m & 1 == 1);
        assert!((m >> 4) & 1 == 1);
        assert!((m >> 1) & 1 == 0);
    }

    #[test]
    fn four_floor_kick_on_quarters() {
        let p = &PRESETS[0];
        for step in 0..STEPS {
            let expected = step % 4 == 0;
            assert_eq!(p.step(0, step), expected, "step {step}");
        }
    }

    #[test]
    fn four_floor_snare_on_two_and_four() {
        let p = &PRESETS[0];
        assert!(p.step(1, 4));
        assert!(p.step(1, 12));
        assert!(!p.step(1, 0));
        assert!(!p.step(1, 8));
    }

    #[test]
    fn hits_returns_all_tracks_for_step_zero_of_backbeat() {
        let p = &PRESETS[1];
        let h = p.hits(0);
        assert!(h[0]); // BD
        assert!(!h[1]); // SD
        assert!(h[2]); // HH
        assert!(!h[3]); // TOM
    }

    #[test]
    fn preset_names_are_unique() {
        let names = preset_names();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names.len(), sorted.len());
    }

    #[test]
    fn preset_index_finds_by_name() {
        assert_eq!(preset_index("four-floor"), Some(0));
        assert_eq!(preset_index("breaks"), Some(2));
        assert_eq!(preset_index("nope"), None);
    }

    #[test]
    fn step_at_phase_maps_quantum_four_bar() {
        assert_eq!(step_at_phase(0.0), 0);
        assert_eq!(step_at_phase(0.25), 1);
        assert_eq!(step_at_phase(1.0), 4);
        assert_eq!(step_at_phase(3.99), 15);
    }

    #[test]
    fn step_at_phase_wraps_for_larger_quantum() {
        // quantum 8: phase 4..8 repeats the 16-step pattern
        assert_eq!(step_at_phase(4.0), 0);
        assert_eq!(step_at_phase(7.75), 15);
    }

    #[test]
    fn step_at_phase_handles_negative_phase() {
        // Link beat can be negative before transport start
        assert_eq!(step_at_phase(-0.25), 15);
    }
}
