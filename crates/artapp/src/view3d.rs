//! Pintura 3D: mostra o modelo do documento com o desenho como textura, gira e aproxima com
//! o dedo, e pinta direto nele com o pincel ou a borracha (o traço vai para a textura).

use std::sync::Arc;

use artapp_core::model3d::{self, Camera, Mesh};
use artapp_core::{PaintSession, StrokePoint};
use eframe::egui::{self, Color32, Pos2, Sense, TextureOptions, Vec2};

use crate::app::{ArtApp, Tool};

/// Lado maior da vista desenhada (ela é ampliada para caber no painel).
const VIEW_MAX: f32 = 900.0;
/// Lado maior da textura usada na vista.
const TEX_MAX: u32 = 1024;
/// Intervalo mínimo entre atualizações da textura enquanto se pinta, em segundos.
const TEX_EVERY: f64 = 0.12;

pub struct View3d {
    source: Arc<String>,
    mesh: Mesh,
    cam: Camera,
    /// Mostrando o modelo (senão, a textura plana na tela normal).
    pub on: bool,
    /// Textura reduzida: (edits de quando foi feita, lado, pixels).
    tex: Option<(u64, u32, u32, Vec<u8>)>,
    tex_at: f64,
    view: Option<model3d::View>,
    /// O que a vista desenhada mostra: (edits da textura, câmera, tamanho).
    shown: Option<(u64, Camera, [u32; 2])>,
    handle: Option<egui::TextureHandle>,
    painting: Option<(PaintSession, (f64, f64))>,
    orbiting: bool,
}

impl View3d {
    fn new(source: Arc<String>) -> Result<Self, String> {
        let mesh = model3d::parse_obj(&source)?;
        Ok(Self {
            source,
            mesh,
            cam: Camera::default(),
            on: true,
            tex: None,
            tex_at: f64::NEG_INFINITY,
            view: None,
            shown: None,
            handle: None,
            painting: None,
            orbiting: false,
        })
    }
}

/// Mantém o estado da vista de acordo com o modelo do documento (abrir outro projeto troca).
pub fn sync(app: &mut ArtApp) {
    match (&app.doc.model, &app.view3d) {
        (None, None) => {}
        (None, Some(_)) => app.view3d = None,
        (Some(m), Some(v)) if Arc::ptr_eq(m, &v.source) => {}
        (Some(m), _) => {
            let m = m.clone();
            match View3d::new(m) {
                Ok(v) => app.view3d = Some(v),
                Err(e) => {
                    app.status = format!("Modelo 3D com problema: {e}");
                    app.doc.model = None;
                    app.view3d = None;
                }
            }
        }
    }
}

/// O modelo está na tela (a vista 3D toma o lugar da tela normal).
pub fn active(app: &ArtApp) -> bool {
    app.view3d.as_ref().is_some_and(|v| v.on)
}

/// Barrinha no alto: alternar entre o modelo e a textura plana, e fechar o modelo.
pub fn bar(app: &mut ArtApp, ctx: &egui::Context, canvas: egui::Rect) {
    let Some(v) = &mut app.view3d else {
        return;
    };
    let mut close = false;
    egui::Area::new(egui::Id::unique("view3d_bar"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::CENTER_TOP)
        .fixed_pos(canvas.center_top() + Vec2::new(0.0, 8.0))
        .show(ctx, |ui| {
            crate::theme::panel_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.selectable_label(v.on, "Modelo 3D").clicked() {
                        v.on = true;
                    }
                    if ui.selectable_label(!v.on, "Textura").clicked() {
                        v.on = false;
                    }
                    if v.on && ui.button("Centralizar").clicked() {
                        v.cam = Camera::default();
                    }
                    if ui
                        .button("Fechar modelo")
                        .on_hover_text("Tira o modelo; o desenho continua como está")
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        });
    if close {
        app.doc.model = None;
        app.view3d = None;
        app.projects.touch();
        app.invalidate_all();
    }
}

/// A textura reduzida, refeita quando o desenho mudou (no máximo a cada [`TEX_EVERY`]).
fn refresh_texture(app: &mut ArtApp, now: f64) {
    let edits = app.projects.edits;
    let Some(v) = &mut app.view3d else {
        return;
    };
    let fresh = v.tex.as_ref().is_some_and(|t| t.0 == edits);
    if fresh || (v.tex.is_some() && now - v.tex_at < TEX_EVERY) {
        return;
    }
    let (tw, th, data) = model3d::texture_of(&app.doc, TEX_MAX);
    v.tex = Some((edits, tw, th, data));
    v.tex_at = now;
}

/// A vista 3D no lugar da tela.
pub fn show(app: &mut ArtApp, ui: &mut egui::Ui) {
    let panel = ui.max_rect();
    app.canvas_rect = panel;
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let resp = ui.allocate_rect(panel, Sense::click_and_drag());
    input(app, &ctx, &resp, panel);
    refresh_texture(app, now);
    let Some(v) = &mut app.view3d else {
        return;
    };
    let k = (VIEW_MAX / panel.width().max(panel.height())).min(1.0);
    let size = [
        (panel.width() * k).round().max(1.0) as u32,
        (panel.height() * k).round().max(1.0) as u32,
    ];
    let tex_edits = v.tex.as_ref().map_or(u64::MAX, |t| t.0);
    let want = (tex_edits, v.cam, size);
    if v.shown != Some(want) {
        let (tw, th, data) = v
            .tex
            .as_ref()
            .map_or((0, 0, &[][..]), |t| (t.1, t.2, t.3.as_slice()));
        let view = model3d::render(&v.mesh, data, tw, th, &v.cam, size[0], size[1]);
        let img = egui::ColorImage::from_rgba_unmultiplied(
            [size[0] as usize, size[1] as usize],
            &view.rgba,
        );
        match &mut v.handle {
            Some(h) => h.set(img, TextureOptions::LINEAR),
            None => v.handle = Some(ctx.load_texture("view3d", img, TextureOptions::LINEAR)),
        }
        v.view = Some(view);
        v.shown = Some(want);
    }
    if let Some(h) = &v.handle {
        ui.painter().image(
            h.id(),
            panel,
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    if v.painting.is_some() || v.tex.as_ref().is_some_and(|t| t.0 != app.projects.edits) {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(TEX_EVERY));
    }
}

/// Ponto da textura (pixels do documento) sob o ponto de tela `p`.
fn texel_at(app: &ArtApp, panel: egui::Rect, p: Pos2) -> Option<(f64, f64)> {
    let v = app.view3d.as_ref()?;
    let view = v.view.as_ref()?;
    let x = (p.x - panel.min.x) / panel.width() * view.width as f32;
    let y = (p.y - panel.min.y) / panel.height() * view.height as f32;
    let uv = view.pick(x, y)?;
    Some(model3d::uv_to_texel(uv, app.doc.width, app.doc.height))
}

fn begin(app: &mut ArtApp) -> Option<PaintSession> {
    let brush = app.stroke_brush();
    let opts = artapp_core::StrokeOptions {
        taper: artapp_core::Taper::default(),
        symmetry: None,
    };
    PaintSession::begin_with(&mut app.doc, &brush, 1.0, opts)
}

fn end(app: &mut ArtApp) {
    if let Some(v) = &mut app.view3d
        && let Some((s, _)) = v.painting.take()
    {
        let r = s.end(&mut app.doc);
        app.invalidate(r);
    }
}

fn input(app: &mut ArtApp, ctx: &egui::Context, resp: &egui::Response, panel: egui::Rect) {
    let (scroll, zoom, multi, secondary) = ctx.input(|i| {
        (
            i.smooth_scroll_delta.y,
            i.zoom_delta(),
            i.multi_touch(),
            i.pointer.secondary_down() || i.pointer.middle_down(),
        )
    });
    let Some(v) = &mut app.view3d else {
        return;
    };
    if resp.hovered() {
        if scroll != 0.0 {
            v.cam.zoom((scroll / 200.0).exp());
        }
        if zoom != 1.0 {
            v.cam.zoom(zoom);
        }
    }
    // Dois dedos: gira (arrastando) e aproxima (pinça).
    if let Some(m) = multi {
        let r = 0.01;
        v.cam
            .orbit(-m.translation_delta.x * r, m.translation_delta.y * r);
        drop_stroke(app);
        return;
    }
    let paints = matches!(app.tool, Tool::Brush | Tool::Eraser);
    let Some(p) = resp.interact_pointer_pos() else {
        return;
    };
    if resp.drag_started() || (resp.is_pointer_button_down_on() && !resp.dragged()) {
        let v = app.view3d.as_mut();
        if let Some(v) = v {
            v.orbiting = !paints || secondary;
        }
    }
    let orbiting = app.view3d.as_ref().is_some_and(|v| v.orbiting);
    if orbiting {
        if resp.dragged()
            && let Some(v) = &mut app.view3d
        {
            let d = resp.drag_delta();
            v.cam.orbit(-d.x * 0.006, d.y * 0.006);
        }
        return;
    }
    let down = resp.is_pointer_button_down_on();
    if !down {
        end(app);
        return;
    }
    let force = ctx.input(|i| {
        i.events.iter().rev().find_map(|e| match e {
            egui::Event::Touch { force, .. } => *force,
            _ => None,
        })
    });
    let pressure = force
        .filter(|f| app.use_pressure && f.is_finite() && *f > 0.0)
        .map_or(1.0, |f| {
            artapp_core::pressure::apply(&app.pressure_curve, f)
        });
    let time = ctx.input(|i| i.time) * 1000.0;
    let Some((x, y)) = texel_at(app, panel, p) else {
        // Saiu do modelo: o traço acaba ali.
        end(app);
        return;
    };
    // Um salto grande na textura é uma costura da UV: começa outro traço.
    let seam = f64::from(app.doc.width.max(app.doc.height)) * 0.08;
    let jump = app
        .view3d
        .as_ref()
        .and_then(|v| v.painting.as_ref())
        .is_some_and(|(_, last)| ((x - last.0).powi(2) + (y - last.1).powi(2)).sqrt() > seam);
    if jump {
        end(app);
    }
    if app.view3d.as_ref().is_some_and(|v| v.painting.is_none()) {
        app.remember_color();
        match begin(app) {
            Some(s) => {
                if let Some(v) = &mut app.view3d {
                    v.painting = Some((s, (x, y)));
                }
            }
            None => {
                app.status = "Escolha uma camada raster visível para pintar no modelo.".into();
                return;
            }
        }
    }
    let pt = StrokePoint {
        time,
        ..StrokePoint::new(x, y, pressure)
    };
    let Some(v) = &mut app.view3d else {
        return;
    };
    if let Some((s, last)) = &mut v.painting {
        let r = s.add(&mut app.doc, &[pt]);
        *last = (x, y);
        app.invalidate(r);
    }
}

/// Dois dedos no meio de um traço: desfaz o começo dele, como na tela normal.
fn drop_stroke(app: &mut ArtApp) {
    if app.view3d.as_ref().is_some_and(|v| v.painting.is_some()) {
        end(app);
    }
}

impl ArtApp {
    /// Abre um modelo OBJ para pintar: tela nova com a textura do `.mtl` (se houver) ou
    /// quadrada e branca.
    pub fn open_model(&mut self, path: &std::path::Path) {
        let text = match std::fs::read(path) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) => {
                self.status = format!("Não foi possível abrir o modelo: {e}");
                return;
            }
        };
        let mesh = match model3d::parse_obj(&text) {
            Ok(m) => m,
            Err(e) => {
                self.status = format!("Não foi possível abrir o modelo: {e}");
                return;
            }
        };
        let dir = path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        let texture = mesh
            .mtllib
            .as_ref()
            .and_then(|m| std::fs::read_to_string(dir.join(m)).ok())
            .and_then(|mtl| model3d::mtl_texture(&mtl))
            .map(|t| dir.join(t))
            .and_then(|p| {
                std::fs::read(&p)
                    .ok()
                    .and_then(|b| artapp_core::edit::decode_image(&b).ok())
            });
        match &texture {
            Some((w, h, _)) => self.new_canvas(*w, *h),
            None => self.new_canvas(2048, 2048),
        }
        if let Some((w, h, px)) = texture {
            let r = self.doc.import_image("Textura", w, h, &px);
            self.invalidate(r);
        }
        self.doc.model = Some(Arc::new(text));
        self.projects.name = path
            .file_stem()
            .map_or("Modelo".into(), |s| s.to_string_lossy().into_owned());
        self.projects.touch();
        sync(self);
        self.status = format!(
            "Modelo aberto ({} triângulos). Arraste com o pincel para pintar; com outra ferramenta \
             ou dois dedos, gira.",
            self.view3d.as_ref().map_or(0, |v| v.mesh.tris.len())
        );
    }
}
