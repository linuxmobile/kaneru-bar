mod generated;
mod utils;
mod widgets;
mod windows;

use gio::ApplicationFlags;
use gtk4::prelude::*;
use gtk4::{glib, Settings};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use utils::{
    apply_css, load_config,
    network::NetworkService,
    notification_manager,
    notification_server::{self, NotificationServer},
    BarConfig,
};
use windows::BarWindow;

const APP_ID: &str = "com.github.linuxmobile.kaneru";

#[tokio::main]
async fn main() -> glib::ExitCode {
    let config = load_config();

    let app = gtk4::Application::builder()
        .application_id(APP_ID)
        .flags(ApplicationFlags::default())
        .build();

    let (notify_tx, notify_rx) = mpsc::channel(32);
    let (command_tx, command_rx) = mpsc::channel(32);
    let notification_server = Arc::new(NotificationServer::new(notify_tx));
    let command_tx_clone = command_tx.clone();
    let server_clone = notification_server.clone();
    let config_for_manager = config.clone();
    notification_manager::run_manager_task(
        app.clone(),
        notify_rx,
        command_tx_clone,
        command_rx,
        server_clone,
        config_for_manager,
    );
    let server_handle = tokio::spawn(notification_server::run_server_task(
        notification_server.clone(),
    ));

    let network_service_result: Result<Arc<NetworkService>, utils::network::NetworkUtilError> =
        NetworkService::new().await;
    let network_service = match network_service_result {
        Ok(service) => Some(service),
        Err(e) => {
            eprintln!("Failed to initialize NetworkService: {}", e);
            None
        }
    };
    let network_service_arc: Arc<Mutex<Option<Arc<NetworkService>>>> =
        Arc::new(Mutex::new(network_service));

    let config_clone_startup = config.clone();
    app.connect_startup(move |_| {
        apply_css();

        if let Some(font_name) = &config_clone_startup.font {
            if let Some(settings) = Settings::default() {
                settings.set_property("gtk-font-name", font_name);
            } else {
                eprintln!("Error: Could not get default GtkSettings to apply font.");
            }
        }
    });

    let config_clone_activate = config.clone();
    let network_service_clone = network_service_arc.clone();
    app.connect_activate(move |app| {
        build_ui(app, &config_clone_activate, network_service_clone.clone());
    });

    let exit_code = app.run();

    server_handle.abort();

    exit_code
}

fn build_ui(
    app: &gtk4::Application,
    config: &BarConfig,
    network_service: Arc<Mutex<Option<Arc<NetworkService>>>>,
) {
    let bar = BarWindow::new(app, config, network_service);
    bar.present();
}
