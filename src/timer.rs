use crate::audio_out::AudioOut;
use crate::engine::Control;
use crate::meter::{pattern_to_codes, BEATS_MAX};
use adw::subclass::prelude::*;
use gtk::{glib, prelude::*};
use std::sync::Arc;

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
        /// Lock-free parameters read by the audio thread.
        control: Arc<Control>,
        /// Present only while the metronome is running; dropping it stops the sound.
        output: RefCell<Option<AudioOut>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MtrTimer {
        const NAME: &'static str = "MtrTimer";
        type Type = super::MtrTimer;

        fn new() -> Self {
            Self {
                active: Default::default(),
                accent_pattern: RefCell::new(String::from("Annn")),
                beats_per_bar: Cell::new(BPB_DEFAULT),
                beats_per_minute: Cell::new(BPM_DEFAULT),
                control: Arc::new(Control::new(BEATS_MAX as usize)),
                output: RefCell::new(None),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for MtrTimer {}

    impl MtrTimer {
        fn set_active(&self, active: bool) {
            if !active {
                self.output.replace(None);
                self.active.set(false);
                return;
            }
            if self.output.borrow().is_some() {
                self.active.set(true);
                return;
            }
            self.push_meter(false);
            self.control.set_bpm(self.beats_per_minute.get());
            match AudioOut::start(self.control.clone()) {
                Ok(out) => {
                    self.output.replace(Some(out));
                    self.active.set(true);
                }
                Err(err) => {
                    log::error!("could not start audio: {err}");
                    self.active.set(false);
                }
            }
        }

        fn push_meter(&self, reset_position: bool) {
            let beats_per_bar = self.beats_per_bar.get().clamp(BPB_MIN, BPB_MAX);
            let accents = pattern_to_codes(&self.accent_pattern.borrow());
            self.control.set_meter(beats_per_bar, &accents, reset_position);
        }

        fn set_accent_pattern(&self, pattern: String) {
            if self.accent_pattern.borrow().as_str() == pattern {
                return;
            }
            self.accent_pattern.replace(pattern);
            self.push_meter(false);
            self.obj().notify_accent_pattern();
        }

        fn set_beats_per_bar(&self, bpb: u32) {
            let bpb = bpb.clamp(BPB_MIN, BPB_MAX);
            if self.beats_per_bar.get() == bpb {
                return;
            }
            self.beats_per_bar.set(bpb);
            self.push_meter(true);
            self.obj().notify_beats_per_bar();
        }

        fn set_beats_per_minute(&self, bpm: u32) {
            self.beats_per_minute.set(bpm);
            self.control.set_bpm(bpm);
        }
    }
}

glib::wrapper! {
    pub struct MtrTimer(ObjectSubclass<imp::MtrTimer>);
}
