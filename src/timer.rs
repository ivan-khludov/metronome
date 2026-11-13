use crate::clicker::MtrClicker;
use crate::meter::{pattern_to_codes, BEATS_MAX, CODE_ACCENT, CODE_NORMAL};
use adw::subclass::prelude::*;
use gtk::{
    glib::{self, clone},
    prelude::*,
};
use std::thread;

enum TimerCommand {
    Stop,
    Bpm(u32),
    /// Replaces the bar length and the accent table together.
    /// `reset_position` is set only when the numerator changes, so an accent
    /// edit does not skip or restart the bar.
    Meter {
        beats_per_bar: u32,
        accents: [u8; BEATS_MAX as usize],
        reset_position: bool,
    },
}

mod imp {
    use super::*;
    use crate::window::{BPB_DEFAULT, BPB_MAX, BPB_MIN, BPM_DEFAULT, BPM_MAX, BPM_MIN};
    use std::cell::{Cell, RefCell};

    #[derive(Debug, glib::Properties)]
    #[properties(wrapper_type = super::MtrTimer)]
    pub struct MtrTimer {
        #[property(get, set = Self::set_active)]
        pub active: Cell<bool>,
        #[property(get, set = Self::set_accent_pattern, default = "Annn")]
        pub accent_pattern: RefCell<String>,
        #[property(get, set = Self::set_beats_per_bar, minimum = BPB_MIN, maximum = BPB_MAX, default = BPB_DEFAULT)]
        pub beats_per_bar: Cell<u32>,
        #[property(get, set = Self::set_beats_per_minute, minimum = BPM_MIN, maximum = BPM_MAX, default = BPM_DEFAULT)]
        pub beats_per_minute: Cell<u32>,
        pub clicker: MtrClicker,
        thread_cmd: RefCell<std::sync::mpsc::Sender<TimerCommand>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MtrTimer {
        const NAME: &'static str = "MtrTimer";
        type Type = super::MtrTimer;

        fn new() -> Self {
            let (tx, _rx) = std::sync::mpsc::channel();
            Self {
                active: Default::default(),
                accent_pattern: RefCell::new(String::from("Annn")),
                beats_per_bar: Cell::new(BPB_DEFAULT),
                beats_per_minute: Cell::new(BPM_DEFAULT),
                clicker: Default::default(),
                thread_cmd: RefCell::new(tx),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for MtrTimer {}

    impl MtrTimer {
        fn set_active(&self, active: bool) {
            self.active.set(active);
            if !active {
                self.thread_cmd
                    .borrow()
                    .send(TimerCommand::Stop)
                    .unwrap_or_default();
                return;
            }
            let mut beats_per_bar = self.beats_per_bar.get().clamp(BPB_MIN, BPB_MAX);
            let mut accents = pattern_to_codes(&self.accent_pattern.borrow());
            let ns_per_beat = 60_000_000_000 / (self.beats_per_minute.get() as u64);
            let clicker = &self.clicker;
            let (tx, rx) = std::sync::mpsc::channel();
            self.thread_cmd.set(tx);
            thread::spawn(clone!(@strong clicker => move || {
                let recv_period = std::time::Duration::from_millis(1);
                let mut ticktime = std::time::Duration::from_nanos(ns_per_beat);
                let mut lastiter = std::time::Instant::now() - ticktime;
                let mut bar_position = 0.0;
                let mut beat_in_bar = 0;

                loop {
                    let msg = rx.recv_timeout(recv_period);
                    match msg {
                        Ok(TimerCommand::Stop) => break,
                        Ok(TimerCommand::Bpm(bpm)) => ticktime = std::time::Duration::from_nanos(60_000_000_000 / bpm as u64),
                        Ok(TimerCommand::Meter { beats_per_bar: bpb, accents: next, reset_position }) => {
                            if reset_position {
                                beat_in_bar = 0;
                            }
                            beats_per_bar = bpb.clamp(BPB_MIN, BPB_MAX);
                            accents = next;
                            if beat_in_bar >= beats_per_bar {
                                beat_in_bar = 0;
                            }
                        },
                        Err(_) => {}
                    }
                    let elapsed = lastiter.elapsed();
                    lastiter = std::time::Instant::now();
                    bar_position += elapsed.as_secs_f64() / ticktime.as_secs_f64();
                    if bar_position > 1.0 {
                        match accents.get(beat_in_bar as usize).copied().unwrap_or(CODE_NORMAL) {
                            CODE_ACCENT => clicker.high(),
                            CODE_NORMAL => clicker.low(),
                            _ => {}
                        }
                        beat_in_bar = (beat_in_bar + 1) % beats_per_bar;
                        bar_position -= 1.0;
                    }
                }
            }));
        }

        fn send_meter(&self, reset_position: bool) {
            let beats_per_bar = self.beats_per_bar.get().clamp(BPB_MIN, BPB_MAX);
            let accents = pattern_to_codes(&self.accent_pattern.borrow());
            self.thread_cmd
                .borrow()
                .send(TimerCommand::Meter {
                    beats_per_bar,
                    accents,
                    reset_position,
                })
                .unwrap_or_default();
        }

        fn set_accent_pattern(&self, pattern: String) {
            if self.accent_pattern.borrow().as_str() == pattern {
                return;
            }
            self.accent_pattern.replace(pattern);
            self.send_meter(false);
            self.obj().notify_accent_pattern();
        }

        fn set_beats_per_bar(&self, bpb: u32) {
            let bpb = bpb.clamp(BPB_MIN, BPB_MAX);
            if self.beats_per_bar.get() == bpb {
                return;
            }
            self.beats_per_bar.set(bpb);
            self.send_meter(true);
            self.obj().notify_beats_per_bar();
        }

        fn set_beats_per_minute(&self, bpm: u32) {
            self.beats_per_minute.set(bpm);
            self.thread_cmd
                .borrow()
                .send(TimerCommand::Bpm(bpm))
                .unwrap_or_default();
        }
    }
}

glib::wrapper! {
    pub struct MtrTimer(ObjectSubclass<imp::MtrTimer>);
}
