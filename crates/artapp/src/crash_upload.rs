//! Envio dos relatórios de erro ao site do ArtApp, para serem corrigidos.
//!
//! Só envia se a pessoa deixar (pergunta uma vez, na janela do relatório; fica no prefs.json em
//! `crash_upload`). Vai só a parte técnica: versão, sistema, a mensagem do erro, o lugar no código
//! e a pilha de chamadas. Não vai o desenho, nomes de arquivo nem caminhos de pastas (são trocados
//! por `<caminho>`), nem nada que identifique a pessoa ou o aparelho.
//!
//! Como o site (GitHub Pages) é só leitura, o relatório passa por um canal público gratuito do
//! ntfy.sh, que guarda as mensagens por 12 horas. Uma tarefa do GitHub Actions
//! (`.github/workflows/crash-reports.yml`) busca de hora em hora, agrupa por tipo de erro, grava em
//! `relatorios.json` no ramo `relatorios-de-erros` e abre/atualiza uma issue por tipo de erro. A
//! página https://rafanunesran.github.io/ArtApp/erros/ mostra a lista. Ver `docs/relatorios-de-erros.md`.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use crate::app::ArtApp;

/// O canal onde os relatórios são postados (tem que ser o mesmo do workflow).
pub const ENDPOINT: &str = "https://ntfy.sh/artapp-relatorios-de-erros-7k2q9m";
/// Primeira linha de todo relatório enviado (o workflow ignora o que não começar assim).
pub const HEADER: &str = "ARTAPP-CRASH 1";
/// O ntfy.sh transforma mensagens maiores que 4 KB em anexo: fica abaixo disso.
const MAX_BYTES: usize = 3800;
/// Quantos relatórios enviar por vez (o resto vai na próxima).
const PER_RUN: usize = 10;
/// Intervalo entre tentativas (segundos), se a fila tiver algo.
const RETRY_EVERY: f64 = 300.0;
/// Relatórios mais velhos que isso não são mais enviados.
const MAX_AGE_SECS: u64 = 30 * 24 * 3600;

/// Estado do envio no app.
#[derive(Default)]
pub struct Upload {
    /// `None` = ainda não perguntou; `Some(true)` = enviar sempre.
    pub consent: Option<bool>,
    job: Option<Receiver<Result<usize, String>>>,
    tried_at: Option<f64>,
    /// Quantos relatórios já foram enviados nesta sessão.
    pub sent: usize,
    pub last_error: Option<String>,
}

impl Upload {
    pub fn load_json(&mut self, v: Option<&serde_json::Value>) {
        self.consent = v.and_then(serde_json::Value::as_bool);
    }

    pub fn to_json(&self) -> serde_json::Value {
        self.consent.map_or(serde_json::Value::Null, Into::into)
    }

    pub fn sending(&self) -> bool {
        self.job.is_some()
    }
}

impl ArtApp {
    /// A cada quadro: envia a fila em segundo plano (se a pessoa deixou).
    pub fn crash_upload_tick(&mut self, ctx: &eframe::egui::Context) {
        if let Some(rx) = self.crash_upload.job.take() {
            match rx.try_recv() {
                Err(TryRecvError::Empty) => self.crash_upload.job = Some(rx),
                Err(TryRecvError::Disconnected) => {
                    self.crash_upload.last_error = Some("erro interno".into());
                }
                Ok(Ok(n)) => {
                    self.crash_upload.sent += n;
                    self.crash_upload.last_error = None;
                }
                Ok(Err(e)) => {
                    log::warn!("Relatório de erros não enviado: {e}");
                    self.crash_upload.last_error = Some(e);
                }
            }
            return;
        }
        if self.crash_upload.consent != Some(true) {
            return;
        }
        let now = ctx.input(|i| i.time);
        if self
            .crash_upload
            .tried_at
            .is_some_and(|t| now - t < RETRY_EVERY)
        {
            return;
        }
        self.crash_upload.tried_at = Some(now);
        let files = crate::crash::queued();
        if files.is_empty() {
            return;
        }
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(send_all(files));
        });
        self.crash_upload.job = Some(rx);
    }

    /// A pessoa respondeu se quer enviar os relatórios.
    pub fn set_crash_upload(&mut self, on: bool) {
        self.crash_upload.consent = Some(on);
        // Tenta já, sem esperar o intervalo.
        self.crash_upload.tried_at = None;
        if !on {
            for p in crate::crash::queued() {
                let _ = std::fs::remove_file(p);
            }
        }
        self.save_prefs();
    }
}

/// Envia os relatórios da fila, do mais velho ao mais novo. Para no primeiro que falhar (sem
/// internet, por exemplo) e tenta de novo depois.
fn send_all(files: Vec<PathBuf>) -> Result<usize, String> {
    let now = unix_now();
    let mut sent = 0;
    for path in files {
        if sent >= PER_RUN {
            break;
        }
        let t = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.split('-').next())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(now);
        let Ok(raw) = std::fs::read_to_string(&path) else {
            let _ = std::fs::remove_file(&path);
            continue;
        };
        if now.saturating_sub(t) > MAX_AGE_SECS || raw.trim().is_empty() {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        let payload = payload(&raw, t);
        post(&payload)?;
        let _ = std::fs::remove_file(&path);
        sent += 1;
    }
    Ok(sent)
}

fn post(body: &str) -> Result<(), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .build()
        .into();
    agent
        .post(ENDPOINT)
        .header("Title", "Relatório de erro do ArtApp")
        .header("Tags", "artapp")
        .content_type("text/plain; charset=utf-8")
        .send(body)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Monta o texto enviado a partir do relatório gravado (ver `crash`): cabeçalho com campos fixos
/// (o workflow agrupa por `tipo` + `onde` + `erro`), depois o resto, sem caminhos pessoais.
pub fn payload(raw: &str, t: u64) -> String {
    let raw = scrub(raw);
    let field = |prefix: &str| {
        raw.lines()
            .find_map(|l| l.strip_prefix(prefix))
            .map(|s| s.trim().to_string())
    };
    let sudden = raw.starts_with("O app fechou de repente");
    let build = raw
        .lines()
        .find_map(|l| {
            let i = l.find("build ")?;
            l[i + 6..].split([' ', '·', ',']).next().map(str::to_string)
        })
        .unwrap_or_else(|| "?".into());
    let (erro, onde) = if sudden {
        (sudden_cause(&raw), String::new())
    } else {
        (
            field("Erro: ").unwrap_or_else(|| "(sem mensagem)".into()),
            field("Onde: ").unwrap_or_default(),
        )
    };
    let mut out = String::new();
    let _ = writeln!(out, "{HEADER}");
    let _ = writeln!(out, "build: {build}");
    let _ = writeln!(
        out,
        "sistema: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let _ = writeln!(out, "tipo: {}", if sudden { "fechou" } else { "panico" });
    let _ = writeln!(out, "hora: {t}");
    let _ = writeln!(out, "erro: {}", one_line(&erro, 300));
    let _ = writeln!(out, "onde: {}", one_line(&onde, 200));
    let _ = writeln!(out, "---");
    for line in body_lines(&raw) {
        if out.len() + line.len() + 1 > MAX_BYTES {
            out.push_str("…\n");
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// O que a saída de erro diz sobre um fechamento de repente (a linha mais útil para agrupar).
fn sudden_cause(raw: &str) -> String {
    let Some(i) = raw.find("Saída de erro:") else {
        return "fechou sem mensagem de erro".into();
    };
    let err = &raw[i..];
    const HINTS: [&str; 6] = [
        "memory allocation",
        "out of memory",
        "abort",
        "SIGSEGV",
        "signal",
        "error",
    ];
    for hint in HINTS {
        if let Some(l) = err
            .lines()
            .rev()
            .find(|l| l.to_ascii_lowercase().contains(&hint.to_ascii_lowercase()))
        {
            return l.trim().to_string();
        }
    }
    err.lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .map_or_else(String::new, |l| l.trim().to_string())
}

/// As linhas do relatório que valem a pena, sem a parte da pilha que é do próprio pânico.
fn body_lines(raw: &str) -> impl Iterator<Item = &str> {
    const NOISE: [&str; 6] = [
        "std::backtrace",
        "std::panicking",
        "core::panicking",
        "rust_begin_unwind",
        "artapp::crash::install",
        "__rust_",
    ];
    raw.lines().filter(|l| {
        let l = l.trim();
        !l.is_empty() && !NOISE.iter().any(|n| l.contains(n))
    })
}

fn one_line(s: &str, max: usize) -> String {
    let s = s.replace(['\n', '\r'], " ");
    if s.len() <= max {
        return s;
    }
    let mut i = max;
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    format!("{}…", &s[..i])
}

/// Troca caminhos de pastas e endereços de arquivo por `<caminho>` (podem ter o nome do desenho ou
/// da pessoa). Caminhos do código-fonte do app (de quem compilou) ficam.
pub fn scrub(text: &str) -> String {
    const STARTS: [&str; 10] = [
        "/storage/",
        "/sdcard/",
        "/data/",
        "/mnt/",
        "/home/",
        "/Users/",
        "/tmp/",
        "content://",
        "file://",
        ":\\",
    ];
    const KEEP: [&str; 4] = ["/home/runner/", "/Users/runner/", "/.cargo/", "/rustc/"];
    const ENDS: [char; 9] = ['"', '\'', '\n', ')', ']', '`', '»', '>', '\r'];
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let hit = STARTS
            .iter()
            .filter_map(|s| rest.find(s).map(|i| (i, *s)))
            .min_by_key(|&(i, _)| i);
        let Some((mut i, start)) = hit else {
            out.push_str(rest);
            break;
        };
        // "C:\..." começa na letra do disco.
        if start == ":\\" {
            if i == 0 || !rest.as_bytes()[i - 1].is_ascii_alphabetic() {
                out.push_str(&rest[..i + 2]);
                rest = &rest[i + 2..];
                continue;
            }
            i -= 1;
        }
        let end = rest[i..].find(ENDS).map_or(rest.len(), |e| i + e);
        let path = &rest[i..end];
        out.push_str(&rest[..i]);
        if KEEP.iter().any(|k| path.contains(k)) {
            out.push_str(path);
        } else {
            out.push_str("<caminho>");
        }
        rest = &rest[end..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_hides_personal_paths() {
        let s = scrub(
            "abrir \"/storage/emulated/0/Documents/ArtApp/Meu Desenho.artapp\": não achou\n\
             em /home/runner/work/ArtApp/src/app.rs:10\n\
             C:\\Users\\Rafa\\Desenho.artapp)",
        );
        assert!(!s.contains("Meu Desenho"), "{s}");
        assert!(!s.contains("Rafa"), "{s}");
        assert!(s.contains("/home/runner/work/ArtApp/src/app.rs:10"), "{s}");
        assert!(s.contains("abrir \"<caminho>\": não achou"), "{s}");
    }

    #[test]
    fn payload_of_panic() {
        let raw = "ArtApp build 140 · Android\nHora (unix): 5\nThread: main\nErro: index out of \
                   bounds: the len is 3 but the index is 7\nOnde: crates/artapp-core/src/fill.rs:88\n\
                   Estado: tela 100×100\n\nPilha:\n   0: std::backtrace::Backtrace::force_capture\n   \
                   1: artapp_core::fill::flood\n";
        let p = payload(raw, 123);
        assert!(p.starts_with(HEADER));
        assert!(p.contains("build: 140\n"), "{p}");
        assert!(p.contains("tipo: panico\n"));
        assert!(p.contains("onde: crates/artapp-core/src/fill.rs:88\n"));
        assert!(p.contains("artapp_core::fill::flood"));
        assert!(!p.contains("force_capture"));
    }

    #[test]
    fn payload_of_sudden_close() {
        let raw = "O app fechou de repente, sem erro do Rust (…).\nÚltimo estado: build 141 · tela \
                   10×10\n\nSaída de erro:\nalgo\nmemory allocation of 4096 bytes failed\n";
        let p = payload(raw, 1);
        assert!(p.contains("tipo: fechou\n"), "{p}");
        assert!(p.contains("build: 141\n"), "{p}");
        assert!(
            p.contains("erro: memory allocation of 4096 bytes failed\n"),
            "{p}"
        );
    }

    #[test]
    fn payload_fits_ntfy() {
        let raw = format!(
            "Erro: x\nOnde: a.rs:1\n{}",
            "linha comprida da pilha\n".repeat(500)
        );
        assert!(payload(&raw, 1).len() < 4096);
    }
}
