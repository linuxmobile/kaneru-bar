use gio::prelude::*;
use gtk4::prelude::*;
use gtk4::{
    Align, Box, Button, Image, Label, ListBox, Orientation, Popover, ScrolledWindow, SearchEntry,
    SelectionMode,
};

pub struct AppMenu {
    popover: Popover,
    list_box: ListBox,
    search_entry: SearchEntry,
    all_apps: Vec<(String, Option<gio::Icon>, String)>,
}

impl AppMenu {
    pub fn new() -> Self {
        let popover = Popover::new();
        popover.add_css_class("AppMenuPopover");

        let container = Box::new(Orientation::Vertical, 6);
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

        let all_apps = Self::load_applications();

        Self::populate_list_box(&list_box, &all_apps);

        scroll.set_child(Some(&list_box));
        container.append(&scroll);

        popover.set_child(Some(&container));

        let menu = Self {
            popover,
            list_box,
            search_entry,
            all_apps,
        };

        menu.connect_search();
        menu
    }

    fn load_applications() -> Vec<(String, Option<gio::Icon>, String)> {
        let mut apps: Vec<(String, Option<gio::Icon>, String)> = gio::AppInfo::all()
            .into_iter()
            .filter(gio::AppInfo::should_show)
            .filter_map(|app_info| {
                let name = app_info.name().to_string();
                let icon = app_info.icon();
                let exec = app_info.commandline()?.to_string_lossy().into_owned();

                if !name.is_empty() && !exec.is_empty() {
                    Some((name, icon, exec))
                } else {
                    None
                }
            })
            .collect();

        apps.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        apps
    }

    fn create_app_row(name: &str, icon: &Option<gio::Icon>, exec: &str) -> Button {
        let row = Box::new(Orientation::Horizontal, 12);
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
        button.connect_clicked(move |_| {
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

    fn populate_list_box(list_box: &ListBox, apps: &[(String, Option<gio::Icon>, String)]) {
        for (name, icon, exec) in apps {
            let row = Self::create_app_row(name, icon, exec);
            list_box.append(&row);
        }
    }

    fn connect_search(&self) {
        let list_box_clone = self.list_box.clone();
        let apps_clone = self.all_apps.clone();
        self.search_entry.connect_search_changed(move |entry| {
            let text = entry.text();
            Self::filter_applications_static(&list_box_clone, &apps_clone, &text);
        });
    }

    fn filter_applications_static(
        list_box: &ListBox,
        apps: &[(String, Option<gio::Icon>, String)],
        search: &str,
    ) {
        while let Some(child) = list_box.first_child() {
            list_box.remove(&child);
        }

        let search_lower = search.to_lowercase();
        for (name, icon, exec) in apps.iter() {
            if search_lower.is_empty() || name.to_lowercase().contains(&search_lower) {
                let row = Self::create_app_row(name, icon, exec);
                list_box.append(&row);
            }
        }
    }

    pub fn popover(&self) -> &Popover {
        &self.popover
    }
}

impl Default for AppMenu {
    fn default() -> Self {
        Self::new()
    }
}
