use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=src/resources/");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env::var("OUT_DIR").context("OUT_DIR not set")?);
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR not set")?);
    let resource_dir = manifest_dir.join("src").join("resources");

    let scss_input = resource_dir.join("style.scss");
    let css_output = out_dir.join("style.css");
    println!("cargo:rerun-if-changed={}", scss_input.display());
    println!(
        "Compiling SCSS: Input: {}, Output: {}",
        scss_input.display(),
        css_output.display()
    );
    let sass_output = Command::new("sass")
        .arg(&scss_input)
        .arg(&css_output)
        .output()
        .context("Failed to execute sass command. Is it installed and in PATH?")?;
    if !sass_output.status.success() {
        let stderr = String::from_utf8_lossy(&sass_output.stderr);
        anyhow::bail!(
            "Sass compilation failed with status: {}\nStderr:\n{}",
            sass_output.status,
            stderr
        );
    }
    let stdout = String::from_utf8_lossy(&sass_output.stdout);
    if !stdout.is_empty() {
        println!("Sass stdout:\n{}", stdout);
    }
    println!("Compiled SCSS successfully to {}", css_output.display());

    let gresource_input_xml_path = resource_dir.join("kaneru.gresource.xml.in");
    println!(
        "cargo:rerun-if-changed={}",
        gresource_input_xml_path.display()
    );
    let gresource_output_bundle = out_dir.join("kaneru.gresource");

    println!(
        "Compiling GResource via command: Input XML: {}, Output Bundle: {}, Sourcedir: {}",
        gresource_input_xml_path.display(),
        gresource_output_bundle.display(),
        out_dir.display()
    );

    let glib_compile_status = Command::new("glib-compile-resources")
        .arg("--target")
        .arg(&gresource_output_bundle)
        .arg("--sourcedir")
        .arg(&out_dir)
        .arg(&gresource_input_xml_path)
        .status()
        .context("Failed to execute glib-compile-resources command")?;

    if !glib_compile_status.success() {
        anyhow::bail!(
            "glib-compile-resources command failed with status: {}. Check XML and source paths.",
            glib_compile_status
        );
    }

    if !gresource_output_bundle.exists() {
        anyhow::bail!(
            "Generated resource bundle {} does not exist after glib-compile-resources call!",
            gresource_output_bundle.display()
        );
    }
    println!(
        "Compiled GResource bundle successfully to {}",
        gresource_output_bundle.display()
    );

    let generated_rust_path = out_dir.join("compiled_resources.rs");

    let bytes = fs::read(&gresource_output_bundle).with_context(|| {
        format!(
            "Failed to read generated resource bundle: {}",
            gresource_output_bundle.display()
        )
    })?;

    let mut rust_code = String::from("pub const RESOURCE_BYTES: &[u8] = &[\n    ");
    for (i, byte) in bytes.iter().enumerate() {
        rust_code.push_str(&format!("0x{:02x},", byte));
        if (i + 1) % 12 == 0 {
            rust_code.push_str("\n    ");
        } else {
            rust_code.push(' ');
        }
    }
    rust_code.push_str("\n];\n");

    fs::write(&generated_rust_path, &rust_code).with_context(|| {
        format!(
            "Failed to write generated Rust code to {:?}",
            generated_rust_path
        )
    })?;

    if !generated_rust_path.exists() {
        anyhow::bail!(
            "Failed to confirm existence of {} after writing!",
            generated_rust_path.display()
        );
    }

    println!(
        "Generated Rust resource loader (byte array) at: {}",
        generated_rust_path.display()
    );

    Ok(())
}
