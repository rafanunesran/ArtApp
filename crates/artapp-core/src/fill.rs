//! Balde de tinta: preenche a região contígua de cor parecida (amostrando a imagem composta, como
//! o "referenciar todas as camadas" do Clip Studio) na camada ativa.

use photocraft_geom::Rect;

use crate::composite::composite_rgba8;
use crate::document::Document;

/// Preenche a partir de `(x, y)` com `color`. `tolerance` 0..1 (diferença máxima por canal).
/// Devolve o retângulo alterado, vazio se nada mudou.
pub fn flood_fill(doc: &mut Document, x: i32, y: i32, color: [f32; 4], tolerance: f32) -> Rect {
    let bounds = doc.bounds();
    if !bounds.contains(x, y) || doc.active_layer().is_none_or(|l| !l.visible) {
        return Rect::EMPTY;
    }
    let (w, h) = (doc.width as usize, doc.height as usize);
    let (_, img) = composite_rgba8(doc, bounds, true);
    let idx = |x: usize, y: usize| y * w + x;
    let Some(&seed) = img.get(idx(x as usize, y as usize)) else {
        return Rect::EMPTY;
    };
    let tol = (tolerance.clamp(0.0, 1.0) * 255.0) as i32;
    let similar = |p: [u8; 4]| (0..4).all(|c| (p[c] as i32 - seed[c] as i32).abs() <= tol);

    let mut mask = vec![false; w * h];
    let mut stack = vec![(x as usize, y as usize)];
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
    while let Some((sx, sy)) = stack.pop() {
        // Varre a linha para a esquerda e a direita a partir do ponto.
        let row_ok = |xx: usize, mask: &[bool]| {
            !mask[idx(xx, sy)] && img.get(idx(xx, sy)).is_some_and(|p| similar(*p))
        };
        if !row_ok(sx, &mask) {
            continue;
        }
        let mut l = sx;
        while l > 0 && row_ok(l - 1, &mask) {
            l -= 1;
        }
        let mut r = sx;
        while r + 1 < w && row_ok(r + 1, &mask) {
            r += 1;
        }
        for xx in l..=r {
            mask[idx(xx, sy)] = true;
            if sy > 0 && !mask[idx(xx, sy - 1)] {
                stack.push((xx, sy - 1));
            }
            if sy + 1 < h && !mask[idx(xx, sy + 1)] {
                stack.push((xx, sy + 1));
            }
        }
        (x0, x1, y0, y1) = (x0.min(l), x1.max(r), y0.min(sy), y1.max(sy));
    }
    if x0 > x1 {
        return Rect::EMPTY;
    }
    // Cresce a máscara 1 px para não sobrar franja clara nas bordas antisserrilhadas do traço.
    let grown: Vec<bool> = (0..w * h)
        .map(|i| {
            let (px, py) = (i % w, i / w);
            mask[i]
                || (px > 0 && mask[i - 1])
                || (px + 1 < w && mask[i + 1])
                || (py > 0 && mask[i - w])
                || (py + 1 < h && mask[i + w])
        })
        .collect();
    let r = Rect::new(x0 as i32, y0 as i32, x1 as i32 + 1, y1 as i32 + 1)
        .inflate(1)
        .intersect(&bounds);
    doc.checkpoint();
    let Some(layer) = doc.active_layer_mut() else {
        return Rect::EMPTY;
    };
    let mut px = vec![[0.0f32; 4]; r.width() as usize * r.height() as usize];
    layer.surface.read_rgba_into(r, &mut px);
    let rw = r.width() as usize;
    let mut out = Vec::with_capacity(px.len() * 4);
    for (i, p) in px.iter().enumerate() {
        let (gx, gy) = (r.x0 as usize + i % rw, r.y0 as usize + i / rw);
        let hit = grown[idx(gx, gy)];
        let keep = layer.lock_alpha && p[3] <= 0.0;
        if hit && !keep {
            let a = if layer.lock_alpha { p[3] } else { 1.0 };
            let src = [color[0], color[1], color[2], color[3]];
            let o = photocraft_color::blend::composite(
                photocraft_color::BlendMode::Normal,
                *p,
                src,
                1.0,
            );
            out.extend_from_slice(&[o[0], o[1], o[2], if layer.lock_alpha { a } else { o[3] }]);
        } else {
            out.extend_from_slice(p);
        }
    }
    layer.surface.write_region(r, &out);
    r
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn fills_inside_a_closed_shape_only() {
        let mut doc = Document::new(20, 20);
        let black = [0.0, 0.0, 0.0, 1.0];
        let s = &mut doc.layers[0].surface;
        for i in 5..15 {
            s.write_pixel(i, 5, &black);
            s.write_pixel(i, 14, &black);
            s.write_pixel(5, i, &black);
            s.write_pixel(14, i, &black);
        }
        let r = flood_fill(&mut doc, 10, 10, [1.0, 0.0, 0.0, 1.0], 0.1);
        assert!(!r.is_empty());
        assert_eq!(doc.layers[0].surface.rgba(10, 10), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(doc.layers[0].surface.rgba(1, 1)[3], 0.0);
        assert!(doc.undo());
        assert_eq!(doc.layers[0].surface.rgba(10, 10)[3], 0.0);
    }
}
