//! Salvar e abrir projetos (`.artapp`), abrir e exportar PSD, salvamento automático e a
//! recuperação do desenho não salvo.
//!
//! Codificar e gravar roda numa thread à parte (as camadas são copy-on-write, então tirar a cópia
//! é barato) para o app não travar enquanto o arquivo é escrito.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::SystemTime;

use artapp_core::{Document, project};
use eframe::egui::{self, Align2, ColorImage, RichText, TextureHandle, Vec2};

use crate::app::ArtApp;
use crate::platform;
use crate::theme::{color, icon, panel_frame};

/// Salvamento automático: espera esse tempo depois de uma alteração antes de gravar.
const AUTOSAVE_EVERY: f64 = 60.0;
/// E só grava quando a pessoa para de desenhar por um instante.
const AUTOSAVE_IDLE: f64 = 2.0;

/// O que é aberto/salvo.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Project,
    Psd,
}

fn kind_of(path: &Path) -> Option<Kind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        project::EXT => Some(Kind::Project),
        "psd" | "psb" => Some(Kind::Psd),
        _ => None,
    }
}

/// Um trabalho em segundo plano.
enum Job {
    Save {
        rx: Receiver<Result<PathBuf, String>>,
        /// Contagem de alterações do documento quando a cópia foi tirada.
        edits: u64,
        /// Salvamento automático no arquivo de recuperação (não muda o projeto atual).
        recovery: bool,
        auto: bool,
        export: bool,
    },
    Open {
        rx: Receiver<Result<Document, String>>,
        path: PathBuf,
        recovery: bool,
    },
}

/// Estado dos projetos dentro do app.
pub struct Projects {
    /// Arquivo `.artapp` do desenho atual (`None` = ainda não salvo).
    pub path: Option<PathBuf>,
    /// Nome mostrado na barra de cima.
    pub name: String,
    /// Conta as alterações do documento (sobe a cada área recomposta).
    pub edits: u64,
    /// `edits` da última vez que o projeto foi salvo (ou aberto).
    pub saved_edits: u64,
    /// Salvamento automático ligado.
    pub autosave: bool,
    /// Quando a última alteração aconteceu e quando o salvamento automático rodou.
    last_edit_at: f64,
    last_auto_at: f64,
    /// Edits gravados no arquivo de recuperação.
    recovery_edits: u64,
    /// `edits` no quadro anterior (para saber quando houve a última alteração).
    seen_edits: u64,
    was_focused: bool,
    job: Option<Job>,
    pub dialog: Option<FileDialog>,
}

impl Default for Projects {
    fn default() -> Self {
        Self {
            path: None,
            name: "Sem título".into(),
            edits: 0,
            saved_edits: 0,
            autosave: true,
            last_edit_at: 0.0,
            last_auto_at: 0.0,
            recovery_edits: 0,
            seen_edits: 0,
            was_focused: true,
            job: None,
            dialog: recovery_path().is_file().then_some(FileDialog::Recover),
        }
    }
}

impl Projects {
    pub fn dirty(&self) -> bool {
        self.edits != self.saved_edits
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Chamado quando algo no desenho muda.
    pub fn touch(&mut self) {
        self.edits += 1;
    }
}

/// Diálogos de arquivo.
pub enum FileDialog {
    /// Escolher o nome antes de salvar (ou exportar PSD).
    SaveAs { name: String, psd: bool },
    /// Navegar pelas pastas e abrir um projeto ou PSD.
    Open(Browser),
    /// Ao abrir o app: há um desenho que não foi salvo.
    Recover,
}

pub struct Browser {
    dir: PathBuf,
    /// O app pode ler arquivos de outros apps (conferido ao abrir a pasta).
    shared_access: bool,
    entries: Vec<Entry>,
    thumbs: HashMap<PathBuf, Option<TextureHandle>>,
    /// Escolhendo uma imagem para importar como camada (em vez de abrir um projeto).
    import: bool,
    /// Para que serve a imagem escolhida (com `import`).
    image_use: ImageUse,
}

/// Imagens que dá para importar como camada.
fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"
        )
    })
}

/// Para que a imagem escolhida vai servir.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageUse {
    #[default]
    Layer,
    Reference,
    Palette,
}

struct Entry {
    path: PathBuf,
    name: String,
    is_dir: bool,
    modified: Option<SystemTime>,
}

impl Browser {
    fn new(dir: PathBuf) -> Self {
        Self::with_mode(dir, false)
    }

    fn with_mode(dir: PathBuf, import: bool) -> Self {
        let mut b = Self {
            image_use: ImageUse::Layer,
            dir,
            shared_access: platform::can_read_shared_files(),
            entries: Vec::new(),
            thumbs: HashMap::new(),
            import,
        };
        b.refresh();
        b
    }

    fn refresh(&mut self) {
        self.entries.clear();
        let Ok(rd) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for e in rd.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let meta = e.metadata().ok();
            let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
            let wanted = if self.import {
                is_image(&path)
            } else {
                kind_of(&path).is_some()
            };
            if !is_dir && !wanted {
                continue;
            }
            self.entries.push(Entry {
                path,
                name,
                is_dir,
                modified: meta.and_then(|m| m.modified().ok()),
            });
        }
        // Pastas primeiro; arquivos do mais recente para o mais antigo.
        self.entries.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then_with(|| {
                if a.is_dir {
                    a.name.to_lowercase().cmp(&b.name.to_lowercase())
                } else {
                    b.modified.cmp(&a.modified)
                }
            })
        });
    }

    fn thumb(&mut self, ctx: &egui::Context, path: &Path) -> Option<TextureHandle> {
        self.thumbs
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                let png = project::read_preview(path)?;
                let (w, h, px) = project::decode_rgba(&png).ok()?;
                let img = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px);
                Some(ctx.load_texture(
                    format!("proj:{}", path.display()),
                    img,
                    egui::TextureOptions::LINEAR,
                ))
            })
            .clone()
    }
}

/// Arquivo do salvamento automático de um desenho que ainda não tem nome.
pub fn recovery_path() -> PathBuf {
    platform::data_dir().join(format!("recuperacao.{}", project::EXT))
}

/// Tira do nome o que não pode ir num nome de arquivo.
fn clean_name(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let s = s.trim_matches(['.', ' ']).to_string();
    if s.is_empty() { "Desenho".into() } else { s }
}

/// "Desenho 1", "Desenho 2"… o primeiro que ainda não existe na pasta de projetos.
fn suggest_name() -> String {
    let dir = platform::projects_dir();
    (1..)
        .map(|i| format!("Desenho {i}"))
        .find(|n| !dir.join(format!("{n}.{}", project::EXT)).exists())
        .unwrap_or_else(|| "Desenho".into())
}

fn time_ago(t: Option<SystemTime>) -> String {
    let Some(secs) = t
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map(|d| d.as_secs())
    else {
        return String::new();
    };
    match secs {
        0..60 => "agora".into(),
        60..3600 => format!("há {} min", secs / 60),
        3600..86400 => format!("há {} h", secs / 3600),
        _ => format!("há {} dias", secs / 86400),
    }
}

impl ArtApp {
    /// Menu › Salvar projeto: grava no arquivo atual ou pergunta o nome.
    pub fn save_project(&mut self) {
        match self.projects.path.clone() {
            Some(path) => self.start_save(path, false, false),
            None => self.save_project_as(),
        }
    }

    pub fn save_project_as(&mut self) {
        let name = if self.projects.path.is_some() || self.projects.name != "Sem título" {
            self.projects.name.clone()
        } else {
            suggest_name()
        };
        self.projects.dialog = Some(FileDialog::SaveAs { name, psd: false });
    }

    pub fn export_psd(&mut self) {
        let name = if self.projects.name == "Sem título" {
            suggest_name()
        } else {
            self.projects.name.clone()
        };
        self.projects.dialog = Some(FileDialog::SaveAs { name, psd: true });
    }

    pub fn open_project_dialog(&mut self) {
        let dir = self
            .projects
            .path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(platform::projects_dir);
        self.projects.dialog = Some(FileDialog::Open(Browser::new(dir)));
    }

    /// Escolher uma imagem (PNG, JPEG, WebP…) para virar camada nova.
    /// Escolher uma imagem para `use_for` (camada nova, referência ou paleta).
    pub fn pick_image_for(&mut self, use_for: ImageUse) {
        self.import_image_dialog();
        if let Some(FileDialog::Open(b)) = &mut self.projects.dialog {
            b.image_use = use_for;
        }
    }

    pub fn import_image_dialog(&mut self) {
        // Começa em Imagens (ou Downloads), se existir.
        let dir = platform::browse_roots()
            .into_iter()
            .filter(|(label, d)| matches!(*label, "Imagens" | "Downloads") && d.is_dir())
            .map(|(_, d)| d)
            .next()
            .unwrap_or_else(platform::projects_dir);
        self.projects.dialog = Some(FileDialog::Open(Browser::with_mode(dir, true)));
    }

    /// Abre a imagem na janela de referência.
    pub fn open_reference(&mut self, path: &Path) {
        match std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| artapp_core::edit::decode_image(&b))
        {
            Ok((w, h, px)) => {
                self.reference = Some(crate::reference::Reference::new(w, h, px));
            }
            Err(e) => self.status = format!("Não foi possível abrir a referência: {e}"),
        }
    }

    /// Cria um conjunto de cores com as cores principais da imagem.
    pub fn palette_from_image(&mut self, path: &Path) {
        let res = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| artapp_core::edit::decode_image(&b));
        let (_, _, px) = match res {
            Ok(v) => v,
            Err(e) => {
                self.status = format!("Não foi possível ler a imagem: {e}");
                return;
            }
        };
        let colors = artapp_core::palette::from_pixels(&px, 12);
        if colors.is_empty() {
            self.status = "A imagem não tem cores (está transparente).".into();
            return;
        }
        let name = path
            .file_stem()
            .map_or("Foto".into(), |s| s.to_string_lossy().into_owned());
        let st = &mut self.color_state;
        st.sets.push(crate::color_panel::ColorSet {
            name,
            colors: colors.iter().map(|c| [c[0], c[1], c[2], 255]).collect(),
        });
        st.set_index = st.sets.len() - 1;
        st.tab = crate::color_panel::ColorTab::Sets;
        crate::color_panel::save_sets(&st.sets);
        self.show_color = true;
        self.status = format!("Conjunto com {} cores criado.", colors.len());
    }

    /// Lê a imagem e põe numa camada nova, no centro da tela.
    pub fn import_image(&mut self, path: &Path) {
        let res = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| artapp_core::edit::decode_image(&b));
        match res {
            Ok((w, h, px)) => {
                let name = path
                    .file_stem()
                    .map_or("Imagem".into(), |s| s.to_string_lossy().into_owned());
                let r = self.doc.import_image(&name, w, h, &px);
                self.invalidate(r);
                self.status = format!("Imagem importada como camada ({w}×{h}).");
            }
            Err(e) => self.status = format!("Não foi possível importar: {e}"),
        }
    }

    /// Grava agora o que estiver pendente, como o salvamento automático (antes de o app fechar
    /// para atualizar).
    pub fn flush_autosave(&mut self) {
        if !self.projects.dirty() || self.projects.busy() {
            return;
        }
        match self.projects.path.clone() {
            Some(path) => self.start_save(path, true, false),
            None => self.start_save(recovery_path(), true, true),
        }
    }

    /// Codifica e grava em segundo plano.
    fn start_save(&mut self, path: PathBuf, auto: bool, recovery: bool) {
        // O conteúdo em transformação está fora da camada: aplica antes de gravar.
        self.commit_transform();
        if self.projects.busy() {
            if !auto {
                self.status = "Aguarde: ainda gravando o arquivo anterior.".into();
            }
            return;
        }
        let snap = self.doc.snapshot();
        let export = kind_of(&path) == Some(Kind::Psd);
        let (tx, rx) = channel();
        let target = path.clone();
        std::thread::spawn(move || {
            let bytes = if export {
                artapp_core::psd::export(&snap)
            } else {
                project::save(&snap)
            };
            let res = bytes.and_then(|b| platform::write_atomic(&target, &b).map(|()| target));
            let _ = tx.send(res);
        });
        if !auto {
            self.status = if export {
                "Exportando PSD…".into()
            } else {
                "Salvando projeto…".into()
            };
        }
        self.projects.job = Some(Job::Save {
            rx,
            edits: self.projects.edits,
            recovery,
            auto,
            export,
        });
    }

    /// Abre um arquivo (projeto ou PSD) em segundo plano.
    pub fn open_path(&mut self, path: PathBuf) {
        self.open_path_as(path, false);
    }

    fn open_path_as(&mut self, path: PathBuf, recovery: bool) {
        if self.projects.busy() {
            self.status = "Aguarde: ainda gravando ou abrindo um arquivo.".into();
            return;
        }
        let (tx, rx) = channel();
        let src = path.clone();
        std::thread::spawn(move || {
            let res = std::fs::read(&src)
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    if project::sniff(&bytes) {
                        project::load(&bytes)
                    } else if artapp_core::psd::sniff(&bytes) {
                        artapp_core::psd::import(&bytes)
                    } else {
                        Err("formato desconhecido (abre .artapp e .psd)".into())
                    }
                });
            let _ = tx.send(res);
        });
        self.status = "Abrindo…".into();
        self.projects.job = Some(Job::Open { rx, path, recovery });
    }

    /// Troca o desenho por um que acabou de abrir.
    fn replace_document(&mut self, doc: Document) {
        self.doc = doc;
        self.view = crate::canvas::CanvasView::default();
        self.selected = None;
        self.bezier.clear();
        self.invalidate_all();
        self.projects.saved_edits = self.projects.edits;
        self.projects.recovery_edits = self.projects.edits;
    }

    /// Começa um desenho novo sem nome (Nova tela).
    pub fn forget_project(&mut self) {
        self.projects.path = None;
        self.projects.name = "Sem título".into();
        self.projects.saved_edits = self.projects.edits;
        self.projects.recovery_edits = self.projects.edits;
        let _ = std::fs::remove_file(recovery_path());
    }

    /// A cada quadro: termina trabalhos em segundo plano e roda o salvamento automático.
    pub fn projects_tick(&mut self, ctx: &egui::Context) {
        self.finish_job();
        if self.projects.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        // Arrastar um arquivo para a janela (desktop) o abre.
        let dropped = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .find(|p| kind_of(p).is_some())
        });
        if let Some(p) = dropped {
            self.open_path(p);
        }

        let now = ctx.input(|i| i.time);
        if self.projects.edits != self.projects.seen_edits {
            self.projects.seen_edits = self.projects.edits;
            self.projects.last_edit_at = now;
        }
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        let lost_focus = self.projects.was_focused && !focused;
        self.projects.was_focused = focused;
        let p = &self.projects;
        let pending = if p.path.is_some() {
            p.dirty()
        } else {
            p.edits != p.recovery_edits && p.dirty()
        };
        if !p.autosave || !pending || p.busy() {
            return;
        }
        let drawing = ctx.input(|i| i.pointer.any_down() || i.any_touches());
        let due = now - p.last_auto_at >= AUTOSAVE_EVERY && now - p.last_edit_at >= AUTOSAVE_IDLE;
        // Sair do app (ir para outro app no Android, trocar de janela) grava na hora.
        if lost_focus || (due && !drawing && self.transform.is_none()) {
            self.projects.last_auto_at = now;
            match self.projects.path.clone() {
                Some(path) => self.start_save(path, true, false),
                None => self.start_save(recovery_path(), true, true),
            }
        } else {
            // Sem toques o egui não redesenha: pede um quadro para quando der a hora.
            let wait = (p.last_auto_at + AUTOSAVE_EVERY - now)
                .max(p.last_edit_at + AUTOSAVE_IDLE - now)
                .max(0.5);
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
    }

    fn finish_job(&mut self) {
        let Some(job) = self.projects.job.take() else {
            return;
        };
        match job {
            Job::Save {
                rx,
                edits,
                recovery,
                auto,
                export,
            } => match rx.try_recv() {
                Err(TryRecvError::Empty) => {
                    self.projects.job = Some(Job::Save {
                        rx,
                        edits,
                        recovery,
                        auto,
                        export,
                    });
                }
                Err(TryRecvError::Disconnected) => {
                    self.status = "Não foi possível salvar (erro interno).".into();
                }
                Ok(Err(e)) => {
                    self.status = format!("Não foi possível salvar: {e}");
                }
                Ok(Ok(path)) => {
                    if export {
                        self.status = format!("PSD exportado em {}", path.display());
                    } else if recovery {
                        self.projects.recovery_edits = edits;
                    } else {
                        self.projects.saved_edits = edits;
                        self.projects.recovery_edits = edits;
                        self.projects.name = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        self.projects.path = Some(path.clone());
                        let _ = std::fs::remove_file(recovery_path());
                        if !auto {
                            self.status = format!("Projeto salvo em {}", path.display());
                        }
                    }
                }
            },
            Job::Open { rx, path, recovery } => match rx.try_recv() {
                Err(TryRecvError::Empty) => {
                    self.projects.job = Some(Job::Open { rx, path, recovery });
                }
                Err(TryRecvError::Disconnected) => {
                    self.status = "Não foi possível abrir (erro interno).".into();
                }
                Ok(Err(e)) => {
                    self.status = format!("Não foi possível abrir: {e}");
                }
                Ok(Ok(doc)) => {
                    let layers = doc.layers.len();
                    self.replace_document(doc);
                    let stem = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if recovery {
                        // Continua sem nome; o arquivo de recuperação segue valendo até salvar.
                        self.projects.path = None;
                        self.projects.name = "Sem título".into();
                        self.projects.saved_edits = self.projects.edits.wrapping_sub(1);
                        self.status = "Desenho recuperado. Salve para guardá-lo.".into();
                    } else if kind_of(&path) == Some(Kind::Psd) {
                        // Um PSD aberto vira um projeto novo com o mesmo nome (o PSD fica intacto).
                        self.projects.path = None;
                        self.projects.name = stem;
                        self.status = format!(
                            "PSD aberto com {layers} camada(s). Salvar grava um projeto .artapp."
                        );
                    } else {
                        self.projects.path = Some(path);
                        self.projects.name = stem;
                        self.status = format!("Projeto aberto ({layers} camada(s)).");
                    }
                }
            },
        }
    }
}

/// Mostra o diálogo de arquivo aberto, se houver.
pub fn show(app: &mut ArtApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.projects.dialog.take() else {
        return;
    };
    let keep = match &mut dialog {
        FileDialog::SaveAs { name, psd } => save_as_ui(app, ctx, name, *psd),
        FileDialog::Open(b) => open_ui(app, ctx, b),
        FileDialog::Recover => recover_ui(app, ctx),
    };
    // A ação pode ter aberto outro diálogo (ex.: salvar antes de abrir).
    if keep && app.projects.dialog.is_none() {
        app.projects.dialog = Some(dialog);
    }
}

fn window(title: &str) -> egui::Window<'_> {
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .order(egui::Order::Foreground)
        .frame(panel_frame())
}

fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).size(12.5).color(color::TEXT_MUTED)
}

fn save_as_ui(app: &mut ArtApp, ctx: &egui::Context, name: &mut String, psd: bool) -> bool {
    let mut open = true;
    let mut cancel = false;
    let mut go = false;
    let title = if psd {
        "Exportar PSD"
    } else {
        "Salvar projeto"
    };
    let ext = if psd { "psd" } else { project::EXT };
    let dir = platform::projects_dir();
    window(title).open(&mut open).show(ctx, |ui| {
        ui.set_width(320.0);
        ui.label("Nome");
        let r = ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            go = true;
        }
        let file = format!("{}.{ext}", clean_name(name));
        ui.add(egui::Label::new(muted(format!("Em {}", dir.join(&file).display()))).wrap());
        if dir.join(&file).exists() {
            ui.label(
                RichText::new("Já existe um arquivo com esse nome: ele será substituído.")
                    .size(12.5)
                    .color(color::ACCENT),
            );
        }
        if psd {
            ui.label(muted(
                "Cada camada vira uma camada do PSD (vetoriais vão rasterizadas). Para continuar \
                 editando no ArtApp, prefira salvar o projeto.",
            ));
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .button(RichText::new(if psd { "Exportar" } else { "Salvar" }).strong())
                .clicked()
            {
                go = true;
            }
            if ui.button("Cancelar").clicked() {
                cancel = true;
            }
        });
    });
    if cancel {
        return false;
    }
    if go {
        let path = dir.join(format!("{}.{ext}", clean_name(name)));
        app.start_save(path, false, false);
        return false;
    }
    open
}

fn open_ui(app: &mut ArtApp, ctx: &egui::Context, b: &mut Browser) -> bool {
    let mut open = true;
    let mut pick: Option<PathBuf> = None;
    let mut go_to: Option<PathBuf> = None;
    let screen = ctx.content_rect();
    let w = (screen.width() - 32.0).clamp(280.0, 520.0);
    let h = (screen.height() - 140.0).clamp(220.0, 560.0);
    let title = match (b.import, b.image_use) {
        (false, _) => "Abrir",
        (true, ImageUse::Layer) => "Importar imagem",
        (true, ImageUse::Reference) => "Imagem de referência",
        (true, ImageUse::Palette) => "Paleta de uma imagem",
    };
    window(title).open(&mut open).show(ctx, |ui| {
        ui.set_width(w);
        ui.set_max_width(w);
        ui.horizontal_wrapped(|ui| {
            for (label, dir) in platform::browse_roots() {
                if ui.selectable_label(b.dir == dir, label).clicked() {
                    go_to = Some(dir);
                }
            }
        });
        ui.horizontal(|ui| {
            let up = b.dir.parent().map(Path::to_path_buf);
            if ui
                .add_enabled(up.is_some(), egui::Button::new("Pasta acima"))
                .clicked()
            {
                go_to = up;
            }
            ui.add(egui::Label::new(muted(b.dir.display().to_string())).truncate());
        });
        if !b.shared_access {
            ui.horizontal_wrapped(|ui| {
                ui.label(muted(
                    "Para abrir PSDs de outras pastas (ex.: Downloads), o ArtApp precisa de acesso \
                     aos arquivos.",
                ));
                if ui.button("Permitir acesso").clicked()
                    && let Err(e) = platform::request_file_access()
                {
                    app.status = format!("Não foi possível pedir o acesso: {e}");
                }
            });
        }
        if app.projects.dirty() && !b.import {
            ui.label(
                RichText::new("O desenho atual tem alterações não salvas.")
                    .size(12.5)
                    .color(color::ACCENT),
            );
        }
        ui.separator();
        egui::ScrollArea::vertical().max_height(h).show(ui, |ui| {
            if b.entries.is_empty() {
                ui.label(muted(if b.import {
                    "Nenhuma imagem (PNG, JPEG, WebP, GIF, BMP) nesta pasta."
                } else {
                    "Nenhum projeto (.artapp) ou PSD nesta pasta."
                }));
            }
            let entries: Vec<_> = b
                .entries
                .iter()
                .map(|e| (e.path.clone(), e.name.clone(), e.is_dir, e.modified))
                .collect();
            for (path, name, is_dir, modified) in entries {
                let thumb = if !is_dir && kind_of(&path) == Some(Kind::Project) {
                    b.thumb(ctx, &path)
                } else {
                    None
                };
                let row = ui.horizontal(|ui| {
                    let side = 56.0;
                    match &thumb {
                        Some(t) => {
                            let s = t.size_vec2();
                            let k = side / s.x.max(s.y).max(1.0);
                            ui.add_sized([side, side], egui::Image::new((t.id(), s * k)));
                        }
                        None => {
                            let glyph = if is_dir {
                                icon::FOLDER_OPEN
                            } else {
                                icon::IMAGE
                            };
                            ui.add_sized(
                                [side, side],
                                egui::Label::new(
                                    RichText::new(glyph.to_string())
                                        .font(crate::theme::icon_font(26.0))
                                        .color(color::TEXT_MUTED),
                                ),
                            );
                        }
                    }
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&name).size(15.0));
                        if !is_dir {
                            ui.label(muted(time_ago(modified)));
                        }
                    });
                    ui.allocate_space(Vec2::new(ui.available_width().max(0.0), 1.0));
                });
                let resp = ui.interact(
                    row.response.rect,
                    ui.scope_id().with(("open-row", &path)),
                    egui::Sense::click(),
                );
                if resp.clicked() {
                    if is_dir {
                        go_to = Some(path);
                    } else {
                        pick = Some(path);
                    }
                }
            }
        });
    });
    if let Some(dir) = go_to {
        let image_use = b.image_use;
        *b = Browser::with_mode(dir, b.import);
        b.image_use = image_use;
    }
    if let Some(path) = pick {
        if b.import {
            match b.image_use {
                ImageUse::Layer => app.import_image(&path),
                ImageUse::Reference => app.open_reference(&path),
                ImageUse::Palette => app.palette_from_image(&path),
            }
        } else {
            app.open_path(path);
        }
        return false;
    }
    open
}

fn recover_ui(app: &mut ArtApp, ctx: &egui::Context) -> bool {
    let mut keep = true;
    window("Desenho não salvo").show(ctx, |ui| {
        ui.set_width(300.0);
        ui.label("O ArtApp fechou com um desenho que não tinha sido salvo. Quer continuá-lo?");
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Recuperar").strong()).clicked() {
                app.open_path_as(recovery_path(), true);
                keep = false;
            }
            if ui.button("Descartar").clicked() {
                let _ = std::fs::remove_file(recovery_path());
                keep = false;
            }
        });
    });
    keep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_safe_for_files() {
        assert_eq!(clean_name("  meu/desenho: 1 "), "meu-desenho- 1");
        assert_eq!(clean_name("..."), "Desenho");
        assert_eq!(clean_name("Gato"), "Gato");
    }
}
