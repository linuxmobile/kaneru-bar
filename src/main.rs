mod generated;
mod utils;
mod widgets;
mod windows;

use gio::ApplicationFlags;
use gtk4::prelude::*;
use gtk4::{glib, Settings};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    sync::Arc,
};
use tokio::sync::mpsc;
use utils::{
    apply_css, load_config,
    network::{NetworkCommand, NetworkResult, NetworkService, NetworkUtilError},
    notification_manager,
    notification_server::{self, NotificationServer},
    BarConfig,
};
use widgets::NetworkWidget;
use windows::{BarWindow, NetworkWindow};

const APP_ID: &str = "com.github.linuxmobile.kaneru";

async fn network_actor_task(
    mut command_rx: mpsc::Receiver<NetworkCommand>,
    result_tx: mpsc::UnboundedSender<NetworkResult>,
    service: Arc<NetworkService>,
) {
    while let Some(command) = command_rx.recv().await {
        let result = match command {
            NetworkCommand::GetDetails => NetworkResult::Details(service.get_wifi_details().await),
            NetworkCommand::GetAccessPoints => {
                NetworkResult::AccessPoints(service.get_access_points().await)
            }
            NetworkCommand::RequestScan => {
                NetworkResult::ScanRequested(service.request_scan().await)
            }
            NetworkCommand::SetWifiEnabled(enabled) => {
                NetworkResult::WifiSet(service.set_wifi_enabled(enabled).await)
            }
            NetworkCommand::SetAirplaneMode(enabled) => {
                NetworkResult::AirplaneModeSet(service.set_airplane_mode(enabled).await)
            }
            NetworkCommand::GetAirplaneModeState => {
                NetworkResult::AirplaneModeState(service.get_airplane_mode_state().await)
            }
            NetworkCommand::ConnectToNetwork {
                ap_path,
                device_path,
            } => NetworkResult::Connected(service.connect_to_network(&ap_path, &device_path).await),
        };

        if result_tx.send(result).is_err() {
            break;
        }
    }
}

fn setup_network_result_handler(
    rx: mpsc::UnboundedReceiver<NetworkResult>,
    widget_weak: Weak<NetworkWidget>,
    window_weak: Weak<NetworkWindow>,
) {
    let rx = Rc::new(RefCell::new(rx));

    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        let mut rx_guard = rx.borrow_mut();

        for _ in 0..5 {
            match rx_guard.try_recv() {
                Ok(result) => {
                    let result_clone = result;
                    let widget_weak_clone = widget_weak.clone();
                    let window_weak_clone = window_weak.clone();

                    glib::idle_add_local_once(move || {
                        process_network_result(
                            result_clone,
                            &widget_weak_clone,
                            &window_weak_clone,
                        );
                    });
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    break;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    return glib::ControlFlow::Break;
                }
            }
        }

        glib::ControlFlow::Continue
    });
}

fn process_network_result(
    result: NetworkResult,
    widget_weak: &Weak<NetworkWidget>,
    window_weak: &Weak<NetworkWindow>,
) {
    match result {
        NetworkResult::Details(details_res) => {
            let details_for_window = details_res.clone();
            if let Some(widget) = widget_weak.upgrade() {
                widget.update_state(details_res);
            }
            if let Some(window) = window_weak.upgrade() {
                window.update_state(details_for_window);
            }
        }
        NetworkResult::AccessPoints(aps_res) => {
            if let Some(window) = window_weak.upgrade() {
                window.update_ap_list(aps_res);
            }
        }
        NetworkResult::ScanRequested(res) => {
            if let Err(e) = res {
                eprintln!("[Handler] Scan request failed: {}", e);
                if let Some(window) = window_weak.upgrade() {
                    window.update_ap_list(Err(e));
                }
            }
        }
        NetworkResult::WifiSet(res) => {
            if let Some(window) = window_weak.upgrade() {
                window.handle_wifi_set_result(res);
            } else if let Err(e) = res {
                eprintln!("[Handler] WifiSet failed, window gone: {}", e);
            }
        }
        NetworkResult::AirplaneModeSet(res) => {
            if let Some(window) = window_weak.upgrade() {
                window.handle_airplane_set_result(res);
            } else if let Err(e) = res {
                eprintln!("[Handler] AirplaneSet failed, window gone: {}", e);
            }
        }
        NetworkResult::AirplaneModeState(res) => {
            if let Some(window) = window_weak.upgrade() {
                window.update_airplane_state(res);
            }
        }
        NetworkResult::Connected(res) => {
            if let Some(window) = window_weak.upgrade() {
                window.handle_connect_result(res);
            } else if let Err(e) = res {
                eprintln!("[Handler] Connect failed, window gone: {}", e);
            }
        }
    }
}

#[tokio::main]
async fn main() -> glib::ExitCode {
    let config = load_config();

    let app = gtk4::Application::builder()
        .application_id(APP_ID)
        .flags(ApplicationFlags::default())
        .build();

    let (notify_tx, notify_rx) = mpsc::channel(32);
    let (command_tx_notify, command_rx_notify) = mpsc::channel(32);
    let notification_server = Arc::new(NotificationServer::new(notify_tx));
    let command_tx_clone = command_tx_notify.clone();
    let server_clone = notification_server.clone();
    let config_for_manager = config.clone();
    notification_manager::run_manager_task(
        app.clone(),
        notify_rx,
        command_tx_clone,
        command_rx_notify,
        server_clone,
        config_for_manager,
    );
    let server_handle = tokio::spawn(notification_server::run_server_task(
        notification_server.clone(),
    ));

    let (net_command_tx, net_command_rx) = mpsc::channel::<NetworkCommand>(32);

    let (net_result_tx, net_result_rx) = mpsc::unbounded_channel::<NetworkResult>();

    let network_service_result: Result<Arc<NetworkService>, NetworkUtilError> =
        NetworkService::new().await;

    let network_service_available = network_service_result.is_ok();

    if let Ok(service) = network_service_result {
        let service_clone = service.clone();
        tokio::spawn(network_actor_task(
            net_command_rx,
            net_result_tx,
            service_clone,
        ));
    } else if let Err(e) = network_service_result {
        eprintln!("Failed to initialize NetworkService: {}", e);
    }

    let bar_window_holder: Rc<RefCell<Option<BarWindow>>> = Rc::new(RefCell::new(None));
    let network_widget_holder: Rc<RefCell<Option<Rc<NetworkWidget>>>> = Rc::new(RefCell::new(None));
    let network_window_holder: Rc<RefCell<Option<Rc<NetworkWindow>>>> = Rc::new(RefCell::new(None));

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
    let net_command_tx_clone = net_command_tx.clone();
    let bar_window_holder_clone = bar_window_holder.clone();
    let network_widget_holder_clone = network_widget_holder.clone();
    let network_window_holder_clone = network_window_holder.clone();

    let net_result_rx_holder = Rc::new(RefCell::new(Some(net_result_rx)));

    app.connect_activate(move |app| {
        let built_bar = build_ui(
            app,
            &config_clone_activate,
            net_command_tx_clone.clone(),
            network_service_available,
        );
        built_bar.present();

        *network_widget_holder_clone.borrow_mut() = built_bar.network_widget.clone();
        *network_window_holder_clone.borrow_mut() = built_bar.network_window.clone();
        *bar_window_holder_clone.borrow_mut() = Some(built_bar);

        if network_service_available {
            if let Some(rx) = net_result_rx_holder.borrow_mut().take() {
                let nw_widget_weak = network_widget_holder_clone
                    .borrow()
                    .as_ref()
                    .map_or(Weak::new(), |rc| Rc::downgrade(rc));

                let nw_window_weak = network_window_holder_clone
                    .borrow()
                    .as_ref()
                    .map_or(Weak::new(), |rc| Rc::downgrade(rc));

                setup_network_result_handler(rx, nw_widget_weak, nw_window_weak);
            }
        }
    });

    let exit_code = app.run();

    server_handle.abort();

    exit_code
}

fn build_ui(
    app: &gtk4::Application,
    config: &BarConfig,
    net_command_tx: mpsc::Sender<NetworkCommand>,
    network_service_available: bool,
) -> BarWindow {
    BarWindow::new(app, config, net_command_tx, network_service_available)
}
