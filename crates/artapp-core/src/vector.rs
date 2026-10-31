//! Camadas vetoriais, sobre o modelo de documento e o renderizador do VectorCraft.
//!
//! Cada camada vetorial guarda um `vectorcraft_doc::Document` (uma árvore persistente: clonar é
//! barato, então o histórico funciona igual ao das camadas raster) e uma cópia rasterizada na
//! superfície da camada, que é o que a composição e a exportação usam. A cópia é refeita a cada
//! edição.

use std::cell::RefCell;

use photocraft_color::PixelFormat;
use photocraft_geom::Rect;
use photocraft_raster::Surface;
use vectorcraft_color::{Color, Gradient, GradientKind, GradientPaint, GradientStop, Paint};
use vectorcraft_doc::appearance::{
    FillLayer, LineCap, LineJoin, PressureProfile, StrokeGradientMode, StrokeLayer, WidthProfile,
};
use vectorcraft_doc::hit::{HitOptions, hit_test};
use vectorcraft_doc::{Appearance, AppearanceItem, Node, NodeId, Shared};
use vectorcraft_geom::path::{PathData, SubPath};
use vectorcraft_geom::{Affine, Point, Vec2, shapes};
use vectorcraft_pathops::SimplifyOptions;
use vectorcraft_render::{RenderOptions, Renderer};

use crate::document::{Document, Layer};

/// A arte de uma camada vetorial.
#[derive(Clone)]
pub struct VectorArt {
    pub doc: vectorcraft_doc::Document,
    /// Muda a cada edição (único no processo): o kit vetorial sabe se a arte que tem ainda é esta.
    pub rev: u64,
}

/// Uma revisão nova, nunca usada antes.
pub fn next_rev() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static REV: AtomicU64 = AtomicU64::new(1);
    REV.fetch_add(1, Ordering::Relaxed)
}

/// Traço e preenchimento de um objeto novo. Cores em sRGB 0..1; `None` = sem traço / sem
/// preenchimento.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorStyle {
    pub stroke: Option<[f32; 3]>,
    pub width: f64,
    pub fill: Option<[f32; 3]>,
    /// Opacidade do traço e do preenchimento (0..1).
    pub opacity: f32,
}

impl Default for VectorStyle {
    fn default() -> Self {
        Self {
            stroke: Some([0.0, 0.0, 0.0]),
            width: 4.0,
            fill: None,
            opacity: 1.0,
        }
    }
}

/// Como a pressão da caneta muda o traço da caneta vetorial. A pressão chega já passada pela curva
/// de pressão do app; o traço continua um caminho editável, com perfil de largura (e um degradê
/// de opacidade ao longo dele) no lugar de pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenPressure {
    /// A pressão afina o traço.
    pub size: bool,
    /// Largura na pressão mais leve, em fração da espessura (0..1).
    pub min_size: f32,
    /// A pressão muda a opacidade.
    pub opacity: bool,
    /// Opacidade na pressão mais leve, em fração da opacidade do traço (0..1).
    pub min_opacity: f32,
}

impl Default for PenPressure {
    fn default() -> Self {
        Self {
            size: true,
            min_size: 0.15,
            opacity: false,
            min_opacity: 0.2,
        }
    }
}

/// Paradas do degradê de opacidade ao longo de um traço (mais deixam o desenho lento).
const MAX_OPACITY_STOPS: usize = 48;

/// A pressão de cada ponto de `points` ao longo do traço: (fração do comprimento, pressão). `None`
/// quando a pressão não muda (mouse, dedo ou caneta sem pressão).
fn pressure_samples(points: &[Point], pressures: &[f32]) -> Option<PressureProfile> {
    if points.len() < 2 || pressures.len() != points.len() {
        return None;
    }
    let (lo, hi) = pressures
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    if !(lo.is_finite() && hi.is_finite()) || (lo >= 0.999 && hi >= 0.999) {
        return None;
    }
    let mut acc = 0.0;
    let mut lens = Vec::with_capacity(points.len());
    lens.push(0.0);
    for w in points.windows(2) {
        acc += w[0].distance(w[1]);
        lens.push(acc);
    }
    if acc <= 1e-9 {
        return None;
    }
    PressureProfile::from_samples(
        lens.iter()
            .zip(pressures)
            .map(|(l, p)| (l / acc, f64::from(*p))),
    )
}

/// A camada de traço de `style` com a pressão `pressure` aplicada como `pen` pede.
fn pressure_stroke(
    st: &mut StrokeLayer,
    color: [f32; 3],
    pressure: &PressureProfile,
    pen: &PenPressure,
) {
    let lerp = |min: f32, p: f64| {
        f64::from(min.clamp(0.0, 1.0)) + (1.0 - f64::from(min.clamp(0.0, 1.0))) * p
    };
    if pen.size {
        st.profile = Some(WidthProfile {
            points: pressure
                .points
                .iter()
                .map(|&(t, p)| {
                    let f = lerp(pen.min_size, p);
                    (t, f, f)
                })
                .collect(),
        });
    }
    if pen.opacity {
        let pts = &pressure.points;
        let n = pts.len();
        let stops: Vec<GradientStop> = (0..n.min(MAX_OPACITY_STOPS))
            .filter_map(|i| {
                let j = if n <= MAX_OPACITY_STOPS {
                    i
                } else {
                    i * (n - 1) / (MAX_OPACITY_STOPS - 1)
                };
                pts.get(j)
            })
            .map(|&(t, p)| GradientStop {
                opacity: lerp(pen.min_opacity, p) as f32,
                ..GradientStop::new(t as f32, Color::rgb(color[0], color[1], color[2]))
            })
            .collect();
        if stops.len() >= 2 {
            st.paint = Paint::Gradient(Box::new(GradientPaint::new(Gradient::new(
                GradientKind::Linear,
                stops,
            ))));
            st.gradient_mode = StrokeGradientMode::Along;
        }
    }
    st.pressure = Some(pressure.clone());
}

fn paint(c: [f32; 3]) -> Paint {
    Paint::solid(Color::rgb(c[0], c[1], c[2]))
}

impl VectorStyle {
    pub fn appearance(&self) -> Appearance {
        let mut items = Vec::new();
        let opacity = self.opacity.clamp(0.0, 1.0);
        if let Some(f) = self.fill {
            let mut fl = FillLayer::new(paint(f));
            fl.opacity = opacity;
            items.push(AppearanceItem::Fill(fl));
        }
        if let Some(s) = self.stroke {
            let mut st = StrokeLayer::new(paint(s), self.width.clamp(0.1, 2000.0));
            st.cap = LineCap::Round;
            st.join = LineJoin::Round;
            st.opacity = opacity;
            items.push(AppearanceItem::Stroke(st));
        }
        Appearance {
            items,
            ..Appearance::default()
        }
    }
}

/// Formas que as ferramentas vetoriais criam.
#[derive(Clone, Debug)]
pub enum Shape {
    Line(Point, Point),
    Rectangle(Point, Point),
    Ellipse(Point, Point),
    /// Traço à mão livre: os pontos viram curvas Bézier ajustadas ([`fit_freehand`]).
    Freehand {
        points: Vec<Point>,
        closed: bool,
    },
    /// Caminho da caneta Bézier.
    Bezier(SubPath),
}

impl Shape {
    pub fn to_path(&self) -> Option<PathData> {
        let rect = |a: Point, b: Point| vectorcraft_geom::Rect::from_points(a, b);
        let path = match self {
            Shape::Line(a, b) => shapes::line(*a, *b),
            Shape::Rectangle(a, b) => shapes::rectangle(rect(*a, *b)),
            Shape::Ellipse(a, b) => shapes::ellipse(rect(*a, *b)),
            Shape::Freehand { points, closed } => {
                PathData::single(fit_freehand(points, 1.5, *closed))
            }
            Shape::Bezier(sp) => PathData::single(sp.clone()),
        };
        let ok = path.subpaths.iter().any(|s| s.anchors.len() >= 2)
            && path
                .subpaths
                .iter()
                .all(|s| s.anchors.iter().all(|a| a.p.is_finite()));
        ok.then_some(path)
    }
}

/// Ajusta curvas a um traço à mão livre: tira o tremor, passa uma spline pelos pontos (curvas
/// fechadas viram cantos) e simplifica até `tol`. Adaptado do Lápis do VectorCraft
/// (`engine/src/cmd/draw2/freehand.rs`, MIT OR Apache-2.0).
pub fn fit_freehand(pts: &[Point], tol: f64, closed: bool) -> SubPath {
    let min = (tol * 0.3).max(0.3);
    let (Some(&p0), Some(&pn)) = (pts.first(), pts.last()) else {
        return SubPath::new(vec![], closed);
    };
    let mut kept: Vec<Point> = vec![p0];
    for &p in pts.iter().skip(1) {
        if kept.last().is_none_or(|k| k.distance(p) >= min) {
            kept.push(p);
        }
    }
    if kept.len() > 1 {
        if let Some(k) = kept.last_mut() {
            *k = pn;
        }
    } else if pts.len() > 1 {
        kept.push(pn);
    }
    let n = kept.len();
    let marked: Vec<(Point, bool)> = (0..n)
        .map(|i| {
            let corner = match (
                i.checked_sub(1).and_then(|j| kept.get(j)),
                kept.get(i),
                kept.get(i + 1),
            ) {
                (Some(a), Some(b), Some(c)) => {
                    let (u, v) = (*b - *a, *c - *b);
                    u.dot(v) / (u.hypot() * v.hypot()).max(1e-12) < -0.2
                }
                _ => false,
            };
            (kept.get(i).copied().unwrap_or(p0), corner)
        })
        .collect();
    let sp = vectorcraft_tools::draw2::catmull_rom(&marked, closed);
    if sp.anchors.len() < 3 {
        return sp;
    }
    let opts = SimplifyOptions {
        tolerance: tol,
        corner_angle_deg: 60.0,
        straight_lines: false,
    };
    let fitted = vectorcraft_pathops::simplify_with(&PathData::single(sp.clone()), &opts);
    fitted
        .subpaths
        .into_iter()
        .next()
        .filter(|s| s.anchors.len() >= 2)
        .unwrap_or(sp)
}

thread_local! {
    static RENDERER: RefCell<Renderer> = RefCell::new(Renderer::new());
}

/// Rasteriza a arte de `art` para uma superfície `width × height`.
pub fn rasterize(art: &VectorArt, width: u32, height: u32) -> Surface {
    let opts = RenderOptions {
        artboards: false,
        background: None,
        ..RenderOptions::default()
    };
    let rendered = RENDERER.with(|r| {
        r.borrow_mut()
            .render(&art.doc, width, height, Affine::IDENTITY, &opts)
    });
    let bounds = Rect::new(0, 0, rendered.width as i32, rendered.height as i32);
    let bytes = rendered.to_straight();
    if bytes.len() != bounds.width() as usize * bounds.height() as usize * 4 {
        return Surface::new(PixelFormat::RGBA8);
    }
    let mut s = Surface::from_interleaved(PixelFormat::RGBA8, bounds, &bytes);
    s.prune();
    s
}

impl VectorArt {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            doc: vectorcraft_doc::Document::new(width as f64, height as f64),
            rev: next_rev(),
        }
    }

    /// A camada (do VectorCraft) onde os objetos novos entram.
    pub(crate) fn target(&self) -> Option<NodeId> {
        self.doc.layers.last().map(|l| l.id)
    }
}

impl Document {
    /// A arte vetorial da camada ativa, se ela for vetorial.
    pub fn active_vector(&self) -> Option<&VectorArt> {
        self.active_layer().and_then(|l| l.vector.as_ref())
    }

    pub fn active_is_vector(&self) -> bool {
        self.active_vector().is_some()
    }

    /// Nova camada vetorial acima da ativa.
    pub fn add_vector_layer(&mut self) {
        self.add_layer();
        let (w, h) = (self.width, self.height);
        if let Some(l) = self.active_layer_mut() {
            l.name = l.name.replace("Camada", "Vetor");
            l.vector = Some(VectorArt::new(w, h));
        }
    }

    /// Edita a arte vetorial da camada ativa e refaz a cópia rasterizada. Devolve a área a recompor.
    pub(crate) fn edit_vector(
        &mut self,
        checkpoint: bool,
        f: impl FnOnce(&mut VectorArt) -> bool,
    ) -> Rect {
        if !self.active_is_vector() {
            return Rect::EMPTY;
        }
        if checkpoint {
            self.checkpoint();
        }
        let (w, h) = (self.width, self.height);
        let bounds = self.bounds();
        let Some(layer) = self.active_layer_mut() else {
            return Rect::EMPTY;
        };
        let Some(art) = layer.vector.as_mut() else {
            return Rect::EMPTY;
        };
        if !f(art) {
            return Rect::EMPTY;
        }
        art.rev = next_rev();
        layer.surface = rasterize(art, w, h);
        bounds
    }

    /// Texto numa camada nova, com o começo da linha de base em `at`. Com `raster`, a camada
    /// já sai rasterizada (texto em camada raster); senão continua editável no Vetor pro.
    pub fn add_text(
        &mut self,
        text: &str,
        at: [f64; 2],
        size: f64,
        color: [f32; 3],
        raster: bool,
    ) -> Rect {
        if text.trim().is_empty() || !(size.is_finite() && size > 0.0) {
            return Rect::EMPTY;
        }
        self.add_vector_layer();
        if let Some(l) = self.active_layer_mut() {
            l.name = format!(
                "Texto: {}",
                text.lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(20)
                    .collect::<String>()
            );
        }
        let style = vectorcraft_doc::CharStyle {
            size,
            fill: paint(color),
            ..Default::default()
        };
        let r = self.edit_vector(false, |art| {
            let Some(parent) = art.target() else {
                return false;
            };
            let id = art.doc.alloc_id();
            let obj = vectorcraft_doc::TextObject::point(Point::new(at[0], at[1]), text, style);
            art.doc
                .insert(
                    Some(parent),
                    usize::MAX,
                    Node::new(id, vectorcraft_doc::NodeKind::Text(Box::new(obj))),
                )
                .is_ok()
        });
        if raster && let Some(l) = self.active_layer_mut() {
            l.vector = None;
        }
        r
    }

    /// Põe polígonos preenchidos com `color` na camada vetorial ativa, de uma vez (sem passo
    /// próprio no desfazer).
    pub(crate) fn add_filled_polygons(&mut self, polys: &[Vec<[f64; 2]>], color: [f32; 3]) -> Rect {
        let paths: Vec<Vec<Vec<[f64; 2]>>> = polys.iter().map(|p| vec![p.clone()]).collect();
        let style = VectorStyle {
            stroke: None,
            width: 0.0,
            fill: Some(color),
            ..VectorStyle::default()
        };
        self.add_polygon_paths(&paths, &style)
    }

    /// Acrescenta à camada vetorial ativa um caminho por item de `paths`, cada um feito de
    /// polígonos fechados (regra não zero: um polígono no sentido contrário vira furo).
    pub(crate) fn add_polygon_paths(
        &mut self,
        paths: &[Vec<Vec<[f64; 2]>>],
        style: &VectorStyle,
    ) -> Rect {
        let appearance = style.appearance();
        self.edit_vector(false, |art| {
            let Some(parent) = art.target() else {
                return false;
            };
            for polys in paths {
                let subpaths: Vec<SubPath> = polys
                    .iter()
                    .filter(|p| p.len() >= 3)
                    .map(|poly| {
                        let anchors = poly
                            .iter()
                            .map(|p| vectorcraft_geom::path::Anchor::corner(Point::new(p[0], p[1])))
                            .collect();
                        SubPath::new(anchors, true)
                    })
                    .collect();
                if subpaths.is_empty() {
                    continue;
                }
                let id = art.doc.alloc_id();
                if art
                    .doc
                    .insert(
                        Some(parent),
                        usize::MAX,
                        Node::path(id, PathData::new(subpaths), appearance.clone()),
                    )
                    .is_err()
                {
                    return false;
                }
            }
            true
        })
    }

    /// Acrescenta uma forma à camada vetorial ativa.
    pub fn vector_add(&mut self, shape: &Shape, style: &VectorStyle) -> Rect {
        let Some(path) = shape.to_path() else {
            return Rect::EMPTY;
        };
        let appearance = style.appearance();
        self.edit_vector(true, |art| {
            let Some(parent) = art.target() else {
                return false;
            };
            let id = art.doc.alloc_id();
            art.doc
                .insert(Some(parent), usize::MAX, Node::path(id, path, appearance))
                .is_ok()
        })
    }

    /// Acrescenta um objeto com vários caminhos (um balão com as bolinhas do rabicho, por
    /// exemplo), num passo só de desfazer.
    pub fn vector_add_paths(&mut self, subpaths: Vec<SubPath>, style: &VectorStyle) -> Rect {
        if !subpaths.iter().any(|s| s.anchors.len() >= 2) {
            return Rect::EMPTY;
        }
        let path = PathData::new(subpaths);
        let appearance = style.appearance();
        self.edit_vector(true, |art| {
            let Some(parent) = art.target() else {
                return false;
            };
            let id = art.doc.alloc_id();
            art.doc
                .insert(Some(parent), usize::MAX, Node::path(id, path, appearance))
                .is_ok()
        })
    }

    /// Acrescenta um traço da caneta vetorial: `points` à mão livre, com a pressão `pressures` de
    /// cada ponto (já passada pela curva) aplicada à largura e à opacidade como `pen` pede.
    pub fn vector_add_pen(
        &mut self,
        points: &[Point],
        pressures: &[f32],
        style: &VectorStyle,
        pen: &PenPressure,
    ) -> Rect {
        let shape = Shape::Freehand {
            points: points.to_vec(),
            closed: false,
        };
        let Some(path) = shape.to_path() else {
            return Rect::EMPTY;
        };
        let mut appearance = style.appearance();
        if let (Some(color), Some(pressure)) = (
            style.stroke,
            pressure_samples(points, pressures).filter(|_| pen.size || pen.opacity),
        ) {
            for item in &mut appearance.items {
                if let AppearanceItem::Stroke(st) = item {
                    pressure_stroke(st, color, &pressure, pen);
                }
            }
        }
        self.edit_vector(true, |art| {
            let Some(parent) = art.target() else {
                return false;
            };
            let id = art.doc.alloc_id();
            art.doc
                .insert(Some(parent), usize::MAX, Node::path(id, path, appearance))
                .is_ok()
        })
    }

    /// O objeto vetorial da camada ativa sob `(x, y)`, com tolerância `tol` em pixels da tela.
    pub fn vector_hit(&self, x: f64, y: f64, tol: f64) -> Option<NodeId> {
        let art = self.active_vector()?;
        let p = Point::new(x, y);
        let opt = HitOptions {
            tol: tol.max(0.5),
            ..HitOptions::default()
        };
        if let Some(h) = hit_test(&art.doc, p, opt) {
            return Some(h.leaf);
        }
        // Dentro de uma forma fechada sem preenchimento também conta (o toque não precisa achar o
        // contorno fino), como no balde e na seleção do Clip Studio.
        let layer = art.doc.node(art.target()?)?;
        layer
            .children()?
            .iter()
            .rev()
            .find(|n| n.visible && !n.locked && encloses(n, p))
            .map(|n| n.id)
    }

    /// Contorno (caminho) do objeto `id`, para desenhar a seleção.
    pub fn vector_outline(&self, id: NodeId) -> Option<vectorcraft_geom::BezPath> {
        self.active_vector()?
            .doc
            .node(id)?
            .path_data()
            .map(|p| p.to_bezpath())
    }

    /// Guarda o estado antes de arrastar objetos (o arraste em si não grava passos).
    pub fn vector_begin_drag(&mut self) {
        self.checkpoint();
    }

    /// Move o objeto `id` por `(dx, dy)`.
    pub fn vector_move(&mut self, id: NodeId, dx: f64, dy: f64) -> Rect {
        self.edit_vector(false, |art| {
            match art.doc.node_mut(id).and_then(|n| n.path_data_mut()) {
                Some(p) => {
                    p.transform(Affine::translate(Vec2::new(dx, dy)));
                    true
                }
                None => false,
            }
        })
    }

    pub fn vector_delete(&mut self, id: NodeId) -> Rect {
        self.edit_vector(true, |art| art.doc.remove(id).is_ok())
    }

    /// Troca o traço e o preenchimento do objeto `id`.
    pub fn vector_restyle(&mut self, id: NodeId, style: &VectorStyle) -> Rect {
        let appearance = style.appearance();
        self.edit_vector(true, |art| match art.doc.node_mut(id) {
            Some(n) => {
                n.appearance = Shared::new(appearance);
                true
            }
            None => false,
        })
    }

    /// Pinta o preenchimento do objeto sob `(x, y)` (o balde numa camada vetorial).
    pub fn vector_fill_at(&mut self, x: f64, y: f64, color: [f32; 3]) -> Rect {
        let Some(id) = self.vector_hit(x, y, 2.0) else {
            return Rect::EMPTY;
        };
        self.edit_vector(true, |art| {
            let Some(n) = art.doc.node_mut(id) else {
                return false;
            };
            let mut app = (*n.appearance).clone();
            match app.fill_mut() {
                Some(f) => f.paint = paint(color),
                None => app
                    .items
                    .insert(0, AppearanceItem::Fill(FillLayer::new(paint(color)))),
            }
            n.appearance = Shared::new(app);
            true
        })
    }

    /// Rasteriza a camada vetorial ativa: ela vira uma camada raster comum (para pintar por cima).
    pub fn rasterize_layer(&mut self) {
        if !self.active_is_vector() {
            return;
        }
        self.checkpoint();
        if let Some(l) = self.active_layer_mut() {
            l.vector = None;
        }
    }
}

/// `p` está dentro de algum subcaminho fechado de `n`?
fn encloses(n: &Node, p: Point) -> bool {
    use vectorcraft_geom::Shape as _;
    let Some(path) = n.path_data() else {
        return false;
    };
    path.subpaths
        .iter()
        .filter(|sp| sp.closed)
        .any(|sp| PathData::single(sp.clone()).to_bezpath().winding(p) != 0)
}

impl Layer {
    pub fn is_vector(&self) -> bool {
        self.vector.is_some()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn doc_with_vector() -> Document {
        let mut d = Document::new(200, 200);
        d.add_vector_layer();
        d
    }

    fn last_stroke(d: &Document) -> StrokeLayer {
        let art = d.active_vector().unwrap();
        let layer = art.doc.node(art.target().unwrap()).unwrap();
        let node = layer.children().unwrap().last().unwrap();
        node.appearance
            .items
            .iter()
            .find_map(|i| match i {
                AppearanceItem::Stroke(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn pen_pressure_shapes_width_and_opacity() {
        let mut d = doc_with_vector();
        let pts: Vec<Point> = (0..=40)
            .map(|i| Point::new(20.0 + i as f64 * 4.0, 100.0))
            .collect();
        let pressures: Vec<f32> = (0..=40).map(|i| i as f32 / 40.0).collect();
        let style = VectorStyle {
            width: 20.0,
            opacity: 0.8,
            ..VectorStyle::default()
        };
        let pen = PenPressure {
            opacity: true,
            ..PenPressure::default()
        };
        let r = d.vector_add_pen(&pts, &pressures, &style, &pen);
        assert!(!r.is_empty());
        let st = last_stroke(&d);
        assert!((st.opacity - 0.8).abs() < 1e-6);
        let prof = st.profile.unwrap();
        let (start, _) = prof.at(0.0);
        let (end, _) = prof.at(1.0);
        assert!(start < 0.2 && end > 0.99, "{start} → {end}");
        assert_eq!(st.gradient_mode, StrokeGradientMode::Along);
        assert!(matches!(st.paint, Paint::Gradient(_)));
        // O começo, leve, é mais fino que o fim.
        let s = &d.layers[1].surface;
        assert_eq!(s.rgba(24, 92)[3], 0.0);
        assert!(s.rgba(176, 92)[3] > 0.3);

        // Sem pressão (mouse): traço uniforme e cor lisa.
        d.vector_add_pen(&pts, &vec![1.0; pts.len()], &style, &pen);
        let st = last_stroke(&d);
        assert!(st.profile.is_none() && st.pressure.is_none());
        assert!(matches!(st.paint, Paint::Solid { .. }));
    }

    #[test]
    fn text_layer_draws_letters() {
        let mut doc = Document::new(300, 120);
        let r = doc.add_text("Olá ArtApp", [20.0, 80.0], 48.0, [0.0, 0.0, 0.0], true);
        assert!(!r.is_empty());
        let l = doc.active_layer().unwrap();
        assert!(!l.is_vector());
        assert!(
            !l.surface.content_bounds().is_empty(),
            "nenhuma letra desenhada"
        );
        assert!(
            doc.add_text("  ", [0.0, 0.0], 10.0, [0.0; 3], true)
                .is_empty()
        );
    }

    #[test]
    fn rectangle_is_drawn_and_undoable() {
        let mut d = doc_with_vector();
        let style = VectorStyle {
            stroke: Some([1.0, 0.0, 0.0]),
            width: 6.0,
            fill: Some([0.0, 0.0, 1.0]),
            ..VectorStyle::default()
        };
        let r = d.vector_add(
            &Shape::Rectangle(Point::new(20.0, 20.0), Point::new(120.0, 100.0)),
            &style,
        );
        assert!(!r.is_empty());
        let s = &d.layers[1].surface;
        let edge = s.rgba(20, 60);
        assert!(edge[0] > 0.8 && edge[3] > 0.9, "traço vermelho: {edge:?}");
        let inside = s.rgba(70, 60);
        assert!(
            inside[2] > 0.9 && inside[0] < 0.1,
            "preenchimento azul: {inside:?}"
        );
        assert_eq!(s.rgba(150, 150)[3], 0.0);
        assert!(d.undo());
        assert_eq!(d.layers[1].surface.rgba(70, 60)[3], 0.0);
    }

    #[test]
    fn hit_move_restyle_delete() {
        let mut d = doc_with_vector();
        d.vector_add(
            &Shape::Ellipse(Point::new(50.0, 50.0), Point::new(150.0, 150.0)),
            &VectorStyle {
                fill: Some([0.0, 1.0, 0.0]),
                ..Default::default()
            },
        );
        let id = d.vector_hit(100.0, 100.0, 2.0).unwrap();
        // Contorno sem preenchimento: tocar dentro também acha o objeto.
        d.vector_add(
            &Shape::Rectangle(Point::new(10.0, 160.0), Point::new(60.0, 190.0)),
            &VectorStyle::default(),
        );
        assert!(d.vector_hit(35.0, 175.0, 1.0).is_some());
        assert!(d.vector_hit(10.0, 10.0, 2.0).is_none());
        d.vector_begin_drag();
        d.vector_move(id, 30.0, 0.0);
        assert!(d.layers[1].surface.rgba(170, 100)[3] > 0.9);
        d.vector_fill_at(130.0, 100.0, [1.0, 1.0, 0.0]);
        let px = d.layers[1].surface.rgba(130, 100);
        assert!(px[0] > 0.9 && px[1] > 0.9 && px[2] < 0.1, "{px:?}");
        d.vector_delete(id);
        assert_eq!(d.layers[1].surface.rgba(130, 100)[3], 0.0);
    }

    #[test]
    fn freehand_is_fitted_to_few_curves() {
        let pts: Vec<Point> = (0..200)
            .map(|i| Point::new(i as f64, 50.0 + (i as f64 / 20.0).sin() * 30.0))
            .collect();
        let sp = fit_freehand(&pts, 1.5, false);
        assert!(
            sp.anchors.len() >= 2 && sp.anchors.len() < 40,
            "{}",
            sp.anchors.len()
        );
        let mut d = doc_with_vector();
        assert!(
            !d.vector_add(
                &Shape::Freehand {
                    points: pts,
                    closed: false
                },
                &VectorStyle::default()
            )
            .is_empty()
        );
    }

    #[test]
    fn raster_tools_skip_vector_layers_and_rasterize_converts() {
        let mut d = doc_with_vector();
        assert!(
            crate::PaintSession::begin(&mut d, &crate::BrushSettings::default(), 1.0).is_none()
        );
        d.rasterize_layer();
        assert!(!d.active_is_vector());
        assert!(
            crate::PaintSession::begin(&mut d, &crate::BrushSettings::default(), 1.0).is_some()
        );
    }
}
