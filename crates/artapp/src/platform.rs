//! O que muda entre Android e desktop: onde salvar arquivos e o tamanho dos controles.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Pastas candidatas para salvar, em ordem de preferência.
static SAVE_DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
/// Pasta privada do app (pincéis do usuário, ajustes).
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
/// Pastas candidatas para os projetos (.artapp), em ordem de preferência.
static PROJECT_DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
/// A raiz do armazenamento compartilhado no Android.
#[cfg(target_os = "android")]
const SHARED: &str = "/storage/emulated/0";

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
    // Projetos em Documentos/ArtApp (o app pode criar arquivos ali sem permissão e eles ficam
    // visíveis no gerenciador de arquivos e sobrevivem a desinstalar). Se não der, na pasta do app.
    let mut projects = vec![PathBuf::from(SHARED).join("Documents").join("ArtApp")];
    projects.extend(app.external_data_path().map(|d| d.join("projetos")));
    projects.extend(app.internal_data_path().map(|d| d.join("projetos")));
    let _ = PROJECT_DIRS.set(projects);
    android_files::init(app);
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

/// Onde os projetos ficam: a primeira pasta candidata que dá para criar.
pub fn projects_dir() -> PathBuf {
    let dirs = PROJECT_DIRS.get().cloned().unwrap_or_else(|| {
        let mut d = Vec::new();
        if let Some(home) = home() {
            d.push(home.join("Documents").join("ArtApp"));
        }
        d.push(data_dir().join("projetos"));
        d
    });
    for d in &dirs {
        if std::fs::create_dir_all(d).is_ok() {
            return d.clone();
        }
    }
    dirs.into_iter().next().unwrap_or_default()
}

/// Atalhos do navegador de arquivos (nome, pasta).
pub fn browse_roots() -> Vec<(&'static str, PathBuf)> {
    let mut roots = vec![("Projetos", projects_dir())];
    #[cfg(target_os = "android")]
    let base = Some(PathBuf::from(SHARED));
    #[cfg(not(target_os = "android"))]
    let base = home();
    if let Some(b) = base {
        let download = if cfg!(target_os = "android") {
            "Download"
        } else {
            "Downloads"
        };
        roots.push(("Downloads", b.join(download)));
        roots.push(("Documentos", b.join("Documents")));
        roots.push(("Imagens", b.join("Pictures")));
        roots.push((
            if cfg!(target_os = "android") {
                "Armazenamento"
            } else {
                "Pasta pessoal"
            },
            b,
        ));
    }
    roots
}

/// O app pode ler arquivos de outros apps (ex.: um PSD baixado)? No desktop, sempre.
pub fn can_read_shared_files() -> bool {
    #[cfg(target_os = "android")]
    {
        android_files::has_all_files_access()
    }
    #[cfg(not(target_os = "android"))]
    {
        true
    }
}

/// Abre o pedido de acesso aos arquivos do aparelho (Android). Devolve o erro, se houver.
pub fn request_file_access() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        android_files::request_all_files_access()
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(())
    }
}

/// Grava `bytes` em `path` sem deixar um arquivo pela metade se faltar espaço ou o app fechar:
/// escreve ao lado e troca no fim.
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let res = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{}: {e}", path.display()));
    }
    Ok(())
}

/// Chamadas ao Java (JNI) para a permissão de acesso a todos os arquivos.
#[cfg(target_os = "android")]
mod android_files {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use jni::objects::{JObject, JValue};
    use jni::{jni_sig, jni_str};

    /// A Activity (jobject global que a android-activity mantém viva enquanto o app roda).
    static ACTIVITY: AtomicUsize = AtomicUsize::new(0);

    pub fn init(app: &winit::platform::android::activity::AndroidApp) {
        ACTIVITY.store(app.activity_as_ptr() as usize, Ordering::Relaxed);
    }

    /// Roda `f` com o JNI e a Activity.
    #[allow(unsafe_code)]
    fn with_activity<T>(
        f: impl FnOnce(&mut jni::Env, &JObject) -> jni::errors::Result<T>,
    ) -> Result<T, String> {
        let ctx = ndk_context::android_context();
        let activity = ACTIVITY.load(Ordering::Relaxed);
        if ctx.vm().is_null() || activity == 0 {
            return Err("JNI indisponível".into());
        }
        // SAFETY: o ponteiro do JavaVM vem da android-activity (via ndk-context) e vale enquanto
        // o processo existir.
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) };
        vm.attach_current_thread(|env| -> jni::errors::Result<T> {
            // SAFETY: a Activity é uma referência global válida enquanto o app roda; o JObject
            // não a apaga ao sair de escopo.
            let act = unsafe { JObject::from_raw(env, activity as jni::sys::jobject) };
            f(env, &act)
        })
        .map_err(|e| e.to_string())
    }

    fn sdk_int(env: &mut jni::Env) -> jni::errors::Result<i32> {
        env.get_static_field(
            jni_str!("android/os/Build$VERSION"),
            jni_str!("SDK_INT"),
            jni_sig!(jint),
        )?
        .i()
    }

    pub fn has_all_files_access() -> bool {
        with_activity(|env, act| {
            if sdk_int(env)? >= 30 {
                env.call_static_method(
                    jni_str!("android/os/Environment"),
                    jni_str!("isExternalStorageManager"),
                    jni_sig!(() -> jboolean),
                    &[],
                )?
                .z()
            } else {
                let perm = env.new_string("android.permission.READ_EXTERNAL_STORAGE")?;
                let r = env
                    .call_method(
                        act,
                        jni_str!("checkSelfPermission"),
                        jni_sig!((perm: java.lang.String) -> jint),
                        &[JValue::Object(&perm)],
                    )?
                    .i()?;
                Ok(r == 0)
            }
        })
        .unwrap_or(false)
    }

    pub fn request_all_files_access() -> Result<(), String> {
        with_activity(|env, act| {
            if sdk_int(env)? >= 30 {
                // Configurações › Acesso a todos os arquivos, já na página do ArtApp.
                let pkg = env
                    .call_method(
                        act,
                        jni_str!("getPackageName"),
                        jni_sig!(() -> java.lang.String),
                        &[],
                    )?
                    .l()?;
                let pkg = jni::objects::JString::cast_local(env, pkg)?;
                let pkg = pkg.try_to_string(env)?;
                let uri_text = env.new_string(format!("package:{pkg}"))?;
                let uri = env
                    .call_static_method(
                        jni_str!("android/net/Uri"),
                        jni_str!("parse"),
                        jni_sig!((s: java.lang.String) -> android.net.Uri),
                        &[JValue::Object(&uri_text)],
                    )?
                    .l()?;
                let action =
                    env.new_string("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION")?;
                let intent = env.new_object(
                    jni_str!("android/content/Intent"),
                    jni_sig!((action: java.lang.String, uri: android.net.Uri) -> void),
                    &[JValue::Object(&action), JValue::Object(&uri)],
                )?;
                env.call_method(
                    act,
                    jni_str!("startActivity"),
                    jni_sig!((intent: android.content.Intent) -> void),
                    &[JValue::Object(&intent)],
                )?;
            } else {
                let perm = env.new_string("android.permission.READ_EXTERNAL_STORAGE")?;
                let arr = env.new_object_array(1, jni_str!("java/lang/String"), &perm)?;
                env.call_method(
                    act,
                    jni_str!("requestPermissions"),
                    jni_sig!((perms: java.lang.String[], code: jint) -> void),
                    &[JValue::Object(&arr), JValue::Int(1)],
                )?;
            }
            Ok(())
        })
    }
}

/// O que se sabe da caneta que fez um toque.
#[derive(Debug, Clone, Copy, Default)]
pub struct PenInfo {
    /// O toque veio de uma caneta (não de um dedo).
    pub stylus: bool,
    /// O sistema marcou o toque como palma da mão.
    pub palm: bool,
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
            stylus: p.stylus,
            palm: p.palm,
            eraser: p.eraser,
            button: p.button,
        };
    }
    let _ = id;
    PenInfo::default()
}
