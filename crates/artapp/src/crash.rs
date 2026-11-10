//! Relatório de erros: quando o app fecha sozinho, o que aconteceu fica gravado em arquivo e é
//! mostrado na próxima abertura (com um botão para copiar e mandar para quem for corrigir).
//!
//! Três fontes, da mais à menos detalhada:
//! - pânico do Rust: o gancho de pânico grava `crash/panico-<hora>.txt` com a mensagem, o lugar
//!   no código e a pilha de chamadas;
//! - saída de erro (stderr): no Android ela vai para `crash/stderr.txt` (a da sessão anterior fica
//!   em `stderr-anterior.txt`). É onde aparece, por exemplo, "memory allocation of N bytes failed"
//!   quando falta memória, que não passa pelo gancho de pânico;
//! - marcador `crash/rodando`: existe enquanto o app está na frente da tela. Se na abertura ele
//!   ainda estiver lá, a sessão anterior terminou sem passar pelo segundo plano, ou seja, o
//!   processo morreu de repente (falta de memória ou falha nativa). Ele guarda o último estado
//!   conhecido (memória do desenho, ferramenta), o que ajuda a achar a causa.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Quantos relatórios de pânico guardar.
const KEEP: usize = 10;
/// O relatório da sessão anterior, lido uma vez ao abrir (antes de a sessão nova sobrescrever).
static PREVIOUS: Mutex<Option<String>> = Mutex::new(None);
/// O que o app estava fazendo (vai junto do relatório de pânico).
static STATE: Mutex<String> = Mutex::new(String::new());

pub fn dir() -> PathBuf {
    crate::platform::data_dir().join("crash")
}

fn marker() -> PathBuf {
    dir().join("rodando")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn build() -> String {
    crate::hotswap::compiled_build().map_or_else(|| "dev".into(), |b| b.to_string())
}

/// Liga o relatório de erros. Chamar logo no começo, depois de saber a pasta de dados.
pub fn install() {
    let _ = std::fs::create_dir_all(dir());
    if let Ok(mut p) = PREVIOUS.lock() {
        *p = previous_session();
    }
    #[cfg(target_os = "android")]
    redirect_stderr();
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = panic_report(info);
        log::error!("{report}");
        let path = dir().join(format!("panico-{}.txt", now()));
        let _ = std::fs::write(path, &report);
        prune();
        default(info);
    }));
}

fn panic_report(info: &std::panic::PanicHookInfo<'_>) -> String {
    let msg = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(sem mensagem)".into());
    let place = info
        .location()
        .map_or_else(String::new, |l| format!("{}:{}", l.file(), l.line()));
    let thread = std::thread::current();
    let mut r = String::new();
    let _ = writeln!(r, "ArtApp build {} · {}", build(), platform_name());
    let _ = writeln!(r, "Hora (unix): {}", now());
    let _ = writeln!(r, "Thread: {}", thread.name().unwrap_or("(sem nome)"));
    let _ = writeln!(r, "Erro: {msg}");
    let _ = writeln!(r, "Onde: {place}");
    if let Ok(s) = STATE.try_lock()
        && !s.is_empty()
    {
        let _ = writeln!(r, "Estado: {s}");
    }
    let _ = writeln!(
        r,
        "\nPilha:\n{}",
        std::backtrace::Backtrace::force_capture()
    );
    r
}

fn platform_name() -> &'static str {
    if cfg!(target_os = "android") {
        "Android"
    } else if cfg!(windows) {
        "Windows"
    } else {
        "desktop"
    }
}

/// Os relatórios de pânico, do mais novo ao mais antigo.
fn panic_files() -> Vec<(u64, PathBuf)> {
    let mut v: Vec<(u64, PathBuf)> = std::fs::read_dir(dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let t = name
                .strip_prefix("panico-")?
                .strip_suffix(".txt")?
                .parse()
                .ok()?;
            Some((t, e.path()))
        })
        .collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.0));
    v
}

fn prune() {
    for (_, p) in panic_files().into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(p);
    }
}

/// Até quando os relatórios já foram mostrados.
fn seen_path() -> PathBuf {
    dir().join("visto")
}

/// Monta o relatório da sessão anterior, se ela terminou com erro.
fn previous_session() -> Option<String> {
    let seen: u64 = std::fs::read_to_string(seen_path())
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let state = std::fs::read_to_string(marker()).ok();
    let _ = std::fs::remove_file(marker());
    let newest = panic_files().into_iter().next();
    let mut r = String::new();
    match newest {
        Some((t, path)) if t > seen => {
            let _ = std::fs::write(seen_path(), t.to_string());
            r.push_str(&std::fs::read_to_string(path).unwrap_or_default());
        }
        _ => {
            let state = state?;
            let _ = writeln!(
                r,
                "O app fechou de repente, sem erro do Rust (costuma ser falta de memória ou uma \
                 falha do driver gráfico).\nÚltimo estado: {}",
                state.trim()
            );
        }
    }
    let err = std::fs::read_to_string(dir().join("stderr.txt")).unwrap_or_default();
    let err = err.trim();
    if !err.is_empty() {
        let tail = &err[floor_char(err, err.len().saturating_sub(4000))..];
        let _ = writeln!(r, "\nSaída de erro:\n{tail}");
    }
    Some(r)
}

fn floor_char(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// No Android a saída de erro some; manda para um arquivo (a da sessão anterior já foi lida).
#[cfg(target_os = "android")]
#[allow(unsafe_code)]
fn redirect_stderr() {
    use std::os::fd::AsRawFd;
    let path = dir().join("stderr.txt");
    let _ = std::fs::rename(&path, dir().join("stderr-anterior.txt"));
    if let Ok(f) = std::fs::File::create(&path) {
        // SAFETY: troca o descritor 2 por um arquivo aberto; `f` pode fechar depois (o dup2
        // fica com a sua própria cópia).
        unsafe {
            libc::dup2(f.as_raw_fd(), 2);
        }
    }
}

/// O relatório da sessão anterior (só na primeira chamada).
pub fn take_previous() -> Option<String> {
    PREVIOUS.lock().ok()?.take()
}

/// Um pânico foi contornado e o app seguiu: não é para mostrar como "fechou" na próxima abertura
/// (continua no menu Relatório de erros).
pub fn mark_seen() {
    if let Some((t, _)) = panic_files().into_iter().next() {
        let _ = std::fs::write(seen_path(), t.to_string());
    }
}

/// O relatório mais recente gravado (para ver pelo menu).
pub fn latest() -> Option<String> {
    let (_, path) = panic_files().into_iter().next()?;
    std::fs::read_to_string(path).ok()
}

/// O app está na frente da tela: grava o marcador com o estado atual.
pub fn mark_running(state: &str) {
    if let Ok(mut s) = STATE.lock() {
        state.clone_into(&mut s);
    }
    let _ = std::fs::write(marker(), format!("build {} · {state}", build()));
}

/// O app foi para o segundo plano ou vai fechar de propósito: um fim sem o marcador não é
/// um fechamento inesperado.
pub fn mark_stopped() {
    let _ = std::fs::remove_file(marker());
}
