use crate::config;
use crate::window::MtrApplicationWindow;
use adw::prelude::AdwDialogExt;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::prelude::*;
use gtk::{gio, glib};

mod imp {
    use super::*;
    use glib::WeakRef;
    use std::cell::OnceCell;

    #[derive(Debug, Default)]
    pub struct MtrApplication {
        pub window: OnceCell<WeakRef<MtrApplicationWindow>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MtrApplication {
        const NAME: &'static str = "MtrApplication";
        type Type = super::MtrApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for MtrApplication {}

    impl ApplicationImpl for MtrApplication {
        fn activate(&self) {
            log::debug!("GtkApplication<MtrApplication>::activate");
            self.parent_activate();

            // Set icons for shell
            gtk::Window::set_default_icon_name(config::APP_ID);

            let app = self.obj();
            if let Some(window) = self.window.get() {
                let window = window.upgrade().unwrap();
                window.present();
                return;
            }

            let window = MtrApplicationWindow::new(&app);
            self.window
                .set(window.downgrade())
                .expect("Window already set.");

            window.present();
        }

        fn startup(&self) {
            log::debug!("GtkApplication<MtrApplication>::startup");
            self.parent_startup();
            load_css();
            let app = self.obj();

            app.setup_gactions();
            app.setup_accels();
        }
    }

    impl GtkApplicationImpl for MtrApplication {}
    impl AdwApplicationImpl for MtrApplication {}
}

glib::wrapper! {
    pub struct MtrApplication(ObjectSubclass<imp::MtrApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl MtrApplication {
    fn get_main_window(&self) -> MtrApplicationWindow {
        self.imp().window.get().unwrap().upgrade().unwrap()
    }

    fn setup_gactions(&self) {
        // Quit
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(|app: &Self, _, _| {
                // This is needed to trigger the delete event
                // and saving the window state
                app.get_main_window().close();
                app.quit();
            })
            .build();

        // About
        let about_action = gio::ActionEntry::builder("about")
            .activate(|app: &Self, _, _| {
                app.show_about_dialog();
            })
            .build();

        //Start and stop playing
        let sound_control_action = gio::ActionEntry::builder("sound-control")
            .activate(|app: &Self, _, _| {
                let window = app.get_main_window();
                let timer = &window.imp().timer_button;
                if timer.active() {
                    timer.pause();
                } else {
                    timer.start();
                }
            })
            .build();

        let shortcuts_action = gio::ActionEntry::builder("shortcuts")
            .activate(|app: &Self, _, _| {
                app.show_shortcuts_dialog();
            })
            .build();

        self.add_action_entries([
            quit_action,
            about_action,
            sound_control_action,
            shortcuts_action,
        ]);
    }

    // Sets up keyboard shortcuts
    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<primary>q"]);
        self.set_accels_for_action("win.tap", &["t"]);
        self.set_accels_for_action("app.sound-control", &["space"]);
        self.set_accels_for_action("win.increase-bpm", &["Up"]);
        self.set_accels_for_action("win.decrease-bpm", &["Down"]);
        self.set_accels_for_action("win.increase-bpm-fast", &["<shift>Up"]);
        self.set_accels_for_action("win.decrease-bpm-fast", &["<shift>Down"]);
        self.set_accels_for_action("app.shortcuts", &["question"]);
    }

    fn show_shortcuts_dialog(&self) {
        self.get_main_window().present_help_dialog();
    }

    fn show_about_dialog(&self) {
        let mut details = String::new();
        details.push_str("<b>");
        details.push_str(&gettext("Keep the tempo"));
        details.push_str("</b>\n");
        details.push_str(&gettext("Metronome beats the rhythm for you, you simply need to tell it the required time signature and beats per minutes.\n"));
        details.push_str(&gettext(
            "You can also tap to let the application guess the required beats per minute",
        ));

        adw::AboutDialog::builder()
            .application_name("Metronome")
            .application_icon(config::APP_ID)
            .license_type(gtk::License::Gpl30)
            .website("https://gitlab.gnome.org/World/metronome/")
            .issue_url("https://gitlab.gnome.org/World/metronome/-/issues")
            .version(config::VERSION)
            .comments(details)
            .developers(vec![
                "Ivan Khludov <ivan.khludov.dev@gmail.com>",
                "Adrien Plazas <kekun.plazas@laposte.net>",
                "Clara Hobbs <clara@clarahobbs.com>",
                "FineFindus https://gitlab.gnome.org/FineFindus",
            ])
            .artists(vec!["Tobias Bernard <tbernard@gnome.org>"])
            // Translators: Please enter your credits here (format: "Name https://example.com" or "Name <email@example.com>", no quotes)
            .translator_credits(gettext("translator-credits"))
            .build()
            .present(&self.get_main_window());
    }

    pub fn run(&self) -> glib::ExitCode {
        log::info!("Metronome ({})", config::APP_ID);
        log::info!("Version: {} ({})", config::VERSION, config::PROFILE);
        log::info!("Datadir: {}", config::PKGDATADIR);

        ApplicationExtManual::run(self)
    }
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_resource("/io/github/ivan_khludov/metronome/style.css");
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_USER,
        );
    }
}

pub(crate) fn register_fonts() {
    let dir = std::path::Path::new(config::PKGDATADIR).join("fonts");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        log::warn!("Font directory not found: {}", dir.display());
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("ttf") {
            continue;
        }
        let Ok(text) = std::ffi::CString::new(path.to_string_lossy().as_bytes()) else {
            continue;
        };
        let added = unsafe {
            fc_config_app_font_add_file(std::ptr::null_mut(), text.as_ptr() as *const u8)
        };
        if added == 0 {
            log::warn!("Failed to add font {}", path.display());
        }
    }
    unsafe {
        fc_config_build_fonts(std::ptr::null_mut());
    }
}

#[link(name = "fontconfig")]
extern "C" {
    #[link_name = "FcConfigAppFontAddFile"]
    fn fc_config_app_font_add_file(config: *mut std::ffi::c_void, file: *const u8) -> i32;
    #[link_name = "FcConfigBuildFonts"]
    fn fc_config_build_fonts(config: *mut std::ffi::c_void) -> i32;
}

impl Default for MtrApplication {
    fn default() -> Self {
        glib::Object::builder()
            .property("application-id", config::APP_ID)
            .property("resource-base-path", "/io/github/ivan_khludov/metronome/")
            .build()
    }
}
