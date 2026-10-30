//! Seleção: uma máscara de 8 bits (1 = selecionado) do tamanho da tela. Limita os traços, o balde
//! e as ações do lançador (limpar, preencher). Como no Clip Studio, ferramentas de retângulo,
//! elipse e laço criam a seleção, e há inverter e desmarcar.

use photocraft_color::PixelFormat;
use photocraft_geom::Rect;
use photocraft_raster::Surface;

use crate::document::Document;

#[derive(Clone)]
pub struct Selection {
    /// Máscara cinza de 8 bits, 0 fora e 1 dentro.
    pub mask: Surface,
    /// Retângulo que contém tudo que está selecionado.
    pub bounds: Rect,
    /// Contorno para desenhar na tela (polígono em pixels do documento).
    pub outline: Vec<[f64; 2]>,
    /// A seleção foi invertida (o contorno desenhado é o da área de fora).
    pub inverted: bool,
}

impl Selection {
    /// Seleção a partir de um polígono (laço) em pixels do documento, recortada à tela.
    pub fn polygon(points: &[[f64; 2]], width: u32, height: u32) -> Option<Self> {
        if points.len() < 3 {
            return None;
        }
        let canvas = Rect::new(0, 0, width as i32, height as i32);
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in points {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        let r = Rect::new(
            x0.floor() as i32,
            y0.floor() as i32,
            x1.ceil() as i32 + 1,
            y1.ceil() as i32 + 1,
        )
        .intersect(&canvas);
        if r.is_empty() {
            return None;
        }
        let (rw, rh) = (r.width() as usize, r.height() as usize);
        let mut data = vec![0.0f32; rw * rh];
        let mut xs = Vec::new();
        // Varredura par-ímpar no centro de cada linha de pixels.
        for row in 0..rh {
            let y = r.y0 as f64 + row as f64 + 0.5;
            xs.clear();
            for (i, a) in points.iter().enumerate() {
                let b = points[(i + 1) % points.len()];
                if (a[1] <= y) != (b[1] <= y) {
                    xs.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
                }
            }
            xs.sort_by(f64::total_cmp);
            for pair in xs.as_chunks::<2>().0 {
                let from = ((pair[0] - 0.5).ceil() as i32).max(r.x0);
                let to = ((pair[1] - 0.5).floor() as i32).min(r.x1 - 1);
                for x in from..=to {
                    data[row * rw + (x - r.x0) as usize] = 1.0;
                }
            }
        }
        if !data.iter().any(|v| *v > 0.0) {
            return None;
        }
        let mut mask = Surface::new(PixelFormat::GRAY8);
        mask.write_region(r, &data);
        Some(Self {
            mask,
            bounds: r,
            outline: points.to_vec(),
            inverted: false,
        })
    }

    pub fn rect(a: [f64; 2], b: [f64; 2], width: u32, height: u32) -> Option<Self> {
        let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
        let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
        Self::polygon(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1]], width, height)
    }

    pub fn ellipse(a: [f64; 2], b: [f64; 2], width: u32, height: u32) -> Option<Self> {
        let (cx, cy) = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0);
        let (rx, ry) = ((a[0] - b[0]).abs() / 2.0, (a[1] - b[1]).abs() / 2.0);
        let n = 96;
        let pts: Vec<[f64; 2]> = (0..n)
            .map(|i| {
                let t = i as f64 / n as f64 * std::f64::consts::TAU;
                [cx + rx * t.cos(), cy + ry * t.sin()]
            })
            .collect();
        Self::polygon(&pts, width, height)
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.mask.sample_channel(x, y, 0) >= 0.5
    }

    /// Troca o dentro pelo fora, dentro da tela.
    pub fn invert(&mut self, width: u32, height: u32) {
        let canvas = Rect::new(0, 0, width as i32, height as i32);
        let data: Vec<f32> = self
            .mask
            .read_region(canvas)
            .into_iter()
            .map(|v| 1.0 - v)
            .collect();
        let mut mask = Surface::new(PixelFormat::GRAY8);
        mask.write_region(canvas, &data);
        self.mask = mask;
        self.bounds = canvas;
        self.inverted = !self.inverted;
    }
}

impl Document {
    /// Troca a seleção (`None` desmarca).
    pub fn set_selection(&mut self, sel: Option<Selection>) {
        self.selection = sel;
    }

    pub fn invert_selection(&mut self) {
        let (w, h) = (self.width, self.height);
        if let Some(s) = &mut self.selection {
            s.invert(w, h);
        }
    }

    /// Apaga o que está selecionado na camada ativa (raster). Devolve o retângulo alterado.
    pub fn clear_selection(&mut self) -> Rect {
        self.fill_selection([0.0, 0.0, 0.0, 0.0])
    }

    /// Pinta a seleção na camada ativa com `color` (alfa 0 apaga). Devolve o retângulo alterado.
    pub fn fill_selection(&mut self, color: [f32; 4]) -> Rect {
        let Some(sel) = self.selection.clone() else {
            return Rect::EMPTY;
        };
        if self
            .active_layer()
            .is_none_or(|l| !l.visible || l.is_vector())
        {
            return Rect::EMPTY;
        }
        let r = sel.bounds.intersect(&self.bounds());
        if r.is_empty() {
            return Rect::EMPTY;
        }
        self.checkpoint();
        let Some(layer) = self.active_layer_mut() else {
            return Rect::EMPTY;
        };
        let m = sel.mask.read_region(r);
        let mut px = vec![[0.0f32; 4]; m.len()];
        layer.surface.read_rgba_into(r, &mut px);
        let lock = layer.lock_alpha;
        let mut out = Vec::with_capacity(px.len() * 4);
        for (p, k) in px.iter().zip(&m) {
            let o = if *k < 0.5 || (lock && p[3] <= 0.0) {
                *p
            } else if color[3] <= 0.0 {
                [p[0], p[1], p[2], 0.0]
            } else if lock {
                [color[0], color[1], color[2], p[3]]
            } else {
                photocraft_color::blend::composite(
                    photocraft_color::BlendMode::Normal,
                    *p,
                    color,
                    1.0,
                )
            };
            out.extend_from_slice(&o);
        }
        layer.surface.write_region(r, &out);
        r
    }

    /// Move o conteúdo da camada ativa (raster) por `dx, dy` pixels. Devolve o retângulo alterado.
    pub fn move_layer_by(&mut self, dx: i32, dy: i32) -> Rect {
        if (dx, dy) == (0, 0) {
            return Rect::EMPTY;
        }
        let Some(layer) = self.active_layer() else {
            return Rect::EMPTY;
        };
        if layer.is_vector() {
            return Rect::EMPTY;
        }
        let content = layer.surface.content_bounds();
        if content.is_empty() {
            return Rect::EMPTY;
        }
        self.checkpoint();
        let Some(layer) = self.active_layer_mut() else {
            return Rect::EMPTY;
        };
        layer.surface = layer.surface.translated(dx, dy, content);
        content
            .union(&content.translate(dx, dy))
            .intersect(&self.bounds())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn rect_selection_limits_fill_and_clear() {
        let mut doc = Document::new(20, 20);
        let b = doc.bounds();
        doc.layers[0].surface.fill_rect(b, &[0.0, 0.0, 0.0, 1.0]);
        doc.set_selection(Selection::rect([5.0, 5.0], [10.0, 10.0], 20, 20));
        assert!(doc.selection.as_ref().unwrap().contains(7, 7));
        assert!(!doc.selection.as_ref().unwrap().contains(12, 7));
        doc.clear_selection();
        assert_eq!(doc.layers[0].surface.rgba(7, 7)[3], 0.0);
        assert_eq!(doc.layers[0].surface.rgba(12, 7)[3], 1.0);
        doc.invert_selection();
        doc.fill_selection([1.0, 0.0, 0.0, 1.0]);
        assert_eq!(doc.layers[0].surface.rgba(12, 7), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(doc.layers[0].surface.rgba(7, 7)[3], 0.0);
    }

    #[test]
    fn lasso_triangle() {
        let s = Selection::polygon(&[[0.0, 0.0], [20.0, 0.0], [0.0, 20.0]], 20, 20).unwrap();
        assert!(s.contains(2, 2));
        assert!(!s.contains(18, 18));
    }

    #[test]
    fn move_layer_shifts_pixels() {
        let mut doc = Document::new(40, 40);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 4, 4), &[1.0, 1.0, 1.0, 1.0]);
        doc.move_layer_by(10, 5);
        assert_eq!(doc.layers[0].surface.rgba(1, 1)[3], 0.0);
        assert_eq!(doc.layers[0].surface.rgba(11, 6)[3], 1.0);
        assert!(doc.undo());
        assert_eq!(doc.layers[0].surface.rgba(1, 1)[3], 1.0);
    }
}
