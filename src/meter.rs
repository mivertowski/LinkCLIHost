//! Time-signature handling on top of Link's quantum.
//!
//! Link itself has no meter concept — only a `quantum` (beats per bar, where
//! a beat is a quarter note). A meter maps onto that: 5/4 -> quantum 5,
//! 7/8 -> quantum 3.5, 6/8 -> quantum 3. The sequencer derives its pattern
//! length (16th notes per bar) from the same fraction.

use crate::sequencer::MAX_STEPS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Meter {
    pub num: u32,
    pub den: u32,
}

impl Meter {
    /// Parse "N/D", e.g. "4/4", "5/4", "7/8". The denominator must be a
    /// note value the 16th-note grid can represent (2, 4, 8, or 16).
    pub fn parse(s: &str) -> Result<Meter, String> {
        let err = || format!("invalid meter \"{s}\" (expected N/D, e.g. 4/4, 5/4, 7/8)");
        let (n, d) = s.split_once('/').ok_or_else(err)?;
        let num: u32 = n.trim().parse().map_err(|_| err())?;
        let den: u32 = d.trim().parse().map_err(|_| err())?;
        if num == 0 {
            return Err(err());
        }
        if !matches!(den, 2 | 4 | 8 | 16) {
            return Err(format!(
                "invalid meter \"{s}\": denominator must be 2, 4, 8, or 16"
            ));
        }
        let m = Meter { num, den };
        if m.steps() > MAX_STEPS {
            return Err(format!(
                "meter \"{s}\" is too long: {} sixteenths per bar (max {MAX_STEPS})",
                m.steps()
            ));
        }
        Ok(m)
    }

    /// Link quantum (quarter-note beats per bar).
    pub fn quantum(&self) -> f64 {
        self.num as f64 * 4.0 / self.den as f64
    }

    /// Sixteenth notes per bar.
    pub fn steps(&self) -> usize {
        (self.num as usize) * 16 / (self.den as usize)
    }

    pub fn label(&self) -> String {
        format!("{}/{}", self.num, self.den)
    }
}

/// Best-effort meter label for a bare quantum value (used when the user
/// passes `--quantum` instead of `--meter`). Prefers /4, then /8, then /16.
pub fn label_for_quantum(quantum: f64) -> String {
    let sixteenths = quantum * 4.0;
    if sixteenths > 0.0 && (sixteenths - sixteenths.round()).abs() < 1e-9 {
        let s = sixteenths.round() as u32;
        if s % 4 == 0 {
            return format!("{}/4", s / 4);
        }
        if s % 2 == 0 {
            return format!("{}/8", s / 2);
        }
        return format!("{s}/16");
    }
    format!("q={quantum}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_meters() {
        assert_eq!(Meter::parse("4/4"), Ok(Meter { num: 4, den: 4 }));
        assert_eq!(Meter::parse("5/4"), Ok(Meter { num: 5, den: 4 }));
        assert_eq!(Meter::parse("7/8"), Ok(Meter { num: 7, den: 8 }));
        assert_eq!(Meter::parse(" 6 / 8 "), Ok(Meter { num: 6, den: 8 }));
    }

    #[test]
    fn quantum_is_quarter_beats_per_bar() {
        assert_eq!(Meter::parse("4/4").unwrap().quantum(), 4.0);
        assert_eq!(Meter::parse("5/4").unwrap().quantum(), 5.0);
        assert_eq!(Meter::parse("7/8").unwrap().quantum(), 3.5);
        assert_eq!(Meter::parse("6/8").unwrap().quantum(), 3.0);
        assert_eq!(Meter::parse("3/4").unwrap().quantum(), 3.0);
    }

    #[test]
    fn steps_is_sixteenths_per_bar() {
        assert_eq!(Meter::parse("4/4").unwrap().steps(), 16);
        assert_eq!(Meter::parse("5/4").unwrap().steps(), 20);
        assert_eq!(Meter::parse("7/8").unwrap().steps(), 14);
        assert_eq!(Meter::parse("6/8").unwrap().steps(), 12);
        assert_eq!(Meter::parse("15/16").unwrap().steps(), 15);
    }

    #[test]
    fn rejects_malformed_input() {
        assert!(Meter::parse("44").is_err());
        assert!(Meter::parse("0/4").is_err());
        assert!(Meter::parse("4/3").is_err());
        assert!(Meter::parse("a/b").is_err());
        assert!(Meter::parse("").is_err());
    }

    #[test]
    fn rejects_bars_longer_than_max_steps() {
        assert!(Meter::parse("9/4").is_err()); // 36 sixteenths
        assert!(Meter::parse("8/4").is_ok()); // 32 sixteenths, exactly max
        assert!(Meter::parse("17/8").is_err()); // 34 sixteenths
    }

    #[test]
    fn label_round_trips() {
        assert_eq!(Meter::parse("7/8").unwrap().label(), "7/8");
    }

    #[test]
    fn quantum_labels_prefer_quarters() {
        assert_eq!(label_for_quantum(4.0), "4/4");
        assert_eq!(label_for_quantum(5.0), "5/4");
        assert_eq!(label_for_quantum(3.5), "7/8");
        assert_eq!(label_for_quantum(3.0), "3/4");
        assert_eq!(label_for_quantum(3.75), "15/16");
        assert_eq!(label_for_quantum(4.3), "q=4.3");
    }
}
