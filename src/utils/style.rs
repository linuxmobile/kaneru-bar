const CSS_DATA: &str = include_str!("../resources/style.css");

pub fn load_css() -> gtk4::CssProvider {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(CSS_DATA);
    provider
}

pub fn apply_css() {
    let provider = load_css();
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    } else {
        eprintln!("Could not get default display for applying CSS");
    }
}
