//! Pincel corretivo (o "corretivo pontual" do PhotoCraft): pinta-se por cima da mancha e, ao
//! soltar, a área é refeita a partir da textura em volta (preenchimento sensível ao conteúdo).

use photocraft_algo::inpaint::{CompleteParams, complete};
use photocraft_algo::poisson::membrane_fill;
use photocraft_geom::Rect;

use crate::document::Document;

pub struct HealSession {
    radius: f64,
    /// Os pontos do traço, em pixels do documento.
    pub points: Vec<[f64; 2]>,
}

impl HealSession {
    pub fn begin(doc: &Document, size: f32) -> Option<Self> {
        let layer = doc.active_layer()?;
        if !layer.visible || layer.is_vector() {
            return None;
        }
        Some(Self {
            radius: f64::from(size.max(2.0)) * 0.5,
            points: Vec::new(),
        })
    }

    pub fn radius(&self) -> f64 {
        self.radius
    }

    pub fn add(&mut self, x: f64, y: f64) {
        if !(x.is_finite() && y.is_finite()) {
            return;
        }
        // Pontos muito juntos não mudam a área e só pesam no fim.
        if let Some(l) = self.points.last()
            && (l[0] - x).hypot(l[1] - y) < self.radius * 0.25
        {
            return;
        }
        self.points.push([x, y]);
    }

    /// Refaz a área pintada. Devolve o retângulo alterado.
    pub fn end(self, doc: &mut Document) -> Rect {
        let Some(first) = self.points.first() else {
            return Rect::EMPTY;
        };
        let r = self.radius;
        let (mut x0, mut y0, mut x1, mut y1) = (first[0], first[1], first[0], first[1]);
        for p in &self.points {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        let hole_box = Rect::new(
            (x0 - r).floor() as i32,
            (y0 - r).floor() as i32,
            (x1 + r).ceil() as i32 + 1,
            (y1 + r).ceil() as i32 + 1,
        )
        .intersect(&doc.bounds());
        if hole_box.is_empty() {
            return Rect::EMPTY;
        }
        // Em volta do buraco, uma faixa de onde copiar a textura.
        let margin = (r * 3.0).max(32.0) as i32;
        let area = hole_box.inflate(margin).intersect(&doc.bounds());
        let (w, h) = (area.width() as usize, area.height() as usize);
        let mut hole = vec![false; w * h];
        let r2 = r * r;
        for (i, cell) in hole.iter_mut().enumerate() {
            let (px, py) = (
                f64::from(area.x0) + (i % w) as f64 + 0.5,
                f64::from(area.y0) + (i / w) as f64 + 0.5,
            );
            *cell = segment_dist_sq(&self.points, px, py) <= r2;
        }
        let Some(layer) = doc.active_layer() else {
            return Rect::EMPTY;
        };
        let mut px = vec![[0.0f32; 4]; w * h];
        layer.surface.read_rgba_into(area, &mut px);
        let img: Vec<f32> = px.iter().flatten().copied().collect();
        let filled = complete(w, h, 4, &img, &hole, &CompleteParams::default())
            .unwrap_or_else(|| membrane_fill(w, h, 4, &img, &hole));
        doc.checkpoint();
        let Some(layer) = doc.active_layer_mut() else {
            return Rect::EMPTY;
        };
        let lock = layer.lock_alpha;
        let mut out = img;
        for (i, &is_hole) in hole.iter().enumerate() {
            if is_hole {
                let alpha = out[i * 4 + 3];
                out[i * 4..i * 4 + 4].copy_from_slice(&filled[i * 4..i * 4 + 4]);
                if lock {
                    out[i * 4 + 3] = alpha;
                }
            }
        }
        layer.surface.write_region(area, &out);
        area
    }
}

/// Menor distância ao quadrado de (x, y) até o traço (segmentos entre os pontos).
fn segment_dist_sq(points: &[[f64; 2]], x: f64, y: f64) -> f64 {
    let d2 = |a: [f64; 2]| (a[0] - x).powi(2) + (a[1] - y).powi(2);
    let mut best = d2(points[0]);
    for s in points.windows(2) {
        let (a, b) = (s[0], s[1]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l2 = dx * dx + dy * dy;
        let t = if l2 > 0.0 {
            (((x - a[0]) * dx + (y - a[1]) * dy) / l2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        best = best.min(d2([a[0] + dx * t, a[1] + dy * t]));
    }
    best
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn heal_removes_a_spot() {
        let mut doc = Document::new(80, 80);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 80, 80), &[0.2, 0.6, 0.3, 1.0]);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(38, 38, 42, 42), &[1.0, 0.0, 0.0, 1.0]);
        let mut s = HealSession::begin(&doc, 10.0).unwrap();
        s.add(40.0, 40.0);
        s.end(&mut doc);
        let p = doc.layers[0].surface.rgba(40, 40);
        assert!(
            (p[0] - 0.2).abs() < 0.05 && (p[1] - 0.6).abs() < 0.05,
            "{p:?}"
        );
        assert!(doc.undo());
        assert!(doc.layers[0].surface.rgba(40, 40)[0] > 0.9);
    }
}
