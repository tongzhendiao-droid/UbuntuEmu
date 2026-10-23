//! Droid 图形界面（GTK4 + libadwaita）。界面只负责展示，所有操作都交给 droid-core。

mod apps_page;
mod context;
mod doctor_page;
mod status_page;
mod window;

use adw::prelude::*;
use gtk::{gio, glib};

/// 占位的应用 ID，产品定名后替换。
const APP_ID: &str = "com.example.Droid";

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_startup(setup_actions);
    app.connect_activate(|app| match app.active_window() {
        Some(window) => window.present(),
        None => window::build(app).present(),
    });
    app.run()
}

fn setup_actions(app: &adw::Application) {
    let quit = gio::ActionEntry::builder("quit")
        .activate(|app: &adw::Application, _, _| app.quit())
        .build();
    let about = gio::ActionEntry::builder("about")
        .activate(|app: &adw::Application, _, _| show_about(app))
        .build();
    app.add_action_entries([quit, about]);
    app.set_accels_for_action("app.quit", &["<Control>q"]);
    app.set_accels_for_action("win.refresh", &["<Control>r", "F5"]);
    app.set_accels_for_action("win.install-apk", &["<Control>o"]);
}

fn show_about(app: &adw::Application) {
    let dialog = adw::AboutDialog::builder()
        .application_name("Droid")
        .version(env!("CARGO_PKG_VERSION"))
        .comments("基于 Waydroid 的 Ubuntu 安卓运行时（MVP）")
        .build();
    dialog.present(app.active_window().as_ref());
}
