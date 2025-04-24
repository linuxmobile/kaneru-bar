mod utils;
mod widgets;
mod windows;

use gio::ApplicationFlags;
use gtk4::prelude::*;
use utils::{apply_css, load_config};
use windows::BarWindow;

const APP_ID: &str = "com.github.linuxmobile.kaneru";

fn main() -> glib::ExitCode {
    let app = gtk4::Application::builder()
        .application_id(APP_ID)
        .flags(ApplicationFlags::default())
        .build();

    app.connect_startup(|_| {
        println!("Applying CSS during startup...");
        apply_css();
        println!("CSS application attempted.");
    });

    app.connect_activate(build_ui);

    app.run()
}

fn build_ui(app: &gtk4::Application) {
    println!("Building UI...");
    let config = load_config();
    let bar = BarWindow::new(app, &config);
    bar.present();
    println!("UI Built and Presented.");
}
