// Sem janela de console atrás do app no Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result {
    artapp::run_desktop()
}

#[cfg(target_os = "android")]
fn main() {}
