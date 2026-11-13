use crate::application::MtrApplication;
use crate::config::{APP_ID, PROFILE};
use crate::meter;
use crate::timer::MtrTimer;
use crate::timerbutton::MtrTimerButton;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};
use std::cell::{Cell, OnceCell, RefCell};
use std::time::Instant;

pub const BPB_MIN: u32 = 1;
pub const BPB_MAX: u32 = crate::meter::BEATS_MAX;
pub const BPB_DEFAULT: u32 = 4;

pub const BPM_MIN: u32 = 20;
pub const BPM_MAX: u32 = 260;
pub const BPM_DEFAULT: u32 = 100;

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct MeterDialog {
        pub dialog: adw::Window,
        pub title: gtk::Label,
        pub beats_label: gtk::Label,
        pub unit_label: gtk::Label,
        pub unit_popover: gtk::Popover,
        pub beats_box: gtk::Grid,
        pub draft: RefCell<meter::Meter>,
        pub editing_index: Cell<Option<usize>>,
        pub accepting: Cell<bool>,
        pub remove_button: gtk::Button,
        pub duplicate_label: gtk::Label,
        pub ok_button: gtk::Button,
    }

    #[derive(Debug, gtk::CompositeTemplate, glib::Properties)]
    #[template(resource = "/io/github/ivan_khludov/metronome/ui/window.ui")]
    #[properties(wrapper_type = super::MtrApplicationWindow)]
    pub struct MtrApplicationWindow {
        #[template_child]
        pub timer_button: TemplateChild<MtrTimerButton>,
        #[template_child]
        pub timer: TemplateChild<MtrTimer>,
        #[template_child]
        pub time_signature_meters: TemplateChild<gtk::Grid>,
        #[template_child]
        pub time_signature_add_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub tempo_marking: TemplateChild<gtk::Label>,
        #[template_child]
        pub maximize_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub window_dimmer: TemplateChild<gtk::Box>,
        #[property(get, set = Self::set_accent_pattern, default = "Annn")]
        pub accent_pattern: RefCell<String>,
        #[property(get, set = Self::set_beat_unit, default = 4)]
        pub beat_unit: Cell<u32>,
        #[property(get, set = Self::set_beats_per_bar, minimum = BPB_MIN, maximum = BPB_MAX, default = BPB_DEFAULT)]
        pub beats_per_bar: Cell<u32>,
        #[property(get, set = Self::set_beats_per_minute, minimum = BPM_MIN, maximum = BPM_MAX, default = BPM_DEFAULT)]
        pub beats_per_minute: Cell<u32>,
        pub tap_time: Cell<Instant>,
        pub settings: gio::Settings,
        pub meter_dialog: OnceCell<MeterDialog>,
        pub updating_meter: Cell<bool>,
        pub meter_refresh_queued: Cell<bool>,
        pub saved_meters: RefCell<Vec<meter::Meter>>,
        pub active_meter_index: Cell<usize>,
        pub rebuilding_meters: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MtrApplicationWindow {
        const NAME: &'static str = "MtrApplicationWindow";
        type Type = super::MtrApplicationWindow;
        type ParentType = adw::ApplicationWindow;

        fn new() -> Self {
            Self {
                timer_button: Default::default(),
                timer: Default::default(),
                time_signature_meters: Default::default(),
                time_signature_add_button: Default::default(),
                tempo_marking: Default::default(),
                maximize_icon: Default::default(),
                window_dimmer: Default::default(),
                accent_pattern: RefCell::new(String::from("Annn")),
                beat_unit: Cell::new(meter::BEAT_UNIT_DEFAULT),
                beats_per_bar: Cell::new(BPB_DEFAULT),
                beats_per_minute: Cell::new(BPM_DEFAULT),
                tap_time: Cell::new(Instant::now()),
                settings: gio::Settings::new(APP_ID),
                meter_dialog: OnceCell::new(),
                updating_meter: Cell::new(false),
                meter_refresh_queued: Cell::new(false),
                saved_meters: RefCell::new(Vec::new()),
                active_meter_index: Cell::new(0),
                rebuilding_meters: Cell::new(false),
            }
        }

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();

            klass.install_action("win.decrease-bpm", None, |win, _, _| {
                win.add_beats_per_minute(-1);
            });

            klass.install_action("win.increase-bpm", None, |win, _, _| {
                win.add_beats_per_minute(1);
            });

            klass.install_action("win.decrease-bpm-fast", None, |win, _, _| {
                win.add_beats_per_minute(-5);
            });

            klass.install_action("win.increase-bpm-fast", None, |win, _, _| {
                win.add_beats_per_minute(5);
            });

            klass.install_action("win.tap", None, |win, _, _| {
                win.tap();
            });
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            MtrTimerButton::ensure_type();
            MtrTimer::ensure_type();
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for MtrApplicationWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
            self.setup_meter_dialog();
            obj.connect_beats_per_minute_notify(|window| {
                window
                    .imp()
                    .tempo_marking
                    .set_label(tempo_name(window.beats_per_minute()));
            });
            obj.load_settings();
            obj.sync_maximize_icon();
            obj.connect_maximized_notify(|window| {
                window.sync_maximize_icon();
            });
            use_pointer_cursor(obj.upcast_ref());
        }
    }

    impl WidgetImpl for MtrApplicationWindow {}

    impl WindowImpl for MtrApplicationWindow {}

    impl ApplicationWindowImpl for MtrApplicationWindow {}

    impl AdwApplicationWindowImpl for MtrApplicationWindow {}

    impl MtrApplicationWindow {
        fn setup_meter_dialog(&self) {
            let obj = self.obj();

            let dialog = adw::Window::new();
            dialog.add_css_class("meter-editor");
            dialog.set_title(Some(&gettext("New Time Signature")));
            dialog.set_modal(true);
            dialog.set_transient_for(Some(&*obj));
            dialog.set_destroy_with_parent(true);
            dialog.set_hide_on_close(true);
            dialog.set_resizable(false);
            dialog.set_default_width(448);

            let title = gtk::Label::new(Some(&gettext("New Time Signature")));
            title.add_css_class("dialog-title");
            title.set_halign(gtk::Align::Start);
            title.set_hexpand(true);
            let close = gtk::Button::with_label("×");
            close.set_size_request(28, 28);
            let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            header.add_css_class("dialog-header");
            header.append(&title);
            header.append(&close);

            let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
            root.append(&header);

            let body = gtk::Box::new(gtk::Orientation::Vertical, 20);
            body.add_css_class("editor-body");

            let beats_label = gtk::Label::new(Some(&BPB_DEFAULT.to_string()));
            beats_label.add_css_class("beat-count");
            beats_label.set_width_chars(2);
            beats_label.set_xalign(0.5);
            let beats_down = gtk::Button::with_label("−");
            beats_down.add_css_class("step");
            beats_down.set_size_request(32, 32);
            let beats_up = gtk::Button::with_label("+");
            beats_up.add_css_class("step");
            beats_up.set_size_request(32, 32);
            let stepper = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            stepper.append(&beats_down);
            stepper.append(&beats_label);
            stepper.append(&beats_up);
            body.append(&editor_row(&gettext("Beats per bar"), &stepper));

            let unit_label = gtk::Label::new(Some("4"));
            let unit_chevron = gtk::Image::from_resource("/io/github/ivan_khludov/metronome/icons/chevron.svg");
            let unit_face = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            unit_face.append(&unit_label);
            unit_face.append(&unit_chevron);
            let unit_button = gtk::Button::new();
            unit_button.set_child(Some(&unit_face));
            unit_button.add_css_class("editor-unit");
            let unit_popover = gtk::Popover::new();
            unit_popover.set_parent(&unit_button);
            unit_popover.set_position(gtk::PositionType::Bottom);
            let unit_list = gtk::Box::new(gtk::Orientation::Vertical, 4);
            for value in [2_u32, 4, 8, 16] {
                let choice = gtk::Button::with_label(&value.to_string());
                choice.add_css_class("editor-unit");
                choice.connect_clicked(glib::clone!(@weak obj, @weak unit_popover => move |_| {
                    unit_popover.popdown();
                    if obj.imp().updating_meter.get() {
                        return;
                    }
                    let Some(dialog) = obj.imp().meter_dialog.get() else {
                        return;
                    };
                    let current = dialog.draft.borrow().clone();
                    if value == current.unit {
                        return;
                    }
                    *dialog.draft.borrow_mut() = meter::Meter::new(current.beats, value, current.pattern);
                    dialog.duplicate_label.set_visible(false);
                    obj.queue_meter_dialog_refresh();
                }));
                unit_list.append(&choice);
            }
            unit_popover.set_child(Some(&unit_list));
            unit_button.connect_clicked(glib::clone!(@weak unit_popover => move |_| {
                unit_popover.popup();
            }));
            body.append(&editor_row(&gettext("Beat unit"), &unit_button));

            let accent_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let accent_head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            let accent_heading = gtk::Label::new(Some(&gettext("Accents")));
            accent_heading.add_css_class("accent-heading");
            accent_heading.set_halign(gtk::Align::Start);
            accent_heading.set_hexpand(true);
            let accent_hint = gtk::Label::new(Some(&gettext("Click to change the beat")));
            accent_hint.add_css_class("accent-hint");
            accent_hint.set_halign(gtk::Align::End);
            accent_head.append(&accent_heading);
            accent_head.append(&accent_hint);
            accent_box.append(&accent_head);

            let beats_box = gtk::Grid::new();
            beats_box.set_column_homogeneous(true);
            beats_box.set_row_homogeneous(true);
            beats_box.set_column_spacing(8);
            beats_box.set_row_spacing(8);
            beats_box.set_hexpand(true);
            beats_box.add_css_class("beat-accents");
            accent_box.append(&beats_box);
            accent_box.append(&beat_legend());
            body.append(&accent_box);
            root.append(&body);

            let actions = gtk::Box::new(gtk::Orientation::Vertical, 12);
            actions.add_css_class("editor-footer");
            let duplicate_label = gtk::Label::new(Some(&gettext("This time signature already exists")));
            duplicate_label.add_css_class("meter-duplicate");
            duplicate_label.set_wrap(true);
            duplicate_label.set_xalign(0.5);
            duplicate_label.set_halign(gtk::Align::Fill);
            duplicate_label.set_justify(gtk::Justification::Center);
            duplicate_label.set_visible(false);

            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let remove_face = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            remove_face.append(&gtk::Image::from_resource(
                "/io/github/ivan_khludov/metronome/icons/trash.svg",
            ));
            remove_face.append(&gtk::Label::new(Some(&gettext("Delete"))));
            let remove_button = gtk::Button::new();
            remove_button.set_child(Some(&remove_face));
            remove_button.add_css_class("editor-delete");
            remove_button.set_visible(false);
            let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            spacer.set_hexpand(true);
            let cancel_button = gtk::Button::with_label(&gettext("Cancel"));
            cancel_button.add_css_class("editor-cancel");
            let ok_button = gtk::Button::with_label(&gettext("Add"));
            ok_button.add_css_class("editor-apply");
            row.append(&remove_button);
            row.append(&spacer);
            row.append(&cancel_button);
            row.append(&ok_button);
            actions.append(&duplicate_label);
            actions.append(&row);
            root.append(&actions);

            dialog.set_content(Some(&root));
            close_on_escape(&dialog);
            track_modal(&obj, &dialog);

            ok_button.connect_clicked(glib::clone!(@weak obj => move |_| {
                obj.confirm_meter_dialog();
            }));
            close.connect_clicked(glib::clone!(@weak dialog => move |_| {
                let _ = dialog.close();
            }));
            cancel_button.connect_clicked(glib::clone!(@weak dialog => move |_| {
                let _ = dialog.close();
            }));
            remove_button.connect_clicked(glib::clone!(@weak obj => move |_| {
                obj.remove_edited_meter();
            }));
            dialog.connect_close_request(glib::clone!(@weak obj => @default-return glib::Propagation::Proceed, move |_| {
                if let Some(dialog) = obj.imp().meter_dialog.get() {
                    dialog.accepting.set(false);
                }
                glib::Propagation::Proceed
            }));

            beats_down.connect_clicked(glib::clone!(@weak obj => move |_| {
                obj.step_draft_beats(-1);
            }));
            beats_up.connect_clicked(glib::clone!(@weak obj => move |_| {
                obj.step_draft_beats(1);
            }));

            use_pointer_cursor(dialog.upcast_ref());
            self.meter_dialog
                .set(MeterDialog {
                    dialog,
                    title,
                    beats_label,
                    unit_label,
                    unit_popover,
                    beats_box,
                    draft: RefCell::new(meter::Meter::new(BPB_DEFAULT, meter::BEAT_UNIT_DEFAULT, "A")),
                    editing_index: Cell::new(None),
                    accepting: Cell::new(false),
                    remove_button,
                    duplicate_label,
                    ok_button,
                })
                .expect("Meter dialog set once");
        }

        fn set_accent_pattern(&self, pattern: String) {
            let pattern = meter::fit_pattern(&pattern, self.beats_per_bar.get());
            if self.accent_pattern.borrow().as_str() == pattern {
                return;
            }
            self.accent_pattern.replace(pattern.clone());
            if let Err(err) = self.settings.set_string("accent-pattern", &pattern) {
                log::warn!("Failed to save the accent pattern, {}", &err);
            }
            self.obj().notify_accent_pattern();
        }

        fn set_beat_unit(&self, unit: u32) {
            let unit = if meter::is_valid_unit(unit) {
                unit
            } else {
                meter::BEAT_UNIT_DEFAULT
            };
            if self.beat_unit.get() == unit {
                return;
            }
            self.beat_unit.set(unit);
            if let Err(err) = self.settings.set_uint("beat-unit", unit) {
                log::warn!("Failed to save the beat unit, {}", &err);
            }
            self.obj().notify_beat_unit();
        }

        fn set_beats_per_bar(&self, beats: u32) {
            let beats = beats.clamp(BPB_MIN, BPB_MAX);
            if self.beats_per_bar.get() == beats {
                return;
            }
            self.beats_per_bar.set(beats);
            if let Err(err) = self.settings.set_uint("beats-per-bar", beats) {
                log::warn!("Failed to save the beats per bar, {}", &err);
            }
            self.obj().notify_beats_per_bar();
        }

        fn set_beats_per_minute(&self, bpm: u32) {
            self.beats_per_minute.set(bpm);

            if let Err(err) = self.settings.set_uint("beats-per-minute", bpm) {
                log::warn!("Failed to save the beats per minute, {}", &err);
            }

            self.obj()
                .action_set_enabled("win.decrease-bpm", bpm != BPM_MIN);
            self.obj()
                .action_set_enabled("win.increase-bpm", bpm != BPM_MAX);
            self.obj()
                .action_set_enabled("win.decrease-bpm-fast", bpm != BPM_MIN);
            self.obj()
                .action_set_enabled("win.increase-bpm-fast", bpm != BPM_MAX);

            self.obj().notify_beats_per_minute();
        }

        pub(super) fn save_saved_meters(&self) {
            let meters = self.saved_meters.borrow();
            let encoded: Vec<String> = meters.iter().map(meter::Meter::encode).collect();
            let refs: Vec<&str> = encoded.iter().map(String::as_str).collect();
            if let Err(err) = self.settings.set_strv("saved-meters", refs) {
                log::warn!("Failed to save the time signatures, {}", &err);
            }
        }

        pub(super) fn meter_buttons_match(&self) -> bool {
            let expected = self.saved_meters.borrow().len() + 1;
            widget_child_count(&self.time_signature_meters.get()) == expected
        }

        pub(super) fn rebuild_meter_buttons(&self) {
            self.rebuilding_meters.set(true);

            let meters_box = self.time_signature_meters.get();
            let add = self.time_signature_add_button.get();
            while let Some(child) = meters_box.first_child() {
                meters_box.remove(&child);
            }
            if add.parent().is_some() {
                add.unparent();
            }

            let meters = self.saved_meters.borrow().clone();
            let active = self
                .active_meter_index
                .get()
                .min(meters.len().saturating_sub(1));
            let obj = self.obj();
            let mut buttons = Vec::with_capacity(meters.len());
            for (index, meter) in meters.iter().enumerate() {
                let button = gtk::ToggleButton::new();
                button.add_css_class("meter-tile");
                button.set_size_request(-1, 56);
                button.set_child(Some(&meter_fraction(
                    &meter.beats.to_string(),
                    &meter.unit.to_string(),
                )));
                button.set_halign(gtk::Align::Fill);
                button.set_hexpand(true);
                button.connect_active_notify(glib::clone!(@weak obj => move |button| {
                    if obj.imp().rebuilding_meters.get() || !button.is_active() {
                        return;
                    }
                    obj.select_meter(index);
                }));
                meters_box.attach(&button, (index % 5) as i32, (index / 5) as i32, 1, 1);
                buttons.push(button);
            }

            if let Some(first) = buttons.first() {
                for button in buttons.iter().skip(1) {
                    button.set_group(Some(first));
                }
            }
            if let Some(button) = buttons.get(active) {
                button.set_active(true);
            }

            let add_index = meters.len();
            add.set_hexpand(true);
            add.set_halign(gtk::Align::Fill);
            add.set_vexpand(true);
            meters_box.attach(&add, (add_index % 5) as i32, (add_index / 5) as i32, 1, 1);
            add.set_sensitive(meters.len() < meter::SAVED_METERS_MAX);
            use_pointer_cursor(meters_box.upcast_ref());
            self.rebuilding_meters.set(false);
        }

        pub(super) fn refresh_meter_dialog(&self) {
            let Some(dialog) = self.meter_dialog.get() else {
                return;
            };
            if self.updating_meter.get() {
                return;
            }
            self.updating_meter.set(true);

            let draft = dialog.draft.borrow().clone();
            let beats = draft.beats.clamp(BPB_MIN, BPB_MAX);
            let unit = draft.unit;
            let pattern = draft.pattern;

            dialog.beats_label.set_label(&beats.to_string());
            dialog.unit_label.set_label(&unit.to_string());

            while let Some(child) = dialog.beats_box.first_child() {
                dialog.beats_box.remove(&child);
            }

            let obj = self.obj();
            for (index, accent_char) in pattern.chars().enumerate() {
                let accent = meter::BeatAccent::from_char(accent_char);
                let shown = match accent {
                    meter::BeatAccent::Accent => meter::BeatAccent::Accent,
                    meter::BeatAccent::Normal | meter::BeatAccent::Mute => meter::BeatAccent::Normal,
                };
                let button = gtk::Button::with_label(&format!("{}", index + 1));
                if shown == meter::BeatAccent::Accent {
                    button.add_css_class("accent");
                }
                button.set_tooltip_text(Some(&beat_tooltip(index, shown)));
                button.set_size_request(36, 36);
                button.connect_clicked(glib::clone!(@weak obj => move |_| {
                    obj.cycle_beat(index);
                }));
                button.set_hexpand(true);
                dialog.beats_box.attach(&button, (index % 8) as i32, (index / 8) as i32, 1, 1);
            }

            let editing = dialog.editing_index.get().is_some();
            dialog
                .remove_button
                .set_visible(editing && self.saved_meters.borrow().len() > 1);

            self.updating_meter.set(false);
        }
    }
}

fn use_pointer_cursor(widget: &gtk::Widget) {
    if widget.is::<gtk::Button>() || widget.is::<gtk::MenuButton>() {
        mark_pointer(widget);
        return;
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        use_pointer_cursor(&current);
        child = current.next_sibling();
    }
}

fn mark_pointer(widget: &gtk::Widget) {
    widget.set_cursor_from_name(Some("pointer"));
    let mut child = widget.first_child();
    while let Some(current) = child {
        mark_pointer(&current);
        child = current.next_sibling();
    }
}

fn meter_fraction(beats: &str, unit: &str) -> gtk::Box {
    let fraction = gtk::Box::new(gtk::Orientation::Vertical, 2);
    fraction.set_halign(gtk::Align::Center);
    fraction.set_valign(gtk::Align::Center);
    let beats_label = gtk::Label::new(Some(beats));
    let unit_label = gtk::Label::new(Some(unit));
    beats_label.set_xalign(0.5);
    unit_label.set_xalign(0.5);
    let rule = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    rule.add_css_class("meter-rule");
    rule.set_halign(gtk::Align::Center);
    rule.set_size_request(14, 2);
    fraction.append(&beats_label);
    fraction.append(&rule);
    fraction.append(&unit_label);
    fraction
}

fn widget_child_count(widget: &impl glib::object::IsA<gtk::Widget>) -> usize {
    let widget = widget.upcast_ref::<gtk::Widget>();
    let mut count = 0;
    let mut child = widget.first_child();
    while let Some(current) = child {
        count += 1;
        child = current.next_sibling();
    }
    count
}

fn track_modal(window: &MtrApplicationWindow, dialog: &adw::Window) {
    dialog.connect_show(glib::clone!(@weak window => move |_| {
        window.imp().window_dimmer.set_visible(true);
    }));
    dialog.connect_hide(glib::clone!(@weak window => move |_| {
        window.imp().window_dimmer.set_visible(false);
    }));
    dialog.connect_destroy(glib::clone!(@weak window => move |_| {
        window.imp().window_dimmer.set_visible(false);
    }));
    dialog.connect_close_request(glib::clone!(@weak window => @default-return glib::Propagation::Proceed, move |_| {
        window.imp().window_dimmer.set_visible(false);
        glib::Propagation::Proceed
    }));
}

fn close_on_escape(dialog: &adw::Window) {
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(glib::clone!(@weak dialog => @default-return glib::Propagation::Proceed, move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape {
            let _ = dialog.close();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    }));
    dialog.add_controller(keys);
}

fn editor_row(title: &str, control: &impl glib::object::IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("editor-row");
    let label = gtk::Label::new(Some(title));
    label.add_css_class("editor-row-label");
    label.set_halign(gtk::Align::Start);
    label.set_hexpand(true);
    row.append(&label);
    row.append(control);
    row
}

fn beat_legend() -> gtk::Box {
    let legend = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    legend.add_css_class("beat-legend");
    legend.set_halign(gtk::Align::Center);
    legend.append(&legend_item(true, &gettext("Accent")));
    legend.append(&legend_item(false, &gettext("Normal")));
    legend
}

fn legend_item(accent: bool, text: &str) -> gtk::Box {
    let item = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    dot.add_css_class("beat-dot");
    if accent {
        dot.add_css_class("accent");
    }
    dot.set_valign(gtk::Align::Center);
    let label = gtk::Label::new(Some(text));
    item.append(&dot);
    item.append(&label);
    item
}

fn tempo_name(bpm: u32) -> &'static str {
    if bpm < 60 {
        "Largo"
    } else if bpm < 76 {
        "Adagio"
    } else if bpm < 108 {
        "Andante"
    } else if bpm < 120 {
        "Moderato"
    } else if bpm < 156 {
        "Allegro"
    } else if bpm < 200 {
        "Presto"
    } else {
        "Prestissimo"
    }
}

fn beat_tooltip(index: usize, accent: meter::BeatAccent) -> String {
    let beat = index + 1;
    let template = match accent {
        meter::BeatAccent::Accent => gettext("Beat {}: accent"),
        meter::BeatAccent::Normal | meter::BeatAccent::Mute => gettext("Beat {}: normal"),
    };
    template.replacen("{}", &beat.to_string(), 1)
}

glib::wrapper! {
    pub struct MtrApplicationWindow(ObjectSubclass<imp::MtrApplicationWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionMap, gio::ActionGroup;
}

#[gtk::template_callbacks]
impl MtrApplicationWindow {
    pub fn new(app: &MtrApplication) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    fn apply_meter(&self, beats: u32, unit: u32, pattern: String) {
        let meter = meter::Meter::new(beats, unit, pattern);
        let imp = self.imp();
        let index = imp.active_meter_index.get();
        let mut label_changed = true;
        {
            let mut meters = imp.saved_meters.borrow_mut();
            if let Some(slot) = meters.get_mut(index) {
                label_changed = slot.beats != meter.beats || slot.unit != meter.unit;
                *slot = meter.clone();
            }
        }
        imp.save_saved_meters();
        self.set_beat_unit(meter.unit);
        self.set_beats_per_bar(meter.beats);
        self.set_accent_pattern(meter.pattern);
        if label_changed || !imp.meter_buttons_match() {
            imp.rebuild_meter_buttons();
        }
    }

    fn select_meter(&self, index: usize) {
        let meter = {
            let meters = self.imp().saved_meters.borrow();
            let Some(meter) = meters.get(index).cloned() else {
                return;
            };
            meter
        };
        self.imp().active_meter_index.set(index);
        self.apply_meter(meter.beats, meter.unit, meter.pattern);
    }

    fn remove_saved_meter(&self, index: usize) {
        let imp = self.imp();
        let next = {
            let mut meters = imp.saved_meters.borrow_mut();
            if meters.len() <= 1 || index >= meters.len() {
                return;
            }
            meters.remove(index);
            let len = meters.len();
            let mut active = imp.active_meter_index.get();
            if index < active {
                active -= 1;
            } else if index == active {
                active = index.min(len - 1);
            }
            if active >= len {
                active = len - 1;
            }
            imp.active_meter_index.set(active);
            meters[active].clone()
        };
        self.apply_meter(next.beats, next.unit, next.pattern);
    }

    fn present_meter_dialog_for(&self, editing_index: Option<usize>) {
        let imp = self.imp();
        let Some(dialog) = imp.meter_dialog.get() else {
            return;
        };
        let draft = if let Some(index) = editing_index {
            let meters = imp.saved_meters.borrow();
            let Some(meter) = meters.get(index).cloned() else {
                return;
            };
            meter
        } else {
            meter::Meter::new(4, 4, "A")
        };
        let editing = editing_index.is_some();
        let heading = if editing {
            gettext("Edit Time Signature")
        } else {
            gettext("New Time Signature")
        };
        dialog.dialog.set_title(Some(&heading));
        dialog.title.set_label(&heading);
        dialog.ok_button.set_label(&if editing {
            gettext("Apply")
        } else {
            gettext("Add")
        });
        dialog.editing_index.set(editing_index);
        dialog.accepting.set(true);
        dialog.duplicate_label.set_visible(false);
        *dialog.draft.borrow_mut() = draft;
        imp.refresh_meter_dialog();
        dialog.dialog.present();
    }

    fn confirm_meter_dialog(&self) {
        let imp = self.imp();
        let Some(dialog) = imp.meter_dialog.get() else {
            return;
        };
        if !dialog.accepting.replace(false) {
            return;
        }
        let draft = dialog.draft.borrow().clone();
        let meter = meter::Meter::new(draft.beats, draft.unit, draft.pattern);
        let editing_index = dialog.editing_index.get();
        if self.signature_taken(&meter, editing_index) {
            dialog.accepting.set(true);
            self.warn_duplicate_signature(&meter);
            return;
        }

        match editing_index {
            Some(index) => {
                {
                    let mut meters = imp.saved_meters.borrow_mut();
                    if let Some(slot) = meters.get_mut(index) {
                        *slot = meter.clone();
                    }
                }
                if imp.active_meter_index.get() == index {
                    self.set_beat_unit(meter.unit);
                    self.set_beats_per_bar(meter.beats);
                    self.set_accent_pattern(meter.pattern);
                }
            }
            None => {
                if imp.saved_meters.borrow().len() >= meter::SAVED_METERS_MAX {
                    let _ = dialog.dialog.close();
                    return;
                }
                imp.saved_meters.borrow_mut().push(meter.clone());
                imp.active_meter_index
                    .set(imp.saved_meters.borrow().len() - 1);
                self.set_beat_unit(meter.unit);
                self.set_beats_per_bar(meter.beats);
                self.set_accent_pattern(meter.pattern);
            }
        }
        self.sort_saved_meters();
        imp.save_saved_meters();
        imp.rebuild_meter_buttons();
        let _ = dialog.dialog.close();
    }

    fn signature_taken(&self, meter: &meter::Meter, ignore_index: Option<usize>) -> bool {
        self.imp()
            .saved_meters
            .borrow()
            .iter()
            .enumerate()
            .any(|(index, saved)| Some(index) != ignore_index && saved.same_signature(meter))
    }

    fn warn_duplicate_signature(&self, _meter: &meter::Meter) {
        let Some(dialog) = self.imp().meter_dialog.get() else {
            return;
        };
        dialog.duplicate_label.set_label(&gettext("This time signature already exists"));
        dialog.duplicate_label.set_visible(true);
    }

    fn sort_saved_meters(&self) {
        let imp = self.imp();
        let playing = {
            let meters = imp.saved_meters.borrow();
            meters.get(imp.active_meter_index.get()).cloned()
        };
        {
            let mut meters = imp.saved_meters.borrow_mut();
            meters.sort_by(|left, right| {
                left.unit
                    .cmp(&right.unit)
                    .then(left.beats.cmp(&right.beats))
            });
        }
        if let Some(playing) = playing {
            if let Some(index) = imp
                .saved_meters
                .borrow()
                .iter()
                .position(|meter| meter == &playing)
            {
                imp.active_meter_index.set(index);
            }
        }
    }

    fn remove_edited_meter(&self) {
        let imp = self.imp();
        let Some(dialog) = imp.meter_dialog.get() else {
            return;
        };
        let Some(index) = dialog.editing_index.get() else {
            return;
        };
        if imp.saved_meters.borrow().len() <= 1 {
            return;
        }
        dialog.accepting.set(false);
        self.remove_saved_meter(index);
        let _ = dialog.dialog.close();
    }

    /// Rebuilds the dialog after the current signal returns. Rebuilding beat
    /// buttons or the grouping model from inside their own callbacks is unsafe.
    fn queue_meter_dialog_refresh(&self) {
        let imp = self.imp();
        if imp.meter_refresh_queued.get() {
            return;
        }
        imp.meter_refresh_queued.set(true);
        let window = self.clone();
        glib::idle_add_local_once(move || {
            window.imp().meter_refresh_queued.set(false);
            window.imp().refresh_meter_dialog();
        });
    }

    fn cycle_beat(&self, index: usize) {
        if self.imp().updating_meter.get() {
            return;
        }
        let Some(dialog) = self.imp().meter_dialog.get() else {
            return;
        };
        let mut chars: Vec<char> = dialog.draft.borrow().pattern.chars().collect();
        let Some(current) = chars.get(index).copied() else {
            return;
        };
        chars[index] = meter::BeatAccent::from_char(current).cycle().to_char();
        let current = dialog.draft.borrow().clone();
        *dialog.draft.borrow_mut() = meter::Meter::new(
            current.beats,
            current.unit,
            chars.into_iter().collect::<String>(),
        );
        self.queue_meter_dialog_refresh();
    }

    fn step_draft_beats(&self, delta: i32) {
        if self.imp().updating_meter.get() {
            return;
        }
        let Some(dialog) = self.imp().meter_dialog.get() else {
            return;
        };
        let current = dialog.draft.borrow().clone();
        let beats = (current.beats as i32 + delta).clamp(BPB_MIN as i32, BPB_MAX as i32) as u32;
        if beats == current.beats {
            return;
        }
        *dialog.draft.borrow_mut() = meter::Meter::new(beats, current.unit, current.pattern);
        dialog.duplicate_label.set_visible(false);
        self.queue_meter_dialog_refresh();
    }

    fn add_beats_per_minute(&self, value: i32) {
        let bpm = (self.beats_per_minute() as i32 + value).clamp(BPM_MIN as i32, BPM_MAX as i32);
        self.set_beats_per_minute(bpm as u32);
    }

    fn tap(&self) {
        let imp = self.imp();
        let now = Instant::now();
        let duration = now - imp.tap_time.get();
        let bpm = 60.0 / duration.as_secs_f64();
        imp.tap_time.set(now);
        self.set_beats_per_minute((bpm as u32).clamp(BPM_MIN, BPM_MAX));
    }

    fn load_settings(&self) {
        let imp = self.imp();
        if !imp.settings.boolean("meter-migrated") {
            let beats = imp
                .settings
                .uint("beats-per-bar")
                .clamp(BPB_MIN, BPB_MAX);
            let mut unit = imp.settings.uint("beat-unit");
            // The previous UI stored 6/8 as six beats and had no denominator.
            if beats == 6 {
                unit = 8;
            }
            if !meter::is_valid_unit(unit) {
                unit = meter::BEAT_UNIT_DEFAULT;
            }
            let pattern = meter::default_pattern(beats);
            if let Err(err) = imp.settings.set_uint("beat-unit", unit) {
                log::warn!("Failed to save the beat unit, {}", &err);
            }
            if let Err(err) = imp.settings.set_string("accent-pattern", &pattern) {
                log::warn!("Failed to save the accent pattern, {}", &err);
            }
            if let Err(err) = imp.settings.set_boolean("meter-migrated", true) {
                log::warn!("Failed to save the meter migration flag, {}", &err);
            }
        }

        let beats = imp
            .settings
            .uint("beats-per-bar")
            .clamp(BPB_MIN, BPB_MAX);
        let mut unit = imp.settings.uint("beat-unit");
        if !meter::is_valid_unit(unit) {
            unit = meter::BEAT_UNIT_DEFAULT;
        }
        let pattern = meter::fit_pattern(&imp.settings.string("accent-pattern"), beats);
        let active = meter::Meter::new(beats, unit, pattern);

        let stored = imp.settings.strv("saved-meters");
        let stored: Vec<meter::Meter> = stored
            .iter()
            .filter_map(|value| meter::Meter::decode(value))
            .collect();
        let meters = if stored.is_empty() {
            meter::normalize_list(&meter::Meter::defaults(), &active)
        } else {
            meter::normalize_list(&stored, &active)
        };
        let index = meters
            .iter()
            .position(|meter| meter.same_signature(&active))
            .unwrap_or(0);
        imp.active_meter_index.set(index);
        *imp.saved_meters.borrow_mut() = meters;
        self.apply_meter(active.beats, active.unit, active.pattern);
        self.set_beats_per_minute(imp.settings.uint("beats-per-minute").clamp(BPM_MIN, BPM_MAX));
    }

    #[template_callback]
    fn on_add_time_signature_clicked(&self) {
        self.present_meter_dialog_for(None);
    }

    #[template_callback]
    fn on_edit_time_signature_clicked(&self) {
        self.present_meter_dialog_for(Some(self.imp().active_meter_index.get()));
    }

    #[template_callback]
    fn on_tempo_marking_clicked(&self) {
        self.present_tempo_dialog();
    }

    fn present_tempo_dialog(&self) {
        let dialog = adw::Window::new();
        dialog.add_css_class("tempo-presets");
        dialog.set_title(Some(&gettext("Tempo")));
        dialog.set_modal(true);
        dialog.set_transient_for(Some(self));
        dialog.set_destroy_with_parent(true);
        dialog.set_resizable(false);
        dialog.set_default_width(384);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let title = gtk::Label::new(Some(&gettext("Tempo markings")));
        title.add_css_class("dialog-title");
        title.set_halign(gtk::Align::Start);
        title.set_hexpand(true);
        let close = gtk::Button::with_label("×");
        close.set_size_request(24, 24);
        close.connect_clicked(glib::clone!(@weak dialog => move |_| {
            let _ = dialog.close();
        }));
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("dialog-header");
        header.append(&title);
        header.append(&close);
        root.append(&header);

        let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
        list.add_css_class("preset-list");
        let presets = [
            ("Largo", gettext("Very slow and broad"), "40–60", 50_u32),
            ("Adagio", gettext("Slow and calm"), "60–76", 68),
            ("Andante", gettext("At a walking pace"), "76–108", 92),
            ("Moderato", gettext("Moderate"), "108–120", 114),
            ("Allegro", gettext("Fast and lively"), "120–156", 138),
            ("Presto", gettext("Very fast"), "156–200", 176),
        ];
        for (name, description, range, bpm) in presets {
            let button = gtk::Button::new();
            button.add_css_class("preset-row");
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
            text.set_hexpand(true);
            text.set_halign(gtk::Align::Start);
            let name_label = gtk::Label::new(Some(name));
            name_label.add_css_class("preset-name");
            name_label.set_halign(gtk::Align::Start);
            let description_label = gtk::Label::new(Some(&description));
            description_label.add_css_class("preset-description");
            description_label.set_halign(gtk::Align::Start);
            text.append(&name_label);
            text.append(&description_label);
            let range_label = gtk::Label::new(Some(range));
            range_label.add_css_class("preset-range");
            row.append(&text);
            row.append(&range_label);
            button.set_child(Some(&row));
            button.connect_clicked(glib::clone!(@weak self as window, @weak dialog => move |_| {
                window.set_beats_per_minute(bpm);
                let _ = dialog.close();
            }));
            list.append(&button);
        }
        root.append(&list);
        dialog.set_content(Some(&root));
        close_on_escape(&dialog);
        track_modal(self, &dialog);
        use_pointer_cursor(dialog.upcast_ref());
        dialog.present();
    }

    #[template_callback]
    fn on_minimize_clicked(&self) {
        self.minimize();
    }

    #[template_callback]
    fn on_maximize_clicked(&self) {
        if self.is_maximized() {
            self.unmaximize();
        } else {
            self.maximize();
        }
    }

    fn sync_maximize_icon(&self) {
        let icon = if self.is_maximized() {
            "/io/github/ivan_khludov/metronome/icons/restore.svg"
        } else {
            "/io/github/ivan_khludov/metronome/icons/maximize.svg"
        };
        self.imp().maximize_icon.set_resource(Some(icon));
    }

    #[template_callback]
    fn on_close_clicked(&self) {
        let _ = self.close();
    }

    pub fn present_help_dialog(&self) {
        let dialog = adw::Window::new();
        dialog.add_css_class("help-dialog");
        dialog.set_title(Some(&gettext("Shortcuts and hotkeys")));
        dialog.set_modal(true);
        dialog.set_transient_for(Some(self));
        dialog.set_destroy_with_parent(true);
        dialog.set_resizable(false);
        dialog.set_default_width(448);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let mark = gtk::Label::new(Some("?"));
        mark.add_css_class("help-mark");
        mark.set_size_request(20, 20);
        mark.set_valign(gtk::Align::Center);
        let title = gtk::Label::new(Some(&gettext("Shortcuts and hotkeys")));
        title.add_css_class("dialog-title");
        title.set_halign(gtk::Align::Start);
        title.set_hexpand(true);
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        heading.append(&mark);
        heading.append(&title);
        let close = gtk::Button::with_label("×");
        close.set_size_request(28, 28);
        close.connect_clicked(glib::clone!(@weak dialog => move |_| {
            let _ = dialog.close();
        }));
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("dialog-header");
        header.append(&heading);
        header.append(&close);
        root.append(&header);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
        body.add_css_class("help-body");
        let kicker = gtk::Label::new(Some(&gettext("Keyboard shortcuts")));
        kicker.add_css_class("help-kicker");
        kicker.set_halign(gtk::Align::Start);
        body.append(&kicker);
        let rows = [
            (gettext("Start or pause"), "Space"),
            (gettext("Tap"), "T"),
            (gettext("BPM ±1"), "↑  ↓"),
            (gettext("BPM ±5"), "Shift + ↑ / ↓"),
            (gettext("Show shortcuts"), "?"),
            (gettext("Close window"), "Esc"),
            (gettext("Quit"), "Ctrl + Q"),
        ];
        for (name, keys) in rows {
            body.append(&help_row(&name, keys));
        }

        let about = gtk::Box::new(gtk::Orientation::Vertical, 4);
        about.add_css_class("help-about");
        let about_title = gtk::Label::new(Some(&gettext("About")));
        about_title.add_css_class("about-title");
        about_title.set_halign(gtk::Align::Start);
        let about_body = gtk::Label::new(Some(&gettext(
            "Metronome keeps the beat. Set a time signature and a tempo, or tap to guess it.",
        )));
        about_body.add_css_class("about-body");
        about_body.set_wrap(true);
        about_body.set_xalign(0.0);
        about_body.set_halign(gtk::Align::Start);
        let version = gtk::Label::new(Some(&format!("{} {}", gettext("Version:"), crate::config::VERSION)));
        version.add_css_class("about-body");
        version.set_halign(gtk::Align::Start);
        about.append(&about_title);
        about.append(&about_body);
        about.append(&version);
        body.append(&about);
        root.append(&body);

        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        footer.add_css_class("help-footer");
        footer.set_halign(gtk::Align::Fill);
        let done = gtk::Button::with_label(&gettext("Got it"));
        done.add_css_class("help-done");
        done.set_halign(gtk::Align::End);
        done.set_hexpand(true);
        done.connect_clicked(glib::clone!(@weak dialog => move |_| {
            let _ = dialog.close();
        }));
        footer.append(&done);
        root.append(&footer);

        dialog.set_content(Some(&root));
        close_on_escape(&dialog);
        track_modal(self, &dialog);
        use_pointer_cursor(dialog.upcast_ref());
        dialog.present();
    }
}

fn help_row(name: &str, keys: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("help-row");
    let label = gtk::Label::new(Some(name));
    label.add_css_class("help-name");
    label.set_halign(gtk::Align::Start);
    label.set_hexpand(true);
    let keys = gtk::Label::new(Some(keys));
    keys.add_css_class("help-keys");
    row.append(&label);
    row.append(&keys);
    row
}
