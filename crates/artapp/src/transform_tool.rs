//! Transformar na tela, como no Procreate: uma caixa com alças em volta do conteúdo da camada
//! (ou da seleção). Arrastar dentro move, os cantos e os lados escalam, a alça de cima (ou
//! arrastar fora da caixa) gira; no modo Distorcer cada canto anda sozinho; no Deformar uma
//! malha de 4×4 pontos curva a imagem. Aplicar ou Cancelar na barra de baixo (Enter / Esc no
//! teclado).

use artapp_core::VPoint;
use artapp_core::transform::{self as tf, Homography, Interp, Quad, TransformSession};
use artapp_core::warp::{self, Grid};
use eframe::egui::{self, Color32, Pos2, RichText, Stroke, TextureHandle, TextureOptions};

use crate::app::ArtApp;
use crate::theme::{color, panel_frame};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Free,
    Uniform,
    Distort,
    /// Malha de deformação.
    Warp,
}

/// O que o dedo pegou.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Handle {
    Corner(usize),
    Edge(usize),
    Rotate,
    Move,
    /// Fora da caixa: gira em volta do centro.
    Spin,
    /// Um ponto da malha (Deformar).
    Point(usize),
}

pub struct Transforming {
    session: TransformSession,
    pub quad: Quad,
    pub mode: Mode,
    pub interp: Interp,
    texture: Option<TextureHandle>,
    /// A malha, depois que o Deformar foi escolhido; manda no resultado a partir daí.
    pub grid: Option<Grid>,
    /// A malha de quando o toque começou.
    grid_start: Option<Grid>,
}

/// Distância da alça de girar até o lado de cima, em pontos de tela.
const ROTATE_GAP: f32 = 30.0;

impl ArtApp {
    /// Começa a transformar a camada ativa (ou a seleção).
    pub fn start_transform(&mut self) {
        if self.transform.is_some() {
            return;
        }
        if self.doc.active_is_vector() {
            self.status = "Em camada vetorial, transforme os objetos com Selecionar.".into();
            return;
        }
        match TransformSession::begin(&mut self.doc) {
            Some(session) => {
                let quad = session.start_quad();
                self.invalidate(session.rect);
                self.transform = Some(Transforming {
                    session,
                    quad,
                    mode: Mode::Free,
                    interp: Interp::Bicubic,
                    texture: None,
                    grid: None,
                    grid_start: None,
                });
            }
            None => self.status = "Nada para transformar: a camada está vazia ou oculta.".into(),
        }
    }

    /// Aplica a transformação em andamento (se houver).
    pub fn commit_transform(&mut self) {
        if let Some(t) = self.transform.take() {
            let r = match &t.grid {
                Some(g) => t.session.end_warp(&mut self.doc, g, t.interp),
                None => t.session.end(&mut self.doc, t.quad, t.interp),
            };
            self.invalidate(r);
        }
    }

    pub fn cancel_transform(&mut self) {
        if let Some(t) = self.transform.take() {
            let r = t.session.cancel(&mut self.doc);
            self.invalidate(r);
        }
    }
}

fn screen(app: &ArtApp, panel: egui::Rect, p: [f64; 2]) -> Pos2 {
    app.view.doc_to_screen(panel, p[0] as f32, p[1] as f32)
}

/// Onde fica a alça de girar, em pixels do documento.
fn rotate_handle(app: &ArtApp, q: &Quad) -> [f64; 2] {
    let c = tf::center(q);
    let top = [(q[0][0] + q[1][0]) / 2.0, (q[0][1] + q[1][1]) / 2.0];
    let (dx, dy) = (top[0] - c[0], top[1] - c[1]);
    let len = dx.hypot(dy).max(1e-6);
    let gap = f64::from(ROTATE_GAP / app.view.zoom.max(0.01));
    [top[0] + dx / len * gap, top[1] + dy / len * gap]
}

fn edge_mid(q: &Quad, side: usize) -> [f64; 2] {
    let (a, b) = (q[side], q[(side + 1) % 4]);
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

fn inside(q: &Quad, p: [f64; 2]) -> bool {
    let mut sign = 0.0;
    for i in 0..4 {
        let (a, b) = (q[i], q[(i + 1) % 4]);
        let cross = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign && cross != 0.0 {
            return false;
        }
    }
    true
}

/// O toque começou: qual alça pegou. `tol` em pixels do documento.
pub fn down(app: &mut ArtApp, vp: VPoint, tol: f64) -> Option<(Handle, Quad)> {
    let rot = app
        .transform
        .as_ref()
        .map(|t| rotate_handle(app, &t.quad))?;
    let t = app.transform.as_mut()?;
    let q = t.quad;
    let p = [vp.x, vp.y];
    let near = |h: [f64; 2]| (h[0] - p[0]).hypot(h[1] - p[1]) <= tol * 1.6;
    if let Some(g) = t.grid {
        t.grid_start = Some(g);
        let corners = [g[0], g[3], g[15], g[12]];
        let handle = if let Some(k) = (0..g.len()).find(|&k| near(g[k])) {
            Handle::Point(k)
        } else if inside(&corners, p) {
            Handle::Move
        } else {
            return None;
        };
        return Some((handle, q));
    }
    let handle = if near(rot) && t.mode != Mode::Distort {
        Handle::Rotate
    } else if let Some(i) = (0..4).find(|&i| near(q[i])) {
        Handle::Corner(i)
    } else if let Some(i) = (0..4).find(|&i| near(edge_mid(&q, i))) {
        Handle::Edge(i)
    } else if inside(&q, p) {
        Handle::Move
    } else {
        Handle::Spin
    };
    Some((handle, q))
}

/// O dedo andou: refaz o quadrilátero a partir do de quando o toque começou.
pub fn drag(app: &mut ArtApp, handle: Handle, from: VPoint, start: &Quad, vp: VPoint) {
    let Some(t) = &mut app.transform else {
        return;
    };
    let p = [vp.x, vp.y];
    let (dx, dy) = (vp.x - from.x, vp.y - from.y);
    if let (Some(g), Some(g0)) = (&mut t.grid, t.grid_start) {
        match handle {
            Handle::Point(k) => g[k] = [g0[k][0] + dx, g0[k][1] + dy],
            Handle::Move => *g = g0.map(|c| [c[0] + dx, c[1] + dy]),
            _ => {}
        }
        return;
    }
    let uniform = t.mode == Mode::Uniform;
    let next = match (handle, t.mode) {
        (Handle::Move, _) => Some(start.map(|c| [c[0] + dx, c[1] + dy])),
        (Handle::Rotate | Handle::Spin, _) => {
            let c = tf::center(start);
            let a0 = (from.y - c[1]).atan2(from.x - c[0]);
            let a1 = (vp.y - c[1]).atan2(vp.x - c[0]);
            Some(tf::rotate(start, c, a1 - a0))
        }
        (Handle::Corner(i), Mode::Distort) => {
            let mut q = *start;
            q[i] = [start[i][0] + dx, start[i][1] + dy];
            tf::is_convex(&q).then_some(q)
        }
        (Handle::Edge(i), Mode::Distort) => {
            let mut q = *start;
            for k in [i, (i + 1) % 4] {
                q[k] = [start[k][0] + dx, start[k][1] + dy];
            }
            tf::is_convex(&q).then_some(q)
        }
        (Handle::Corner(i), _) => tf::drag_corner(start, i, p, uniform),
        (Handle::Edge(i), _) => tf::drag_edge(start, i, p, uniform),
        (Handle::Point(_), _) => None,
    };
    if let Some(q) = next {
        t.quad = q;
    }
}

/// A prévia do conteúdo deformado e a caixa com as alças.
pub fn draw(app: &mut ArtApp, ctx: &egui::Context, painter: &egui::Painter, panel: egui::Rect) {
    let opacity = app
        .transform
        .as_ref()
        .and_then(|t| app.doc.layers.iter().find(|l| l.id == t.session.layer_id()))
        .map_or(1.0, |l| l.opacity);
    let Some(t) = &mut app.transform else {
        return;
    };
    let tex = t
        .texture
        .get_or_insert_with(|| {
            let (w, h, px) = t.session.preview(2048);
            let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px);
            ctx.load_texture("transform", image, TextureOptions::LINEAR)
        })
        .id();
    let q = t.quad;
    let r = t.session.rect;
    let src = [
        f64::from(r.x0),
        f64::from(r.y0),
        f64::from(r.x1),
        f64::from(r.y1),
    ];
    let map = Homography::rect_to_quad(src, q);
    let grid = t.grid;
    let place = |u: f64, v: f64| -> Option<[f64; 2]> {
        match &grid {
            Some(g) => Some(warp::point(g, u, v)),
            None => map.map(|h| {
                let (x, y) = h.apply(
                    src[0] + u * (src[2] - src[0]),
                    src[1] + v * (src[3] - src[1]),
                );
                [x, y]
            }),
        }
    };
    if place(0.0, 0.0).is_some() {
        // Uma grade fina deixa a perspectiva certa (cada triângulo da malha é afim).
        const N: usize = 16;
        let mut mesh = egui::Mesh::with_texture(tex);
        let tint = Color32::from_white_alpha((opacity.clamp(0.0, 1.0) * 255.0) as u8);
        for j in 0..=N {
            for i in 0..=N {
                let (u, v) = (i as f64 / N as f64, j as f64 / N as f64);
                let xy = place(u, v).unwrap_or_default();
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: screen(app, panel, xy),
                    uv: Pos2::new(u as f32, v as f32),
                    color: tint,
                });
            }
        }
        let at = |i: usize, j: usize| (j * (N + 1) + i) as u32;
        for j in 0..N {
            for i in 0..N {
                mesh.add_triangle(at(i, j), at(i + 1, j), at(i + 1, j + 1));
                mesh.add_triangle(at(i, j), at(i + 1, j + 1), at(i, j + 1));
            }
        }
        painter.add(egui::Shape::mesh(mesh));
    }
    let Some(t) = &app.transform else {
        return;
    };
    if let Some(g) = &t.grid {
        draw_grid(app, painter, panel, g);
        return;
    }
    let pts: Vec<Pos2> = q.iter().map(|p| screen(app, panel, *p)).collect();
    painter.add(egui::Shape::closed_line(
        pts.clone(),
        Stroke::new(2.5, Color32::from_black_alpha(120)),
    ));
    painter.add(egui::Shape::closed_line(
        pts.clone(),
        Stroke::new(1.2, color::SELECTION),
    ));
    let dot = |c: Pos2, r: f32| {
        painter.circle_filled(c, r, Color32::WHITE);
        painter.circle_stroke(c, r, Stroke::new(1.5, color::SELECTION));
    };
    if t.mode != Mode::Distort {
        let top = screen(app, panel, edge_mid(&q, 0));
        let rot = screen(app, panel, rotate_handle(app, &q));
        painter.line_segment([top, rot], Stroke::new(1.2, color::SELECTION));
        painter.circle_filled(rot, 7.0, Color32::from_rgb(0xff, 0xc4, 0x3d));
        painter.circle_stroke(rot, 7.0, Stroke::new(1.5, Color32::from_black_alpha(150)));
    }
    for i in 0..4 {
        dot(screen(app, panel, edge_mid(&q, i)), 5.0);
    }
    for p in &pts {
        dot(*p, 7.5);
    }
}

/// A malha do Deformar: as linhas da superfície e os 16 pontos.
fn draw_grid(app: &ArtApp, painter: &egui::Painter, panel: egui::Rect, g: &Grid) {
    const N: usize = 24;
    let line = |pts: Vec<Pos2>| {
        painter.add(egui::Shape::line(
            pts.clone(),
            Stroke::new(2.5, Color32::from_black_alpha(110)),
        ));
        painter.add(egui::Shape::line(pts, Stroke::new(1.2, color::SELECTION)));
    };
    for k in 0..warp::SIDE {
        let a = k as f64 / (warp::SIDE - 1) as f64;
        line(
            (0..=N)
                .map(|i| screen(app, panel, warp::point(g, i as f64 / N as f64, a)))
                .collect(),
        );
        line(
            (0..=N)
                .map(|i| screen(app, panel, warp::point(g, a, i as f64 / N as f64)))
                .collect(),
        );
    }
    for p in g {
        let c = screen(app, panel, *p);
        painter.circle_filled(c, 6.5, Color32::WHITE);
        painter.circle_stroke(c, 6.5, Stroke::new(1.5, color::SELECTION));
    }
}

/// A barra de baixo: modos e ações. Também aplica sozinho se a camada ativa mudou.
pub fn toolbar(app: &mut ArtApp, ctx: &egui::Context, canvas: egui::Rect) {
    let Some(t) = &app.transform else {
        return;
    };
    if app.doc.active_layer().map(|l| l.id) != Some(t.session.layer_id()) {
        app.commit_transform();
        return;
    }
    let (w, h) = (app.doc.width, app.doc.height);
    let mut act: Option<fn(&mut ArtApp)> = None;
    egui::Area::new(egui::Id::unique("transform_bar"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(canvas.center_bottom() - egui::vec2(0.0, 14.0))
        .show(ctx, |ui| {
            panel_frame().show(ui, |ui| {
                ui.set_max_width((canvas.width() - 32.0).max(240.0));
                let Some(t) = &mut app.transform else {
                    return;
                };
                ui.horizontal_wrapped(|ui| {
                    for (m, name) in [
                        (Mode::Free, "Livre"),
                        (Mode::Uniform, "Uniforme"),
                        (Mode::Distort, "Distorcer"),
                        (Mode::Warp, "Deformar"),
                    ] {
                        // Depois de curvar a malha, os outros modos não a representam mais:
                        // só Redefinir volta para eles.
                        let enabled = t.grid.is_none() || m == Mode::Warp;
                        if ui
                            .add_enabled(enabled, egui::Button::selectable(t.mode == m, name))
                            .clicked()
                        {
                            t.mode = m;
                        }
                    }
                    if t.mode == Mode::Warp && t.grid.is_none() {
                        t.grid = warp::grid_from_quad(t.session.rect, &t.quad);
                    }
                    let warping = t.grid.is_some();
                    ui.separator();
                    let c = tf::center(&t.quad);
                    if !warping && ui.button("Espelhar horizontal").clicked() {
                        let q = t.quad;
                        t.quad = [q[1], q[0], q[3], q[2]];
                    }
                    if !warping && ui.button("Espelhar vertical").clicked() {
                        let q = t.quad;
                        t.quad = [q[3], q[2], q[1], q[0]];
                    }
                    if !warping && ui.button("Girar 90°").clicked() {
                        t.quad = tf::rotate(&t.quad, c, std::f64::consts::FRAC_PI_2);
                    }
                    if !warping && ui.button("Encaixar na tela").clicked() {
                        t.quad = tf::fit(t.session.rect, w, h);
                    }
                    if ui.button("Redefinir").clicked() {
                        t.quad = t.session.start_quad();
                        t.grid = None;
                        if t.mode == Mode::Warp {
                            t.mode = Mode::Free;
                        }
                    }
                    egui::ComboBox::from_id_salt("transform_interp")
                        .selected_text(match t.interp {
                            Interp::Nearest => "Vizinho mais próximo",
                            Interp::Bilinear => "Bilinear",
                            Interp::Bicubic => "Bicúbica",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut t.interp, Interp::Bicubic, "Bicúbica");
                            ui.selectable_value(&mut t.interp, Interp::Bilinear, "Bilinear");
                            ui.selectable_value(
                                &mut t.interp,
                                Interp::Nearest,
                                "Vizinho mais próximo (pixel art)",
                            );
                        });
                    ui.separator();
                    if ui.button("Cancelar").clicked() {
                        act = Some(ArtApp::cancel_transform);
                    }
                    if ui
                        .button(RichText::new("Aplicar").strong().color(color::SELECTION))
                        .clicked()
                    {
                        act = Some(ArtApp::commit_transform);
                    }
                });
            });
        });
    if let Some(f) = act {
        f(app);
    }
}
