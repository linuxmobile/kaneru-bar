use crate::generated;
use std::error::Error;
use std::sync::OnceLock;

const CSS_RESOURCE_PATH: &str = "/com/github/linuxmobile/kaneru/style.css";

static RESOURCE_REGISTRATION_RESULT: OnceLock<Result<(), String>> = OnceLock::new();

fn load_resources_once() -> Result<(), String> {
    RESOURCE_REGISTRATION_RESULT.get_or_init(|| {
        println!("Loading GResource data from embedded bytes...");
        let resource_data = glib::Bytes::from_static(generated::RESOURCE_BYTES);
        match gio::Resource::from_data(&resource_data) {
            Ok(res) => {
                gio::resources_register(&res);
                println!("GResource registered successfully from embedded bytes.");
                if gio::resources_lookup_data(CSS_RESOURCE_PATH, gio::ResourceLookupFlags::NONE).is_err() {
                     let err_msg = format!(
                        "CRITICAL ERROR: CSS resource NOT found at path '{}' after registering embedded resource.",
                        CSS_RESOURCE_PATH
                    );
                    eprintln!("{}", err_msg);
                    Err(err_msg)
                } else {
                    println!("SUCCESS: CSS resource found at path: {}", CSS_RESOURCE_PATH);
                    Ok(())
                }
            }
            Err(e) => {
                let err_msg = format!("CRITICAL: Failed to load GResource bundle from embedded data: {}", e);
                eprintln!("{}", err_msg);
                Err(err_msg)
            }
        }
    }).clone()
}

thread_local! {
    static CSS_PROVIDER: std::cell::RefCell<Option<gtk4::CssProvider>> = std::cell::RefCell::new(None);
}

pub fn load_css_from_resource() -> Result<gtk4::CssProvider, Box<dyn Error>> {
    load_resources_once()
        .map_err(|e| format!("Failed to load/register embedded GResource: {}", e))?;

    let provider = CSS_PROVIDER.with(|cell| {
        let mut opt = cell.borrow_mut();
        if let Some(ref provider) = *opt {
            provider.clone()
        } else {
            let provider = gtk4::CssProvider::new();
            println!(
                "Attempting to load CSS provider from resource: {}",
                CSS_RESOURCE_PATH
            );

            if gio::resources_lookup_data(CSS_RESOURCE_PATH, gio::ResourceLookupFlags::NONE).is_err() {
                eprintln!(
                    "WARNING: CSS resource '{}' still not found before loading into provider.",
                    CSS_RESOURCE_PATH
                );
            }

            provider.load_from_resource(CSS_RESOURCE_PATH);
            println!("CSS Provider created and potentially loaded from resource.");
            *opt = Some(provider.clone());
            provider
        }
    });
    Ok(provider)
}

pub fn apply_css() {
    match load_css_from_resource() {
        Ok(provider) => {
            if let Some(display) = gtk4::gdk::Display::default() {
                println!("Applying CSS provider to default display with USER priority.");
                gtk4::style_context_add_provider_for_display(
                    &display,
                    &provider,
                    gtk4::STYLE_PROVIDER_PRIORITY_USER,
                );
                println!("CSS applied successfully.");
            } else {
                eprintln!("Error: Could not get default display for applying CSS");
            }
        }
        Err(e) => {
            eprintln!("######################################################");
            eprintln!("# Critical Error: Failed to load/apply CSS:          #");
            eprintln!("# {}", e);
            eprintln!("######################################################");
        }
    }
}
