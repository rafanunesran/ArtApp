//! Correção de linha em camada vetorial (como "Correção de linha › Simplificar" e "Conectar
//! linhas" do Clip Studio, refeito do zero): simplifica os traços tirando pontos de ancoragem
//! e junta pontas soltas que ficaram perto uma da outra.

use photocraft_geom::Rect;
use vectorcraft_doc::NodeId;
use vectorcraft_geom::Point;
use vectorcraft_geom::path::{PathData, SubPath};
use vectorcraft_pathops::SimplifyOptions;

use crate::document::Document;
use crate::vector::VectorArt;

/// Tolerância máxima do Simplificar, em pixels.
pub const SIMPLIFY_MAX: f64 = 40.0;
/// Distância máxima entre pontas para Conectar, em pixels.
pub const CONNECT_MAX: f64 = 200.0;

/// Os objetos com caminho da camada vetorial, ou só `only`, se houver.
fn path_ids(art: &VectorArt, only: Option<NodeId>) -> Vec<NodeId> {
    let Some(layer) = art.doc.layers.last() else {
        return vec![];
    };
    layer
        .children()
        .map(|c| {
            c.iter()
                .filter(|n| n.path_data().is_some() && !n.locked)
                .map(|n| n.id)
                .filter(|id| only.is_none_or(|o| o == *id))
                .collect()
        })
        .unwrap_or_default()
}

fn anchors_of(p: &PathData) -> usize {
    p.subpaths.iter().map(|s| s.anchors.len()).sum()
}

/// O único subcaminho aberto de um objeto (só esses dá para conectar).
fn open_line(art: &VectorArt, id: NodeId) -> Option<SubPath> {
    let p = art.doc.node(id)?.path_data()?;
    match p.subpaths.as_slice() {
        [s] if !s.closed && s.anchors.len() >= 2 => Some(s.clone()),
        _ => None,
    }
}

fn ends(s: &SubPath) -> Option<(Point, Point)> {
    Some((s.anchors.first()?.p, s.anchors.last()?.p))
}

/// Junta `b` ao fim de `a`. Se as pontas quase coincidem, vira um ponto só.
fn join(mut a: SubPath, b: SubPath) -> SubPath {
    let mut rest = b.anchors.into_iter();
    if let (Some(last), Some(first)) = (a.anchors.last_mut(), rest.as_slice().first()) {
        if last.p.distance(first.p) < 0.5 {
            last.h_out = first.h_out;
            rest.next();
        } else {
            last.h_out = last.p;
        }
    }
    a.anchors.extend(rest);
    a
}

impl Document {
    /// Simplifica os traços da camada vetorial ativa (ou só o objeto `only`) com tolerância
    /// `tol` em pixels. Devolve a área a recompor; vazia se não mudou nada.
    pub fn vector_simplify(&mut self, only: Option<NodeId>, tol: f64) -> Rect {
        let tol = tol.clamp(0.1, SIMPLIFY_MAX);
        let opts = SimplifyOptions {
            tolerance: tol,
            // Tremor de mão faz zigue-zague com viradas fechadas; só as viradas bem agudas
            // (quase voltando) ficam como cantos.
            corner_angle_deg: 150.0,
            straight_lines: false,
        };
        let Some(art) = self.active_vector() else {
            return Rect::EMPTY;
        };
        let changes: Vec<(NodeId, PathData)> = path_ids(art, only)
            .into_iter()
            .filter_map(|id| {
                let p = art.doc.node(id)?.path_data()?;
                let s = vectorcraft_pathops::simplify_with(p, &opts);
                (anchors_of(&s) < anchors_of(p) && anchors_of(&s) >= 2).then_some((id, s))
            })
            .collect();
        if changes.is_empty() {
            return Rect::EMPTY;
        }
        self.edit_vector(true, |art| {
            for (id, s) in changes {
                if let Some(p) = art.doc.node_mut(id).and_then(|n| n.path_data_mut()) {
                    *p = s;
                }
            }
            true
        })
    }

    /// Junta as linhas abertas cujas pontas estão a menos de `gap` pixels, a mais perto
    /// primeiro. Devolve quantas junções fez e a área a recompor.
    pub fn vector_connect(&mut self, gap: f64) -> (usize, Rect) {
        let gap = gap.clamp(0.5, CONNECT_MAX);
        let Some(art) = self.active_vector() else {
            return (0, Rect::EMPTY);
        };
        let mut lines: Vec<(NodeId, SubPath)> = path_ids(art, None)
            .into_iter()
            .filter_map(|id| Some((id, open_line(art, id)?)))
            .collect();
        let mut removed = vec![];
        let mut joins = 0;
        loop {
            // O par de pontas mais perto: (i, j, inverter i, inverter j) de modo que o fim de i
            // encoste no começo de j.
            let mut best: Option<(f64, usize, usize, bool, bool)> = None;
            for i in 0..lines.len() {
                for j in i + 1..lines.len() {
                    let (Some((a0, a1)), Some((b0, b1))) = (ends(&lines[i].1), ends(&lines[j].1))
                    else {
                        continue;
                    };
                    for (d, ri, rj) in [
                        (a1.distance(b0), false, false),
                        (a1.distance(b1), false, true),
                        (a0.distance(b0), true, false),
                        (a0.distance(b1), true, true),
                    ] {
                        if d <= gap && best.is_none_or(|b| d < b.0) {
                            best = Some((d, i, j, ri, rj));
                        }
                    }
                }
            }
            let Some((_, i, j, ri, rj)) = best else {
                break;
            };
            let (bid, mut b) = lines.swap_remove(j);
            let a = &mut lines[i].1;
            if ri {
                a.reverse();
            }
            if rj {
                b.reverse();
            }
            *a = join(std::mem::replace(a, SubPath::new(vec![], false)), b);
            removed.push(bid);
            joins += 1;
        }
        if joins == 0 {
            return (0, Rect::EMPTY);
        }
        let r = self.edit_vector(true, |art| {
            for (id, s) in lines {
                if let Some(p) = art.doc.node_mut(id).and_then(|n| n.path_data_mut()) {
                    *p = PathData::single(s);
                }
            }
            art.doc.remove_many(&removed).is_ok()
        });
        (joins, r)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::vector::{Shape, VectorStyle};
    use vectorcraft_geom::path::Anchor;

    fn doc() -> Document {
        let mut d = Document::new(200, 200);
        d.add_vector_layer();
        d
    }

    fn line(a: f64, b: f64, c: f64, d: f64) -> Shape {
        Shape::Line(Point::new(a, b), Point::new(c, d))
    }

    fn count(d: &Document) -> usize {
        d.active_vector()
            .unwrap()
            .doc
            .layers
            .last()
            .unwrap()
            .children()
            .unwrap()
            .len()
    }

    #[test]
    fn connect_joins_nearby_ends() {
        let mut d = doc();
        let st = VectorStyle::default();
        d.vector_add(&line(10.0, 10.0, 90.0, 10.0), &st);
        d.vector_add(&line(180.0, 10.0, 95.0, 12.0), &st);
        d.vector_add(&line(10.0, 150.0, 90.0, 150.0), &st);
        assert_eq!(count(&d), 3);
        let (n, r) = d.vector_connect(10.0);
        assert_eq!(n, 1);
        assert!(!r.is_empty());
        assert_eq!(count(&d), 2);
        assert!(d.undo());
        assert_eq!(count(&d), 3);
    }

    #[test]
    fn simplify_drops_anchors() {
        let mut d = doc();
        let pts = (0..60)
            .map(|i| {
                let x = i as f64 * 3.0;
                // Zigue-zague de tremor: ±4 px em cada ponto.
                let y = 100.0 + (x * 0.05).sin() * 20.0 + if i % 2 == 0 { 4.0 } else { -4.0 };
                Anchor::corner(Point::new(x, y))
            })
            .collect();
        d.vector_add(
            &Shape::Bezier(SubPath::new(pts, false)),
            &VectorStyle::default(),
        );
        let before = anchors_of(
            d.active_vector()
                .unwrap()
                .doc
                .layers
                .last()
                .unwrap()
                .children()
                .unwrap()[0]
                .path_data()
                .unwrap(),
        );
        assert!(!d.vector_simplify(None, 8.0).is_empty());
        let after = anchors_of(
            d.active_vector()
                .unwrap()
                .doc
                .layers
                .last()
                .unwrap()
                .children()
                .unwrap()[0]
                .path_data()
                .unwrap(),
        );
        assert!(after * 4 < before, "{after} < {before}");
    }
}
