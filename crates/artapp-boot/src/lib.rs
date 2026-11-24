//! Carregador do ArtApp no Android (`libartapp_boot.so`).
//!
//! A NativeActivity abre esta biblioteca (veja `android.app.lib_name` no AndroidManifest) e chama
//! `ANativeActivity_onCreate`. Ela escolhe qual `libartapp.so` usar, a que veio no APK ou uma
//! atualização rápida baixada para `files/hot/` (veja [`hot`]), abre com `dlopen` e repassa a
//! chamada. Se a baixada não abrir, volta para a do APK. Assim uma versão nova do Rust entra sem
//! reinstalar o APK; só mudanças na "casca" (Java, manifesto, este carregador) pedem APK novo.
//!
//! Para o app, deixa nas variáveis de ambiente:
//! - `ARTAPP_SHELL`: a identidade da casca (hash dos arquivos de `android/` e deste crate, que o
//!   CI calcula);
//! - `ARTAPP_BOOT`: `hot:<build>` ou `bundled`;
//! - `ARTAPP_BOOT_NOTE`: aviso quando uma atualização foi desfeita;
//! - `ARTAPP_HOT_DIR`: a pasta `hot/`.

pub mod hot;

/// Identidade da casca, do CI (`ARTAPP_SHELL_ID`). Fora do CI vale "dev".
pub const SHELL_ID: &str = match option_env!("ARTAPP_SHELL_ID") {
    Some(s) => s,
    None => "dev",
};

/// O build do APK (o `versionCode`, do CI).
pub fn bundled_build() -> u64 {
    option_env!("ARTAPP_VERSION_CODE")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

#[cfg(target_os = "android")]
#[allow(unsafe_code)] // dlopen/dlsym e o ponto de entrada da NativeActivity.
mod android {
    use std::ffi::{CStr, CString, c_char, c_int, c_void};
    use std::path::{Path, PathBuf};

    use super::hot::{self, Choice};

    /// O começo da `struct ANativeActivity` (android/native_activity.h); só lemos
    /// `internalDataPath`.
    #[repr(C)]
    struct ActivityHead {
        callbacks: *mut c_void,
        vm: *mut c_void,
        env: *mut c_void,
        clazz: *mut c_void,
        internal_data_path: *const c_char,
    }

    type OnCreate = unsafe extern "C" fn(*mut c_void, *mut c_void, usize);

    #[link(name = "log")]
    unsafe extern "C" {
        fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }

    fn log(msg: &str) {
        if let Ok(text) = CString::new(msg) {
            // SAFETY: duas strings C válidas; 4 = ANDROID_LOG_INFO.
            unsafe { __android_log_write(4, c"ArtAppBoot".as_ptr(), text.as_ptr()) };
        }
    }

    fn dl_error() -> String {
        // SAFETY: dlerror devolve null ou uma string C válida até a próxima chamada dl*.
        let e = unsafe { libc::dlerror() };
        if e.is_null() {
            "erro desconhecido".into()
        } else {
            // SAFETY: não é null (acima).
            unsafe { CStr::from_ptr(e) }.to_string_lossy().into_owned()
        }
    }

    /// `dlopen` + `dlsym("ANativeActivity_onCreate")`.
    fn open(path: &Path) -> Result<OnCreate, String> {
        let c = CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        // SAFETY: caminho C válido. RTLD_NOW: falta de símbolo aparece aqui, não no meio do uso.
        let handle = unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            return Err(dl_error());
        }
        // SAFETY: handle válido e nome C válido.
        let sym = unsafe { libc::dlsym(handle, c"ANativeActivity_onCreate".as_ptr()) };
        if sym.is_null() {
            return Err(dl_error());
        }
        // SAFETY: é a função de entrada da android-activity, com esta assinatura.
        Ok(unsafe { std::mem::transmute::<*mut c_void, OnCreate>(sym) })
    }

    /// A `libartapp.so` do APK: na mesma pasta que esta biblioteca.
    fn bundled_path() -> PathBuf {
        let mut info = libc::Dl_info {
            dli_fname: std::ptr::null(),
            dli_fbase: std::ptr::null_mut(),
            dli_sname: std::ptr::null(),
            dli_saddr: std::ptr::null_mut(),
        };
        let addr = bundled_path as *const c_void;
        // SAFETY: endereço de uma função desta biblioteca; `info` é gravável.
        let ok = unsafe { libc::dladdr(addr, &mut info) } != 0 && !info.dli_fname.is_null();
        if ok {
            // SAFETY: dli_fname não é null (acima).
            let me = unsafe { CStr::from_ptr(info.dli_fname) }.to_string_lossy();
            if let Some(dir) = Path::new(me.as_ref()).parent() {
                return dir.join("libartapp.so");
            }
        }
        // Sem o caminho, o dlopen procura pelo nome nas pastas do app.
        PathBuf::from("libartapp.so")
    }

    fn set_env(key: &str, value: &str) {
        // SAFETY: roda antes de a biblioteca do app existir; nada mais lê o ambiente agora.
        unsafe { std::env::set_var(key, value) };
    }

    /// Chamada pela NativeActivity ao criar a Activity.
    ///
    /// # Safety
    /// `activity` é o `ANativeActivity*` que a NativeActivity passa.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn ANativeActivity_onCreate(
        activity: *mut c_void,
        saved_state: *mut c_void,
        saved_state_size: usize,
    ) {
        // SAFETY: a NativeActivity passa um ANativeActivity válido.
        let data = unsafe {
            let head = activity.cast::<ActivityHead>();
            let p = (*head).internal_data_path;
            (!p.is_null()).then(|| PathBuf::from(CStr::from_ptr(p).to_string_lossy().as_ref()))
        };
        let shell = super::SHELL_ID;
        let bundled = super::bundled_build();
        set_env("ARTAPP_SHELL", shell);
        let mut entry = None;
        if let Some(dir) = data.map(|d| d.join(hot::DIR)) {
            set_env("ARTAPP_HOT_DIR", &dir.to_string_lossy());
            match hot::choose(&dir, shell, bundled) {
                Choice::Hot { path, build } => match open(&path) {
                    Ok(f) => {
                        log(&format!("abrindo a atualização rápida, build {build}"));
                        set_env("ARTAPP_BOOT", &format!("hot:{build}"));
                        entry = Some(f);
                    }
                    Err(e) => {
                        log(&format!("a atualização {build} não abriu: {e}"));
                        hot::mark_bad(&dir, build, true);
                        set_env(
                            "ARTAPP_BOOT_NOTE",
                            &format!(
                                "Este aparelho não abriu a atualização rápida (build {build}); \
                                 as próximas vêm pelo APK."
                            ),
                        );
                    }
                },
                Choice::Bundled { note } => {
                    if let Some(n) = note {
                        log(&n);
                        set_env("ARTAPP_BOOT_NOTE", &n);
                    }
                }
            }
        }
        let entry = match entry {
            Some(f) => f,
            None => {
                set_env("ARTAPP_BOOT", "bundled");
                let path = bundled_path();
                match open(&path).or_else(|_| open(Path::new("libartapp.so"))) {
                    Ok(f) => f,
                    Err(e) => {
                        log(&format!("não deu para abrir {}: {e}", path.display()));
                        std::process::abort();
                    }
                }
            }
        };
        // SAFETY: repassa os argumentos que recebemos para a entrada da biblioteca do app.
        unsafe { entry(activity, saved_state, saved_state_size) };
    }
}
