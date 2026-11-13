//! Sample-accurate metronome engine.
//!
//! The audio stream is the only clock. `render` is called from the audio
//! thread for every chunk and writes the clicks at exact sample positions.
//! There are no timers, no sleeps and no locks: the UI talks to the engine
//! through atomics in `Control`.

use crate::meter::{CODE_ACCENT, CODE_NORMAL};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering::Relaxed},
    Arc,
};

/// Parameters shared between the UI thread and the audio thread.
#[derive(Debug)]
pub struct Control {
    bpm: AtomicU32,
    beats_per_bar: AtomicU32,
    accents: Vec<AtomicU8>,
    reset_position: AtomicBool,
}

impl Control {
    pub fn new(max_beats: usize) -> Self {
        let accents = (0..max_beats.max(1))
            .map(|i| AtomicU8::new(if i == 0 { CODE_ACCENT } else { CODE_NORMAL }))
            .collect();
        Self {
            bpm: AtomicU32::new(120),
            beats_per_bar: AtomicU32::new(4),
            accents,
            reset_position: AtomicBool::new(false),
        }
    }

    pub fn set_bpm(&self, bpm: u32) {
        self.bpm.store(bpm.max(1), Relaxed);
    }

    /// `reset_position` restarts the bar from the downbeat on the next click;
    /// set it only when the numerator changes, not for accent edits.
    pub fn set_meter(&self, beats_per_bar: u32, accents: &[u8], reset_position: bool) {
        for (slot, &code) in self.accents.iter().zip(accents) {
            slot.store(code, Relaxed);
        }
        self.beats_per_bar.store(beats_per_bar.max(1), Relaxed);
        if reset_position {
            self.reset_position.store(true, Relaxed);
        }
    }
}

/// Mono click samples at the engine's sample rate.
#[derive(Debug)]
pub struct Clicks {
    pub high: Vec<f32>,
    pub low: Vec<f32>,
}

impl Clicks {
    /// Fallback clicks, used when the bundled files cannot be decoded.
    pub fn synthesized(rate: u32) -> Self {
        fn burst(rate: u32, freq: f32, amp: f32) -> Vec<f32> {
            let n = (rate as f32 * 0.03) as usize;
            (0..n)
                .map(|i| {
                    let t = i as f32 / rate as f32;
                    amp * (std::f32::consts::TAU * freq * t).cos() * (-200.0 * t).exp()
                })
                .collect()
        }
        Self {
            high: burst(rate, 1600.0, 1.0),
            low: burst(rate, 800.0, 0.7),
        }
    }
}

pub struct ClickEngine {
    ctl: Arc<Control>,
    clicks: Arc<Clicks>,
    rate: f64,
    bpm: u32,
    /// Samples left until the next beat. Fractional on purpose: it carries the
    /// rounding error forward, so odd tempos do not drift.
    to_next: f64,
    beat_in_bar: u32,
    /// (is_high, position in the click sample)
    voice: Option<(bool, usize)>,
}

impl ClickEngine {
    /// The first click (downbeat) lands on the very first rendered sample.
    pub fn new(ctl: Arc<Control>, clicks: Arc<Clicks>, rate: u32) -> Self {
        let bpm = ctl.bpm.load(Relaxed).max(1);
        Self {
            ctl,
            clicks,
            rate: f64::from(rate),
            bpm,
            to_next: 0.0,
            beat_in_bar: 0,
            voice: None,
        }
    }

    fn samples_per_beat(&self, bpm: u32) -> f64 {
        60.0 * self.rate / f64::from(bpm.max(1))
    }

    fn trigger(&mut self) {
        let max = self.ctl.accents.len() as u32;
        let beats = self.ctl.beats_per_bar.load(Relaxed).clamp(1, max);
        if self.beat_in_bar >= beats {
            self.beat_in_bar = 0;
        }
        match self.ctl.accents[self.beat_in_bar as usize].load(Relaxed) {
            CODE_ACCENT => self.voice = Some((true, 0)),
            CODE_NORMAL => self.voice = Some((false, 0)),
            _ => {} // silent beat
        }
        self.beat_in_bar = (self.beat_in_bar + 1) % beats;
    }

    fn next_sample(&mut self) -> f32 {
        let Some((high, pos)) = self.voice else {
            return 0.0;
        };
        let buf = if high { &self.clicks.high } else { &self.clicks.low };
        match buf.get(pos) {
            Some(&v) => {
                self.voice = Some((high, pos + 1));
                v
            }
            None => {
                self.voice = None;
                0.0
            }
        }
    }

    /// Fills `out` with mono audio. Does not allocate or block.
    pub fn render(&mut self, out: &mut [f32]) {
        let bpm = self.ctl.bpm.load(Relaxed).max(1);
        if bpm != self.bpm {
            // Keep the phase: scale the rest of the current beat.
            let ratio = self.samples_per_beat(bpm) / self.samples_per_beat(self.bpm);
            self.to_next = (self.to_next * ratio).max(0.0);
            self.bpm = bpm;
        }
        if self.ctl.reset_position.swap(false, Relaxed) {
            self.beat_in_bar = 0;
        }
        let spb = self.samples_per_beat(self.bpm);
        for s in out.iter_mut() {
            if self.to_next <= 0.0 {
                self.trigger();
                self.to_next += spb;
            }
            self.to_next -= 1.0;
            *s = self.next_sample();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn engine(bpm: u32, bpb: u32, accents: &[u8]) -> (ClickEngine, Arc<Control>) {
        let ctl = Arc::new(Control::new(8));
        ctl.set_bpm(bpm);
        ctl.set_meter(bpb, accents, false);
        let e = ClickEngine::new(ctl.clone(), Arc::new(Clicks::synthesized(RATE)), RATE);
        (e, ctl)
    }

    /// Renders in awkward chunk sizes and returns (sample index, first value) per click.
    fn render_onsets(e: &mut ClickEngine, total: usize, start: usize) -> Vec<(usize, f32)> {
        let sizes = [480usize, 333, 1024, 97, 4096];
        let (mut done, mut k, mut last) = (0, 0, None::<usize>);
        let mut res = Vec::new();
        while done < total {
            let n = sizes[k % sizes.len()].min(total - done);
            k += 1;
            let mut buf = vec![0.0; n];
            e.render(&mut buf);
            for (i, &s) in buf.iter().enumerate() {
                let g = start + done + i;
                if s.abs() > 0.5 && last.map_or(true, |l| g - l > 1000) {
                    res.push((g, s));
                    last = Some(g);
                }
            }
            done += n;
        }
        res
    }

    const N4: [u8; 4] = [CODE_ACCENT, CODE_NORMAL, CODE_NORMAL, CODE_NORMAL];

    #[test]
    fn exact_spacing_at_120_bpm() {
        let (mut e, _) = engine(120, 4, &N4);
        let on = render_onsets(&mut e, 24_000 * 8, 0);
        let pos: Vec<_> = on.iter().map(|o| o.0).collect();
        assert_eq!(pos, (0..8).map(|k| k * 24_000).collect::<Vec<_>>());
    }

    #[test]
    fn odd_tempo_does_not_drift() {
        let (mut e, _) = engine(113, 4, &N4);
        let spb = 60.0 * RATE as f64 / 113.0;
        let on = render_onsets(&mut e, (spb * 200.0) as usize, 0);
        assert!(on.len() >= 199);
        for (k, (pos, _)) in on.iter().enumerate() {
            let expected = (k as f64 * spb).ceil() as i64;
            assert!((*pos as i64 - expected).abs() <= 1, "beat {k}: {pos} vs {expected}");
        }
    }

    #[test]
    fn accents_and_silent_beats() {
        let (mut e, _) = engine(120, 3, &[CODE_ACCENT, CODE_NORMAL, crate::meter::CODE_MUTE]);
        let on = render_onsets(&mut e, 24_000 * 6, 0);
        let got: Vec<_> = on.iter().map(|o| (o.0, (o.1 * 10.0).round() as i32)).collect();
        assert_eq!(got, vec![(0, 10), (24_000, 7), (72_000, 10), (96_000, 7)]);
    }

    #[test]
    fn tempo_change_keeps_phase() {
        let (mut e, ctl) = engine(120, 4, &N4);
        let mut first = render_onsets(&mut e, 12_000, 0);
        ctl.set_bpm(240);
        first.extend(render_onsets(&mut e, 12_000, 12_000));
        let pos: Vec<_> = first.iter().map(|o| o.0).collect();
        // Half of the beat was left; at double tempo it takes 6000 samples.
        assert_eq!(pos, vec![0, 18_000]);
    }

    #[test]
    fn numerator_change_restarts_bar() {
        let (mut e, ctl) = engine(120, 4, &N4);
        let _ = render_onsets(&mut e, 24_000 * 2 + 10, 0);
        ctl.set_meter(3, &N4, true);
        let on = render_onsets(&mut e, 24_000, 24_000 * 2 + 10);
        let downbeat = on
            .iter()
            .find(|(pos, _)| *pos >= 72_000)
            .expect("downbeat after the meter change");
        assert_eq!(downbeat.0, 72_000);
        assert!(
            (downbeat.1 - 1.0).abs() < 0.05,
            "next click should be the high downbeat, got {}",
            downbeat.1
        );
    }
}
