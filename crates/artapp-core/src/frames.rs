//! Requadros de quadrinho (a "Borda de quadro" do Clip Studio): divide a página numa grade de
//! quadros com margem e calha, numa camada vetorial nova. A calha sai pintada da cor do papel
//! para cobrir o que passar do quadro, e as bordas ficam como traços editáveis.

use photocraft_geom::Rect;

use crate::document::Document;
use crate::vector::VectorStyle;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frames {
    pub rows: u32,
    pub cols: u32,
    /// Margem da página, em pixels.
    pub margin: f32,
    /// Espaço entre os quadros, em pixels.
    pub gutter: f32,
    /// Espessura da borda, em pixels.
    pub width: f32,
    /// Pintar a calha e a margem com a cor do papel.
    pub cover: bool,
}

impl Default for Frames {
    fn default() -> Self {
        Self {
            rows: 3,
            cols: 2,
            margin: 80.0,
            gutter: 30.0,
            width: 6.0,
            cover: true,
        }
    }
}

pub const MAX_SPLIT: u32 = 8;

/// Os quadros como `[x0, y0, x1, y1]`, linha a linha. Vazio se não couberem.
pub fn frame_rects(p: &Frames, w: f64, h: f64) -> Vec<[f64; 4]> {
    let (rows, cols) = (p.rows.clamp(1, MAX_SPLIT), p.cols.clamp(1, MAX_SPLIT));
    let (m, g) = (f64::from(p.margin.max(0.0)), f64::from(p.gutter.max(0.0)));
    let fw = (w - 2.0 * m - g * f64::from(cols - 1)) / f64::from(cols);
    let fh = (h - 2.0 * m - g * f64::from(rows - 1)) / f64::from(rows);
    if fw < 1.0 || fh < 1.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let x0 = m + f64::from(c) * (fw + g);
            let y0 = m + f64::from(r) * (fh + g);
            out.push([x0, y0, x0 + fw, y0 + fh]);
        }
    }
    out
}

fn rect_poly(r: [f64; 4], clockwise: bool) -> Vec<[f64; 2]> {
    let pts = vec![[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]];
    if clockwise {
        pts
    } else {
        pts.into_iter().rev().collect()
    }
}

/// Recorta o polígono convexo `poly` ao semiplano `n·(p − a) ≤ d` (Sutherland–Hodgman).
fn clip_half(poly: &[[f64; 2]], a: [f64; 2], n: [f64; 2], d: f64) -> Vec<[f64; 2]> {
    let side = |p: [f64; 2]| n[0] * (p[0] - a[0]) + n[1] * (p[1] - a[1]) - d;
    let mut out = Vec::new();
    for (i, &cur) in poly.iter().enumerate() {
        let prev = poly[(i + poly.len() - 1) % poly.len()];
        let (sc, sp) = (side(cur), side(prev));
        if (sc <= 0.0) != (sp <= 0.0) {
            let t = sp / (sp - sc);
            out.push([
                prev[0] + (cur[0] - prev[0]) * t,
                prev[1] + (cur[1] - prev[1]) * t,
            ]);
        }
        if sc <= 0.0 {
            out.push(cur);
        }
    }
    // Pontos repetidos (a reta passando num canto) atrapalham o contorno.
    out.dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6);
    if out.len() > 1
        && (out[0][0] - out[out.len() - 1][0]).hypot(out[0][1] - out[out.len() - 1][1]) < 1e-6
    {
        out.pop();
    }
    out
}

fn area(poly: &[[f64; 2]]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (p, q) = (poly[i], poly[(i + 1) % n]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum::<f64>()
        / 2.0
}

/// Divide o quadro `poly` (convexo) pela reta de `a` a `b`, deixando uma calha de largura `gap`
/// entre as duas partes. Devolve as duas partes e a faixa da calha, ou `None` se a reta não
/// atravessa o quadro.
pub fn split_poly(
    poly: &[[f64; 2]],
    a: [f64; 2],
    b: [f64; 2],
    gap: f64,
) -> Option<[Vec<[f64; 2]>; 3]> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy);
    if len < 1e-6 || poly.len() < 3 {
        return None;
    }
    let n = [-dy / len, dx / len];
    let half = gap.max(0.0) / 2.0;
    let one = clip_half(poly, a, n, -half);
    let other = clip_half(poly, a, [-n[0], -n[1]], -half);
    let strip = {
        let s = clip_half(poly, a, n, half);
        clip_half(&s, a, [-n[0], -n[1]], half)
    };
    // As duas partes precisam de área de verdade (senão a reta só raspou o quadro).
    let min = area(poly).abs() * 0.01;
    (one.len() >= 3 && other.len() >= 3 && area(&one).abs() > min && area(&other).abs() > min)
        .then_some([one, other, strip])
}

/// O contorno de um objeto como polígono, se ele for um polígono fechado só de cantos.
fn as_polygon(n: &vectorcraft_doc::Node) -> Option<Vec<[f64; 2]>> {
    let path = n.path_data()?;
    let [sp] = path.subpaths.as_slice() else {
        return None;
    };
    let corner = |a: &vectorcraft_geom::path::Anchor| {
        a.h_in.distance(a.p) < 1e-6 && a.h_out.distance(a.p) < 1e-6
    };
    (sp.closed && sp.anchors.len() >= 3 && sp.anchors.iter().all(corner))
        .then(|| sp.anchors.iter().map(|a| [a.p.x, a.p.y]).collect())
}

/// O ponto `p` está dentro do polígono?
fn inside(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut odd = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            odd = !odd;
        }
    }
    odd
}

impl Document {
    /// Camada vetorial "Requadros" com as bordas na cor `color`. Devolve a área mudada (vazia se
    /// os quadros não couberem na página).
    pub fn add_frames(&mut self, p: &Frames, color: [f32; 3]) -> Rect {
        let (w, h) = (f64::from(self.width), f64::from(self.height));
        let rects = frame_rects(p, w, h);
        if rects.is_empty() {
            return Rect::EMPTY;
        }
        self.add_vector_layer();
        if let Some(l) = self.active_layer_mut() {
            l.name = "Requadros".into();
        }
        let mut dirty = Rect::EMPTY;
        if p.cover {
            // A página inteira menos os quadros (no sentido contrário, viram furos).
            let paper = [self.paper[0], self.paper[1], self.paper[2]];
            let mut polys = vec![rect_poly([0.0, 0.0, w, h], true)];
            polys.extend(rects.iter().map(|r| rect_poly(*r, false)));
            let style = VectorStyle {
                stroke: None,
                width: 0.0,
                fill: Some(paper),
                ..VectorStyle::default()
            };
            dirty = dirty.union(&self.add_polygon_paths(&[polys], &style));
        }
        let borders: Vec<_> = rects.iter().map(|r| vec![rect_poly(*r, true)]).collect();
        let style = VectorStyle {
            stroke: Some(color),
            width: f64::from(p.width.max(0.1)),
            fill: None,
            ..VectorStyle::default()
        };
        dirty.union(&self.add_polygon_paths(&borders, &style))
    }

    /// Divide o quadro (um polígono com borda na camada vetorial ativa) que a reta de `a` a `b`
    /// atravessa, como "Dividir quadro" do Clip Studio: as duas partes ficam com a borda do
    /// original e a calha de largura `gap` sai pintada da cor do papel.
    pub fn split_frame(&mut self, a: [f64; 2], b: [f64; 2], gap: f64) -> Rect {
        let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        let paper = [self.paper[0], self.paper[1], self.paper[2]];
        let Some(art) = self.active_vector() else {
            return Rect::EMPTY;
        };
        let Some(layer) = art.target().and_then(|t| art.doc.node(t)) else {
            return Rect::EMPTY;
        };
        // O menor quadro com borda que tem o meio da reta dentro e que ela atravessa.
        let found = layer
            .children()
            .into_iter()
            .flatten()
            .filter(|n| n.visible && !n.locked && n.appearance.stroke().is_some())
            .filter_map(|n| {
                let poly = as_polygon(n)?;
                if !inside(&poly, mid) {
                    return None;
                }
                let parts = split_poly(&poly, a, b, gap)?;
                Some((area(&poly).abs(), n.id, parts))
            })
            .min_by(|x, y| x.0.total_cmp(&y.0));
        let Some((_, id, [one, other, strip])) = found else {
            return Rect::EMPTY;
        };
        let poly_path = |poly: &[[f64; 2]]| {
            let anchors = poly
                .iter()
                .map(|p| {
                    vectorcraft_geom::path::Anchor::corner(vectorcraft_geom::Point::new(p[0], p[1]))
                })
                .collect();
            vectorcraft_geom::path::PathData::single(vectorcraft_geom::path::SubPath::new(
                anchors, true,
            ))
        };
        let cover = VectorStyle {
            stroke: None,
            width: 0.0,
            fill: Some(paper),
            ..VectorStyle::default()
        }
        .appearance();
        self.edit_vector(true, |art| {
            let Some(parent) = art.doc.parent_of(id) else {
                return false;
            };
            let Some(index) = art
                .doc
                .children(Some(parent))
                .and_then(|c| c.iter().position(|n| n.id == id))
            else {
                return false;
            };
            let Ok(old) = art.doc.remove(id) else {
                return false;
            };
            let look = (*old.appearance).clone();
            let mut nodes = Vec::new();
            if strip.len() >= 3 {
                nodes.push((poly_path(&strip), cover.clone()));
            }
            nodes.push((poly_path(&one), look.clone()));
            nodes.push((poly_path(&other), look));
            for (k, (path, appearance)) in nodes.into_iter().enumerate() {
                let nid = art.doc.alloc_id();
                if art
                    .doc
                    .insert(
                        Some(parent),
                        index + k,
                        vectorcraft_doc::Node::path(nid, path, appearance),
                    )
                    .is_err()
                {
                    return false;
                }
            }
            true
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_fills_the_page_inside_the_margin() {
        let p = Frames {
            rows: 2,
            cols: 3,
            margin: 10.0,
            gutter: 5.0,
            ..Frames::default()
        };
        let r = frame_rects(&p, 120.0, 80.0);
        assert_eq!(r.len(), 6);
        assert_eq!(r[0], [10.0, 10.0, 40.0, 37.5]);
        assert_eq!(r[5], [80.0, 42.5, 110.0, 70.0]);
        assert!(frame_rects(&p, 20.0, 80.0).is_empty());
    }

    #[test]
    fn split_leaves_a_gutter() {
        let sq = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
        let parts = split_poly(&sq, [50.0, -10.0], [50.0, 110.0], 10.0);
        let Some([one, other, strip]) = parts else {
            unreachable!("a reta atravessa o quadro");
        };
        let xs = |p: &Vec<[f64; 2]>| {
            p.iter()
                .map(|q| q[0])
                .fold((f64::MAX, f64::MIN), |m, x| (m.0.min(x), m.1.max(x)))
        };
        let (a, b) = (xs(&one), xs(&other));
        let (lo, hi) = if a.0 < b.0 { (a, b) } else { (b, a) };
        assert!(
            (lo.1 - 45.0).abs() < 1e-9 && (hi.0 - 55.0).abs() < 1e-9,
            "{lo:?} {hi:?}"
        );
        assert!((area(&strip).abs() - 1000.0).abs() < 1e-6);
        // Uma reta de fora não divide nada.
        assert!(split_poly(&sq, [200.0, 0.0], [200.0, 100.0], 10.0).is_none());
    }

    #[test]
    fn split_frame_in_document() {
        let mut d = Document::new(400, 300);
        let p = Frames {
            rows: 1,
            cols: 1,
            ..Frames::default()
        };
        d.add_frames(&p, [0.0; 3]);
        let count = |d: &Document| {
            let art = d.active_vector().map(|a| {
                a.doc
                    .node(a.target().unwrap_or_default())
                    .and_then(|l| l.children())
                    .map_or(0, |c| c.len())
            });
            art.unwrap_or(0)
        };
        let before = count(&d);
        assert!(!d.split_frame([200.0, 0.0], [200.0, 300.0], 20.0).is_empty());
        assert_eq!(count(&d), before + 2, "borda vira duas e ganha a calha");
    }
}
