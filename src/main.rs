mod utils;
mod widgets;
mod windows;

use gio::ApplicationFlags;
use gtk4::prelude::*;
use gtk4::Settings;
use utils::{apply_css, load_config, BarConfig};
use windows::BarWindow;

const APP_ID: &str = "com.github.linuxmobile.kaneru";

fn main() -> glib::ExitCode {
    let config = load_config();

    let app = gtk4::Application::builder()
        .application_id(APP_ID)
        .flags(ApplicationFlags::default())
        .build();

    let config_clone = config.clone();

    app.connect_startup(move |_| {
        println!("Applying CSS during startup...");
        apply_css();
        println!("CSS application attempted.");

        if let Some(font_name) = &config_clone.font {
            if let Some(settings) = Settings::default() {
                println!("Applying configured font: '{}'", font_name);
                settings.set_property("gtk-font-name", font_name);
                let current_font: Option<String> = settings.property("gtk-font-name");
                println!("Current gtk-font-name after setting: {:?}", current_font);
            } else {
                eprintln!("Error: Could not get default GtkSettings to apply font.");
            }
        } else {
            println!("No font specified in config, using default.");
        }
    });

    app.connect_activate(move |app| build_ui(app, &config));

    app.run()
}

fn build_ui(app: &gtk4::Application, config: &BarConfig) {
    println!("Building UI with config: {:?}", config);
    let bar = BarWindow::new(app, config);
    bar.present();
    println!("UI Built and Presented.");
}
