//! "Mais pincéis": abre a página de pincéis compatíveis do site no navegador e, enquanto ela está
//! aberta, olha a pasta Downloads. Cada `.sut`, `.abr`, `.zip` ou `.artbrushes` que aparecer
//! depois de abrir a página (e parar de crescer, ou seja, terminou de baixar) é instalado em
//! Meus pincéis sozinho, sem a pessoa precisar procurar o arquivo.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;

use crate::app::ArtApp;

/// Página de pincéis do site.
pub const PAGE: &str = "https://rafanunesran.github.io/ArtApp/pinceis/";

/// Depois disso sem abrir a página de novo, para de olhar a pasta.
const WATCH_FOR: Duration = Duration::from_secs(45 * 60);
/// Intervalo entre olhadas na pasta.
const EVERY: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub struct Watch {
    /// Quando a página foi aberta (arquivos mais velhos que isso não são instalados).
    since: Option<(SystemTime, Instant)>,
    last_scan: Option<Instant>,
    /// Arquivo novo → tamanho na olhada anterior (instala quando repete: terminou de baixar).
    pending: HashMap<PathBuf, u64>,
    /// Já instalados (ou que falharam) nesta sessão.
    done: Vec<PathBuf>,
}

fn is_brush_file(p: &std::path::Path) -> bool {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    artapp_core::brushes::IMPORT_EXTS
        .iter()
        .any(|e| name.ends_with(&format!(".{e}")))
}

impl ArtApp {
    /// Abre a página de pincéis e passa a instalar o que for baixado.
    pub fn open_brush_store(&mut self) {
        if !crate::platform::can_read_shared_files() {
            // Sem acesso à pasta Downloads não dá para instalar sozinho: pede antes.
            let _ = crate::platform::request_file_access();
        }
        match crate::platform::open_url(PAGE) {
            Ok(()) => {
                self.brush_watch.since = Some((SystemTime::now(), Instant::now()));
                self.brush_watch.pending.clear();
                self.status = "Baixe um pincel na página e volte: o ArtApp instala sozinho.".into();
            }
            Err(e) => self.status = format!("Não foi possível abrir o navegador: {e}"),
        }
    }

    /// Chamado a cada quadro: instala os pincéis que terminaram de baixar.
    pub fn brush_store_tick(&mut self, ctx: &egui::Context) {
        let Some((since, started)) = self.brush_watch.since else {
            return;
        };
        if started.elapsed() > WATCH_FOR {
            self.brush_watch.since = None;
            return;
        }
        // Continua acordando de vez em quando, mesmo sem toque na tela.
        ctx.request_repaint_after(EVERY);
        if self
            .brush_watch
            .last_scan
            .is_some_and(|t| t.elapsed() < EVERY)
        {
            return;
        }
        self.brush_watch.last_scan = Some(Instant::now());
        let Some(dir) = crate::platform::downloads_dir() else {
            return;
        };
        let Ok(rd) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut ready = Vec::new();
        let mut seen = HashMap::new();
        for e in rd.flatten() {
            let p = e.path();
            if !is_brush_file(&p) || self.brush_watch.done.contains(&p) {
                continue;
            }
            let Ok(meta) = e.metadata() else { continue };
            // Alguns navegadores guardam a data do servidor: vale a mais nova das duas.
            let t = [meta.modified().ok(), meta.created().ok()]
                .into_iter()
                .flatten()
                .max();
            if !meta.is_file() || t.is_none_or(|t| t < since) {
                continue;
            }
            let size = meta.len();
            if size > 0 && self.brush_watch.pending.get(&p) == Some(&size) {
                ready.push(p);
            } else {
                seen.insert(p, size);
            }
        }
        self.brush_watch.pending = seen;
        for p in ready {
            self.brush_watch.done.push(p.clone());
            self.import_brushes(&p);
        }
    }
}
