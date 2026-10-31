//! ArtApp: a interface (egui) sobre o `artapp-core`. O mesmo código roda no desktop
//! (`cargo run -p artapp`) e no Android (biblioteca nativa carregada pela NativeActivity, ver
//! `android/`).

mod app;
mod brush_editor;
mod canvas;
mod color_panel;
mod dialogs;
mod panels;
pub mod platform;
mod projects;
mod quick_menu;
mod theme;
mod transform_tool;
mod updater;

pub use app::ArtApp;

/// Opções de janela comuns às duas plataformas.
fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("ArtApp")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    }
}

/// Ponto de entrada do desktop.
#[cfg(not(target_os = "android"))]
pub fn run_desktop() -> eframe::Result {
    env_logger::init();
    eframe::run_native(
        "ArtApp",
        native_options(),
        Box::new(|cc| {
            let mut app = ArtApp::new(cc);
            // `artapp desenho.artapp` (ou um .psd) já abre o arquivo.
            if let Some(path) = std::env::args_os().nth(1) {
                app.open_path(path.into());
            }
            Ok(Box::new(app))
        }),
    )
}

/// Ponto de entrada do Android: a NativeActivity carrega `libartapp.so` e chama esta função.
#[cfg(target_os = "android")]
#[allow(unsafe_code)] // `no_mangle` é o único jeito de a NativeActivity achar a função.
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("ArtApp"),
    );
    platform::init_android(&app);
    let mut options = native_options();
    options.android_app = Some(app);
    if let Err(e) = eframe::run_native(
        "ArtApp",
        options,
        Box::new(|cc| Ok(Box::new(ArtApp::new(cc)))),
    ) {
        log::error!("ArtApp terminou com erro: {e}");
    }
}
