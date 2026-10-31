//! Ajustar a largura da linha (a "Correção de linha › Ajustar largura" do Clip Studio, em camada
//! raster): engrossa ou afina a tinta da camada ativa, ou só da seleção, por alguns pixels.

use photocraft_geom::Rect;

use crate::border::distance_field;
use crate::document::Document;

/// Mais que isso estraga o desenho mais do que corrige.
pub const MAX_DELTA: f32 = 20.0;

impl Document {
    /// Engrossa (`delta` > 0) ou afina (`delta` < 0) a tinta da camada ativa em `delta` pixels
    /// de cada lado. Devolve a área alterada.
    pub fn adjust_line_width(&mut self, delta: f32) -> Rect {
        let delta = delta.clamp(-MAX_DELTA, MAX_DELTA);
        if delta.abs() < 0.25 {
            return Rect::EMPTY;
        }
        let Some(area) = self.edit_area(false) else {
            return Rect::EMPTY;
        };
        let pad = delta.max(0.0).ceil() as i32 + 1;
        let r = area.inflate(pad).intersect(&self.bounds());
        let sel = self.selection.clone();
        let Some(layer) = self.active_layer() else {
            return Rect::EMPTY;
        };
        let (w, h) = (r.width() as usize, r.height() as usize);
        let mut px = vec![[0.0f32; 4]; w * h];
        layer.surface.read_rgba_into(r, &mut px);
        let mut out = px.clone();
        if delta > 0.0 {
            // Cada pixel novo pega a cor do pixel de tinta mais perto.
            let seed: Vec<bool> = px.iter().map(|p| p[3] >= 0.5).collect();
            let (d, near) = distance_field(&seed, w, h);
            for k in 0..w * h {
                let a = (delta + 0.5 - d[k]).clamp(0.0, 1.0);
                if a > px[k][3] {
                    let c = px[near[k]];
                    out[k] = [c[0], c[1], c[2], a];
                }
            }
        } else {
            // Afinar: a tinta some a até `-delta` pixels do vazio.
            let seed: Vec<bool> = px.iter().map(|p| p[3] < 0.5).collect();
            let (d, _) = distance_field(&seed, w, h);
            for k in 0..w * h {
                let keep = (d[k] + delta).clamp(0.0, 1.0);
                out[k][3] = px[k][3].min(keep);
            }
        }
        // Só dentro da seleção, se houver.
        if let Some(s) = &sel {
            let m = s.mask.read_region(r);
            for k in 0..w * h {
                let t = m[k];
                out[k] = std::array::from_fn(|c| px[k][c] + (out[k][c] - px[k][c]) * t);
            }
        }
        self.checkpoint();
        if let Some(l) = self.active_layer_mut() {
            l.surface.write_region(r, out.as_flattened());
            l.surface.prune();
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thicken_and_thin_a_line() {
        let mut doc = Document::new(40, 40);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(5, 18, 35, 22), &[0.2, 0.3, 0.9, 1.0]);
        assert!(!doc.adjust_line_width(3.0).is_empty());
        let p = doc.layers[0].surface.rgba(20, 16);
        assert!(p[3] > 0.99 && (p[2] - 0.9).abs() < 0.01);
        assert_eq!(doc.layers[0].surface.rgba(20, 12)[3], 0.0);
        assert!(doc.undo());
        assert!(!doc.adjust_line_width(-1.0).is_empty());
        assert_eq!(doc.layers[0].surface.rgba(20, 18)[3], 0.0);
        assert!(doc.layers[0].surface.rgba(20, 20)[3] > 0.99);
    }
}
