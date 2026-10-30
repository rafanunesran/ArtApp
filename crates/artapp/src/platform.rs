//! O que muda entre Android e desktop: onde salvar arquivos e o tamanho dos controles.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Pastas candidatas para salvar, em ordem de preferência.
static SAVE_DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
/// Pasta privada do app (pincéis do usuário, ajustes).
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// `true` no Android: a interface usa controles maiores para o dedo.
pub const TOUCH_FIRST: bool = cfg!(target_os = "android");

#[cfg(target_os = "android")]
pub fn init_android(app: &winit::platform::android::activity::AndroidApp) {
    // A partir do Android 11 o app pode criar imagens em Pictures/ sem pedir permissão, e a galeria
    // as encontra. Se não der, fica na pasta do app (Android/data/<pacote>/files).
    let mut dirs = vec![PathBuf::from("/storage/emulated/0/Pictures/ArtApp")];
    dirs.extend(app.external_data_path());
    dirs.extend(app.internal_data_path());
    if let Some(d) = app.internal_data_path() {
        let _ = DATA_DIR.set(d);
    }
    let _ = SAVE_DIRS.set(dirs);
}

/// A pasta do usuário (`HOME`; no Windows, `USERPROFILE`).
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Onde ficam os dados do app: no Windows `%APPDATA%\ArtApp`, nos outros desktops
/// `~/.config/artapp`.
pub fn data_dir() -> PathBuf {
    if let Some(d) = DATA_DIR.get() {
        return d.clone();
    }
    if let Some(appdata) = std::env::var_os("APPDATA").filter(|_| cfg!(windows)) {
        return PathBuf::from(appdata).join("ArtApp");
    }
    home().unwrap_or_default().join(".config").join("artapp")
}

fn save_dirs() -> Vec<PathBuf> {
    if let Some(d) = SAVE_DIRS.get() {
        return d.clone();
    }
    let mut dirs = Vec::new();
    if let Some(home) = home() {
        dirs.push(home.join("Pictures").join("ArtApp"));
        dirs.push(home);
    }
    dirs.extend(std::env::current_dir().ok());
    dirs
}

/// Salva `bytes` como `artapp-<hora>.<ext>` na primeira pasta que aceitar. Devolve o caminho.
pub fn save_file(ext: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let mut last_err = String::from("nenhuma pasta disponível");
    for dir in save_dirs() {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            last_err = format!("{}: {e}", dir.display());
            continue;
        }
        let path = dir.join(format!("artapp-{stamp}.{ext}"));
        match std::fs::write(&path, bytes) {
            Ok(()) => return Ok(path),
            Err(e) => last_err = format!("{}: {e}", path.display()),
        }
    }
    Err(last_err)
}

/// O que se sabe da caneta que fez um toque.
#[derive(Debug, Clone, Copy, Default)]
pub struct PenInfo {
    /// A ponta de borracha (ou a caneta virada).
    pub eraser: bool,
    /// O botão lateral apertado.
    pub button: bool,
}

/// A caneta do toque `id`. No Android vem da cópia corrigida do winit (`third_party/winit`); no
/// desktop a caneta chega como mouse e isto fica vazio.
pub fn pen_info(id: u64) -> PenInfo {
    #[cfg(target_os = "android")]
    if let Some(p) = winit::platform::android::stylus::pointer_info(id) {
        return PenInfo {
            eraser: p.eraser,
            button: p.button,
        };
    }
    let _ = id;
    PenInfo::default()
}
