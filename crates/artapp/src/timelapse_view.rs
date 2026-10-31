//! Timelapse: grava uma foto da tela depois de cada alteração (em segundo plano) e mostra a
//! janela para assistir e exportar a gravação.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use artapp_core::timelapse::{self, FPS_RANGE};
use eframe::egui::{self, Align2, RichText, Vec2};

use crate::app::ArtApp;
use crate::platform;
use crate::theme::{color, icon, icon_button_sized, panel_frame};

/// Quanto tempo a tela fica sem mudar antes da foto, em segundos (o fim do traço ainda é
/// refeito logo depois de soltar).
const QUIET: f64 = 0.5;

/// Uma foto sendo feita em segundo plano: a geração do desenho e o JPEG (ou o erro).
type Capture = (u64, Receiver<Result<Vec<u8>, String>>);

pub struct TimelapseState {
    /// Foto sendo feita: (geração do desenho, resultado).
    job: Option<Capture>,
    /// `edits` da última foto, o mais recente visto e quando ele mudou.
    seen_edits: u64,
    latest_edits: u64,
    last: f64,
    /// Muda quando o desenho é trocado (abrir, tela nova): fotos antigas não entram no novo.
    generation: u64,
    pub open: bool,
    pos: usize,
    playing: bool,
    fps: u32,
    next: f64,
    /// Textura mostrada: (foto, quantas fotos havia, textura).
    tex: Option<(usize, usize, egui::TextureHandle)>,
    export: Option<Receiver<Result<PathBuf, String>>>,
}

impl Default for TimelapseState {
    fn default() -> Self {
        Self {
            job: None,
            seen_edits: 0,
            latest_edits: 0,
            last: f64::NEG_INFINITY,
            generation: 0,
            open: false,
            pos: 0,
            playing: false,
            fps: 30,
            next: 0.0,
            tex: None,
            export: None,
        }
    }
}

impl TimelapseState {
    /// O desenho foi trocado por outro.
    pub fn new_document(&mut self, edits: u64) {
        self.generation += 1;
        self.job = None;
        self.seen_edits = edits;
        self.latest_edits = edits;
        self.pos = 0;
        self.playing = false;
        self.tex = None;
    }
}

/// Fotografa a tela quando houve alteração e o dedo/caneta já saiu da tela.
pub fn record(app: &mut ArtApp, ctx: &egui::Context) {
    let now = ctx.input(|i| i.time);
    if let Some((generation, rx)) = &app.timelapse.job {
        match rx.try_recv() {
            Ok(res) => {
                if *generation == app.timelapse.generation
                    && let Ok(jpeg) = res
                {
                    app.doc.timelapse.push(jpeg);
                }
                app.timelapse.job = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => app.timelapse.job = None,
        }
    }
    let edits = app.projects.edits;
    if edits != app.timelapse.latest_edits {
        app.timelapse.latest_edits = edits;
        app.timelapse.last = now;
    }
    if !app.doc.timelapse.enabled || edits == app.timelapse.seen_edits {
        return;
    }
    if ctx.input(|i| i.pointer.any_down()) || now - app.timelapse.last < QUIET {
        ctx.request_repaint_after(std::time::Duration::from_millis(150));
        return;
    }
    app.timelapse.seen_edits = edits;
    let snap = app.doc.snapshot();
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(timelapse::capture(&snap));
    });
    app.timelapse.job = Some((app.timelapse.generation, rx));
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}

/// A janela do timelapse.
pub fn show(app: &mut ArtApp, ctx: &egui::Context) {
    if let Some(rx) = &app.timelapse.export {
        match rx.try_recv() {
            Ok(res) => {
                app.status = match res {
                    Ok(path) => format!("Timelapse salvo em {}", path.display()),
                    Err(e) => format!("Não foi possível salvar o timelapse: {e}"),
                };
                app.timelapse.export = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => app.timelapse.export = None,
        }
    }
    if !app.timelapse.open {
        return;
    }
    let n = app.doc.timelapse.len();
    let st = &mut app.timelapse;
    if n == 0 {
        st.playing = false;
        st.pos = 0;
    } else {
        st.pos = st.pos.min(n - 1);
    }
    if st.playing {
        let now = ctx.input(|i| i.time);
        if now >= st.next {
            if st.pos + 1 >= n {
                st.playing = false;
            } else {
                st.pos += 1;
            }
            st.next = now + 1.0 / f64::from(st.fps.max(1));
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64((st.next - now).max(0.0)));
    }
    // A textura só é refeita quando muda a foto (ou chegou foto nova).
    let stale = !matches!(&st.tex, Some((p, len, _)) if *p == st.pos && *len == n);
    if stale
        && n > 0
        && let Some(img) = app.doc.timelapse.decode(st.pos)
    {
        let size = [img.width() as usize, img.height() as usize];
        let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
        match &mut st.tex {
            Some((p, len, t)) => {
                t.set(ci, egui::TextureOptions::LINEAR);
                *p = st.pos;
                *len = n;
            }
            None => {
                let t = ctx.load_texture("timelapse", ci, egui::TextureOptions::LINEAR);
                st.tex = Some((st.pos, n, t));
            }
        }
    }

    let mut open = true;
    let mut action = None;
    let max = ctx.content_rect().size() * Vec2::new(0.8, 0.55);
    egui::Window::new("Timelapse")
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(panel_frame())
        .open(&mut open)
        .show(ctx, |ui| {
            let st = &mut app.timelapse;
            let tl = &mut app.doc.timelapse;
            let shown = st.tex.as_ref().filter(|_| n > 0).map(|(_, _, t)| {
                let s = t.size_vec2();
                (t.id(), s * (max.x / s.x).min(max.y / s.y).min(1.0))
            });
            ui.set_width(shown.map_or(360.0, |(_, s)| s.x).max(360.0));
            match shown {
                Some(img) => {
                    ui.vertical_centered(|ui| ui.add(egui::Image::new(img)));
                }
                _ => {
                    ui.label(
                        RichText::new(
                            "Ainda não há nada gravado. Desenhe com a gravação ligada e volte aqui.",
                        )
                        .color(color::TEXT_MUTED),
                    );
                }
            }
            ui.horizontal(|ui| {
                let glyph = if st.playing { icon::PAUSE } else { icon::PLAY };
                if icon_button_sized(ui, glyph, st.playing, "Reproduzir", 34.0).clicked() && n > 0
                {
                    st.playing = !st.playing;
                    if st.playing && st.pos + 1 >= n {
                        st.pos = 0;
                    }
                    st.next = 0.0;
                }
                if n > 1 {
                    let mut p = st.pos;
                    ui.spacing_mut().slider_width = (ui.available_width() - 80.0).max(80.0);
                    if ui
                        .add(egui::Slider::new(&mut p, 0..=n - 1).show_value(false))
                        .changed()
                    {
                        st.pos = p;
                        st.playing = false;
                    }
                }
                ui.label(
                    RichText::new(format!("{}/{}", (st.pos + 1).min(n), n))
                        .size(12.0)
                        .color(color::TEXT_MUTED),
                );
            });
            ui.add(egui::Slider::new(&mut st.fps, FPS_RANGE).text("fotos por segundo"));
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut tl.enabled, "Gravar neste desenho");
                let busy = st.export.is_some();
                if ui
                    .add_enabled(n > 0 && !busy, egui::Button::new("Exportar GIF"))
                    .clicked()
                {
                    action = Some(false);
                }
                if ui
                    .add_enabled(n > 0 && !busy, egui::Button::new("Exportar PNG animado"))
                    .clicked()
                {
                    action = Some(true);
                }
                if ui
                    .add_enabled(n > 0, egui::Button::new("Apagar gravação"))
                    .clicked()
                {
                    tl.clear();
                    st.pos = 0;
                    st.playing = false;
                    st.tex = None;
                }
                if busy {
                    ui.spinner();
                }
            });
            ui.label(
                RichText::new(format!(
                    "Uma foto depois de cada alteração, guardada no projeto (até {} fotos; \
                     depois a gravação fica mais espaçada).",
                    timelapse::MAX_FRAMES
                ))
                .size(12.0)
                .color(color::TEXT_MUTED),
            );
        });
    if let Some(apng) = action {
        let tl = app.doc.timelapse.clone();
        let fps = app.timelapse.fps;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let res = if apng {
                timelapse::export_apng(&tl, fps).and_then(|b| platform::save_file("png", &b))
            } else {
                timelapse::export_gif(&tl, fps).and_then(|b| platform::save_file("gif", &b))
            };
            let _ = tx.send(res);
        });
        app.timelapse.export = Some(rx);
        app.status = "Exportando o timelapse…".into();
    }
    if !open {
        app.timelapse.open = false;
        app.timelapse.playing = false;
        app.timelapse.tex = None;
    }
}
