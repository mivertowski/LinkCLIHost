//! Step-sequencer pattern data and Link-phase-to-step mapping.
//!
//! Patterns are one bar of 16th notes (4 steps per Link beat) across four
//! drum tracks; the bar length follows the meter (4/4 -> 16 steps,
//! 5/4 -> 20, 7/8 -> 14). Steps are derived from the session *phase*, so
//! every peer that joins the same Link session hears the same step at the
//! same time.
//!
//! Each preset name has hand-written variants for common bar lengths
//! (3/4 & 6/8, 7/8, 4/4, 5/4). Other lengths fall back to tiling the 4/4
//! pattern across the bar, which keeps the kick on the grid but won't
//! re-accent the meter's grouping.

pub const TRACKS: usize = 4;
/// Longest supported bar: 32 sixteenths (e.g. 8/4 or 16/8).
pub const MAX_STEPS: usize = 32;
pub const STEPS_PER_BEAT: f64 = 4.0;
pub const NUM_PRESETS: usize = 4;

pub const TRACK_NAMES: [&str; TRACKS] = ["BD", "SD", "HH", "TOM"];

/// One pattern preset. Each row is a bitmask, bit 0 = step 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    pub name: &'static str,
    pub steps: usize,
    rows: [u32; TRACKS],
}

impl Preset {
    pub const fn new(
        name: &'static str,
        steps: usize,
        bd: u32,
        sd: u32,
        hh: u32,
        tom: u32,
    ) -> Self {
        Self {
            name,
            steps,
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

    /// Repeat this pattern across `steps` steps (truncating or tiling).
    fn tiled_to(&self, steps: usize) -> Preset {
        let mut rows = [0u32; TRACKS];
        for (t, row) in rows.iter_mut().enumerate() {
            for i in 0..steps {
                if self.step(t, i % self.steps) {
                    *row |= 1 << i;
                }
            }
        }
        Preset {
            name: self.name,
            steps,
            rows,
        }
    }
}

/// Build a row mask from a human-readable pattern string, e.g. "x...x...".
/// Any char other than '.' or ' ' marks an active step.
const fn row(s: &str) -> u32 {
    let bytes = s.as_bytes();
    let mut mask: u32 = 0;
    let mut i = 0;
    while i < bytes.len() && i < MAX_STEPS {
        if bytes[i] != b'.' && bytes[i] != b' ' {
            mask |= 1 << i;
        }
        i += 1;
    }
    mask
}

/// 4/4 — one bar of 16 sixteenths. Also the tiling source for uncommon meters.
const BANK_16: [Preset; NUM_PRESETS] = [
    Preset::new(
        "four-floor",
        16,
        row("x...x...x...x..."), // BD
        row("....x.......x..."), // SD
        row("..x...x...x...x."), // HH
        row("..............x."), // TOM
    ),
    Preset::new(
        "backbeat",
        16,
        row("x......x..x....."),
        row("....x.......x..."),
        row("x.x.x.x.x.x.x.x."),
        row(".............x.."),
    ),
    Preset::new(
        "breaks",
        16,
        row("x.x.......x..x.."),
        row("....x..x.x..x..x"),
        row("x.x.x.x.x.x.x.x."),
        row("................"),
    ),
    Preset::new(
        "tom-jam",
        16,
        row("x.......x......."),
        row("....x.......x..."),
        row("x..x..x..x..x..x"),
        row("..x...x...x.xx.."),
    ),
];

/// 5/4 — 20 sixteenths, grouped 3+2 quarters (Take-Five feel).
const BANK_20: [Preset; NUM_PRESETS] = [
    Preset::new(
        "four-floor",
        20,
        row("x...x...x...x...x..."),
        row("....x.......x......."),
        row("..x...x...x...x...x."),
        row("..................x."),
    ),
    Preset::new(
        "backbeat",
        20,
        row("x.....x.....x...x..."),
        row("....x.......x.....x."),
        row("x.x.x.x.x.x.x.x.x.x."),
        row(".................x.."),
    ),
    Preset::new(
        "breaks",
        20,
        row("x.x.......x..x......"),
        row("....x..x.x..x..x..x."),
        row("x.x.x.x.x.x.x.x.x.x."),
        row("................x.x."),
    ),
    Preset::new(
        "tom-jam",
        20,
        row("x.......x.......x..."),
        row("....x.......x......."),
        row("x..x..x..x..x..x..x."),
        row("..x...x...x.xx....xx"),
    ),
];

/// 7/8 — 14 sixteenths, grouped 2+2+3 eighths (group starts at 16ths 0, 4, 8).
const BANK_14: [Preset; NUM_PRESETS] = [
    Preset::new(
        "four-floor",
        14,
        row("x...x...x....."),
        row("........x....."),
        row("x.x.x.x.x.x.x."),
        row("............x."),
    ),
    Preset::new(
        "backbeat",
        14,
        row("x.....x...x..."),
        row("....x.......x."),
        row("x.x.x.x.x.x.x."),
        row(".........x...."),
    ),
    Preset::new(
        "breaks",
        14,
        row("x.x...x.....x."),
        row("....x....x...."),
        row("x.x.x.x.x.x.x."),
        row(".............."),
    ),
    Preset::new(
        "tom-jam",
        14,
        row("x.......x....."),
        row("....x........."),
        row("x..x..x..x..x."),
        row("..x...x...xx.."),
    ),
];

/// 3/4 or 6/8 — 12 sixteenths.
const BANK_12: [Preset; NUM_PRESETS] = [
    Preset::new(
        "four-floor",
        12,
        row("x...x...x..."),
        row("....x......."),
        row("..x...x...x."),
        row("..........x."),
    ),
    Preset::new(
        "backbeat",
        12,
        row("x.....x....."),
        row("....x...x..."),
        row("x.x.x.x.x.x."),
        row(".........x.."),
    ),
    Preset::new(
        "breaks",
        12,
        row("x.x....x...."),
        row("...x..x...x."),
        row("x.x.x.x.x.x."),
        row("............"),
    ),
    Preset::new(
        "tom-jam",
        12,
        row("x.....x....."),
        row("....x......."),
        row("x..x..x..x.."),
        row("..x..x..xx.x"),
    ),
];

/// Preset bank for a bar of `steps` sixteenths: hand-written for common
/// meters, tiled from the 4/4 bank otherwise.
pub fn presets_for(steps: usize) -> [Preset; NUM_PRESETS] {
    let steps = steps.clamp(1, MAX_STEPS);
    match steps {
        12 => BANK_12,
        14 => BANK_14,
        16 => BANK_16,
        20 => BANK_20,
        n => BANK_16.map(|p| p.tiled_to(n)),
    }
}

pub fn preset_names() -> Vec<&'static str> {
    BANK_16.iter().map(|p| p.name).collect()
}

pub fn preset_index(name: &str) -> Option<usize> {
    BANK_16.iter().position(|p| p.name == name)
}

/// Sixteenths per bar for a Link quantum (quarter-note beats per bar).
pub fn steps_for_quantum(quantum: f64) -> usize {
    ((quantum * STEPS_PER_BEAT).round() as usize).clamp(1, MAX_STEPS)
}

/// Map a Link phase (0..quantum, in beats) to a step index within the bar.
pub fn step_at_phase(phase: f64, steps: usize) -> usize {
    let step = (phase * STEPS_PER_BEAT).floor() as i64;
    step.rem_euclid(steps.max(1) as i64) as usize
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
    fn four_floor_kick_on_quarters_in_four_four() {
        let p = &presets_for(16)[0];
        for step in 0..16 {
            assert_eq!(p.step(0, step), step % 4 == 0, "step {step}");
        }
    }

    #[test]
    fn five_four_bank_has_twenty_steps_and_five_kicks() {
        let p = &presets_for(20)[0]; // four-floor
        assert_eq!(p.steps, 20);
        let kicks = (0..20).filter(|s| p.step(0, *s)).count();
        assert_eq!(kicks, 5);
        assert!(p.step(0, 16), "kick on beat 5");
    }

    #[test]
    fn seven_eight_bank_has_fourteen_steps() {
        for p in &presets_for(14) {
            assert_eq!(p.steps, 14);
            assert!(p.step(0, 0), "{}: downbeat kick", p.name);
            // Nothing may be set beyond the bar.
            for s in 14..MAX_STEPS {
                for t in 0..TRACKS {
                    assert!(!p.step(t, s), "{}: stray hit at {s}", p.name);
                }
            }
        }
    }

    #[test]
    fn uncommon_meter_tiles_the_four_four_bank() {
        // 9/8 = 18 sixteenths: no hand-written bank, tiled from 4/4.
        let bank = presets_for(18);
        let base = &presets_for(16)[0];
        let p = &bank[0];
        assert_eq!(p.steps, 18);
        for s in 0..18 {
            assert_eq!(p.step(0, s), base.step(0, s % 16), "step {s}");
        }
    }

    #[test]
    fn all_banks_share_preset_names_in_order() {
        let names = preset_names();
        for steps in [12, 14, 16, 18, 20] {
            let bank = presets_for(steps);
            for (i, p) in bank.iter().enumerate() {
                assert_eq!(p.name, names[i], "bank {steps}");
            }
        }
    }

    #[test]
    fn hits_returns_all_tracks_for_step_zero_of_backbeat() {
        let p = &presets_for(16)[1];
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
    fn steps_for_quantum_matches_meters() {
        assert_eq!(steps_for_quantum(4.0), 16); // 4/4
        assert_eq!(steps_for_quantum(5.0), 20); // 5/4
        assert_eq!(steps_for_quantum(3.5), 14); // 7/8
        assert_eq!(steps_for_quantum(3.0), 12); // 3/4, 6/8
        assert_eq!(steps_for_quantum(100.0), MAX_STEPS);
    }

    #[test]
    fn step_at_phase_maps_four_four_bar() {
        assert_eq!(step_at_phase(0.0, 16), 0);
        assert_eq!(step_at_phase(0.25, 16), 1);
        assert_eq!(step_at_phase(1.0, 16), 4);
        assert_eq!(step_at_phase(3.99, 16), 15);
    }

    #[test]
    fn step_at_phase_covers_odd_meter_bars() {
        // 5/4: phase runs 0..5, steps 0..19
        assert_eq!(step_at_phase(4.0, 20), 16);
        assert_eq!(step_at_phase(4.99, 20), 19);
        // 7/8: phase runs 0..3.5, steps 0..13
        assert_eq!(step_at_phase(3.25, 14), 13);
    }

    #[test]
    fn step_at_phase_handles_negative_phase() {
        // Link beat can be negative before transport start
        assert_eq!(step_at_phase(-0.25, 16), 15);
        assert_eq!(step_at_phase(-0.25, 14), 13);
    }
}
