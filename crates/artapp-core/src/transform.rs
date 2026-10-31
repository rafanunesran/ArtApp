//! Transformar na tela (o "Transformar" do Procreate e do Clip Studio): o conteúdo da camada
//! ativa, ou da seleção, sai da camada enquanto a pessoa arrasta as alças, e volta deformado
//! para o quadrilátero escolhido ao aplicar. Livre e uniforme dão retângulos girados; distorcer
//! move cada canto (perspectiva).

use photocraft_algo::transform::warp_surface;
pub use photocraft_algo::transform::{Homography, Interp};
use photocraft_color::PixelFormat;
use photocraft_geom::Rect;
use photocraft_raster::Surface;

use crate::document::Document;
use crate::edit::{downscale, paste_over, split_by_mask};
use crate::selection::Selection;

/// Cantos em sentido horário a partir do de cima à esquerda, em pixels do documento.
pub type Quad = [[f64; 2]; 4];

pub struct TransformSession {
    /// Onde o conteúdo estava (o retângulo de origem).
    pub rect: Rect,
    layer_id: u64,
    moving: Surface,
    /// A camada antes de começar, para cancelar.
    original: Surface,
    selection: Option<Selection>,
}

impl TransformSession {
    /// Tira o conteúdo da camada ativa (ou da seleção) para transformar. `None` se não há o
    /// que transformar.
    pub fn begin(doc: &mut Document) -> Option<Self> {
        let rect = doc.edit_area(false)?;
        let selection = doc.selection.clone();
        doc.checkpoint();
        let layer = doc.active_layer_mut()?;
        let original = layer.surface.clone();
        let (moving, rest) = match &selection {
            Some(s) => split_by_mask(&layer.surface, &s.mask, rect),
            None => (layer.surface.clone(), Surface::new(PixelFormat::RGBA8)),
        };
        layer.surface = rest;
        let layer_id = layer.id;
        doc.selection = None;
        Some(Self {
            rect,
            layer_id,
            moving,
            original,
            selection,
        })
    }

    pub fn layer_id(&self) -> u64 {
        self.layer_id
    }

    /// O retângulo de origem como quadrilátero.
    pub fn start_quad(&self) -> Quad {
        let r = self.rect;
        let (x0, y0, x1, y1) = (
            f64::from(r.x0),
            f64::from(r.y0),
            f64::from(r.x1),
            f64::from(r.y1),
        );
        [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    /// O que está sendo transformado, em RGBA 8 bits sem pré-multiplicar, reduzido para caber
    /// em `max_side` (para a prévia na tela).
    pub fn preview(&self, max_side: u32) -> (u32, u32, Vec<u8>) {
        let (w, h) = (self.rect.width(), self.rect.height());
        let mut px = vec![[0.0f32; 4]; w as usize * h as usize];
        self.moving.read_rgba_into(self.rect, &mut px);
        let bytes: Vec<u8> = px
            .iter()
            .flatten()
            .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect();
        let k = (f64::from(max_side.max(1)) / f64::from(w.max(h))).min(1.0);
        if k >= 1.0 {
            return (w, h, bytes);
        }
        let nw = ((f64::from(w) * k).round() as u32).max(1);
        let nh = ((f64::from(h) * k).round() as u32).max(1);
        (nw, nh, downscale(&bytes, w, h, nw, nh))
    }

    /// Põe o conteúdo de volta, deformado para `quad`. Devolve a área alterada.
    pub fn end(self, doc: &mut Document, quad: Quad, interp: Interp) -> Rect {
        let r = self.rect;
        let src = [
            f64::from(r.x0),
            f64::from(r.y0),
            f64::from(r.x1),
            f64::from(r.y1),
        ];
        let Some(h) = Homography::rect_to_quad(src, quad) else {
            return self.cancel(doc);
        };
        let canvas = doc.bounds();
        let Some(layer) = doc.layers.iter_mut().find(|l| l.id == self.layer_id) else {
            return Rect::EMPTY;
        };
        let Ok(warped) = warp_surface(&self.moving, r, &h, interp) else {
            layer.surface = self.original;
            return r;
        };
        let out_r = paste_over(&mut layer.surface, &warped, canvas);
        // A seleção acompanha o conteúdo.
        doc.selection = self.selection.and_then(|s| {
            let pts: Vec<[f64; 2]> = s
                .outline
                .iter()
                .map(|p| {
                    let (x, y) = h.apply(p[0], p[1]);
                    [x, y]
                })
                .collect();
            (pts.len() >= 3 && !s.inverted)
                .then(|| Selection::polygon(&pts, doc.width, doc.height))
                .flatten()
        });
        r.union(&out_r)
    }

    /// Desiste: a camada volta como estava e o passo do desfazer some.
    pub fn cancel(self, doc: &mut Document) -> Rect {
        if let Some(layer) = doc.layers.iter_mut().find(|l| l.id == self.layer_id) {
            layer.surface = self.original;
            doc.drop_checkpoint();
        }
        doc.selection = self.selection;
        self.rect
    }
}

/// O quadrilátero é convexo e não está virado do avesso pela metade (o distorcer recusa isso).
pub fn is_convex(q: &Quad) -> bool {
    let mut sign = 0.0;
    for i in 0..4 {
        let (a, b, c) = (q[i], q[(i + 1) % 4], q[(i + 2) % 4]);
        let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
        if cross.abs() < 1e-6 {
            return false;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}

/// Centro (média dos cantos).
pub fn center(q: &Quad) -> [f64; 2] {
    let (x, y) = q.iter().fold((0.0, 0.0), |a, p| (a.0 + p[0], a.1 + p[1]));
    [x / 4.0, y / 4.0]
}

/// Gira os cantos em volta de `c` (radianos).
pub fn rotate(q: &Quad, c: [f64; 2], angle: f64) -> Quad {
    let (sin, cos) = angle.sin_cos();
    q.map(|p| {
        let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
        [c[0] + dx * cos - dy * sin, c[1] + dx * sin + dy * cos]
    })
}

/// Arrasta o canto `i` (0..4) para `p` num paralelogramo, com o canto oposto parado. Com
/// `uniform`, mantém a proporção (a escala vem da projeção na diagonal).
pub fn drag_corner(q: &Quad, i: usize, p: [f64; 2], uniform: bool) -> Option<Quad> {
    let (t, s) = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)][i % 4];
    let fixed = q[(i + 2) % 4];
    let e1 = [q[1][0] - q[0][0], q[1][1] - q[0][1]];
    let e2 = [q[3][0] - q[0][0], q[3][1] - q[0][1]];
    let d = [p[0] - fixed[0], p[1] - fixed[1]];
    let (st, ss) = (2.0 * t - 1.0, 2.0 * s - 1.0);
    let (k1, k2) = if uniform {
        let diag = [st * e1[0] + ss * e2[0], st * e1[1] + ss * e2[1]];
        let dd = diag[0] * diag[0] + diag[1] * diag[1];
        if dd < 1e-9 {
            return None;
        }
        let k = (d[0] * diag[0] + d[1] * diag[1]) / dd;
        (k, k)
    } else {
        // d = a·e1 + b·e2
        let det = e1[0] * e2[1] - e1[1] * e2[0];
        if det.abs() < 1e-9 {
            return None;
        }
        let a = (d[0] * e2[1] - d[1] * e2[0]) / det;
        let b = (e1[0] * d[1] - e1[1] * d[0]) / det;
        (a * st, b * ss)
    };
    if k1.abs() < 1e-3 || k2.abs() < 1e-3 {
        return None;
    }
    Some(rebuild(e1, e2, k1, k2, fixed, (1.0 - t, 1.0 - s)))
}

/// Arrasta o meio do lado `side` (0 cima, 1 direita, 2 baixo, 3 esquerda) para `p`, com o lado
/// oposto parado. Com `uniform`, a outra direção cresce junto, em volta do meio do lado oposto.
pub fn drag_edge(q: &Quad, side: usize, p: [f64; 2], uniform: bool) -> Option<Quad> {
    let e1 = [q[1][0] - q[0][0], q[1][1] - q[0][1]];
    let e2 = [q[3][0] - q[0][0], q[3][1] - q[0][1]];
    // Lado tocado: (eixo que muda, posição dele 0 ou 1).
    let (axis_e, axis_t, other) = match side % 4 {
        0 => (e2, 0.0, e1),
        1 => (e1, 1.0, e2),
        2 => (e2, 1.0, e1),
        _ => (e1, 0.0, e2),
    };
    let o = q[0];
    // Meio do lado oposto (parado).
    let fixed = [
        o[0] + (1.0 - axis_t) * axis_e[0] + 0.5 * other[0],
        o[1] + (1.0 - axis_t) * axis_e[1] + 0.5 * other[1],
    ];
    let ll = axis_e[0] * axis_e[0] + axis_e[1] * axis_e[1];
    if ll < 1e-9 {
        return None;
    }
    let sign = 2.0 * axis_t - 1.0;
    let k = ((p[0] - fixed[0]) * axis_e[0] + (p[1] - fixed[1]) * axis_e[1]) / ll * sign;
    if k.abs() < 1e-3 {
        return None;
    }
    let ko = if uniform { k.abs() } else { 1.0 };
    let (na, no) = (axis_e.map(|v| v * k), other.map(|v| v * ko));
    let base = [
        fixed[0] - (1.0 - axis_t) * na[0] - 0.5 * no[0],
        fixed[1] - (1.0 - axis_t) * na[1] - 0.5 * no[1],
    ];
    let corner = |a: f64, b: f64| {
        [
            base[0] + a * na[0] + b * no[0],
            base[1] + a * na[1] + b * no[1],
        ]
    };
    Some(if matches!(side % 4, 1 | 3) {
        [
            corner(0.0, 0.0),
            corner(1.0, 0.0),
            corner(1.0, 1.0),
            corner(0.0, 1.0),
        ]
    } else {
        [
            corner(0.0, 0.0),
            corner(0.0, 1.0),
            corner(1.0, 1.0),
            corner(1.0, 0.0),
        ]
    })
}

/// Paralelogramo de lados `k1·e1` e `k2·e2` com o canto (ft, fs) em `fixed`.
fn rebuild(e1: [f64; 2], e2: [f64; 2], k1: f64, k2: f64, fixed: [f64; 2], f: (f64, f64)) -> Quad {
    let (a, b) = (e1.map(|v| v * k1), e2.map(|v| v * k2));
    let o = [
        fixed[0] - f.0 * a[0] - f.1 * b[0],
        fixed[1] - f.0 * a[1] - f.1 * b[1],
    ];
    let at = |t: f64, s: f64| [o[0] + t * a[0] + s * b[0], o[1] + t * a[1] + s * b[1]];
    [at(0.0, 0.0), at(1.0, 0.0), at(1.0, 1.0), at(0.0, 1.0)]
}

/// Encaixa o retângulo `r` na tela `w`×`h`, no centro, mantendo a proporção.
pub fn fit(r: Rect, w: u32, h: u32) -> Quad {
    let (rw, rh) = (f64::from(r.width().max(1)), f64::from(r.height().max(1)));
    let k = (f64::from(w) / rw).min(f64::from(h) / rh);
    let (nw, nh) = (rw * k, rh * k);
    let (x0, y0) = ((f64::from(w) - nw) / 2.0, (f64::from(h) - nh) / 2.0);
    [[x0, y0], [x0 + nw, y0], [x0 + nw, y0 + nh], [x0, y0 + nh]]
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn close(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
    }

    #[test]
    fn corner_and_edge_drags_keep_the_opposite_side() {
        let q: Quad = [[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]];
        let n = drag_corner(&q, 2, [20.0, 20.0], false).unwrap();
        assert!(close(n[0], [0.0, 0.0]) && close(n[2], [20.0, 20.0]));
        let u = drag_corner(&q, 2, [20.0, 20.0], true).unwrap();
        // Proporção 2:1 mantida.
        let (w, h) = (u[1][0] - u[0][0], u[3][1] - u[0][1]);
        assert!((w / h - 2.0).abs() < 1e-6 && close(u[0], [0.0, 0.0]));
        let e = drag_edge(&q, 1, [30.0, 2.0], false).unwrap();
        assert!(close(e[0], [0.0, 0.0]) && close(e[2], [30.0, 5.0]));
        let t = drag_edge(&q, 0, [3.0, -5.0], false).unwrap();
        assert!(close(t[0], [0.0, -5.0]) && close(t[2], [10.0, 5.0]));
        assert!(is_convex(&q) && !is_convex(&[q[0], q[2], q[1], q[3]]));
    }

    #[test]
    fn session_moves_content_and_cancel_restores() {
        let mut doc = Document::new(40, 40);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 10, 10), &[1.0, 0.0, 0.0, 1.0]);
        let s = TransformSession::begin(&mut doc).unwrap();
        assert_eq!(doc.layers[0].surface.rgba(5, 5)[3], 0.0);
        let (w, h, px) = s.preview(4);
        assert_eq!((w, h, px.len()), (4, 4, 64));
        s.cancel(&mut doc);
        assert_eq!(doc.layers[0].surface.rgba(5, 5)[3], 1.0);
        assert!(!doc.can_undo());

        let s = TransformSession::begin(&mut doc).unwrap();
        let q = [[20.0, 20.0], [40.0, 20.0], [40.0, 40.0], [20.0, 40.0]];
        s.end(&mut doc, q, Interp::Bilinear);
        assert_eq!(doc.layers[0].surface.rgba(5, 5)[3], 0.0);
        assert!(doc.layers[0].surface.rgba(30, 30)[0] > 0.9);
        assert!(doc.undo());
        assert_eq!(doc.layers[0].surface.rgba(5, 5)[3], 1.0);
    }
}
