//! A tela de desenho: mostra o documento como textura, converte toques/mouse em traços e trata
//! zoom e pan (pinça com dois dedos, roda do mouse, botão do meio ou a ferramenta Mão).

use std::collections::HashMap;

use artapp_core::{PaintSession, Rect, StrokePoint, composite, fill};
use eframe::egui::{self, Color32, Pos2, Sense, TextureHandle, TextureOptions, TouchPhase, Vec2};

use crate::app::{ArtApp, Tool, color_from_f32, color_to_f32};

/// Um traço que começou há menos que isto quando o segundo dedo encosta é considerado parte do
/// gesto de zoom e é desfeito.
const GESTURE_CANCEL_SECS: f64 = 0.35;
const MIN_ZOOM: f32 = 0.02;
const MAX_ZOOM: f32 = 64.0;

struct ActiveStroke {
    session: PaintSession,
    touch: Option<u64>,
    started: f64,
}

#[derive(Default)]
pub struct CanvasView {
    /// Posição da origem da tela relativa ao canto do painel, em pontos.
    pub offset: Vec2,
    /// Pontos de tela por pixel do documento.
    pub zoom: f32,
    fitted: bool,
    texture: Option<(TextureHandle, [usize; 2])>,
    stroke: Option<ActiveStroke>,
    touches: HashMap<u64, Pos2>,
    /// Dois ou mais dedos: zoom/pan até todos saírem da tela.
    gesturing: bool,
    /// Já chegou algum evento de toque (então os eventos de ponteiro são os simulados e ignoramos).
    saw_touch: bool,
    /// Último ponto do arraste de pan (mouse ou ferramenta Mão).
    pan_from: Option<Pos2>,
}

impl CanvasView {
    pub fn fit(&mut self, panel: egui::Rect, w: u32, h: u32) {
        let (w, h) = (w.max(1) as f32, h.max(1) as f32);
        self.zoom = (panel.width() / w).min(panel.height() / h) * 0.94;
        if !self.zoom.is_finite() || self.zoom <= 0.0 {
            self.zoom = 1.0;
        }
        self.offset = (panel.size() - egui::vec2(w, h) * self.zoom) / 2.0;
        self.fitted = true;
    }

    pub fn request_fit(&mut self) {
        self.fitted = false;
    }

    fn to_canvas(&self, panel: egui::Rect, p: Pos2) -> (f64, f64) {
        let v = (p - panel.min - self.offset) / self.zoom;
        (v.x as f64, v.y as f64)
    }

    fn zoom_at(&mut self, panel: egui::Rect, p: Pos2, factor: f32) {
        let new = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        if !new.is_finite() {
            return;
        }
        let anchor = p - panel.min;
        self.offset = anchor - (anchor - self.offset) * (new / self.zoom);
        self.zoom = new;
    }
}

pub fn show(app: &mut ArtApp, ui: &mut egui::Ui) {
    let panel = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(panel, Sense::click_and_drag());
    if !app.view.fitted {
        app.view.fit(panel, app.doc.width, app.doc.height);
    }
    handle_input(app, ui, panel, response.hovered());
    update_texture(app, ui.ctx());

    let painter = ui.painter_at(panel);
    painter.rect_filled(panel, 0.0, Color32::from_gray(48));
    let size = egui::vec2(app.doc.width as f32, app.doc.height as f32) * app.view.zoom;
    let canvas = egui::Rect::from_min_size(panel.min + app.view.offset, size);
    painter.rect_filled(
        canvas.translate(egui::vec2(3.0, 4.0)),
        0.0,
        Color32::from_black_alpha(90),
    );
    if app.doc.paper[3] < 1.0 {
        checkerboard(&painter, canvas.intersect(panel));
    }
    if let Some((tex, _)) = &app.view.texture {
        let uv = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        painter.image(tex.id(), canvas, uv, Color32::WHITE);
    }

    // Contorno do pincel sob o mouse (no toque não há "hover").
    let hover = ui
        .input(|i| i.pointer.hover_pos())
        .filter(|p| panel.contains(*p));
    if let Some(p) =
        hover.filter(|_| !app.view.saw_touch && matches!(app.tool, Tool::Brush | Tool::Eraser))
    {
        let r = app.stroke_brush().size * 0.5 * app.view.zoom;
        painter.circle_stroke(
            p,
            r.max(1.5),
            egui::Stroke::new(1.0, Color32::from_white_alpha(180)),
        );
        painter.circle_stroke(
            p,
            (r - 1.0).max(0.5),
            egui::Stroke::new(1.0, Color32::from_black_alpha(140)),
        );
    }
}

fn checkerboard(painter: &egui::Painter, r: egui::Rect) {
    painter.rect_filled(r, 0.0, Color32::from_gray(200));
    let s = 16.0;
    let (cols, rows) = (
        (r.width() / s).ceil() as i32,
        (r.height() / s).ceil() as i32,
    );
    if cols > 400 || rows > 400 {
        return;
    }
    for y in 0..rows {
        for x in (y % 2..cols).step_by(2) {
            let cell = egui::Rect::from_min_size(
                r.min + egui::vec2(x as f32 * s, y as f32 * s),
                egui::vec2(s, s),
            )
            .intersect(r);
            painter.rect_filled(cell, 0.0, Color32::from_gray(160));
        }
    }
}

fn update_texture(app: &mut ArtApp, ctx: &egui::Context) {
    let size = [app.doc.width as usize, app.doc.height as usize];
    let options = TextureOptions::LINEAR;
    let fresh = app.view.texture.as_ref().is_none_or(|(_, s)| *s != size);
    let dirty = if fresh {
        app.doc.bounds()
    } else {
        app.take_pending()
    };
    if fresh {
        let _ = app.take_pending();
    }
    if dirty.is_empty() {
        return;
    }
    let (r, px) = composite::composite_rgba8(&app.doc, dirty, true);
    if r.is_empty() {
        return;
    }
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [r.width() as usize, r.height() as usize],
        px.as_flattened(),
    );
    match &mut app.view.texture {
        Some((tex, _)) if !fresh => tex.set_partial([r.x0 as usize, r.y0 as usize], image, options),
        _ => app.view.texture = Some((ctx.load_texture("canvas", image, options), size)),
    }
}

fn handle_input(app: &mut ArtApp, ui: &egui::Ui, panel: egui::Rect, hovered: bool) {
    let ctx = ui.ctx().clone();
    let layer = ui.layer_id();
    let on_canvas = |p: Pos2| panel.contains(p) && ctx.layer_id_at(p).is_none_or(|l| l == layer);
    let (events, time, multi, scroll, pinch) = ctx.input(|i| {
        (
            i.events.clone(),
            i.time,
            i.multi_touch(),
            i.smooth_scroll_delta,
            i.zoom_delta(),
        )
    });

    for ev in &events {
        match ev {
            egui::Event::Touch {
                id,
                phase,
                pos,
                force,
                ..
            } => {
                app.view.saw_touch = true;
                let id = id.0;
                match phase {
                    TouchPhase::Start => {
                        if app.view.touches.is_empty() && !on_canvas(*pos) {
                            continue;
                        }
                        app.view.touches.insert(id, *pos);
                        if app.view.touches.len() >= 2 {
                            start_gesture(app, time);
                        } else if !app.view.gesturing {
                            tool_down(app, panel, *pos, *force, Some(id), time);
                        }
                    }
                    TouchPhase::Move => {
                        let Some(prev) = app.view.touches.insert(id, *pos) else {
                            continue;
                        };
                        if app.view.gesturing {
                            continue;
                        }
                        if app.tool == Tool::Hand {
                            app.view.offset += *pos - prev;
                        }
                        if app
                            .view
                            .stroke
                            .as_ref()
                            .is_some_and(|s| s.touch == Some(id))
                        {
                            add_point(app, panel, *pos, *force, time);
                        }
                    }
                    TouchPhase::End | TouchPhase::Cancel => {
                        app.view.touches.remove(&id);
                        if app
                            .view
                            .stroke
                            .as_ref()
                            .is_some_and(|s| s.touch == Some(id))
                        {
                            if *phase == TouchPhase::Cancel {
                                cancel_stroke(app)
                            } else {
                                end_stroke(app)
                            }
                        }
                        if app.view.touches.is_empty() {
                            app.view.gesturing = false;
                        }
                    }
                }
            }
            // Com toque, o egui também manda eventos de ponteiro simulados: só usamos os de toque.
            egui::Event::PointerButton {
                pos,
                button,
                pressed,
                ..
            } if !app.view.saw_touch => match (button, pressed) {
                (egui::PointerButton::Primary, true) if on_canvas(*pos) => {
                    if app.tool == Tool::Hand {
                        app.view.pan_from = Some(*pos);
                    } else {
                        tool_down(app, panel, *pos, None, None, time);
                    }
                }
                (egui::PointerButton::Primary, false) => {
                    app.view.pan_from = None;
                    end_stroke(app);
                }
                (egui::PointerButton::Middle | egui::PointerButton::Secondary, true)
                    if on_canvas(*pos) =>
                {
                    app.view.pan_from = Some(*pos)
                }
                (egui::PointerButton::Middle | egui::PointerButton::Secondary, false) => {
                    app.view.pan_from = None
                }
                _ => {}
            },
            egui::Event::PointerMoved(pos) if !app.view.saw_touch => {
                if let Some(from) = app.view.pan_from {
                    app.view.offset += *pos - from;
                    app.view.pan_from = Some(*pos);
                } else if app.view.stroke.is_some() {
                    add_point(app, panel, *pos, None, time);
                }
            }
            _ => {}
        }
    }

    if app.view.gesturing {
        if let Some(mt) = multi {
            app.view.offset += mt.translation_delta;
            app.view.zoom_at(panel, mt.center_pos, mt.zoom_delta);
        }
    } else if hovered {
        // Roda do mouse: zoom em volta do cursor. Pinça de touchpad / Ctrl+roda chega como zoom_delta.
        let hover = ctx
            .input(|i| i.pointer.hover_pos())
            .unwrap_or(panel.center());
        let factor = if pinch != 1.0 {
            pinch
        } else {
            (scroll.y * 0.0025).exp()
        };
        if factor != 1.0 {
            app.view.zoom_at(panel, hover, factor);
        }
    }
}

fn start_gesture(app: &mut ArtApp, time: f64) {
    app.view.gesturing = true;
    if let Some(s) = &app.view.stroke {
        if time - s.started < GESTURE_CANCEL_SECS {
            cancel_stroke(app);
        } else {
            end_stroke(app);
        }
    }
}

fn point(app: &ArtApp, panel: egui::Rect, p: Pos2, force: Option<f32>, time: f64) -> StrokePoint {
    let (x, y) = app.view.to_canvas(panel, p);
    let pressure = force
        .filter(|f| app.use_pressure && f.is_finite() && *f > 0.0)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    StrokePoint {
        time: time * 1000.0,
        ..StrokePoint::new(x, y, pressure)
    }
}

fn tool_down(
    app: &mut ArtApp,
    panel: egui::Rect,
    p: Pos2,
    force: Option<f32>,
    touch: Option<u64>,
    time: f64,
) {
    let pt = point(app, panel, p, force, time);
    let (x, y) = (pt.x.floor() as i32, pt.y.floor() as i32);
    match app.tool {
        Tool::Brush | Tool::Eraser => {
            end_stroke(app);
            let brush = app.stroke_brush();
            let Some(mut session) = PaintSession::begin(&mut app.doc, &brush, app.view.zoom) else {
                app.status = "A camada ativa está oculta.".into();
                return;
            };
            let dirty = session.add(&mut app.doc, &[pt]);
            app.invalidate(dirty);
            app.view.stroke = Some(ActiveStroke {
                session,
                touch,
                started: time,
            });
            if app.tool == Tool::Brush {
                app.remember_color();
            }
        }
        Tool::Fill => {
            let r = fill::flood_fill(
                &mut app.doc,
                x,
                y,
                color_to_f32(app.color),
                app.fill_tolerance,
            );
            app.invalidate(r);
            app.remember_color();
        }
        Tool::Eyedropper => {
            if let Some(c) = composite::sample(&app.doc, x, y) {
                app.color = color_from_f32(c);
                app.remember_color();
                app.tool = app.previous_tool;
            }
        }
        Tool::Hand => {}
    }
}

fn add_point(app: &mut ArtApp, panel: egui::Rect, p: Pos2, force: Option<f32>, time: f64) {
    let pt = point(app, panel, p, force, time);
    if let Some(s) = &mut app.view.stroke {
        let dirty = s.session.add(&mut app.doc, &[pt]);
        app.invalidate(dirty);
    }
}

fn end_stroke(app: &mut ArtApp) {
    if let Some(s) = app.view.stroke.take() {
        let dirty: Rect = s.session.end(&mut app.doc);
        app.invalidate(dirty);
    }
}

fn cancel_stroke(app: &mut ArtApp) {
    if let Some(s) = app.view.stroke.take() {
        let dirty = s.session.cancel(&mut app.doc);
        app.invalidate(dirty);
    }
}
