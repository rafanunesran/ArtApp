#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result {
    artapp::run_desktop()
}

#[cfg(target_os = "android")]
fn main() {}
