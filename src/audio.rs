//! Audio output: drives the drum synth from the Link timeline inside the
//! cpal callback.
//!
//! Each callback captures the *audio* session state, maps the buffer's
//! wall-clock span (plus reported output latency) to a Link beat range, and
//! triggers sequencer steps at the exact frame where each 16th-note boundary
//! falls. All shared control state is atomic, so the audio thread never
//! blocks on a lock.

use crate::sequencer::{self, NUM_PRESETS, STEPS_PER_BEAT, TRACKS};
use crate::synth::DrumSynth;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use rusty_link::{AblLink, SessionState};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

/// Control/observation state shared between the UI and the audio callback.
pub struct AudioShared {
    pub preset_idx: AtomicUsize,
    pub muted: AtomicBool,
    /// Step currently sounding, or -1 while transport is stopped.
    pub current_step: AtomicI64,
    pub stream_errors: AtomicU64,
}

impl AudioShared {
    pub fn new(preset_idx: usize) -> Self {
        Self {
            preset_idx: AtomicUsize::new(preset_idx),
            muted: AtomicBool::new(false),
            current_step: AtomicI64::new(-1),
            stream_errors: AtomicU64::new(0),
        }
    }

    pub fn next_preset(&self) {
        let _ = self
            .preset_idx
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |i| {
                Some((i + 1) % NUM_PRESETS)
            });
    }

    pub fn toggle_mute(&self) {
        let _ = self
            .muted
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |m| Some(!m));
    }
}

pub struct AudioEngine {
    pub device_name: String,
    pub sample_rate: u32,
    // Held to keep the stream alive; dropped (and stopped) with the engine.
    _stream: cpal::Stream,
}

pub fn list_devices() -> Result<Vec<String>, String> {
    let host = cpal::default_host();
    let devices = host
        .output_devices()
        .map_err(|e| format!("audio devices: {e}"))?;
    Ok(devices
        .map(|d| d.name().unwrap_or_else(|_| "<unknown>".into()))
        .collect())
}

fn pick_device(selector: Option<&str>) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    match selector {
        None => host
            .default_output_device()
            .ok_or_else(|| "no default audio output device".into()),
        Some(sel) => {
            let needle = sel.to_lowercase();
            let mut devices = host
                .output_devices()
                .map_err(|e| format!("audio devices: {e}"))?;
            devices
                .find(|d| {
                    d.name()
                        .map(|n| n.to_lowercase().contains(&needle))
                        .unwrap_or(false)
                })
                .ok_or_else(|| {
                    format!("no audio device matches \"{sel}\" (try --list-audio-devices)")
                })
        }
    }
}

pub fn start(
    link: Arc<AblLink>,
    quantum: f64,
    shared: Arc<AudioShared>,
    selector: Option<&str>,
    gain: f32,
) -> Result<AudioEngine, String> {
    let device = pick_device(selector)?;
    let device_name = device.name().unwrap_or_else(|_| "<unknown>".into());
    let config = device
        .default_output_config()
        .map_err(|e| format!("audio config: {e}"))?;
    let sample_rate = config.sample_rate().0;

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            build_stream::<f32>(&device, &config.into(), link, quantum, shared, gain)
        }
        cpal::SampleFormat::I16 => {
            build_stream::<i16>(&device, &config.into(), link, quantum, shared, gain)
        }
        cpal::SampleFormat::U16 => {
            build_stream::<u16>(&device, &config.into(), link, quantum, shared, gain)
        }
        other => Err(format!("unsupported sample format: {other}")),
    }?;
    stream.play().map_err(|e| format!("audio start: {e}"))?;

    Ok(AudioEngine {
        device_name,
        sample_rate,
        _stream: stream,
    })
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    link: Arc<AblLink>,
    quantum: f64,
    shared: Arc<AudioShared>,
    gain: f32,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate.0 as f64;
    let mut synth = DrumSynth::new(sample_rate as f32, gain);
    let mut state = SessionState::new();
    // Bar length and preset bank are fixed for the run (derived from the
    // meter/quantum), so resolve them once outside the callback.
    let steps = sequencer::steps_for_quantum(quantum);
    let bank = sequencer::presets_for(steps);
    // Global 16th-note counter of the last triggered tick; avoids
    // double-triggering across buffer boundaries.
    let mut last_tick: i64 = i64::MIN;

    let err_shared = shared.clone();
    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
                let ts = info.timestamp();
                let latency_us = ts
                    .playback
                    .duration_since(&ts.callback)
                    .map(|d| d.as_micros() as i64)
                    .unwrap_or(0);
                let t0 = link.clock_micros() + latency_us;

                link.capture_audio_session_state(&mut state);
                let playing = state.is_playing();
                let frames = data.len() / channels;
                let buf_us = (frames as f64 / sample_rate * 1e6) as i64;
                let b0 = state.beat_at_time(t0, quantum);
                let b1 = state.beat_at_time(t0 + buf_us, quantum);
                let beats_per_frame = (b1 - b0) / frames.max(1) as f64;

                let preset = &bank[shared.preset_idx.load(Ordering::Relaxed) % NUM_PRESETS];
                let muted = shared.muted.load(Ordering::Relaxed);

                if playing {
                    shared
                        .current_step
                        .store(step_from_beat(b0, quantum, steps) as i64, Ordering::Relaxed);
                } else {
                    shared.current_step.store(-1, Ordering::Relaxed);
                }

                for frame in 0..frames {
                    if playing {
                        let beat = b0 + frame as f64 * beats_per_frame;
                        let tick = (beat * STEPS_PER_BEAT).floor() as i64;
                        if tick != last_tick {
                            last_tick = tick;
                            let hits: [bool; TRACKS] =
                                preset.hits(step_from_beat(beat, quantum, steps));
                            synth.trigger(&hits);
                        }
                    }
                    let s = synth.next_sample();
                    let v = T::from_sample(if muted { 0.0 } else { s });
                    for ch in 0..channels {
                        data[frame * channels + ch] = v;
                    }
                }
            },
            move |err| {
                let _ = err;
                err_shared.stream_errors.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|e| format!("audio stream: {e}"))?;
    Ok(stream)
}

/// Step index for a Link beat value. Link defines phase = beat mod quantum,
/// so this stays aligned with what other peers display.
fn step_from_beat(beat: f64, quantum: f64, steps: usize) -> usize {
    sequencer::step_at_phase(beat.rem_euclid(quantum.max(f64::EPSILON)), steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_from_beat_wraps_at_quantum() {
        assert_eq!(step_from_beat(0.0, 4.0, 16), 0);
        assert_eq!(step_from_beat(4.0, 4.0, 16), 0);
        assert_eq!(step_from_beat(5.25, 4.0, 16), 5);
        assert_eq!(step_from_beat(-0.25, 4.0, 16), 15);
    }

    #[test]
    fn step_from_beat_covers_odd_meters() {
        // 5/4: quantum 5, 20 steps
        assert_eq!(step_from_beat(4.75, 5.0, 20), 19);
        assert_eq!(step_from_beat(5.0, 5.0, 20), 0);
        // 7/8: quantum 3.5, 14 steps
        assert_eq!(step_from_beat(3.25, 3.5, 14), 13);
        assert_eq!(step_from_beat(3.5, 3.5, 14), 0);
    }

    #[test]
    fn shared_preset_cycles() {
        let s = AudioShared::new(0);
        for _ in 0..NUM_PRESETS {
            s.next_preset();
        }
        assert_eq!(s.preset_idx.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn shared_mute_toggles() {
        let s = AudioShared::new(0);
        assert!(!s.muted.load(Ordering::SeqCst));
        s.toggle_mute();
        assert!(s.muted.load(Ordering::SeqCst));
        s.toggle_mute();
        assert!(!s.muted.load(Ordering::SeqCst));
    }
}
