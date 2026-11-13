use crate::timerbuttontrough::MtrTimerButtonTrough;
use adw::subclass::prelude::*;
use gtk::{gdk, glib, graphene, prelude::*};
use std::time::Instant;

mod imp {
    use crate::window::{BPB_DEFAULT, BPB_MAX, BPB_MIN, BPM_DEFAULT, BPM_MAX, BPM_MIN};

    use super::*;
    use std::{
        cell::{Cell, RefCell},
        marker::PhantomData,
    };

    #[derive(Debug, glib::Properties, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/ivan_khludov/metronome/ui/timerbutton.ui")]
    #[properties(wrapper_type = super::MtrTimerButton)]
    pub struct MtrTimerButton {
        #[template_child]
        pub trough: TemplateChild<MtrTimerButtonTrough>,
        #[template_child]
        pub start_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub pause_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub marks_overlay: TemplateChild<gtk::Overlay>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        marks_container: TemplateChild<gtk::Box>,
        #[property(get, set = Self::set_accent_pattern, default = "Annn")]
        pub accent_pattern: RefCell<String>,
        #[property(get, set = Self::set_beats_per_bar, minimum = BPB_MIN, maximum = BPB_MAX, default = BPB_DEFAULT)]
        pub beats_per_bar: Cell<u32>,
        #[property(get, set = Self::set_beats_per_minute, minimum = BPM_MIN, maximum = BPM_MAX, default = BPM_DEFAULT)]
        pub beats_per_minute: Cell<u32>,
        pub snapshot_time: Cell<Instant>,
        pub ring_started: Cell<Instant>,
        pub running_id: RefCell<Option<gtk::TickCallbackId>>,
        #[property(get = Self::active)]
        pub active: PhantomData<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MtrTimerButton {
        const NAME: &'static str = "MtrTimerButton";
        type Type = super::MtrTimerButton;
        type ParentType = gtk::Widget;

        fn new() -> Self {
            Self {
                trough: Default::default(),
                start_button: Default::default(),
                pause_button: Default::default(),
                marks_overlay: Default::default(),
                stack: Default::default(),
                marks_container: Default::default(),
                accent_pattern: RefCell::new(String::from("Annn")),
                beats_per_bar: std::cell::Cell::new(BPB_DEFAULT),
                beats_per_minute: std::cell::Cell::new(BPM_DEFAULT),
                snapshot_time: std::cell::Cell::new(Instant::now()),
                ring_started: std::cell::Cell::new(Instant::now()),
                running_id: Default::default(),
                active: Default::default(),
            }
        }

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
            klass.set_layout_manager_type::<gtk::BinLayout>();
            klass.set_css_name("timerbutton");
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            MtrTimerButtonTrough::ensure_type();
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for MtrTimerButton {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            obj.update_marks();
        }

        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for MtrTimerButton {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();
            let s_per_beat = 60.0 / widget.beats_per_minute() as f64;
            let s_per_bar = s_per_beat * widget.beats_per_bar() as f64;

            let elapsed = self.snapshot_time.get().elapsed();
            self.snapshot_time.set(Instant::now());

            let progress = if self.running_id.borrow().is_some() {
                let progress = self.trough.progress() + elapsed.as_secs_f64() / s_per_bar;
                // Perform a kind of floating point modulus between 0 and 2.
                progress.fract() + (progress as i32 % 2) as f64
            } else {
                0.0
            };

            self.trough.set_progress(progress);

            self.parent_snapshot(snapshot);

            if self.running_id.borrow().is_some() {
                draw_beat_ring(
                    snapshot,
                    &*widget,
                    &self.stack,
                    widget.beats_per_bar().max(1),
                    progress,
                    self.ring_started.get(),
                );
            }
        }
    }

    impl MtrTimerButton {
        fn set_accent_pattern(&self, pattern: String) {
            if self.accent_pattern.borrow().as_str() == pattern {
                return;
            }
            self.accent_pattern.replace(pattern);
            self.obj().update_marks();
            self.obj().notify_accent_pattern();
        }

        fn set_beats_per_bar(&self, beats_per_bar: u32) {
            let obj = self.obj();

            let beat_pos = (self.trough.progress() * self.beats_per_bar.get() as f64).fract();
            self.beats_per_bar.set(beats_per_bar);
            let bar_remaining = (1.0 - beat_pos) / beats_per_bar as f64;
            self.trough.set_progress(2.0 - bar_remaining);
            obj.update_marks();
        }

        fn set_beats_per_minute(&self, beats_per_minute: u32) {
            self.beats_per_minute.set(beats_per_minute);
        }

        pub fn active(&self) -> bool {
            match self.stack.visible_child() {
                Some(child) => child == self.pause_button.get(),
                None => false,
            }
        }
    }
}

glib::wrapper! {
    pub struct MtrTimerButton(ObjectSubclass<imp::MtrTimerButton>)
        @extends gtk::Widget;
}

#[gtk::template_callbacks]
impl MtrTimerButton {
    fn update_marks(&self) {
        let imp = self.imp();
        while let Some(child) = imp.marks_overlay.first_child() {
            child.unparent();
        }
    }

    #[template_callback]
    pub fn start(&self) {
        let imp = self.imp();

        imp.snapshot_time.set(Instant::now());
        imp.ring_started.set(Instant::now());
        imp.stack.set_visible_child(&*imp.pause_button);

        let source_id = self.add_tick_callback(move |this, _clock| {
            this.queue_draw();
            glib::ControlFlow::Continue
        });

        imp.running_id.replace(Some(source_id));

        self.set_state_flags(gtk::StateFlags::CHECKED, false);
        self.queue_draw();

        self.notify_active();
    }

    #[template_callback]
    pub fn pause(&self) {
        let imp = self.imp();

        if let Some(id) = imp.running_id.take() {
            id.remove();
        }

        imp.stack.set_visible_child(&*imp.start_button);

        self.unset_state_flags(gtk::StateFlags::CHECKED);
        self.queue_draw();

        self.notify_active();
    }
}

fn stroke_arc(snapshot: &gtk::Snapshot, cx: f32, cy: f32, radius: f32, start: f32, end: f32, color: gdk::RGBA) {
    let sweep = end - start;
    if sweep < 0.2 {
        return;
    }
    let pad = 4.0;
    let bounds = graphene::Rect::new(cx - radius - pad, cy - radius - pad, (radius + pad) * 2.0, (radius + pad) * 2.0);
    let context = snapshot.append_cairo(&bounds);
    context.set_source_rgba(color.red() as f64, color.green() as f64, color.blue() as f64, color.alpha() as f64);
    context.set_line_width(3.5);
    context.set_line_cap(gtk::cairo::LineCap::Butt);
    // Cairo angles grow clockwise here because y points down. 0° is 3 o'clock,
    // so 12 o'clock is -90°.
    context.arc(cx as f64, cy as f64, radius as f64, (start - 90.0).to_radians() as f64, (end - 90.0).to_radians() as f64);
    let _ = context.stroke();
}

/// Sectors start at 12 o'clock and run clockwise. The current beat fills over
/// its own duration; earlier beats of this bar stay dim.
fn draw_beat_ring(
    snapshot: &gtk::Snapshot,
    widget: &MtrTimerButton,
    stack: &gtk::Stack,
    beats: u32,
    progress: f64,
    started: Instant,
) {
    let Some(bounds) = stack.compute_bounds(widget) else {
        return;
    };
    let cx = bounds.x() + bounds.width() / 2.0;
    let cy = bounds.y() + bounds.height() / 2.0;
    let side = bounds.width().min(bounds.height()) + 12.0;
    let radius = 46.0 / 100.0 * side;
    let fade = (started.elapsed().as_secs_f32() / 0.2).clamp(0.0, 1.0);
    let beats_f = beats as f32;
    let gap = (6.0_f32).min(40.0 / beats_f);
    let segment = (360.0 - gap * beats_f) / beats_f;
    let position = (progress.fract() * beats as f64) as f32;
    let current = (position.floor() as u32).min(beats - 1);
    let fraction = if position >= beats_f { 1.0 } else { position.fract() };

    let track = gdk::RGBA::new(1.0, 1.0, 1.0, 0.12 * fade);
    let done = gdk::RGBA::new(0.0, 180.0 / 255.0, 197.0 / 255.0, 0.5 * fade);
    let active = gdk::RGBA::new(0.0, 180.0 / 255.0, 197.0 / 255.0, fade);

    for index in 0..beats {
        let start = index as f32 * (segment + gap);
        let end = start + segment;
        stroke_arc(snapshot, cx, cy, radius, start, end, track);
        if index < current {
            stroke_arc(snapshot, cx, cy, radius, start, end, done);
        } else if index == current {
            stroke_arc(snapshot, cx, cy, radius, start, start + segment * fraction, active);
        }
    }
}
