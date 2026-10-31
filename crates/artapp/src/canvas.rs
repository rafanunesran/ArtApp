//! A tela de desenho: mostra o documento como textura, converte toques/mouse em traços e trata
//! zoom e pan (pinça com dois dedos, roda do mouse, botão do meio ou a ferramenta Mão).

use std::collections::HashMap;

use artapp_core::vector::Shape;
use artapp_core::vectorkit::PointerKind;
use artapp_core::{
    Anchor, BezPath, NodeId, PaintSession, Rect, StrokePoint, SubPath, VPoint, composite, fill,
};
use eframe::egui::{self, Color32, Pos2, Sense, TextureHandle, TextureOptions, TouchPhase, Vec2};

use crate::app::{ArtApp, Tool, color_from_f32, color_to_f32};

/// Um traço que começou há menos que isto quando o segundo dedo encosta é considerado parte do
/// gesto de zoom e é desfeito.
const GESTURE_CANCEL_SECS: f64 = 0.35;
/// Toque de dois (desfazer) ou três dedos (refazer): todos saem antes disto e quase sem mexer.
const TAP_MAX_SECS: f64 = 0.45;
const TAP_MAX_MOVE: f32 = 24.0;
/// Os painéis somem quando o traço passa disto (antes pode ser o começo de um gesto de dois dedos).
const PANEL_CLOSE_SECS: f64 = 0.15;
/// Segurar parado por isto: conta-gotas (no começo do traço) ou forma rápida (no fim).
const HOLD_SECS: f64 = 0.55;
/// "Parado" é mexer menos que isto (pontos de tela).
const HOLD_MOVE: f32 = 5.0;
/// Segurar dois ou três dedos: desfaz/refaz em sequência a partir disto, nesse ritmo.
const REPEAT_AFTER: f64 = 0.65;
const REPEAT_EVERY: f64 = 0.18;
/// Esfregar três dedos: idas e voltas para limpar a camada.
const SCRUB_REVERSALS: u32 = 3;
const SCRUB_MIN_RUN: f32 = 25.0;
const MIN_ZOOM: f32 = 0.02;
const MAX_ZOOM: f32 = 64.0;

/// O que o dedo/mouse está fazendo desde que encostou.
enum ActiveKind {
    Paint(Box<PaintSession>),
    /// Ferramenta Misturar.
    Mix(Box<artapp_core::mix::MixSession>),
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
    /// Seleção retangular ou elíptica: do ponto inicial ao atual.
    Marquee {
        tool: Tool,
        from: VPoint,
        to: VPoint,
    },
    /// Laço: os pontos do contorno.
    Lasso(Vec<VPoint>),
    /// Mover camada: aplicado ao soltar.
    MoveLayer {
        from: VPoint,
        to: VPoint,
    },
    /// Degradê: do ponto inicial ao atual.
    Gradient {
        from: VPoint,
        to: VPoint,
    },
    Liquify(Box<artapp_core::liquify::LiquifySession>),
    /// Uma ferramenta do VectorCraft: o último ponto, para o soltar.
    Kit(VPoint, f32),
    /// Ferramenta Régua: arrastando uma alça (`Some`) ou criando a régua a partir de `from`.
    Ruler {
        handle: Option<usize>,
        from: VPoint,
    },
    /// Pincel corretivo: o traço marca a área; ao soltar, ela é refeita.
    Heal(Box<artapp_core::heal::HealSession>),
    /// Transformar na tela: a alça pegada e o quadrilátero de quando o toque começou.
    Transform {
        handle: crate::transform_tool::Handle,
        from: VPoint,
        start: artapp_core::transform::Quad,
    },
}

/// Acompanha os dedos desde que o primeiro encostou, para reconhecer o toque de vários dedos.
#[derive(Default)]
struct TapGesture {
    started: f64,
    /// Máximo de dedos juntos na tela.
    fingers: usize,
    origins: HashMap<u64, Pos2>,
    /// Quanto o dedo que mais andou se afastou de onde encostou, em pontos.
    moved: f32,
    /// Desfazer/refazer em sequência (segurando): próximo instante e se já aconteceu.
    repeat_at: f64,
    repeated: bool,
    /// Esfregar com três dedos: x do centro, sentido atual, quanto andou nele, idas e voltas.
    scrub_x: Option<f32>,
    scrub_dir: f32,
    scrub_run: f32,
    reversals: u32,
    scrubbed: bool,
}

struct ActiveStroke {
    kind: ActiveKind,
    touch: Option<u64>,
    started: f64,
    /// Pontos do traço do pincel (para a forma rápida).
    path: Vec<StrokePoint>,
    /// Onde e quando o traço parou de se mexer.
    still: (Pos2, f64),
    /// Quanto o traço andou na tela, em pontos.
    travel: f32,
    /// A forma rápida em que o traço virou.
    quick: Option<artapp_core::quickshape::QuickShape>,
}

impl ActiveStroke {
    fn new(kind: ActiveKind, touch: Option<u64>, time: f64, at: Pos2) -> Self {
        Self {
            kind,
            touch,
            started: time,
            path: Vec::new(),
            still: (at, time),
            travel: 0.0,
            quick: None,
        }
    }
}

#[derive(Default)]
pub struct CanvasView {
    /// Posição da origem da tela relativa ao canto do painel, em pontos.
    pub offset: Vec2,
    /// Pontos de tela por pixel do documento.
    pub zoom: f32,
    /// Giro da vista em radianos (só a visualização; o desenho não muda).
    pub rotation: f32,
    /// Espelha a vista na horizontal (para conferir a proporção, como no Clip Studio).
    pub flip: bool,
    fitted: bool,
    /// Tamanho do painel no último encaixe.
    fitted_for: Vec2,
    texture: Option<(TextureHandle, [usize; 2])>,
    stroke: Option<ActiveStroke>,
    touches: HashMap<u64, Pos2>,
    /// Dois ou mais dedos: zoom/pan até todos saírem da tela.
    gesturing: bool,
    /// Já chegou algum evento de toque (então os eventos de ponteiro são os simulados e ignoramos).
    saw_touch: bool,
    /// Último ponto do arraste de pan (mouse ou ferramenta Mão).
    pan_from: Option<Pos2>,
    /// Ferramenta trocada pela caneta (botão lateral ou ponta de borracha): o toque que trocou e
    /// a ferramenta a que volta quando ele sair da tela.
    pen_swap: Option<(u64, Tool)>,
    tap: TapGesture,
    /// A caneta já tocou a tela nesta sessão (modo automático de rejeição de palma).
    pen_seen: bool,
    /// O toque da caneta encostada agora: dedos que chegam enquanto ela está na tela são palma.
    pen_touch: Option<u64>,
}

impl CanvasView {
    pub fn fit(&mut self, panel: egui::Rect, w: u32, h: u32) {
        let (w, h) = (w.max(1) as f32, h.max(1) as f32);
        self.zoom = (panel.width() / w).min(panel.height() / h) * 0.94;
        if !self.zoom.is_finite() || self.zoom <= 0.0 {
            self.zoom = 1.0;
        }
        self.rotation = 0.0;
        // Com o espelho, a origem fica no canto direito (o desenho continua no mesmo lugar).
        let x0 = if self.flip { w * self.zoom } else { 0.0 };
        self.offset = (panel.size() - egui::vec2(w, h) * self.zoom) / 2.0 + egui::vec2(x0, 0.0);
        self.fitted = true;
        self.fitted_for = panel.size();
    }

    pub fn request_fit(&mut self) {
        self.fitted = false;
    }

    /// Espelho e giro aplicados a um vetor em pixels da tela (já com o zoom).
    fn orient(&self, v: Vec2) -> Vec2 {
        let v = if self.flip { egui::vec2(-v.x, v.y) } else { v };
        let (sin, cos) = self.rotation.sin_cos();
        egui::vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
    }

    fn unorient(&self, v: Vec2) -> Vec2 {
        let (sin, cos) = (-self.rotation).sin_cos();
        let v = egui::vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
        if self.flip { egui::vec2(-v.x, v.y) } else { v }
    }

    /// Ponto do documento (pixels) para a tela.
    pub fn doc_to_screen(&self, panel: egui::Rect, x: f32, y: f32) -> Pos2 {
        panel.min + self.offset + self.orient(egui::vec2(x, y) * self.zoom)
    }

    fn to_canvas(&self, panel: egui::Rect, p: Pos2) -> (f64, f64) {
        let v = self.unorient(p - panel.min - self.offset) / self.zoom;
        (v.x as f64, v.y as f64)
    }

    /// Ajusta o deslocamento para que o ponto do documento `d` fique sob o ponto de tela `p`.
    fn pin(&mut self, panel: egui::Rect, d: (f64, f64), p: Pos2) {
        self.offset = (p - panel.min) - self.orient(egui::vec2(d.0 as f32, d.1 as f32) * self.zoom);
    }

    fn zoom_at(&mut self, panel: egui::Rect, p: Pos2, factor: f32) {
        let new = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        if !new.is_finite() {
            return;
        }
        let d = self.to_canvas(panel, p);
        self.zoom = new;
        self.pin(panel, d, p);
    }

    /// Gira a vista em volta do ponto de tela `p`.
    pub fn rotate_at(&mut self, panel: egui::Rect, p: Pos2, radians: f32) {
        let d = self.to_canvas(panel, p);
        self.rotation = (self.rotation + radians).rem_euclid(std::f32::consts::TAU);
        // Perto de reto, encaixa no ângulo reto (fica fácil voltar ao normal).
        let quarter = std::f32::consts::FRAC_PI_2;
        let snapped = (self.rotation / quarter).round() * quarter;
        if (self.rotation - snapped).abs() < 0.03 {
            self.rotation = snapped.rem_euclid(std::f32::consts::TAU);
        }
        self.pin(panel, d, p);
    }

    /// Liga/desliga o espelho, mantendo o centro da vista no mesmo ponto do desenho.
    pub fn toggle_flip(&mut self, panel: egui::Rect) {
        let c = panel.center();
        let d = self.to_canvas(panel, c);
        self.flip = !self.flip;
        // Espelhar inverte o sentido do giro, como num espelho de verdade.
        self.rotation = (-self.rotation).rem_euclid(std::f32::consts::TAU);
        self.pin(panel, d, c);
    }

    /// Os quatro cantos da tela de desenho na tela (no sentido horário a partir da origem).
    fn corners(&self, panel: egui::Rect, w: u32, h: u32) -> [Pos2; 4] {
        let (w, h) = (w as f32, h as f32);
        [
            self.doc_to_screen(panel, 0.0, 0.0),
            self.doc_to_screen(panel, w, 0.0),
            self.doc_to_screen(panel, w, h),
            self.doc_to_screen(panel, 0.0, h),
        ]
    }
}

pub fn show(app: &mut ArtApp, ui: &mut egui::Ui) {
    let panel = ui.available_rect_before_wrap();
    app.canvas_rect = panel;
    let response = ui.allocate_rect(panel, Sense::click_and_drag());
    // Girar o aparelho (ou redimensionar a janela) reencaixa a tela.
    if (panel.size() - app.view.fitted_for).length() > 1.0 {
        app.view.fitted = false;
    }
    if !app.view.fitted {
        app.view.fit(panel, app.doc.width, app.doc.height);
    }
    handle_input(app, ui, panel, response.hovered());
    check_holds(app, ui.ctx(), panel);
    update_texture(app, ui.ctx());

    let painter = ui.painter_at(panel);
    painter.rect_filled(panel, 0.0, crate::theme::color::BACKDROP);
    let corners = app.view.corners(panel, app.doc.width, app.doc.height);
    let straight = app.view.rotation == 0.0 && !app.view.flip;
    let quad = |tex: egui::TextureId, pts: [Pos2; 4], tint: Color32| {
        let mut mesh = egui::Mesh::with_texture(tex);
        let uvs = [
            Pos2::new(0.0, 0.0),
            Pos2::new(1.0, 0.0),
            Pos2::new(1.0, 1.0),
            Pos2::new(0.0, 1.0),
        ];
        let plain = tex == egui::TextureId::default();
        for (p, uv) in pts.into_iter().zip(uvs) {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: p,
                // Sem textura: o canto branco do atlas de fontes, que só tinge.
                uv: if plain { egui::epaint::WHITE_UV } else { uv },
                color: tint,
            });
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(mesh));
    };
    let shadow = corners.map(|p| p + egui::vec2(3.0, 4.0));
    quad(
        egui::TextureId::default(),
        shadow,
        Color32::from_black_alpha(90),
    );
    if app.doc.paper[3] < 1.0 {
        if straight {
            let canvas = egui::Rect::from_two_pos(corners[0], corners[2]);
            checkerboard(&painter, canvas.intersect(panel));
        } else {
            quad(egui::TextureId::default(), corners, Color32::from_gray(185));
        }
    }
    if let Some((tex, _)) = &app.view.texture {
        quad(tex.id(), corners, Color32::WHITE);
    }

    app.kit.zoom = app.view.zoom as f64;
    app.kit_overlays = if app.tool == Tool::VectorKit {
        app.kit.overlays(&app.doc)
    } else {
        Vec::new()
    };
    crate::transform_tool::draw(app, ui.ctx(), &painter, panel);
    if app.show_grid {
        draw_grid(app, &painter, panel);
    }
    draw_overlay(app, &painter, panel);
    draw_ruler(app, &painter, panel);
    draw_kit_overlays(app, &painter, panel);
    if app.tool == Tool::Blend
        && app.mix_kind == artapp_core::mix::MixKind::Clone
        && let Some([x, y]) = app.clone_source
    {
        let c = app.view.doc_to_screen(panel, x as f32, y as f32);
        let st = egui::Stroke::new(1.5, Color32::from_rgb(40, 170, 255));
        painter.circle_stroke(c, 8.0, st);
        painter.line_segment([c - egui::vec2(12.0, 0.0), c + egui::vec2(12.0, 0.0)], st);
        painter.line_segment([c - egui::vec2(0.0, 12.0), c + egui::vec2(0.0, 12.0)], st);
    }

    // Contorno do pincel sob o mouse (no toque não há "hover").
    let hover = ui
        .input(|i| i.pointer.hover_pos())
        .filter(|p| panel.contains(*p));
    if let Some(p) = hover.filter(|_| {
        !app.view.saw_touch
            && matches!(
                app.tool,
                Tool::Brush | Tool::Eraser | Tool::Blend | Tool::Liquify
            )
    }) {
        let size = if app.tool == Tool::Liquify {
            app.liquify_size
        } else if app.tool == Tool::Blend {
            app.mix_brush.size
        } else {
            app.stroke_brush().size
        };
        let r = size * 0.5 * app.view.zoom;
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
                        let pen = crate::platform::pen_info(id);
                        if pen.palm {
                            continue;
                        }
                        if pen.stylus {
                            app.view.pen_seen = true;
                        }
                        let finger_blocked = !pen.stylus && fingers_blocked(app);
                        // Palma: com a caneta encostada, outros toques não contam.
                        if finger_blocked && app.view.pen_touch.is_some() {
                            continue;
                        }
                        if app.view.touches.is_empty() && !on_canvas(*pos) {
                            continue;
                        }
                        if pen.stylus && fingers_blocked(app) && !app.view.touches.is_empty() {
                            // A mão já estava apoiada quando a caneta encostou: esquece os dedos.
                            cancel_stroke(app);
                            app.view.touches.clear();
                            app.view.gesturing = false;
                        }
                        if pen.stylus {
                            app.view.pen_touch = Some(id);
                        }
                        if app.view.touches.is_empty() {
                            app.view.tap = TapGesture {
                                started: time,
                                ..Default::default()
                            };
                        }
                        app.view.touches.insert(id, *pos);
                        let tap = &mut app.view.tap;
                        tap.origins.insert(id, *pos);
                        tap.fingers = tap.fingers.max(app.view.touches.len());
                        if app.view.touches.len() >= 2 {
                            start_gesture(app, time);
                        } else if !app.view.gesturing && !finger_blocked {
                            pen_swap(app, id);
                            tool_down(app, panel, *pos, *force, Some(id), time);
                        }
                    }
                    TouchPhase::Move => {
                        // Toques ignorados (palma) nunca entram em `touches`.
                        let Some(prev) = app.view.touches.get(&id).copied() else {
                            continue;
                        };
                        app.view.touches.insert(id, *pos);
                        let tap = &mut app.view.tap;
                        if let Some(o) = tap.origins.get(&id) {
                            tap.moved = tap.moved.max(o.distance(*pos));
                        }
                        if app.view.touches.len() >= 3 {
                            scrub(app);
                        }
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
                        if app.view.pen_touch == Some(id) {
                            app.view.pen_touch = None;
                        }
                        if app.view.touches.remove(&id).is_none() {
                            continue;
                        }
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
                        if let Some((_, prev)) = app.view.pen_swap.filter(|(t, _)| *t == id) {
                            app.tool = prev;
                            app.view.pen_swap = None;
                        }
                        if app.view.touches.is_empty() {
                            app.view.gesturing = false;
                            if *phase == TouchPhase::End {
                                finish_tap(app, time);
                            }
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
            if app.gesture_rotate && mt.rotation_delta.abs() > 0.0 {
                app.view.rotate_at(panel, mt.center_pos, mt.rotation_delta);
            }
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

/// Se os dedos só mexem na vista (rejeição de palma), conforme o ajuste e se já houve caneta.
fn fingers_blocked(app: &ArtApp) -> bool {
    match app.finger_mode {
        crate::app::FingerMode::Draw => false,
        crate::app::FingerMode::PenOnly => true,
        crate::app::FingerMode::Auto => app.view.pen_seen,
    }
}

/// Caneta com o botão lateral apertado (ou virada na ponta de borracha): troca a ferramenta até
/// esse toque sair da tela.
fn pen_swap(app: &mut ArtApp, id: u64) {
    let pen = crate::platform::pen_info(id);
    let tool = if pen.eraser {
        Some(Tool::Eraser)
    } else if pen.button {
        app.pen_button_tool
    } else {
        None
    };
    if let Some(t) = tool.filter(|t| *t != app.tool) {
        app.view.pen_swap = Some((id, app.tool));
        app.tool = t;
    }
}

/// Todos os dedos saíram: um toque rápido de dois dedos desfaz, de três refaz (como no Clip
/// Studio e no Procreate).
fn finish_tap(app: &mut ArtApp, time: f64) {
    let tap = std::mem::take(&mut app.view.tap);
    if tap.repeated || tap.scrubbed {
        return;
    }
    if tap.fingers == 4 && time - tap.started <= TAP_MAX_SECS && tap.moved <= TAP_MAX_MOVE {
        // Quatro dedos: esconde ou mostra a interface (tela cheia, como no Procreate).
        app.hide_ui = !app.hide_ui;
        return;
    }
    if !app.tap_undo || time - tap.started > TAP_MAX_SECS || tap.moved > TAP_MAX_MOVE {
        return;
    }
    match tap.fingers {
        2 if app.doc.can_undo() => {
            app.undo();
            app.status = "Desfeito".into();
        }
        3 if app.doc.can_redo() => {
            app.redo();
            app.status = "Refeito".into();
        }
        _ => {}
    }
}

/// Esfregar três dedos de um lado para o outro limpa a camada (Procreate).
fn scrub(app: &mut ArtApp) {
    let n = app.view.touches.len() as f32;
    let x = app.view.touches.values().map(|p| p.x).sum::<f32>() / n;
    let tap = &mut app.view.tap;
    if tap.scrubbed {
        return;
    }
    let Some(last) = tap.scrub_x.replace(x) else {
        return;
    };
    let dx = x - last;
    if dx == 0.0 {
        return;
    }
    if dx.signum() == tap.scrub_dir {
        tap.scrub_run += dx.abs();
    } else {
        if tap.scrub_run >= SCRUB_MIN_RUN {
            tap.reversals += 1;
        }
        tap.scrub_dir = dx.signum();
        tap.scrub_run = dx.abs();
    }
    if tap.reversals >= SCRUB_REVERSALS {
        tap.scrubbed = true;
        app.doc.clear_layer();
        app.invalidate_all();
        app.status = "Camada limpa".into();
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

/// ColorDrop: a cor principal solta no ponto de tela `at` preenche a área ali (como o balde,
/// com a tolerância do balde).
pub fn color_drop(app: &mut ArtApp, at: Pos2) {
    let panel = app.canvas_rect;
    if !panel.contains(at) {
        return;
    }
    let (x, y) = app.view.to_canvas(panel, at);
    let r = if app.doc.active_is_vector() {
        let [r, g, b, _] = color_to_f32(app.color);
        app.doc.vector_fill_at(x, y, [r, g, b])
    } else {
        let opts = fill::FillOptions {
            tolerance: app.fill_tolerance,
            close_gap: app.fill_close_gap,
            expand: app.fill_expand,
        };
        fill::flood_fill_with(
            &mut app.doc,
            x.floor() as i32,
            y.floor() as i32,
            color_to_f32(app.color),
            &opts,
        )
    };
    app.invalidate(r);
    app.remember_color();
    app.status = "Preenchido (ColorDrop)".into();
}

/// Começa um traço de pincel ou borracha com os ajustes atuais.
fn begin_paint(app: &mut ArtApp) -> Option<PaintSession> {
    let brush = app.stroke_brush();
    let taper = if app.tool == Tool::Brush {
        app.taper
    } else {
        artapp_core::Taper::default()
    };
    let opts = artapp_core::StrokeOptions {
        taper,
        symmetry: app.symmetry_ruler(),
    };
    PaintSession::begin_with(&mut app.doc, &brush, app.view.zoom, opts)
}

/// Refaz o traço atual como a forma rápida `shape`, com o mesmo pincel e a pressão média.
fn redraw_quick(app: &mut ArtApp, shape: artapp_core::quickshape::QuickShape) {
    let Some(mut s) = app.view.stroke.take() else {
        return;
    };
    let ActiveKind::Paint(session) = s.kind else {
        app.view.stroke = Some(s);
        return;
    };
    let dirty = session.cancel(&mut app.doc);
    app.invalidate(dirty);
    let Some(mut session) = begin_paint(app) else {
        return;
    };
    let pressure = if s.path.is_empty() {
        1.0
    } else {
        s.path.iter().map(|p| p.pressure).sum::<f32>() / s.path.len() as f32
    };
    let t0 = s.path.first().map_or(0.0, |p| p.time);
    let pts: Vec<StrokePoint> = shape
        .sample(1.0)
        .into_iter()
        .enumerate()
        .map(|(i, [x, y])| StrokePoint {
            time: t0 + i as f64,
            ..StrokePoint::new(x, y, pressure)
        })
        .collect();
    let dirty = session.add(&mut app.doc, &pts);
    app.invalidate(dirty);
    s.kind = ActiveKind::Paint(Box::new(session));
    s.quick = Some(shape);
    app.view.stroke = Some(s);
}

/// A cada quadro: segurar parado vira conta-gotas (no começo do traço) ou forma rápida (no fim);
/// segurar dois ou três dedos desfaz/refaz em sequência.
fn check_holds(app: &mut ArtApp, ctx: &egui::Context, panel: egui::Rect) {
    let time = ctx.input(|i| i.time);
    // Desfazer/refazer em sequência.
    let tap = &app.view.tap;
    if app.tap_undo
        && matches!(tap.fingers, 2 | 3)
        && app.view.touches.len() == tap.fingers
        && tap.moved <= TAP_MAX_MOVE
        && !tap.scrubbed
    {
        ctx.request_repaint_after(std::time::Duration::from_millis(60));
        if time - tap.started > REPEAT_AFTER && time >= tap.repeat_at {
            let undo = tap.fingers == 2;
            app.view.tap.repeat_at = time + REPEAT_EVERY;
            app.view.tap.repeated = true;
            if undo && app.doc.can_undo() {
                app.undo();
                app.status = "Desfazendo…".into();
            } else if !undo && app.doc.can_redo() {
                app.redo();
                app.status = "Refazendo…".into();
            }
        }
    }
    let Some(s) = &app.view.stroke else {
        return;
    };
    if !matches!(s.kind, ActiveKind::Paint(_)) || s.quick.is_some() {
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(80));
    if time - s.still.1 < HOLD_SECS {
        return;
    }
    let (at, travel) = (s.still.0, s.travel);
    if travel < HOLD_MOVE * 2.0 {
        if app.hold_quickmenu {
            cancel_stroke(app);
            app.quick_menu = Some(at);
            return;
        }
        if !app.hold_eyedropper {
            return;
        }
        // Segurou sem desenhar: pega a cor do desenho naquele ponto.
        cancel_stroke(app);
        let (x, y) = app.view.to_canvas(panel, at);
        if let Some(c) = composite::sample(&app.doc, x.floor() as i32, y.floor() as i32) {
            app.color = color_from_f32(c);
            app.remember_color();
            app.status = "Cor pega".into();
        }
        return;
    }
    if !app.quick_shape || !matches!(app.tool, Tool::Brush | Tool::Eraser) {
        return;
    }
    let pts: Vec<[f64; 2]> = s.path.iter().map(|p| [p.x, p.y]).collect();
    if let Some(shape) = artapp_core::quickshape::detect(&pts) {
        app.status = match &shape {
            artapp_core::quickshape::QuickShape::Line(..) => "Forma rápida: reta",
            artapp_core::quickshape::QuickShape::Arc { .. } => "Forma rápida: arco",
            artapp_core::quickshape::QuickShape::Ellipse { .. } => "Forma rápida: elipse",
            artapp_core::quickshape::QuickShape::Polygon { .. } => "Forma rápida: polígono",
        }
        .into();
        redraw_quick(app, shape);
    }
}

/// Prende o ponto do pincel, da borracha ou do misturar à régua especial, se ligada.
fn snap_to_ruler(app: &mut ArtApp, pt: &mut StrokePoint) {
    if !app.ruler_snap || !matches!(app.tool, Tool::Brush | Tool::Eraser | Tool::Blend) {
        return;
    }
    let Some(ruler) = &app.ruler else {
        return;
    };
    // A perspectiva decide o ponto de fuga depois de uns 12 pontos de tela.
    let decide = 12.0 / f64::from(app.view.zoom.max(0.01));
    let snap = app.ruler_live.get_or_insert_with(|| {
        artapp_core::ruler::Snap::new(ruler, [pt.x, pt.y]).with_decide(decide)
    });
    let [x, y] = snap.apply(ruler, [pt.x, pt.y]);
    pt.x = x;
    pt.y = y;
}

/// Começa a mexer na régua: pega a alça tocada ou cria uma régua nova do tipo escolhido.
fn ruler_down(app: &mut ArtApp, vp: VPoint, tol: f64) -> ActiveKind {
    use artapp_core::ruler::Ruler;
    let handle = app.ruler.as_ref().and_then(|r| {
        r.handles()
            .iter()
            .position(|h| (h[0] - vp.x).hypot(h[1] - vp.y) <= tol * 1.5)
    });
    if handle.is_none() {
        let p = [vp.x, vp.y];
        app.ruler = Some(match (app.ruler_kind, app.ruler.take()) {
            (1, _) => Ruler::Parallel { angle: 0.0 },
            (2, _) => Ruler::Concentric { center: p },
            // Perspectiva: cada toque fora das alças põe mais um ponto de fuga (até três).
            (3, Some(Ruler::Perspective { mut vanishing })) => {
                if vanishing.len() >= 3 {
                    vanishing.remove(0);
                }
                vanishing.push(p);
                Ruler::Perspective { vanishing }
            }
            (3, _) => Ruler::Perspective { vanishing: vec![p] },
            _ => Ruler::Line { a: p, b: p },
        });
        if app.ruler_kind == 0 {
            return ActiveKind::Ruler {
                handle: Some(1),
                from: vp,
            };
        }
    }
    ActiveKind::Ruler { handle, from: vp }
}

fn ruler_drag(app: &mut ArtApp, handle: Option<usize>, from: VPoint, vp: VPoint) {
    use artapp_core::ruler::Ruler;
    let Some(r) = &mut app.ruler else {
        return;
    };
    match (handle, r) {
        (_, Ruler::Parallel { angle }) => {
            if from.distance(vp) > 2.0 {
                *angle = (vp.y - from.y).atan2(vp.x - from.x);
            }
        }
        (Some(i), r) => r.set_handle(i, [vp.x, vp.y]),
        (None, _) => {}
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
    app.ruler_live = None;
    // O toque que fecha o QuickMenu não desenha.
    if app.quick_menu.is_some() {
        return;
    }
    // Pasta não tem tinta: só as ferramentas que não pintam funcionam com ela ativa.
    if app.doc.active_layer().is_some_and(|l| l.folder)
        && !(app.tool.is_selection()
            || matches!(app.tool, Tool::Hand | Tool::Eyedropper | Tool::Ruler))
    {
        app.status = "É uma pasta: escolha uma camada dentro dela para desenhar.".into();
        return;
    }
    let mut pt = point(app, panel, p, force, time);
    snap_to_ruler(app, &mut pt);
    let (x, y) = (pt.x.floor() as i32, pt.y.floor() as i32);
    let vp = VPoint::new(pt.x, pt.y);
    let vector_layer = app.doc.active_is_vector();
    // Tolerância de toque em pixels do documento (uns 12 pontos de tela).
    let tol = 12.0 / app.view.zoom.max(0.01) as f64;
    if app.transform.is_some() {
        // Sem a régua: o transformar usa o ponto de verdade.
        let raw = point(app, panel, p, force, time);
        let from = VPoint::new(raw.x, raw.y);
        if let Some((handle, start)) = crate::transform_tool::down(app, from, tol) {
            let kind = ActiveKind::Transform {
                handle,
                from,
                start,
            };
            app.view.stroke = Some(ActiveStroke::new(kind, touch, time, p));
        }
        return;
    }
    let kind = match app.tool {
        Tool::Eraser if vector_layer => {
            // Borracha vetorial: apaga o objeto tocado inteiro.
            if let Some(id) = app.doc.vector_hit(vp.x, vp.y, tol) {
                let r = app.doc.vector_delete(id);
                app.invalidate(r);
            }
            None
        }
        Tool::Eraser if app.eraser_magic => {
            let r = app
                .doc
                .magic_erase(x, y, app.wand_tolerance, app.wand_contiguous);
            app.invalidate(r);
            None
        }
        Tool::Brush | Tool::Eraser => {
            if vector_layer {
                app.status =
                    "Camada vetorial: use as ferramentas do menu Vetor, ou Rasterizar a camada."
                        .into();
                return;
            }
            let Some(mut session) = begin_paint(app) else {
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
        Tool::Blend => {
            if vector_layer {
                app.status = "Misturar funciona em camadas raster.".into();
                return;
            }
            if app.mix_kind == artapp_core::mix::MixKind::Heal {
                let Some(mut heal) =
                    artapp_core::heal::HealSession::begin(&app.doc, app.mix_brush.size)
                else {
                    app.status = "A camada ativa está oculta.".into();
                    return;
                };
                heal.add(vp.x, vp.y);
                app.view.stroke = Some(ActiveStroke::new(
                    ActiveKind::Heal(Box::new(heal)),
                    touch,
                    time,
                    p,
                ));
                return;
            }
            let clone = app.mix_kind == artapp_core::mix::MixKind::Clone;
            if clone && (app.clone_picking || app.clone_source.is_none()) {
                app.clone_source = Some([vp.x, vp.y]);
                app.clone_picking = false;
                app.status = "Origem do carimbo marcada. Agora pinte onde quer copiar.".into();
                return;
            }
            let Some(mut session) = artapp_core::mix::MixSession::begin(
                &mut app.doc,
                app.mix_kind,
                &app.mix_brush,
                app.mix_strength,
            ) else {
                app.status = "A camada ativa está oculta.".into();
                return;
            };
            if let Some(src) = app.clone_source.filter(|_| clone) {
                session = session.with_source(src);
            }
            let dirty = session.add(&mut app.doc, &[pt]);
            app.invalidate(dirty);
            Some(ActiveKind::Mix(Box::new(session)))
        }
        Tool::Fill => {
            let r = if vector_layer {
                let [r, g, b, _] = color_to_f32(app.color);
                app.doc.vector_fill_at(vp.x, vp.y, [r, g, b])
            } else if app.fill_recolor {
                let [r, g, b, _] = color_to_f32(app.color);
                app.doc
                    .recolor(x, y, app.fill_tolerance, app.wand_contiguous, [r, g, b])
            } else {
                let opts = fill::FillOptions {
                    tolerance: app.fill_tolerance,
                    close_gap: app.fill_close_gap,
                    expand: app.fill_expand,
                };
                fill::flood_fill_with(&mut app.doc, x, y, color_to_f32(app.color), &opts)
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
        Tool::MoveLayer => {
            if vector_layer {
                app.status = "Em camada vetorial, mova os objetos com Selecionar.".into();
                None
            } else {
                Some(ActiveKind::MoveLayer { from: vp, to: vp })
            }
        }
        Tool::SelectRect | Tool::SelectEllipse => Some(ActiveKind::Marquee {
            tool: app.tool,
            from: vp,
            to: vp,
        }),
        Tool::Lasso => Some(ActiveKind::Lasso(vec![vp])),
        Tool::MagicWand => {
            let sel = app.doc.magic_wand(
                x,
                y,
                app.wand_tolerance,
                app.wand_all_layers,
                app.wand_contiguous,
            );
            app.doc.combine_selection(sel, app.select_mode);
            None
        }
        Tool::Gradient => {
            if vector_layer {
                app.status = "Degradê funciona em camadas raster.".into();
                None
            } else {
                Some(ActiveKind::Gradient { from: vp, to: vp })
            }
        }
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
        Tool::Liquify => {
            if vector_layer {
                app.status = "Liquefazer funciona em camadas raster.".into();
                return;
            }
            let Some(mut session) = artapp_core::liquify::LiquifySession::begin(
                &mut app.doc,
                app.liquify_tool,
                app.liquify_size,
                app.liquify_pressure,
            ) else {
                app.status = "A camada ativa está oculta.".into();
                return;
            };
            let r = session.add(&mut app.doc, vp.x, vp.y, pt.pressure);
            app.invalidate(r);
            Some(ActiveKind::Liquify(Box::new(session)))
        }
        Tool::Ruler => Some(ruler_down(app, vp, tol)),
        Tool::VectorKit => {
            app.ensure_vector_layer();
            let style = app.vector_style();
            app.kit.set_style(&style);
            let r = kit_pointer(app, PointerKind::Down, vp, pt.pressure);
            app.invalidate(r);
            Some(ActiveKind::Kit(vp, pt.pressure))
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
    app.view.stroke = kind.map(|kind| {
        let mut s = ActiveStroke::new(kind, touch, time, p);
        if matches!(s.kind, ActiveKind::Paint(_)) {
            s.path.push(pt);
        }
        s
    });
}

fn add_point(app: &mut ArtApp, panel: egui::Rect, p: Pos2, force: Option<f32>, time: f64) {
    let mut pt = point(app, panel, p, force, time);
    snap_to_ruler(app, &mut pt);
    let vp = VPoint::new(pt.x, pt.y);
    let min_handle = 3.0 / app.view.zoom.max(0.01) as f64;
    if app
        .view
        .stroke
        .as_ref()
        .is_some_and(|s| time - s.started > PANEL_CLOSE_SECS)
    {
        app.close_panels();
    }
    if let Some(s) = &mut app.view.stroke
        && matches!(s.kind, ActiveKind::Paint(_))
    {
        let moved = p.distance(s.still.0);
        if moved > HOLD_MOVE {
            s.travel += moved;
            s.still = (p, time);
        }
        match &s.quick {
            // Depois da forma rápida, a reta segue a caneta; as outras formas ficam.
            Some(artapp_core::quickshape::QuickShape::Line(a, _)) => {
                let shape = artapp_core::quickshape::QuickShape::Line(*a, [pt.x, pt.y]);
                redraw_quick(app, shape);
                return;
            }
            Some(_) => return,
            None => s.path.push(pt),
        }
    }
    let Some(s) = &mut app.view.stroke else {
        return;
    };
    match &mut s.kind {
        ActiveKind::Paint(session) => {
            let dirty = session.add(&mut app.doc, &[pt]);
            app.invalidate(dirty);
        }
        ActiveKind::Mix(session) => {
            let dirty = session.add(&mut app.doc, &[pt]);
            app.invalidate(dirty);
        }
        ActiveKind::Shape { to, .. }
        | ActiveKind::Move { to, .. }
        | ActiveKind::Marquee { to, .. }
        | ActiveKind::MoveLayer { to, .. }
        | ActiveKind::Gradient { to, .. } => *to = vp,
        ActiveKind::Freehand(points) | ActiveKind::Lasso(points) => points.push(vp),
        ActiveKind::Liquify(session) => {
            let r = session.add(&mut app.doc, vp.x, vp.y, pt.pressure);
            app.invalidate(r);
        }
        ActiveKind::Heal(heal) => heal.add(vp.x, vp.y),
        ActiveKind::Transform {
            handle,
            from,
            start,
        } => {
            let (handle, from, start) = (*handle, *from, *start);
            let raw = point(app, panel, p, force, time);
            crate::transform_tool::drag(app, handle, from, &start, VPoint::new(raw.x, raw.y));
        }
        ActiveKind::Ruler { handle, from } => {
            let (handle, from) = (*handle, *from);
            ruler_drag(app, handle, from, vp);
        }
        ActiveKind::Kit(last, pressure) => {
            *last = vp;
            *pressure = pt.pressure;
            let r = kit_pointer(app, PointerKind::Drag, vp, pt.pressure);
            app.invalidate(r);
        }
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
    app.close_panels();
    let style = app.vector_style();
    let dirty = match s.kind {
        ActiveKind::Paint(session) => session.end(&mut app.doc),
        ActiveKind::Mix(session) => session.end(),
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
        ActiveKind::Kit(last, pressure) => kit_pointer(app, PointerKind::Up, last, pressure),
        ActiveKind::Liquify(session) => session.end(),
        ActiveKind::Ruler { .. } | ActiveKind::Transform { .. } => return,
        ActiveKind::Heal(heal) => heal.end(&mut app.doc),
        ActiveKind::Marquee { tool, from, to } => {
            let (w, h) = (app.doc.width, app.doc.height);
            let (a, b) = ([from.x, from.y], [to.x, to.y]);
            // Um toque sem arrastar desmarca, como no Clip Studio.
            let sel = if from.distance(to) < 2.0 {
                None
            } else if tool == Tool::SelectEllipse {
                artapp_core::Selection::ellipse(a, b, w, h)
            } else {
                artapp_core::Selection::rect(a, b, w, h)
            };
            app.doc.combine_selection(sel, app.select_mode);
            Rect::EMPTY
        }
        ActiveKind::Lasso(points) => {
            let pts: Vec<[f64; 2]> = points.iter().map(|p| [p.x, p.y]).collect();
            let sel = artapp_core::Selection::polygon(&pts, app.doc.width, app.doc.height);
            app.doc.combine_selection(sel, app.select_mode);
            Rect::EMPTY
        }
        ActiveKind::MoveLayer { from, to } => {
            let d = to - from;
            app.doc
                .move_layer_by(d.x.round() as i32, d.y.round() as i32)
        }
        ActiveKind::Gradient { from, to } => {
            if from.distance(to) < 2.0 {
                return;
            }
            let c0 = app.paint_color();
            let c1 = if app.gradient_to_sub && !app.transparent {
                color_to_f32(app.sub_color)
            } else {
                [c0[0], c0[1], c0[2], 0.0]
            };
            app.remember_color();
            app.doc
                .gradient([from.x, from.y], [to.x, to.y], app.gradient_shape, c0, c1)
        }
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
        ActiveKind::Mix(session) => {
            let dirty = session.cancel(&mut app.doc);
            app.invalidate(dirty);
        }
        ActiveKind::BezierHandle => {
            app.bezier.pop();
        }
        ActiveKind::Liquify(session) => {
            let r = session.cancel(&mut app.doc);
            app.invalidate(r);
        }
        ActiveKind::Kit(last, pressure) => {
            let r = kit_pointer(app, PointerKind::Up, last, pressure);
            app.invalidate(r);
            app.kit_key(artapp_core::vectorkit::ToolKey::Escape);
        }
        ActiveKind::Transform { start, .. } => {
            if let Some(t) = &mut app.transform {
                t.quad = start;
            }
        }
        _ => {}
    }
}

/// A régua especial: linhas-guia em roxo e, com a ferramenta Régua, as alças.
fn draw_ruler(app: &ArtApp, painter: &egui::Painter, panel: egui::Rect) {
    use artapp_core::ruler::Ruler;
    let Some(ruler) = &app.ruler else {
        return;
    };
    let editing = app.tool == Tool::Ruler;
    if !editing && !app.ruler_snap {
        return;
    }
    let st = egui::Stroke::new(1.2, Color32::from_rgba_unmultiplied(170, 90, 255, 200));
    let faint = egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(170, 90, 255, 90));
    let s = |x: f64, y: f64| app.view.doc_to_screen(panel, x as f32, y as f32);
    // Uma reta "infinita": longa o bastante para cruzar a tela toda.
    let reach = f64::from(app.doc.width.max(app.doc.height)) * 4.0;
    let line = |o: [f64; 2], dx: f64, dy: f64, stroke: egui::Stroke| {
        painter.line_segment(
            [
                s(o[0] - dx * reach, o[1] - dy * reach),
                s(o[0] + dx * reach, o[1] + dy * reach),
            ],
            stroke,
        );
    };
    let (w, h) = (f64::from(app.doc.width), f64::from(app.doc.height));
    match ruler {
        Ruler::Line { a, b } => {
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l = dx.hypot(dy);
            if l > 1e-6 {
                line(*a, dx / l, dy / l, st);
            }
        }
        Ruler::Parallel { angle } => {
            let (dy, dx) = angle.sin_cos();
            let step = w.max(h) / 12.0;
            for i in -12..=12 {
                let o = [
                    w / 2.0 - dy * step * i as f64,
                    h / 2.0 + dx * step * i as f64,
                ];
                line(o, dx, dy, if i == 0 { st } else { faint });
            }
        }
        Ruler::Concentric { center } => {
            let c = s(center[0], center[1]);
            let step = w.max(h) / 10.0 * f64::from(app.view.zoom);
            for i in 1..=10 {
                painter.circle_stroke(c, (step * i as f64) as f32, faint);
            }
        }
        Ruler::Perspective { vanishing } => {
            for v in vanishing {
                for i in 0..24 {
                    let (dy, dx) = (i as f64 / 24.0 * std::f64::consts::PI).sin_cos();
                    line(*v, dx, dy, faint);
                }
            }
        }
    }
    if editing {
        for [x, y] in ruler.handles() {
            let c = s(x, y);
            painter.circle_filled(c, 7.0, Color32::WHITE);
            painter.circle_stroke(
                c,
                7.0,
                egui::Stroke::new(2.0, Color32::from_rgb(170, 90, 255)),
            );
        }
    }
}

/// Grade de `grid_size` pixels do documento, com uma linha mais forte a cada quatro.
fn draw_grid(app: &ArtApp, painter: &egui::Painter, panel: egui::Rect) {
    let (w, h) = (app.doc.width as f32, app.doc.height as f32);
    let step = app.grid_size.max(2.0);
    // Linhas muito juntas na tela viram ruído: pula de 2 em 2 até ficarem a 6 px ou mais.
    let mut skip = 1usize;
    while step * skip as f32 * app.view.zoom < 6.0 {
        skip *= 2;
    }
    let thin = egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(60, 140, 255, 60));
    let bold = egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(60, 140, 255, 130));
    let mut i = 0usize;
    let mut x = 0.0;
    while x <= w {
        let st = if i.is_multiple_of(4) { bold } else { thin };
        painter.line_segment(
            [
                app.view.doc_to_screen(panel, x, 0.0),
                app.view.doc_to_screen(panel, x, h),
            ],
            st,
        );
        i += skip;
        x = i as f32 * step;
    }
    i = 0;
    let mut y = 0.0;
    while y <= h {
        let st = if i.is_multiple_of(4) { bold } else { thin };
        painter.line_segment(
            [
                app.view.doc_to_screen(panel, 0.0, y),
                app.view.doc_to_screen(panel, w, y),
            ],
            st,
        );
        i += skip;
        y = i as f32 * step;
    }
}

/// Um evento do dedo para a ferramenta do VectorCraft.
fn kit_pointer(app: &mut ArtApp, kind: PointerKind, p: VPoint, pressure: f32) -> Rect {
    let mods = artapp_core::vectorkit::Mods::default();
    let r = app
        .kit
        .pointer(&mut app.doc, kind, p.x, p.y, pressure, mods);
    if let Some(msg) = app.kit.status.take() {
        app.status = msg;
    }
    r
}

/// Desenha as alças, guias e contornos da ferramenta do VectorCraft.
fn draw_kit_overlays(app: &ArtApp, painter: &egui::Painter, panel: egui::Rect) {
    use artapp_core::vectorkit::Overlay;
    let rgb = |c: [u8; 3]| Color32::from_rgb(c[0], c[1], c[2]);
    let rgba = |c: [u8; 4]| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
    let sp = |p: VPoint| to_screen(app, panel, p);
    let font = egui::FontId::proportional(12.0);
    for o in &app.kit_overlays {
        match o {
            Overlay::Marquee(r) => {
                let pts = [
                    VPoint::new(r.x0, r.y0),
                    VPoint::new(r.x1, r.y0),
                    VPoint::new(r.x1, r.y1),
                    VPoint::new(r.x0, r.y1),
                    VPoint::new(r.x0, r.y0),
                ]
                .map(sp);
                painter.add(egui::Shape::dashed_line(
                    &pts,
                    egui::Stroke::new(1.0, Color32::from_rgb(40, 170, 255)),
                    4.0,
                    3.0,
                ));
            }
            Overlay::Path {
                path, color, width, ..
            } => draw_bez(
                app,
                painter,
                panel,
                path,
                egui::Stroke::new(width.max(1.0), rgb(*color)),
            ),
            Overlay::Line {
                a,
                b,
                color,
                dashed,
            } => {
                let pts = [sp(*a), sp(*b)];
                let stroke = egui::Stroke::new(1.0, rgb(*color));
                if *dashed {
                    painter.add(egui::Shape::dashed_line(&pts, stroke, 4.0, 3.0));
                } else {
                    painter.line_segment(pts, stroke);
                }
            }
            Overlay::Anchor {
                p,
                color,
                filled,
                size,
            } => {
                let r = egui::Rect::from_center_size(sp(*p), egui::Vec2::splat(size.max(7.0)));
                if *filled {
                    painter.rect_filled(r, 0.0, rgb(*color));
                } else {
                    painter.rect_filled(r, 0.0, Color32::WHITE);
                    painter.rect_stroke(
                        r,
                        0.0,
                        egui::Stroke::new(1.0, rgb(*color)),
                        egui::StrokeKind::Inside,
                    );
                }
            }
            Overlay::CurvePoint {
                p,
                color,
                filled,
                size,
            } => {
                let r = size.max(7.0) / 2.0;
                if *filled {
                    painter.circle_filled(sp(*p), r, rgb(*color));
                } else {
                    painter.circle(
                        sp(*p),
                        r,
                        Color32::WHITE,
                        egui::Stroke::new(1.0, rgb(*color)),
                    );
                }
            }
            Overlay::Handle { p, color } => {
                painter.circle_filled(sp(*p), 4.0, rgb(*color));
            }
            Overlay::Label { p, text, color } => {
                painter.text(
                    sp(*p),
                    egui::Align2::LEFT_BOTTOM,
                    text,
                    font.clone(),
                    rgb(*color),
                );
            }
            Overlay::Measure { p, text } => {
                painter.text(
                    sp(*p) + egui::vec2(12.0, 12.0),
                    egui::Align2::LEFT_TOP,
                    text,
                    font.clone(),
                    Color32::WHITE,
                );
            }
            Overlay::Distance { a, b, text, color } => {
                let (a, b) = (sp(*a), sp(*b));
                painter.line_segment([a, b], egui::Stroke::new(1.0, rgb(*color)));
                painter.text(
                    a.lerp(b, 0.5),
                    egui::Align2::CENTER_BOTTOM,
                    text,
                    font.clone(),
                    rgb(*color),
                );
            }
            Overlay::Highlight { quad, color } => {
                painter.add(egui::Shape::convex_polygon(
                    quad.map(sp).to_vec(),
                    rgba(*color),
                    egui::Stroke::NONE,
                ));
            }
            Overlay::Swatch { p, color, selected } => {
                let r = egui::Rect::from_center_size(sp(*p), egui::Vec2::splat(12.0));
                painter.rect_filled(r, 2.0, rgba(*color));
                if *selected {
                    painter.rect_stroke(
                        r.expand(2.0),
                        2.0,
                        egui::Stroke::new(1.5, Color32::WHITE),
                        egui::StrokeKind::Outside,
                    );
                }
            }
            Overlay::GridLine { a, b, color } => {
                painter.line_segment([sp(*a), sp(*b)], egui::Stroke::new(1.0, rgba(*color)));
            }
        }
    }
}

/// Converte um ponto do documento para a tela.
fn to_screen(app: &ArtApp, panel: egui::Rect, p: VPoint) -> Pos2 {
    app.view.doc_to_screen(panel, p.x as f32, p.y as f32)
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
            ActiveKind::Heal(heal) => {
                // A área a corrigir, em vermelho translúcido.
                let r = (heal.radius() as f32 * app.view.zoom).max(1.0);
                let fill = Color32::from_rgba_unmultiplied(255, 60, 60, 70);
                let line: Vec<Pos2> = heal
                    .points
                    .iter()
                    .map(|p| app.view.doc_to_screen(panel, p[0] as f32, p[1] as f32))
                    .collect();
                if line.len() > 1 {
                    painter.add(egui::Shape::line(
                        line.clone(),
                        egui::Stroke::new(r * 2.0, fill),
                    ));
                }
                for p in [line.first(), line.last()].into_iter().flatten() {
                    painter.circle_filled(*p, r, fill);
                }
            }
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
            ActiveKind::Marquee { tool, from, to } => {
                let pts: Vec<Pos2> = if *tool == Tool::SelectEllipse {
                    let (c, r) = (from.midpoint(*to), (*to - *from) / 2.0);
                    (0..=64)
                        .map(|i| {
                            let t = i as f64 / 64.0 * std::f64::consts::TAU;
                            to_screen(
                                app,
                                panel,
                                VPoint::new(c.x + r.x * t.cos(), c.y + r.y * t.sin()),
                            )
                        })
                        .collect()
                } else {
                    [
                        (from.x, from.y),
                        (to.x, from.y),
                        (to.x, to.y),
                        (from.x, to.y),
                        (from.x, from.y),
                    ]
                    .into_iter()
                    .map(|(x, y)| to_screen(app, panel, VPoint::new(x, y)))
                    .collect()
                };
                ants(painter, &pts);
            }
            ActiveKind::Lasso(points) => {
                let pts: Vec<Pos2> = points.iter().map(|p| to_screen(app, panel, *p)).collect();
                ants(painter, &pts);
            }
            ActiveKind::Gradient { from, to } => {
                let (a, b) = (to_screen(app, panel, *from), to_screen(app, panel, *to));
                painter.line_segment(
                    [a, b],
                    egui::Stroke::new(3.0, Color32::from_black_alpha(140)),
                );
                painter.line_segment([a, b], egui::Stroke::new(1.5, Color32::WHITE));
                painter.circle_filled(a, 5.0, app.color);
                painter.circle_stroke(a, 5.0, egui::Stroke::new(1.5, Color32::WHITE));
                painter.circle_stroke(b, 5.0, egui::Stroke::new(1.5, Color32::WHITE));
            }
            ActiveKind::MoveLayer { from, to } => {
                if let Some(l) = app.doc.active_layer() {
                    let c = l.surface.content_bounds().intersect(&app.doc.bounds());
                    let d = *to - *from;
                    let pts: Vec<Pos2> = [
                        (c.x0, c.y0),
                        (c.x1, c.y0),
                        (c.x1, c.y1),
                        (c.x0, c.y1),
                        (c.x0, c.y0),
                    ]
                    .into_iter()
                    .map(|(x, y)| {
                        to_screen(app, panel, VPoint::new(x as f64 + d.x, y as f64 + d.y))
                    })
                    .collect();
                    painter.add(egui::Shape::line(pts, guide));
                }
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
    if let Some(sym) = app.symmetry_ruler() {
        let axis = egui::Stroke::new(1.5, Color32::from_rgb(160, 90, 255));
        for [a, b] in sym.segments([app.doc.width as f64, app.doc.height as f64]) {
            let (a, b) = (
                to_screen(app, panel, VPoint::new(a[0], a[1])),
                to_screen(app, panel, VPoint::new(b[0], b[1])),
            );
            painter.extend(egui::Shape::dashed_line(&[a, b], axis, 10.0, 6.0));
        }
    }
    if let Some(sel) = app.doc.selection.as_ref().filter(|s| !s.edges.is_empty()) {
        // Seleção automática: as bordas entre pixels (muitas, então sem tracejado).
        if sel.edges.len() <= 60_000 {
            let white = egui::Stroke::new(1.0, Color32::WHITE);
            let black = egui::Stroke::new(1.0, Color32::BLACK);
            for [a, b] in &sel.edges {
                let (a, b) = (
                    to_screen(app, panel, VPoint::new(a[0], a[1])),
                    to_screen(app, panel, VPoint::new(b[0], b[1])),
                );
                painter.line_segment([a, b], white);
                painter.line_segment([a + egui::vec2(0.5, 0.5), b + egui::vec2(0.5, 0.5)], black);
            }
        }
    }
    if let Some(sel) = &app.doc.selection {
        let mut pts: Vec<Pos2> = sel
            .outline
            .iter()
            .map(|p| to_screen(app, panel, VPoint::new(p[0], p[1])))
            .collect();
        if let Some(first) = pts.first().copied() {
            pts.push(first);
        }
        ants(painter, &pts);
        if sel.inverted {
            let (w, h) = (app.doc.width as f64, app.doc.height as f64);
            let border: Vec<Pos2> = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h), (0.0, 0.0)]
                .into_iter()
                .map(|(x, y)| to_screen(app, panel, VPoint::new(x, y)))
                .collect();
            ants(painter, &border);
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

/// Contorno tracejado preto e branco (as "formigas" da seleção).
fn ants(painter: &egui::Painter, pts: &[Pos2]) {
    if pts.len() < 2 {
        return;
    }
    painter.add(egui::Shape::line(
        pts.to_vec(),
        egui::Stroke::new(1.0, Color32::WHITE),
    ));
    painter.extend(egui::Shape::dashed_line(
        pts,
        egui::Stroke::new(1.0, Color32::BLACK),
        5.0,
        5.0,
    ));
}

/// Navegador: miniatura do desenho com o retângulo da vista. Tocar ou arrastar leva a vista
/// para aquele ponto. `bottom_right` é o canto onde o painel encosta.
pub fn navigator(app: &mut ArtApp, ctx: &egui::Context, bottom_right: Pos2) {
    let Some(tex) = app.view.texture.as_ref().map(|(t, _)| t.id()) else {
        return;
    };
    let (w, h) = (app.doc.width.max(1) as f32, app.doc.height.max(1) as f32);
    let scale = 170.0 / w.max(h);
    let size = egui::vec2(w * scale, h * scale);
    let frame = size + Vec2::splat(26.0);
    let panel = app.canvas_rect;
    egui::Area::new(egui::Id::unique("navigator"))
        .order(egui::Order::Foreground)
        .fixed_pos(bottom_right - frame)
        .show(ctx, |ui| {
            crate::theme::panel_frame().show(ui, |ui| {
                let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 0.0, Color32::from_gray(185));
                let uv = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                painter.image(tex, rect, uv, Color32::WHITE);
                // Os cantos da vista, levados para a miniatura.
                let view: Vec<Pos2> = [
                    panel.left_top(),
                    panel.right_top(),
                    panel.right_bottom(),
                    panel.left_bottom(),
                ]
                .into_iter()
                .map(|p| {
                    let (x, y) = app.view.to_canvas(panel, p);
                    rect.min + egui::vec2(x as f32, y as f32) * scale
                })
                .collect();
                painter.add(egui::Shape::closed_line(
                    view,
                    egui::Stroke::new(1.5, Color32::from_rgb(255, 70, 70)),
                ));
                if let Some(p) = resp.interact_pointer_pos()
                    && (resp.dragged() || resp.clicked())
                {
                    let d = (p - rect.min) / scale;
                    app.view
                        .pin(panel, (d.x as f64, d.y as f64), panel.center());
                }
            });
        });
}
