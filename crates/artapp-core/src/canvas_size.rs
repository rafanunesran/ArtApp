//! Tamanho da tela: cortar, aumentar ou diminuir a área (sem mudar o desenho) e redimensionar a
//! imagem inteira (escala todas as camadas). Cada operação é um passo do desfazer.

use photocraft_algo::transform::{Homography, Interp, warp_surface};
use photocraft_color::PixelFormat;
use photocraft_geom::Rect;
use photocraft_raster::Surface;
use vectorcraft_geom::{Affine, Vec2};

use crate::document::{Document, MAX_CANVAS_SIDE};
use crate::vector::{VectorArt, next_rev, rasterize};

/// Aplica `affine` a todos os caminhos da arte vetorial.
fn transform_art(art: &mut VectorArt, affine: Affine) {
    let mut ids = Vec::new();
    art.doc.walk(|n| {
        if n.path_data().is_some() {
            ids.push(n.id);
        }
    });
    for id in ids {
        if let Some(p) = art.doc.node_mut(id).and_then(|n| n.path_data_mut()) {
            p.transform(affine);
        }
    }
    art.rev = next_rev();
}

impl Document {
    /// Muda a tela para `w`×`h`, com o desenho deslocado por (`dx`, `dy`) pixels. O que sai da
    /// tela é cortado. Devolve `false` se nada mudou.
    pub fn resize_canvas(&mut self, w: u32, h: u32, dx: i32, dy: i32) -> bool {
        let (w, h) = (w.clamp(1, MAX_CANVAS_SIDE), h.clamp(1, MAX_CANVAS_SIDE));
        if (w, h, dx, dy) == (self.width, self.height, 0, 0) {
            return false;
        }
        self.checkpoint();
        let bounds = Rect::new(0, 0, w as i32, h as i32);
        for layer in &mut self.layers {
            if let Some(art) = &mut layer.vector {
                transform_art(
                    art,
                    Affine::translate(Vec2::new(f64::from(dx), f64::from(dy))),
                );
                layer.surface = rasterize(art, w, h);
                continue;
            }
            let keep = layer
                .surface
                .content_bounds()
                .translate(dx, dy)
                .intersect(&bounds);
            let mut out = Surface::new(PixelFormat::RGBA8);
            if !keep.is_empty() {
                let mut px = vec![[0.0f32; 4]; keep.width() as usize * keep.height() as usize];
                layer
                    .surface
                    .read_rgba_into(keep.translate(-dx, -dy), &mut px);
                let flat: Vec<f32> = px.iter().flatten().copied().collect();
                out.write_region(keep, &flat);
                out.prune();
            }
            layer.surface = out;
        }
        self.width = w;
        self.height = h;
        self.selection = None;
        true
    }

    /// Corta a tela no retângulo da seleção.
    pub fn crop_to_selection(&mut self) -> bool {
        let Some(r) = self
            .selection
            .as_ref()
            .map(|s| s.bounds.intersect(&self.bounds()))
        else {
            return false;
        };
        if r.is_empty() {
            return false;
        }
        self.resize_canvas(r.width(), r.height(), -r.x0, -r.y0)
    }

    /// Escala o desenho inteiro (todas as camadas) para `w`×`h`.
    pub fn resize_image(&mut self, w: u32, h: u32, interp: Interp) -> bool {
        let (w, h) = (w.clamp(1, MAX_CANVAS_SIDE), h.clamp(1, MAX_CANVAS_SIDE));
        if (w, h) == (self.width, self.height) {
            return false;
        }
        let (sx, sy) = (
            f64::from(w) / f64::from(self.width),
            f64::from(h) / f64::from(self.height),
        );
        self.checkpoint();
        let scale = Homography([sx, 0.0, 0.0, 0.0, sy, 0.0, 0.0, 0.0, 1.0]);
        let bounds = Rect::new(0, 0, w as i32, h as i32);
        let old = self.bounds();
        for layer in &mut self.layers {
            if let Some(art) = &mut layer.vector {
                transform_art(art, Affine::scale_non_uniform(sx, sy));
                layer.surface = rasterize(art, w, h);
                continue;
            }
            let content = layer.surface.content_bounds().intersect(&old);
            if content.is_empty() {
                continue;
            }
            if let Ok(mut s) = warp_surface(&layer.surface, content, &scale, interp) {
                // O que a interpolação espalhou para fora da tela nova não conta.
                let keep = s.content_bounds().intersect(&bounds);
                if keep != s.content_bounds() {
                    let mut px = vec![[0.0f32; 4]; keep.width() as usize * keep.height() as usize];
                    s.read_rgba_into(keep, &mut px);
                    let flat: Vec<f32> = px.iter().flatten().copied().collect();
                    s = Surface::new(PixelFormat::RGBA8);
                    s.write_region(keep, &flat);
                }
                s.prune();
                layer.surface = s;
            }
        }
        self.width = w;
        self.height = h;
        self.selection = None;
        true
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::selection::Selection;

    #[test]
    fn crop_and_undo_restore_the_size() {
        let mut doc = Document::new(100, 80);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(10, 10, 30, 30), &[1.0, 0.0, 0.0, 1.0]);
        doc.selection = Selection::rect([20.0, 20.0], [60.0, 50.0], 100, 80);
        let r = doc.selection.as_ref().unwrap().bounds;
        assert!(doc.crop_to_selection());
        let size = (doc.width, doc.height);
        assert_eq!(size, (r.width(), r.height()));
        assert_eq!(doc.layers[0].surface.rgba(25 - r.x0, 25 - r.y0)[0], 1.0);
        assert_eq!(doc.layers[0].surface.rgba(35 - r.x0, 35 - r.y0)[3], 0.0);
        assert!(doc.undo());
        assert_eq!((doc.width, doc.height), (100, 80));
        assert_eq!(doc.layers[0].surface.rgba(15, 15)[0], 1.0);
        assert!(doc.redo());
        assert_eq!((doc.width, doc.height), size);
    }

    #[test]
    fn resize_image_scales_every_layer() {
        let mut doc = Document::new(100, 50);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 50, 50), &[0.0, 0.0, 1.0, 1.0]);
        assert!(doc.resize_image(200, 100, Interp::Bilinear));
        assert_eq!((doc.width, doc.height), (200, 100));
        assert!(doc.layers[0].surface.rgba(90, 50)[2] > 0.9);
        assert!(doc.layers[0].surface.rgba(110, 50)[3] < 0.1);
        assert!(doc.resize_canvas(300, 100, 50, 0));
        assert!(doc.layers[0].surface.rgba(140, 50)[2] > 0.9);
    }
}
