//! Composição das camadas para exibição e exportação, com os modos de mesclagem do PhotoCraft.

use photocraft_color::blend;
use photocraft_geom::Rect;
use photocraft_raster::Surface;

use crate::document::{Document, Layer};

/// Compõe `r` (recortado à tela) em RGBA 8 bits não pré-multiplicado, linha a linha.
/// Com `with_paper = false` o papel fica transparente (exportar com fundo transparente).
/// Devolve o retângulo efetivamente composto e os pixels.
pub fn composite_rgba8(doc: &Document, r: Rect, with_paper: bool) -> (Rect, Vec<[u8; 4]>) {
    let r = r.intersect(&doc.bounds());
    if r.is_empty() {
        return (r, Vec::new());
    }
    let n = r.width() as usize * r.height() as usize;
    let base = if with_paper { doc.paper } else { [0.0; 4] };
    let mut acc = vec![base; n];
    let mut row = vec![[0.0f32; 4]; n];
    for layer in doc.layers.iter().filter(|l| l.visible && l.opacity > 0.0) {
        if !layer.surface.has_tiles_in(r) {
            continue;
        }
        layer.surface.read_rgba_into(r, &mut row);
        for (dst, src) in acc.iter_mut().zip(&row) {
            if src[3] > 0.0 {
                *dst = blend::composite(layer.blend, *dst, *src, layer.opacity);
            }
        }
    }
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    (
        r,
        acc.into_iter()
            .map(|p| [q(p[0]), q(p[1]), q(p[2]), q(p[3])])
            .collect(),
    )
}

/// Cor composta (com papel) num pixel da tela, para o conta-gotas.
pub fn sample(doc: &Document, x: i32, y: i32) -> Option<[f32; 4]> {
    let (_, px) = composite_rgba8(doc, Rect::new(x, y, x + 1, y + 1), true);
    px.first().map(|p| {
        [
            p[0] as f32 / 255.0,
            p[1] as f32 / 255.0,
            p[2] as f32 / 255.0,
            p[3] as f32 / 255.0,
        ]
    })
}

/// Mescla `top` em `dst` dentro da tela `width × height`.
pub fn merge_into(dst: &mut Surface, top: &Layer, width: u32, height: u32) {
    if !top.visible {
        return;
    }
    let r = top
        .surface
        .content_bounds()
        .intersect(&Rect::new(0, 0, width as i32, height as i32));
    if r.is_empty() {
        return;
    }
    let n = r.width() as usize * r.height() as usize;
    let mut a = vec![[0.0f32; 4]; n];
    let mut b = vec![[0.0f32; 4]; n];
    dst.read_rgba_into(r, &mut a);
    top.surface.read_rgba_into(r, &mut b);
    let mut out = Vec::with_capacity(n * 4);
    for (d, s) in a.iter().zip(&b) {
        let p = if s[3] > 0.0 {
            blend::composite(top.blend, *d, *s, top.opacity)
        } else {
            *d
        };
        out.extend_from_slice(&p);
    }
    dst.write_region(r, &out);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::BlendMode;

    fn paint(doc: &mut Document, layer: usize, r: Rect, c: [f32; 4]) {
        doc.layers[layer].surface.fill_rect(r, &c);
    }

    #[test]
    fn layers_stack_over_paper() {
        let mut doc = Document::new(10, 10);
        paint(&mut doc, 0, Rect::new(0, 0, 5, 10), [1.0, 0.0, 0.0, 1.0]);
        let (_, px) = composite_rgba8(&doc, doc.bounds(), true);
        assert_eq!(px[0], [255, 0, 0, 255]);
        assert_eq!(px[9], [255, 255, 255, 255]);
        let (_, px) = composite_rgba8(&doc, doc.bounds(), false);
        assert_eq!(px[9][3], 0);
    }

    #[test]
    fn multiply_and_opacity() {
        let mut doc = Document::new(4, 4);
        let b = doc.bounds();
        paint(&mut doc, 0, b, [1.0, 1.0, 0.0, 1.0]);
        doc.add_layer();
        paint(&mut doc, 1, b, [0.0, 1.0, 1.0, 1.0]);
        doc.layers[1].blend = BlendMode::Multiply;
        assert_eq!(
            composite_rgba8(&doc, doc.bounds(), true).1[0],
            [0, 255, 0, 255]
        );
        doc.layers[1].blend = BlendMode::Normal;
        doc.layers[1].opacity = 0.5;
        let p = composite_rgba8(&doc, doc.bounds(), true).1[0];
        assert!((p[0] as i32 - 128).abs() <= 1 && p[1] == 255, "{p:?}");
    }

    #[test]
    fn merge_down_keeps_the_look() {
        let mut doc = Document::new(8, 8);
        let b = doc.bounds();
        paint(&mut doc, 0, b, [0.2, 0.4, 0.6, 1.0]);
        doc.add_layer();
        paint(&mut doc, 1, Rect::new(0, 0, 4, 4), [1.0, 1.0, 1.0, 1.0]);
        doc.layers[1].blend = BlendMode::Screen;
        doc.layers[1].opacity = 0.7;
        let before = composite_rgba8(&doc, doc.bounds(), true).1;
        doc.merge_down();
        assert_eq!(doc.layers.len(), 1);
        let after = composite_rgba8(&doc, doc.bounds(), true).1;
        for (a, b) in before.iter().zip(&after) {
            for c in 0..4 {
                assert!((a[c] as i32 - b[c] as i32).abs() <= 1, "{a:?} {b:?}");
            }
        }
    }
}
