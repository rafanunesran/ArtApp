//! A tela de desenho: mostra o documento como textura, converte toques/mouse em traços e trata
//! zoom e pan (pinça com dois dedos, roda do mouse, botão do meio ou a ferramenta Mão).

use std::collections::HashMap;

use artapp_core::vector::Shape;
use artapp_core::{
    Anchor, BezPath, NodeId, PaintSession, Rect, StrokePoint, SubPath, VPoint, composite, fill,
};
use eframe::egui::{self, Color32, Pos2, Sense, TextureHandle, TextureOptions, TouchPhase, Vec2};

use crate::app::{ArtApp, Tool, color_from_f32, color_to_f32};

/// Um traço que começou há menos que isto quando o segundo dedo encosta é considerado parte do
/// gesto de zoom e é desfeito.
const GESTURE_CANCEL_SECS: f64 = 0.35;
const MIN_ZOOM: f32 = 0.02;
const MAX_ZOOM: f32 = 64.0;

/// O que o dedo/mouse está fazendo desde que encostou.
enum ActiveKind {
    Paint(Box<PaintSession>),
    /// Linha, retângulo ou elipse: do ponto inicial ao atual (pixels da tela de desenho).
    Shape {
        tool: Tool,
        from: VPoint,
        to: VPoint,
    },
    Freehand(Vec<VPoint>),
    /// Arrastando um objeto vetorial: aplicado ao soltar.
    Move {
        id: NodeId,
        from: VPoint,
        to: VPoint,
    },
    /// Puxando as alças do último ponto Bézier.
    BezierHandle,
}

struct ActiveStroke {
    kind: ActiveKind,
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

    draw_overlay(app, &painter, panel);

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
    end_stroke(app);
    let pt = point(app, panel, p, force, time);
    let (x, y) = (pt.x.floor() as i32, pt.y.floor() as i32);
    let vp = VPoint::new(pt.x, pt.y);
    let vector_layer = app.doc.active_is_vector();
    // Tolerância de toque em pixels do documento (uns 12 pontos de tela).
    let tol = 12.0 / app.view.zoom.max(0.01) as f64;
    let kind = match app.tool {
        Tool::Eraser if vector_layer => {
            // Borracha vetorial: apaga o objeto tocado inteiro.
            if let Some(id) = app.doc.vector_hit(vp.x, vp.y, tol) {
                let r = app.doc.vector_delete(id);
                app.invalidate(r);
            }
            None
        }
        Tool::Brush | Tool::Eraser => {
            if vector_layer {
                app.status =
                    "Camada vetorial: use as ferramentas do menu Vetor, ou Rasterizar a camada."
                        .into();
                return;
            }
            let brush = app.stroke_brush();
            let Some(mut session) = PaintSession::begin(&mut app.doc, &brush, app.view.zoom) else {
                app.status = "A camada ativa está oculta.".into();
                return;
            };
            let dirty = session.add(&mut app.doc, &[pt]);
            app.invalidate(dirty);
            if app.tool == Tool::Brush {
                app.remember_color();
            }
            Some(ActiveKind::Paint(Box::new(session)))
        }
        Tool::Fill => {
            let r = if vector_layer {
                let [r, g, b, _] = color_to_f32(app.color);
                app.doc.vector_fill_at(vp.x, vp.y, [r, g, b])
            } else {
                fill::flood_fill(
                    &mut app.doc,
                    x,
                    y,
                    color_to_f32(app.color),
                    app.fill_tolerance,
                )
            };
            app.invalidate(r);
            app.remember_color();
            None
        }
        Tool::Eyedropper => {
            if let Some(c) = composite::sample(&app.doc, x, y) {
                app.color = color_from_f32(c);
                app.remember_color();
                app.tool = app.previous_tool;
            }
            None
        }
        Tool::Hand => None,
        Tool::Select => {
            app.selected = app.doc.vector_hit(vp.x, vp.y, tol);
            if !vector_layer {
                app.status = "Selecionar funciona em camadas vetoriais.".into();
            }
            app.selected.map(|id| ActiveKind::Move {
                id,
                from: vp,
                to: vp,
            })
        }
        Tool::VectorPen | Tool::Line | Tool::Rectangle | Tool::Ellipse => {
            app.ensure_vector_layer();
            Some(if app.tool == Tool::VectorPen {
                ActiveKind::Freehand(vec![vp])
            } else {
                ActiveKind::Shape {
                    tool: app.tool,
                    from: vp,
                    to: vp,
                }
            })
        }
        Tool::Bezier => {
            app.ensure_vector_layer();
            let closes = app.bezier.len() >= 2
                && app.bezier.first().is_some_and(|a| a.p.distance(vp) <= tol);
            if closes {
                app.finish_bezier(true);
                None
            } else {
                app.bezier.push(Anchor::corner(vp));
                Some(ActiveKind::BezierHandle)
            }
        }
    };
    app.view.stroke = kind.map(|kind| ActiveStroke {
        kind,
        touch,
        started: time,
    });
}

fn add_point(app: &mut ArtApp, panel: egui::Rect, p: Pos2, force: Option<f32>, time: f64) {
    let pt = point(app, panel, p, force, time);
    let vp = VPoint::new(pt.x, pt.y);
    let min_handle = 3.0 / app.view.zoom.max(0.01) as f64;
    let Some(s) = &mut app.view.stroke else {
        return;
    };
    match &mut s.kind {
        ActiveKind::Paint(session) => {
            let dirty = session.add(&mut app.doc, &[pt]);
            app.invalidate(dirty);
        }
        ActiveKind::Shape { to, .. } | ActiveKind::Move { to, .. } => *to = vp,
        ActiveKind::Freehand(points) => points.push(vp),
        ActiveKind::BezierHandle => {
            if let Some(a) = app.bezier.last_mut() {
                *a = if a.p.distance(vp) > min_handle {
                    Anchor::smooth(a.p, vp)
                } else {
                    Anchor::corner(a.p)
                };
            }
        }
    }
}

fn end_stroke(app: &mut ArtApp) {
    let Some(s) = app.view.stroke.take() else {
        return;
    };
    let style = app.vector_style();
    let dirty = match s.kind {
        ActiveKind::Paint(session) => session.end(&mut app.doc),
        ActiveKind::Shape { tool, from, to } => {
            if from.distance(to) < 1.0 {
                return;
            }
            let shape = match tool {
                Tool::Line => Shape::Line(from, to),
                Tool::Ellipse => Shape::Ellipse(from, to),
                _ => Shape::Rectangle(from, to),
            };
            app.remember_color();
            app.doc.vector_add(&shape, &style)
        }
        ActiveKind::Freehand(points) => {
            if points.len() < 2 {
                return;
            }
            app.remember_color();
            app.doc.vector_add(
                &Shape::Freehand {
                    points,
                    closed: false,
                },
                &style,
            )
        }
        ActiveKind::Move { id, from, to } => {
            let d = to - from;
            if d.hypot() < 0.5 {
                return;
            }
            app.doc.vector_begin_drag();
            app.doc.vector_move(id, d.x, d.y)
        }
        ActiveKind::BezierHandle => Rect::EMPTY,
    };
    app.invalidate(dirty);
}

fn cancel_stroke(app: &mut ArtApp) {
    let Some(s) = app.view.stroke.take() else {
        return;
    };
    match s.kind {
        ActiveKind::Paint(session) => {
            let dirty = session.cancel(&mut app.doc);
            app.invalidate(dirty);
        }
        ActiveKind::BezierHandle => {
            app.bezier.pop();
        }
        _ => {}
    }
}

/// Converte um ponto do documento para a tela.
fn to_screen(app: &ArtApp, panel: egui::Rect, p: VPoint) -> Pos2 {
    panel.min + app.view.offset + egui::vec2(p.x as f32, p.y as f32) * app.view.zoom
}

/// Desenha um caminho Bézier (achatado em segmentos) na tela.
fn draw_bez(
    app: &ArtApp,
    painter: &egui::Painter,
    panel: egui::Rect,
    path: &BezPath,
    stroke: egui::Stroke,
) {
    let mut pts: Vec<Pos2> = Vec::new();
    let tol = 0.5 / app.view.zoom.max(0.01) as f64;
    let flush = |pts: &mut Vec<Pos2>| {
        if pts.len() >= 2 {
            painter.add(egui::Shape::line(std::mem::take(pts), stroke));
        }
        pts.clear();
    };
    kurbo::flatten(path.iter(), tol, |el| match el {
        kurbo::PathEl::MoveTo(p) => {
            flush(&mut pts);
            pts.push(to_screen(app, panel, p));
        }
        kurbo::PathEl::LineTo(p) => pts.push(to_screen(app, panel, p)),
        kurbo::PathEl::ClosePath => {
            if let Some(first) = pts.first().copied() {
                pts.push(first);
            }
        }
        _ => {}
    });
    flush(&mut pts);
}

/// Prévias das ferramentas vetoriais e a seleção, por cima da tela.
fn draw_overlay(app: &ArtApp, painter: &egui::Painter, panel: egui::Rect) {
    let preview = egui::Stroke::new(
        ((app.vector_width as f32) * app.view.zoom).max(1.0),
        app.color,
    );
    let guide = egui::Stroke::new(1.5, Color32::from_rgb(40, 170, 255));
    if let Some(s) = &app.view.stroke {
        match &s.kind {
            ActiveKind::Shape { tool, from, to } => {
                let shape = match tool {
                    Tool::Line => Shape::Line(*from, *to),
                    Tool::Ellipse => Shape::Ellipse(*from, *to),
                    _ => Shape::Rectangle(*from, *to),
                };
                if let Some(p) = shape.to_path() {
                    draw_bez(app, painter, panel, &p.to_bezpath(), preview);
                }
            }
            ActiveKind::Freehand(points) => {
                let line: Vec<Pos2> = points.iter().map(|p| to_screen(app, panel, *p)).collect();
                painter.add(egui::Shape::line(line, preview));
            }
            ActiveKind::Move { id, from, to } => {
                if let Some(mut path) = app.doc.vector_outline(*id) {
                    path.apply_affine(kurbo::Affine::translate(*to - *from));
                    draw_bez(app, painter, panel, &path, guide);
                }
            }
            _ => {}
        }
    }
    if let Some(path) = app.selected.and_then(|id| app.doc.vector_outline(id)) {
        draw_bez(app, painter, panel, &path, guide);
    }
    if !app.bezier.is_empty() {
        let path =
            artapp_core::vector::Shape::Bezier(SubPath::new(app.bezier.clone(), false)).to_path();
        if let Some(p) = path {
            draw_bez(app, painter, panel, &p.to_bezpath(), preview);
        }
        for (i, a) in app.bezier.iter().enumerate() {
            let c = to_screen(app, panel, a.p);
            if a.h_out.distance(a.p) > 0.01 {
                for h in [a.h_in, a.h_out] {
                    let hp = to_screen(app, panel, h);
                    painter.line_segment([c, hp], guide);
                    painter.circle_filled(hp, 4.0, guide.color);
                }
            }
            let fill = if i == 0 { Color32::WHITE } else { guide.color };
            painter.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(9.0, 9.0)),
                1.0,
                fill,
            );
            painter.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(9.0, 9.0)),
                1.0,
                guide,
                egui::StrokeKind::Outside,
            );
        }
    }
}
