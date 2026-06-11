//! Minimal percussion synth: one mono voice per sequencer track.
//!
//! Everything is computed per-sample with exponential decay envelopes, so
//! there are no lookup tables or allocations in the audio path.

use crate::sequencer::TRACKS;
use std::f32::consts::TAU;

/// Simple xorshift noise source; deterministic and allocation-free.
struct Noise(u32);

impl Noise {
    fn new() -> Self {
        Noise(0x9E37_79B9)
    }

    /// Uniform noise in [-1, 1).
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DrumKind {
    Kick,
    Snare,
    HatClosed,
    Tom,
}

const KINDS: [DrumKind; TRACKS] = [
    DrumKind::Kick,
    DrumKind::Snare,
    DrumKind::HatClosed,
    DrumKind::Tom,
];

struct Voice {
    kind: DrumKind,
    active: bool,
    /// Seconds since trigger.
    t: f32,
    /// Oscillator phase in radians.
    phase: f32,
    noise: Noise,
    /// One-pole highpass state (for the hat).
    hp_prev_in: f32,
    hp_prev_out: f32,
}

impl Voice {
    fn new(kind: DrumKind) -> Self {
        Self {
            kind,
            active: false,
            t: 0.0,
            phase: 0.0,
            noise: Noise::new(),
            hp_prev_in: 0.0,
            hp_prev_out: 0.0,
        }
    }

    fn trigger(&mut self) {
        self.active = true;
        self.t = 0.0;
        self.phase = 0.0;
    }

    fn render(&mut self, dt: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        let t = self.t;
        self.t += dt;
        let out = match self.kind {
            DrumKind::Kick => {
                // Pitch sweep 150 Hz -> 48 Hz, punchy decay.
                let f = 48.0 + 102.0 * (-t * 28.0).exp();
                self.phase = (self.phase + TAU * f * dt) % TAU;
                self.phase.sin() * (-t * 9.0).exp()
            }
            DrumKind::Snare => {
                let f = 185.0;
                self.phase = (self.phase + TAU * f * dt) % TAU;
                let body = self.phase.sin() * (-t * 24.0).exp() * 0.5;
                let rattle = self.noise.next() * (-t * 16.0).exp() * 0.6;
                body + rattle
            }
            DrumKind::HatClosed => {
                // Highpassed noise, very short decay.
                let n = self.noise.next();
                let hp = 0.95 * (self.hp_prev_out + n - self.hp_prev_in);
                self.hp_prev_in = n;
                self.hp_prev_out = hp;
                hp * (-t * 55.0).exp() * 0.5
            }
            DrumKind::Tom => {
                // Pitch sweep 130 Hz -> 90 Hz, longer decay than the kick.
                let f = 90.0 + 40.0 * (-t * 12.0).exp();
                self.phase = (self.phase + TAU * f * dt) % TAU;
                self.phase.sin() * (-t * 7.0).exp() * 0.8
            }
        };
        // Kill the voice once it has decayed to silence.
        if self.t > 1.5 {
            self.active = false;
        }
        out
    }
}

pub struct DrumSynth {
    voices: [Voice; TRACKS],
    dt: f32,
    gain: f32,
}

impl DrumSynth {
    pub fn new(sample_rate: f32, gain: f32) -> Self {
        Self {
            voices: KINDS.map(Voice::new),
            dt: 1.0 / sample_rate,
            gain,
        }
    }

    /// Retrigger the voices for tracks marked true.
    pub fn trigger(&mut self, hits: &[bool; TRACKS]) {
        for (voice, hit) in self.voices.iter_mut().zip(hits) {
            if *hit {
                voice.trigger();
            }
        }
    }

    /// Render and mix one mono sample, soft-clipped to [-1, 1].
    pub fn next_sample(&mut self) -> f32 {
        let mut mix = 0.0;
        for v in self.voices.iter_mut() {
            mix += v.render(self.dt);
        }
        (mix * self.gain).tanh()
    }

    #[cfg(test)]
    pub fn any_active(&self) -> bool {
        self.voices.iter().any(|v| v.active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn render(synth: &mut DrumSynth, n: usize) -> Vec<f32> {
        (0..n).map(|_| synth.next_sample()).collect()
    }

    #[test]
    fn silent_until_triggered() {
        let mut s = DrumSynth::new(SR, 0.8);
        let out = render(&mut s, 256);
        assert!(out.iter().all(|x| *x == 0.0));
        assert!(!s.any_active());
    }

    #[test]
    fn kick_produces_audio_then_decays() {
        let mut s = DrumSynth::new(SR, 0.8);
        s.trigger(&[true, false, false, false]);
        let early = render(&mut s, 4_800); // first 100 ms
        let peak_early = early.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(
            peak_early > 0.1,
            "kick should be audible, peak={peak_early}"
        );

        render(&mut s, (SR * 1.5) as usize); // let it decay fully
        let late = render(&mut s, 256);
        let peak_late = late.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak_late < 1e-3, "kick should decay, peak={peak_late}");
        assert!(!s.any_active());
    }

    #[test]
    fn every_track_makes_sound() {
        for track in 0..TRACKS {
            let mut s = DrumSynth::new(SR, 0.8);
            let mut hits = [false; TRACKS];
            hits[track] = true;
            s.trigger(&hits);
            let out = render(&mut s, 2_400);
            let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(peak > 0.05, "track {track} silent, peak={peak}");
        }
    }

    #[test]
    fn output_is_bounded() {
        let mut s = DrumSynth::new(SR, 2.0); // hot gain
        s.trigger(&[true, true, true, true]);
        let out = render(&mut s, 9_600);
        assert!(out.iter().all(|x| x.abs() <= 1.0));
    }

    #[test]
    fn retrigger_restarts_envelope() {
        let mut s = DrumSynth::new(SR, 0.8);
        s.trigger(&[true, false, false, false]);
        render(&mut s, (SR * 0.5) as usize);
        s.trigger(&[true, false, false, false]);
        let out = render(&mut s, 480);
        let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.1, "retrigger should restart the kick, peak={peak}");
    }
}
