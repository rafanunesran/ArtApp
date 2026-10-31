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
}
