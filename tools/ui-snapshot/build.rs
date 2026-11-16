fn main() {
    slint_build::compile("../../crates/sepiida/ui/settings.slint")
        .expect("failed to compile settings.slint");
}
