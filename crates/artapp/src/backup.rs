//! Backup dos projetos numa pasta à parte (do aparelho, cartão SD ou Google Drive).
//!
//! Para cada projeto ficam duas cópias além do arquivo original:
//! - `<nome> - backup automático.artapp`: o desenho como está, a cada intervalo (1 h por padrão),
//!   só quando houve mudança;
//! - `<nome> - salvo.artapp`: a última versão salva com "Salvar", conferida antes (se o arquivo
//!   salvo não abrir, o backup anterior fica).
//!
//! No Android a pasta pode vir do seletor do sistema (`platform::saf`), que também lista o Google
//! Drive; no computador é um caminho comum (a pasta do Google Drive para computador serve).
//! Gravar roda numa thread e sempre escreve ao lado antes de trocar o arquivo anterior.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use artapp_core::project;
use eframe::egui::{self, Align2, RichText, Vec2};

use crate::app::ArtApp;
use crate::platform::{self, saf};
use crate::projects::{clean_name, time_ago};
use crate::theme::{self, color, panel_frame};

pub const AUTO_SUFFIX: &str = " - backup automático";
pub const SAVED_SUFFIX: &str = " - salvo";
/// Intervalos oferecidos, em minutos.
pub const INTERVALS: [u32; 6] = [15, 30, 60, 120, 240, 480];

/// Onde os backups ficam.
#[derive(Clone, PartialEq, Eq)]
pub enum Dest {
    Folder(PathBuf),
    /// Pasta do seletor do Android (URI com permissão guardada) e o nome para mostrar.
    Saf {
        uri: String,
        label: String,
    },
}

impl Dest {
    pub fn default_folder() -> Self {
        Dest::Folder(platform::projects_dir().join("Backup"))
    }

    pub fn label(&self) -> String {
        match self {
            Dest::Folder(p) => p.display().to_string(),
            Dest::Saf { label, .. } => label.clone(),
        }
    }

    /// Grava `bytes` como `name` na pasta, sem perder o arquivo anterior se algo falhar.
    fn put(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        match self {
            Dest::Folder(dir) => platform::write_atomic(&dir.join(name), bytes),
            Dest::Saf { uri, .. } => {
                let tmp = platform::data_dir().join("backup-tmp").join(name);
                platform::write_atomic(&tmp, bytes)?;
                let r = saf::put(uri, &tmp, name);
                let _ = std::fs::remove_file(&tmp);
                r
            }
        }
    }

    /// Os backups da pasta, do mais novo para o mais velho.
    fn list(&self) -> Result<Vec<Entry>, String> {
        let mut out: Vec<Entry> = match self {
            Dest::Folder(dir) => std::fs::read_dir(dir)
                .map_err(|e| format!("{}: {e}", dir.display()))?
                .flatten()
                .filter_map(|e| {
                    let meta = e.metadata().ok()?;
                    meta.is_file().then(|| Entry {
                        name: e.file_name().to_string_lossy().into_owned(),
                        size: meta.len(),
                        modified: meta.modified().ok(),
                    })
                })
                .collect(),
            Dest::Saf { uri, .. } => saf::list(uri)?
                .into_iter()
                .map(|(name, size, ms)| Entry {
                    name,
                    size,
                    modified: (ms > 0).then(|| UNIX_EPOCH + Duration::from_millis(ms as u64)),
                })
                .collect(),
        };
        let ext = format!(".{}", project::EXT);
        out.retain(|e| e.name.to_ascii_lowercase().ends_with(&ext));
        out.sort_by_key(|e| std::cmp::Reverse(e.modified));
        Ok(out)
    }

    /// Um arquivo local com o conteúdo do backup `name` (copia do Drive, se preciso).
    fn fetch(&self, name: &str) -> Result<PathBuf, String> {
        match self {
            Dest::Folder(dir) => Ok(dir.join(name)),
            Dest::Saf { uri, .. } => {
                let dir = platform::data_dir().join("restaurado");
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let dest = dir.join(name);
                saf::get(uri, name, &dest)?;
                Ok(dest)
            }
        }
    }
}

#[derive(Clone)]
struct Entry {
    name: String,
    size: u64,
    modified: Option<SystemTime>,
}

enum Done {
    Auto { edits: u64, manual: bool },
    Saved,
    Listed(Vec<Entry>),
    Fetched(PathBuf),
}

enum Pending {
    /// Copiar o arquivo recém-salvo.
    Saved { path: PathBuf, name: String },
}

/// Estado do backup no app (os ajustes são gravados no prefs.json).
pub struct Backup {
    pub enabled: bool,
    pub dest: Dest,
    pub interval_min: u32,
    /// Também guardar uma cópia a cada "Salvar".
    pub on_save: bool,
    /// Hora (relógio do egui) em que a contagem do intervalo começou.
    since: Option<f64>,
    /// `edits` do projeto no último backup automático.
    last_edits: u64,
    job: Option<Receiver<Result<Done, String>>>,
    pending: Option<Pending>,
    pub last_ok: Option<SystemTime>,
    pub last_error: Option<String>,
    /// Janela de ajustes aberta.
    pub window: bool,
    /// Seletor de pasta do Android aberto.
    picking: bool,
    /// Lista para restaurar (`Some` = mostrando a lista).
    restore: Option<Vec<Entry>>,
    /// Texto da pasta sendo editado (computador).
    folder_text: String,
}

impl Default for Backup {
    fn default() -> Self {
        Self {
            enabled: true,
            dest: Dest::default_folder(),
            interval_min: 60,
            on_save: true,
            since: None,
            last_edits: 0,
            job: None,
            pending: None,
            last_ok: None,
            last_error: None,
            window: false,
            picking: false,
            restore: None,
            folder_text: String::new(),
        }
    }
}

impl Backup {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Ajustes para o prefs.json.
    pub fn to_json(&self) -> serde_json::Value {
        let (folder, uri, label) = match &self.dest {
            Dest::Folder(p) => (Some(p.to_string_lossy().into_owned()), None, None),
            Dest::Saf { uri, label } => (None, Some(uri.clone()), Some(label.clone())),
        };
        serde_json::json!({
            "enabled": self.enabled,
            "interval_min": self.interval_min,
            "on_save": self.on_save,
            "folder": folder,
            "uri": uri,
            "label": label,
        })
    }

    pub fn load_json(&mut self, v: &serde_json::Value) {
        let flag = |k: &str| v.get(k).and_then(serde_json::Value::as_bool);
        let text = |k: &str| {
            v.get(k)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        if let Some(b) = flag("enabled") {
            self.enabled = b;
        }
        if let Some(b) = flag("on_save") {
            self.on_save = b;
        }
        if let Some(m) = v.get("interval_min").and_then(serde_json::Value::as_u64) {
            self.interval_min = (m as u32).clamp(5, 24 * 60);
        }
        if let Some(uri) = text("uri") {
            let label = text("label").unwrap_or_else(|| uri.clone());
            self.dest = Dest::Saf { uri, label };
        } else if let Some(f) = text("folder") {
            self.dest = Dest::Folder(PathBuf::from(f));
        }
    }
}

fn file_name(project_name: &str, suffix: &str) -> String {
    format!("{}{suffix}.{}", clean_name(project_name), project::EXT)
}

/// Nome do projeto sem o sufixo de backup.
fn project_name_of(file: &str) -> String {
    let stem = Path::new(file)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    for suf in [AUTO_SUFFIX, SAVED_SUFFIX] {
        if let Some(s) = stem.strip_suffix(suf) {
            return s.to_string();
        }
    }
    stem
}

fn interval_label(min: u32) -> String {
    if min < 60 {
        format!("{min} min")
    } else if min.is_multiple_of(60) {
        format!("{} h", min / 60)
    } else {
        format!("{} h {} min", min / 60, min % 60)
    }
}

fn size_label(bytes: u64) -> String {
    if bytes >= 1 << 20 {
        format!("{:.1} MB", bytes as f64 / f64::from(1 << 20))
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

impl ArtApp {
    /// A cada quadro: termina o backup em andamento e começa o próximo quando chega a hora.
    pub fn backup_tick(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        self.finish_backup();
        self.poll_folder_pick();
        if self.backup.busy() || self.backup.picking {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        if !self.backup.enabled {
            self.backup.since = None;
            return;
        }
        if self.backup.busy() {
            return;
        }
        if let Some(p) = self.backup.pending.take() {
            self.start_backup_saved(p);
            return;
        }
        let since = *self.backup.since.get_or_insert(now);
        let changed = self.projects.edits != self.backup.last_edits;
        let wait = f64::from(self.backup.interval_min) * 60.0;
        if now - since >= wait {
            if changed {
                self.start_backup_auto(false);
            }
            self.backup.since = Some(now);
        } else if changed {
            ctx.request_repaint_after(Duration::from_secs_f64((wait - (now - since)).max(1.0)));
        }
    }

    /// Menu › Backup › Fazer backup agora.
    pub fn backup_now(&mut self) {
        if self.backup.busy() {
            self.status = "Aguarde: o backup anterior ainda está gravando.".into();
            return;
        }
        self.start_backup_auto(true);
    }

    fn start_backup_auto(&mut self, manual: bool) {
        let snap = self.doc.snapshot();
        let edits = self.projects.edits;
        let name = file_name(&self.projects.name, AUTO_SUFFIX);
        let dest = self.backup.dest.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = project::save(&snap)
                .and_then(|b| dest.put(&name, &b))
                .map(|()| Done::Auto { edits, manual });
            let _ = tx.send(r);
        });
        if manual {
            self.status = "Fazendo backup…".into();
        }
        self.backup.job = Some(rx);
    }

    /// Chamado quando "Salvar" termina: guarda a cópia de quando foi salvo.
    pub fn backup_after_save(&mut self, path: &Path) {
        if !self.backup.enabled || !self.backup.on_save {
            return;
        }
        self.backup.pending = Some(Pending::Saved {
            path: path.to_path_buf(),
            name: file_name(&self.projects.name, SAVED_SUFFIX),
        });
    }

    fn start_backup_saved(&mut self, p: Pending) {
        let Pending::Saved { path, name } = p;
        let dest = self.backup.dest.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = std::fs::read(&path)
                .map_err(|e| format!("{}: {e}", path.display()))
                .and_then(|bytes| {
                    // Só substitui a cópia anterior se o arquivo salvo abre direito.
                    project::load(&bytes).map_err(|e| {
                        format!("o arquivo salvo não abriu ({e}); o backup anterior foi mantido")
                    })?;
                    dest.put(&name, &bytes)
                })
                .map(|()| Done::Saved);
            let _ = tx.send(r);
        });
        self.backup.job = Some(rx);
    }

    fn finish_backup(&mut self) {
        let Some(rx) = self.backup.job.take() else {
            return;
        };
        let r = match rx.try_recv() {
            Err(TryRecvError::Empty) => {
                self.backup.job = Some(rx);
                return;
            }
            Err(TryRecvError::Disconnected) => Err("erro interno".to_string()),
            Ok(r) => r,
        };
        match r {
            Err(e) => {
                self.status = format!("Backup falhou: {e}");
                self.backup.last_error = Some(e);
            }
            Ok(Done::Auto { edits, manual }) => {
                self.backup.last_edits = edits;
                self.backup.last_ok = Some(SystemTime::now());
                self.backup.last_error = None;
                if manual {
                    self.status = format!("Backup feito em {}", self.backup.dest.label());
                }
            }
            Ok(Done::Saved) => {
                self.backup.last_ok = Some(SystemTime::now());
                self.backup.last_error = None;
            }
            Ok(Done::Listed(list)) => {
                self.backup.restore = Some(list);
            }
            Ok(Done::Fetched(path)) => {
                let name = project_name_of(&path.to_string_lossy());
                self.open_backup(path, name);
                self.backup.window = false;
                self.backup.restore = None;
            }
        }
    }

    fn poll_folder_pick(&mut self) {
        if !self.backup.picking {
            return;
        }
        match saf::pick_result() {
            None => {}
            Some(r) => {
                self.backup.picking = false;
                match r {
                    Ok(Some(uri)) => {
                        let label = saf::folder_name(&uri);
                        self.backup.dest = Dest::Saf { uri, label };
                        self.backup.since = None;
                        self.backup.last_edits = self.projects.edits.wrapping_sub(1);
                        self.save_prefs();
                        self.status = "Pasta de backup escolhida.".into();
                    }
                    Ok(None) => {}
                    Err(e) => self.status = format!("Não foi possível escolher a pasta: {e}"),
                }
            }
        }
    }

    fn list_backups(&mut self) {
        if self.backup.busy() {
            self.status = "Aguarde: o backup ainda está gravando.".into();
            return;
        }
        let dest = self.backup.dest.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(dest.list().map(Done::Listed));
        });
        self.backup.job = Some(rx);
    }

    fn restore_backup(&mut self, name: String) {
        let dest = self.backup.dest.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(dest.fetch(&name).map(Done::Fetched));
        });
        self.status = "Buscando o backup…".into();
        self.backup.job = Some(rx);
    }
}

/// Janela Menu › Arquivo › Backup.
pub fn show(app: &mut ArtApp, ctx: &egui::Context) {
    if !app.backup.window {
        return;
    }
    let mut open = true;
    let mut changed = false;
    egui::Window::new("Backup")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .order(egui::Order::Foreground)
        .frame(panel_frame())
        .show(ctx, |ui| {
            ui.set_max_width(380.0);
            egui::ScrollArea::vertical()
                .max_height(ctx.content_rect().height() * 0.75)
                .show(ui, |ui| {
                    if app.backup.restore.is_some() {
                        restore_ui(app, ui);
                    } else {
                        changed |= settings_ui(app, ui);
                    }
                });
        });
    if !open {
        app.backup.window = false;
        app.backup.restore = None;
    }
    if changed {
        app.save_prefs();
    }
}

fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).size(12.5).color(color::TEXT_MUTED)
}

fn settings_ui(app: &mut ArtApp, ui: &mut egui::Ui) -> bool {
    let mut changed = false;
    ui.label(muted(
        "Além do arquivo original, cada projeto ganha duas cópias na pasta de backup: \
         uma automática, atualizada no intervalo escolhido, e uma da última vez que foi salvo.",
    ));
    ui.add_space(6.0);
    changed |= theme::switch(ui, &mut app.backup.enabled, "Backup ligado").changed();
    ui.add_enabled_ui(app.backup.enabled, |ui| {
        ui.add_space(4.0);
        ui.label(RichText::new("Pasta").strong());
        ui.label(muted(app.backup.dest.label()));
        if saf::SUPPORTED {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(!app.backup.picking, egui::Button::new("Escolher pasta…"))
                    .clicked()
                {
                    match saf::pick_folder() {
                        Ok(()) => app.backup.picking = true,
                        Err(e) => app.status = format!("Não foi possível abrir o seletor: {e}"),
                    }
                }
                if ui.button("Usar a pasta padrão").clicked() {
                    app.backup.dest = Dest::default_folder();
                    changed = true;
                }
            });
            ui.label(muted(
                "Para guardar no Google Drive, toque em Escolher pasta, abra o menu do seletor \
                 e escolha Drive (o app do Google Drive precisa estar instalado).",
            ));
        } else {
            if app.backup.folder_text.is_empty() {
                app.backup.folder_text = app.backup.dest.label();
            }
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.backup.folder_text).desired_width(250.0),
                );
                if ui.button("Usar").clicked() {
                    let p = PathBuf::from(app.backup.folder_text.trim());
                    match std::fs::create_dir_all(&p) {
                        Ok(()) => {
                            app.backup.dest = Dest::Folder(p);
                            changed = true;
                        }
                        Err(e) => app.status = format!("Não foi possível usar a pasta: {e}"),
                    }
                }
            });
            if ui.button("Usar a pasta padrão").clicked() {
                app.backup.dest = Dest::default_folder();
                app.backup.folder_text = app.backup.dest.label();
                changed = true;
            }
            ui.label(muted(
                "Para guardar no Google Drive, use a pasta do Google Drive para computador \
                 (por exemplo G:\\Meu Drive\\ArtApp).",
            ));
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Backup automático a cada");
            egui::ComboBox::from_id_salt("backup_interval")
                .selected_text(interval_label(app.backup.interval_min))
                .show_ui(ui, |ui| {
                    for m in INTERVALS {
                        changed |= ui
                            .selectable_value(&mut app.backup.interval_min, m, interval_label(m))
                            .changed();
                    }
                });
        });
        changed |= theme::switch(ui, &mut app.backup.on_save, "Cópia a cada Salvar").changed();
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            let busy = app.backup.busy();
            if ui
                .add_enabled(!busy, egui::Button::new("Fazer backup agora"))
                .clicked()
            {
                app.backup_now();
            }
            if ui
                .add_enabled(!busy, egui::Button::new("Restaurar um backup…"))
                .clicked()
            {
                app.list_backups();
            }
        });
    });
    ui.add_space(4.0);
    if app.backup.busy() {
        ui.label(muted("Gravando…"));
    } else if let Some(e) = &app.backup.last_error {
        ui.label(
            RichText::new(format!("Último backup falhou: {e}"))
                .size(12.5)
                .color(egui::Color32::from_rgb(0xe5, 0x6b, 0x6b)),
        );
    } else if app.backup.last_ok.is_some() {
        ui.label(muted(format!(
            "Último backup: {}",
            time_ago(app.backup.last_ok)
        )));
    }
    changed
}

fn restore_ui(app: &mut ArtApp, ui: &mut egui::Ui) {
    if ui.button("‹ Voltar").clicked() {
        app.backup.restore = None;
        return;
    }
    ui.label(muted(
        "O backup abre como um desenho novo: salve com outro nome ou por cima do original.",
    ));
    ui.add_space(4.0);
    let list = app.backup.restore.clone().unwrap_or_default();
    if list.is_empty() {
        ui.label("Nenhum backup nesta pasta ainda.");
    }
    let busy = app.backup.busy();
    let mut pick = None;
    for e in &list {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(250.0);
                ui.label(&e.name);
                ui.label(muted(format!(
                    "{} · {}",
                    time_ago(e.modified),
                    size_label(e.size)
                )));
            });
            if ui.add_enabled(!busy, egui::Button::new("Abrir")).clicked() {
                pick = Some(e.name.clone());
            }
        });
        ui.separator();
    }
    if let Some(name) = pick {
        app.restore_backup(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(
            file_name("Gato", AUTO_SUFFIX),
            "Gato - backup automático.artapp"
        );
        assert_eq!(project_name_of("Gato - salvo.artapp"), "Gato");
        assert_eq!(project_name_of("Gato - backup automático.artapp"), "Gato");
        assert_eq!(project_name_of("Outro.artapp"), "Outro");
    }

    #[test]
    fn folder_roundtrip() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("artapp-backup-{}", std::process::id()));
        let dest = Dest::Folder(dir.clone());
        dest.put("a - salvo.artapp", b"um")?;
        dest.put("a - salvo.artapp", b"dois")?;
        let list = dest.list()?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].size, 4);
        let back = std::fs::read(dest.fetch("a - salvo.artapp")?).map_err(|e| e.to_string())?;
        assert_eq!(back, b"dois");
        let _ = std::fs::remove_dir_all(dir);
        Ok(())
    }
}
