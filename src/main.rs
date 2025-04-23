mod utils;
mod windows;

use gio::ApplicationFlags;
use gtk4::prelude::*;
use utils::load_config;
use windows::BarWindow;

fn main() {
    let app = gtk4::Application::builder()
        .application_id("com.github.yourusername.kaneru")
        .flags(ApplicationFlags::default())
        .build();

    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &gtk4::Application) {
    let config = load_config();
    let bar = BarWindow::new(app, &config);
    bar.present();
}
