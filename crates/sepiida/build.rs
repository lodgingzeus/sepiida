fn main() {
    // The settings window only exists on Windows; skipping it elsewhere keeps
    // Linux builds free of windowing-system dependencies.
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        slint_build::compile("ui/app.slint").expect("failed to compile ui/app.slint");
    }
}
