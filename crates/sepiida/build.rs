use std::{env, path::Path};

fn main() {
    // The settings window and the resources only exist on Windows;
    // skipping them elsewhere keeps Linux builds free of windowing-system
    // dependencies.
    if env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }
    slint_build::compile("ui/app.slint").expect("failed to compile ui/app.slint");
    embed_resources();
}

/// Compiles `assets/sepiida.rc` (icon and version information) into both
/// programs. A Windows build without them fails rather than ship an exe
/// without its icon.
fn embed_resources() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let assets = Path::new(&manifest_dir).join("assets");
    // The resource compiler may run on a preprocessed copy elsewhere, so
    // the icon's path is absolute; forward slashes need no escaping.
    let icon = assets
        .join("sepiida.ico")
        .display()
        .to_string()
        .replace('\\', "/");
    let version = env::var("CARGO_PKG_VERSION").expect("cargo sets CARGO_PKG_VERSION");
    let numbers = version_numbers(&version);
    let prerelease = !env::var("CARGO_PKG_VERSION_PRE")
        .unwrap_or_default()
        .is_empty();
    let macros = [
        format!("SEPIIDA_ICON=\"{icon}\""),
        format!("SEPIIDA_VERSION=\"{version}\""),
        format!("SEPIIDA_VERSION_NUMBERS={numbers}"),
        format!(
            "SEPIIDA_FILE_FLAGS={}",
            if prerelease { "0x2L" } else { "0x0L" }
        ),
    ];
    println!("cargo:rerun-if-changed=assets/sepiida.rc");
    println!("cargo:rerun-if-changed=assets/sepiida.ico");
    embed_resource::compile(assets.join("sepiida.rc"), &macros)
        .manifest_required()
        .expect("failed to compile assets/sepiida.rc");
}

/// `major,minor,patch,0` for the version resource's numeric fields.
fn version_numbers(version: &str) -> String {
    let core = version.split(['-', '+']).next().unwrap_or("0.0.0");
    let mut parts: Vec<u16> = core.split('.').map(|p| p.parse().unwrap_or(0)).collect();
    parts.resize(4, 0);
    parts
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(",")
}
