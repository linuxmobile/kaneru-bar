use anyhow::{Context, Result};
use std::sync::Once;

include!(concat!(env!("OUT_DIR"), "/compiled_resources.rs"));

const CSS_RESOURCE_PATH: &str = "/com/github/linuxmobile/kaneru/style.css";

static RESOURCES_LOADED: Once = Once::new();

fn load_resources() -> Result<()> {
    let mut registration_result = Ok(());

    RESOURCES_LOADED.call_once(|| {
        println!("Loading GResource data from embedded bytes...");
        let resource_data = glib::Bytes::from_static(RESOURCE_BYTES);
        match gio::Resource::from_data(&resource_data) {
            Ok(res) => {
                gio::resources_register(&res);
                println!("GResource registered successfully from embedded bytes.");
                if gio::resources_lookup_data(CSS_RESOURCE_PATH, gio::ResourceLookupFlags::NONE).is_err() {
                     eprintln!(
                        "CRITICAL ERROR: CSS resource NOT found at path '{}' after registering embedded resource.",
                        CSS_RESOURCE_PATH
                    );
                    registration_result = Err(anyhow::anyhow!(
                        "CSS resource '{}' not found after registration", CSS_RESOURCE_PATH
                    ));
                } else {
                    println!("SUCCESS: CSS resource found at path: {}", CSS_RESOURCE_PATH);
                }
            }
            Err(e) => {
                eprintln!("CRITICAL: Failed to load GResource bundle from embedded data: {}", e);
                registration_result = Err(anyhow::anyhow!("Failed to load GResource from data").context(e));
            }
        }
    });

    registration_result
}

pub fn load_css_from_resource() -> Result<gtk4::CssProvider> {
    load_resources().context("Failed to load/register embedded GResource")?;

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
            eprintln!("# {:#?}", e);
            eprintln!("######################################################");
        }
    }
}
