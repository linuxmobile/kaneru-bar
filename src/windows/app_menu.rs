use gtk4::prelude::*;
use gtk4::{
    gio, glib, Align, Box as GtkBox, Button, Image, Label, ListBox, Orientation, Popover,
    ScrolledWindow, SearchEntry, SelectionMode, Spinner,
};
use std::{cell::Cell, cell::RefCell, rc::Rc};

type AppInfoData = (String, Option<String>, String);
type AppInfoEntry = (String, Option<gio::Icon>, String);

pub struct AppMenu {
    popover: Popover,
    list_box: ListBox,
    search_entry: SearchEntry,
    spinner: Spinner,
    all_apps: Rc<RefCell<Option<Vec<AppInfoEntry>>>>,
    apps_loaded: Rc<Cell<bool>>,
}

impl AppMenu {
    pub fn new() -> Rc<Self> {
        let popover = Popover::new();
        popover.add_css_class("AppMenuPopover");

        let container = GtkBox::new(Orientation::Vertical, 6);
        container.add_css_class("app-menu-container");

        let search_entry = SearchEntry::new();
        search_entry.add_css_class("app-menu-search");
        container.append(&search_entry);

        let scroll = ScrolledWindow::new();
        scroll.set_min_content_height(400);
        scroll.set_min_content_width(300);
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.add_css_class("app-menu-scroll");

        let list_box = ListBox::new();
        list_box.add_css_class("app-menu-list");
        list_box.set_selection_mode(SelectionMode::None);

        let spinner = Spinner::builder()
            .spinning(true)
            .halign(Align::Center)
            .valign(Align::Center)
            .hexpand(true)
            .vexpand(true)
            .visible(false)
            .build();

        let list_overlay = gtk4::Overlay::new();
        list_overlay.set_child(Some(&list_box));
        list_overlay.add_overlay(&spinner);

        scroll.set_child(Some(&list_overlay));
        container.append(&scroll);

        popover.set_child(Some(&container));

        let menu = Rc::new(Self {
            popover,
            list_box,
            search_entry,
            spinner,
            all_apps: Rc::new(RefCell::new(None)),
            apps_loaded: Rc::new(Cell::new(false)),
        });

        menu.connect_search();
        menu.connect_popover_visibility();

        menu
    }

    fn load_applications_async(self: Rc<Self>) {
        self.spinner.set_visible(true);
        self.spinner.start();
        self.list_box.set_visible(false);

        let list_box_clone = self.list_box.clone();
        let spinner_clone = self.spinner.clone();
        let all_apps_clone = self.all_apps.clone();
        let apps_loaded_clone = self.apps_loaded.clone();
        let search_entry_clone = self.search_entry.clone();
        let self_clone = self.clone();

        glib::MainContext::default().spawn_local(async move {
            let app_data_result = tokio::task::spawn_blocking(Self::load_applications_sync).await;

            match app_data_result {
                Ok(app_data) => {
                    let mut final_apps: Vec<AppInfoEntry> = Vec::with_capacity(app_data.len());
                    for (name, icon_name_opt, exec) in app_data {
                        let icon = icon_name_opt
                            .and_then(|icon_name| gio::Icon::for_string(&icon_name).ok());
                        final_apps.push((name, icon, exec));
                    }

                    *all_apps_clone.borrow_mut() = Some(final_apps);
                    apps_loaded_clone.set(true);

                    Self::populate_list_box_static(
                        &self_clone,
                        &list_box_clone,
                        &all_apps_clone.borrow().as_ref().unwrap(),
                    );
                    Self::filter_applications_static(
                        &self_clone,
                        &list_box_clone,
                        &all_apps_clone.borrow().as_ref().unwrap(),
                        &search_entry_clone.text(),
                    );
                }
                Err(e) => {
                    eprintln!("Failed to load applications in background task: {}", e);
                    *all_apps_clone.borrow_mut() = Some(Vec::new());
                    apps_loaded_clone.set(true);
                }
            }

            spinner_clone.stop();
            spinner_clone.set_visible(false);
            list_box_clone.set_visible(true);
        });
    }

    fn load_applications_sync() -> Vec<AppInfoData> {
        let mut apps: Vec<AppInfoData> = gio::AppInfo::all()
            .into_iter()
            .filter(gio::AppInfo::should_show)
            .filter_map(|app_info| {
                let name = app_info.name().to_string();
                let icon_name = app_info
                    .icon()
                    .and_then(|i| i.to_string())
                    .map(|gs| gs.to_string());
                let exec = app_info.commandline()?.to_string_lossy().into_owned();

                if !name.is_empty() && !exec.is_empty() {
                    Some((name, icon_name, exec))
                } else {
                    None
                }
            })
            .collect();

        apps.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        apps
    }

    fn create_app_row(
        app_menu: &Rc<Self>,
        name: &str,
        icon: &Option<gio::Icon>,
        exec: &str,
    ) -> Button {
        let row = GtkBox::new(Orientation::Horizontal, 12);
        row.add_css_class("app-menu-item-box");

        let image = match icon {
            Some(gicon) => Image::from_gicon(gicon),
            None => {
                let fallback_icon = gio::Icon::for_string("application-x-executable")
                    .expect("Failed to create fallback gio::Icon");
                Image::from_gicon(&fallback_icon)
            }
        };
        image.add_css_class("app-menu-item-icon");
        image.set_pixel_size(32);
        row.append(&image);

        let label = Label::new(Some(name));
        label.add_css_class("app-menu-item-label");
        label.set_halign(Align::Start);
        label.set_hexpand(true);
        row.append(&label);

        let button = Button::new();
        button.add_css_class("app-menu-item");
        button.set_child(Some(&row));

        let exec = exec.to_string();
        let name = name.to_string();
        let popover_weak = app_menu.popover.downgrade();
        button.connect_clicked(move |_| {
            if let Some(popover) = popover_weak.upgrade() {
                popover.popdown();
            }
            match gio::AppInfo::create_from_commandline(
                &exec,
                Some(&name),
                gio::AppInfoCreateFlags::NONE,
            ) {
                Ok(app_info) => {
                    if let Err(e) = app_info.launch(&[], None::<&gio::AppLaunchContext>) {
                        eprintln!("Failed to launch '{}' ({}): {}", name, exec, e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to create AppInfo for '{}' ({}): {}", name, exec, e);
                }
            }
        });

        button
    }

    fn populate_list_box_static(app_menu: &Rc<Self>, list_box: &ListBox, apps: &[AppInfoEntry]) {
        while let Some(child) = list_box.first_child() {
            list_box.remove(&child);
        }
        for (name, icon, exec) in apps {
            let row = Self::create_app_row(app_menu, name, icon, exec);
            list_box.append(&row);
        }
    }

    fn connect_popover_visibility(self: &Rc<Self>) {
        let self_clone = self.clone();
        self.popover.connect_visible_notify(move |popover| {
            if popover.is_visible() && !self_clone.apps_loaded.get() {
                self_clone.clone().load_applications_async();
            }
        });
    }

    fn connect_search(self: &Rc<Self>) {
        let list_box_clone = self.list_box.clone();
        let apps_rc_clone = self.all_apps.clone();
        let apps_loaded_clone = self.apps_loaded.clone();
        let self_clone = self.clone();

        self.search_entry.connect_search_changed(move |entry| {
            if !apps_loaded_clone.get() {
                return;
            }
            let text = entry.text();
            let apps_borrow = apps_rc_clone.borrow();
            if let Some(apps) = apps_borrow.as_ref() {
                Self::filter_applications_static(&self_clone, &list_box_clone, apps, &text);
            }
        });
    }

    fn filter_applications_static(
        app_menu: &Rc<Self>,
        list_box: &ListBox,
        apps: &[AppInfoEntry],
        search: &str,
    ) {
        while let Some(child) = list_box.first_child() {
            list_box.remove(&child);
        }

        let search_lower = search.to_lowercase();
        for (name, icon, exec) in apps.iter() {
            if search_lower.is_empty() || name.to_lowercase().contains(&search_lower) {
                let row = Self::create_app_row(app_menu, name, icon, exec);
                list_box.append(&row);
            }
        }
    }

    pub fn popover(&self) -> &Popover {
        &self.popover
    }
}
