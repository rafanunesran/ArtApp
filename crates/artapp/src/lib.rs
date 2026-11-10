//! ArtApp: a interface (egui) sobre o `artapp-core`. O mesmo código roda no desktop
//! (`cargo run -p artapp`) e no Android (biblioteca nativa carregada pela NativeActivity, ver
//! `android/`).

mod anim_bar;
mod app;
mod backup;
mod brush_editor;
mod brush_store;
mod canvas;
mod color_panel;
mod crash;
mod crash_view;
mod dialogs;
mod gallery;
mod history_panel;
mod hotswap;
mod materials_view;
mod panels;
pub mod platform;
mod pose_view;
mod projects;
mod quick_menu;
mod reference;
mod theme;
mod timelapse_view;
mod tour;
mod transform_tool;
mod units;
mod updater;
mod view3d;
mod workspace;

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
    crash::install();
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
    // O winit só cria um laço de eventos por processo. Se o Android criar a Activity de novo sem
    // matar o processo (a anterior foi destruída), não dá para rodar aqui: reabre o app num
    // processo novo em vez de ficar em tela preta até o Android fechá-lo.
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        log::warn!("Activity recriada no mesmo processo: reabrindo o app");
        crash::mark_stopped();
        if let Err(e) = platform::update::restart() {
            log::error!("não deu para reabrir: {e}");
        }
        std::process::exit(0);
    }
    crash::install();
    let mut options = native_options();
    options.android_app = Some(app);
    if let Err(e) = eframe::run_native(
        "ArtApp",
        options,
        Box::new(|cc| Ok(Box::new(ArtApp::new(cc)))),
    ) {
        log::error!("ArtApp terminou com erro: {e}");
    }
    // A Activity foi destruída (o app já gravou o que faltava ao sair do laço). Encerra o processo
    // para a próxima abertura começar do zero: o laço de eventos não pode ser criado de novo.
    crash::mark_stopped();
    std::process::exit(0);
}
