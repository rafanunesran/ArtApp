//! Balde de tinta: preenche a região contígua de cor parecida na camada ativa. Como no Clip Studio,
//! amostra as camadas de referência se houver alguma; senão, a imagem composta sem os rascunhos.
//! Com uma seleção ativa, só pinta dentro dela. Cor com alfa 0 (a cor transparente) apaga.

use photocraft_geom::Rect;

use crate::composite::{Pass, composite_pass};
use crate::document::Document;

/// Opções do balde, como na paleta de propriedades do Clip Studio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillOptions {
    /// 0..1: diferença máxima por canal.
    pub tolerance: f32,
    /// Fecha aberturas de até tantos pixels nas linhas (0 desliga).
    pub close_gap: f32,
    /// Cresce (> 0) ou encolhe (< 0) a área pintada, em pixels; 0 = só 1 px para cobrir a franja.
    pub expand: f32,
}

impl Default for FillOptions {
    fn default() -> Self {
        Self {
            tolerance: 0.1,
            close_gap: 0.0,
            expand: 0.0,
        }
    }
}

/// Preenche a partir de `(x, y)` com `color`. `tolerance` 0..1 (diferença máxima por canal).
/// Devolve o retângulo alterado, vazio se nada mudou.
pub fn flood_fill(doc: &mut Document, x: i32, y: i32, color: [f32; 4], tolerance: f32) -> Rect {
    let opts = FillOptions {
        tolerance,
        ..FillOptions::default()
    };
    flood_fill_with(doc, x, y, color, &opts)
}

/// O mesmo, com fechar lacunas e expandir área.
pub fn flood_fill_with(
    doc: &mut Document,
    x: i32,
    y: i32,
    color: [f32; 4],
    opts: &FillOptions,
) -> Rect {
    let bounds = doc.bounds();
    if !bounds.contains(x, y)
        || doc
            .active_layer()
            .is_none_or(|l| !l.visible || l.is_vector())
    {
        return Rect::EMPTY;
    }
    let (w, h) = (doc.width as usize, doc.height as usize);
    let reference = doc.layers.iter().any(|l| l.reference && l.visible);
    let (_, img) = if reference {
        composite_pass(doc, bounds, false, Pass::Reference)
    } else {
        composite_pass(doc, bounds, true, Pass::Final)
    };
    let idx = |x: usize, y: usize| y * w + x;
    let Some(&seed) = img.get(idx(x as usize, y as usize)) else {
        return Rect::EMPTY;
    };
    let tol = (opts.tolerance.clamp(0.0, 1.0) * 255.0) as i32;
    let similar: Vec<bool> = {
        use rayon::prelude::*;
        img.par_iter()
            .map(|p| (0..4).all(|c| (p[c] as i32 - seed[c] as i32).abs() <= tol))
            .collect()
    };
    drop(img);
    // Fechar lacunas: engrossa as linhas pela metade da abertura antes de preencher.
    let half = (opts.close_gap.max(0.0) / 2.0).ceil();
    let thick = (half > 0.0).then(|| {
        let barrier: Vec<bool> = similar.iter().map(|s| !s).collect();
        let thick = crate::mask::grow(&barrier, w, h, half);
        thick.iter().map(|b| !b).collect::<Vec<bool>>()
    });
    let allowed: &[bool] = match &thick {
        Some(a) if a[idx(x as usize, y as usize)] => a,
        _ => &similar,
    };

    let mut mask = vec![false; w * h];
    let mut stack = vec![(x as usize, y as usize)];
    while let Some((sx, sy)) = stack.pop() {
        // Varre a linha para a esquerda e a direita a partir do ponto.
        let row_ok = |xx: usize, mask: &[bool]| !mask[idx(xx, sy)] && allowed[idx(xx, sy)];
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
        mask[idx(l, sy)..=idx(r, sy)].fill(true);
        // Nas linhas de cima e de baixo, só o começo de cada trecho preenchível entra na pilha
        // (antes entrava um ponto por pixel: milhões num balde que pega a tela toda).
        for ny in [sy.wrapping_sub(1), sy + 1] {
            if ny >= h {
                continue;
            }
            let mut inside = false;
            for xx in l..=r {
                let ok = !mask[idx(xx, ny)] && allowed[idx(xx, ny)];
                if ok && !inside {
                    stack.push((xx, ny));
                }
                inside = ok;
            }
        }
    }
    if half > 0.0 {
        // Devolve o que as linhas engrossadas tiraram, sem passar para fora da lacuna.
        let back = crate::mask::grow(&mask, w, h, half + 1.0);
        for i in 0..mask.len() {
            mask[i] = mask[i] || (back[i] && similar[i]);
        }
    }
    // Cresce a máscara (1 px por padrão, para não sobrar franja clara no antisserrilhado).
    let grown = if opts.expand != 0.0 {
        crate::mask::grow(&mask, w, h, opts.expand)
    } else {
        use rayon::prelude::*;
        let mut g = vec![false; w * h];
        g.par_chunks_mut(w).enumerate().for_each(|(py, row)| {
            for (px, o) in row.iter_mut().enumerate() {
                let i = py * w + px;
                *o = mask[i]
                    || (px > 0 && mask[i - 1])
                    || (px + 1 < w && mask[i + 1])
                    || (py > 0 && mask[i - w])
                    || (py + 1 < h && mask[i + w]);
            }
        });
        g
    };
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
    for (py, row) in grown.chunks_exact(w).enumerate() {
        let (Some(a), Some(b)) = (row.iter().position(|g| *g), row.iter().rposition(|g| *g)) else {
            continue;
        };
        (x0, x1, y0, y1) = (x0.min(a), x1.max(b), y0.min(py), y1.max(py));
    }
    if x0 > x1 {
        return Rect::EMPTY;
    }
    let r = Rect::new(x0 as i32, y0 as i32, x1 as i32 + 1, y1 as i32 + 1).intersect(&bounds);
    // Cobertura da seleção dentro de `r` (1 = selecionado).
    let sel_mask = doc.selection.as_ref().map(|s| s.mask.clone());
    let erase = color[3] <= 0.0;
    doc.checkpoint();
    let Some(layer) = doc.active_layer_mut() else {
        return Rect::EMPTY;
    };
    let rw = r.width() as usize;
    let lock = layer.lock_alpha;
    // Em faixas de 256 linhas, para não pedir centenas de MB de uma vez numa tela grande.
    let (mut px, mut out) = (Vec::new(), Vec::new());
    let mut y0 = r.y0;
    while y0 < r.y1 {
        use rayon::prelude::*;
        let r = Rect::new(r.x0, y0, r.x1, (y0 + 256).min(r.y1));
        y0 = r.y1;
        let n = rw * r.height() as usize;
        px.resize(n, [0.0f32; 4]);
        layer.surface.read_rgba_into(r, &mut px);
        out.resize(n * 4, 0.0f32);
        let sel = sel_mask.as_ref().map(|m| m.read_region(r));
        out.par_chunks_mut(rw * 4)
            .zip(px.par_chunks(rw))
            .enumerate()
            .for_each(|(ry, (dst, src))| {
                let gy = r.y0 as usize + ry;
                for (j, (o, p)) in dst.as_chunks_mut::<4>().0.iter_mut().zip(src).enumerate() {
                    let i = ry * rw + j;
                    let gx = r.x0 as usize + j;
                    let hit = grown[idx(gx, gy)]
                        && sel
                            .as_ref()
                            .is_none_or(|m| m.get(i).is_some_and(|v| *v >= 0.5));
                    let keep = lock && p[3] <= 0.0;
                    *o = if hit && erase {
                        [p[0], p[1], p[2], 0.0]
                    } else if hit && !keep {
                        let a = if lock { p[3] } else { 1.0 };
                        let src = [color[0], color[1], color[2], color[3]];
                        let c = photocraft_color::blend::composite(
                            photocraft_color::BlendMode::Normal,
                            *p,
                            src,
                            1.0,
                        );
                        [c[0], c[1], c[2], if lock { a } else { c[3] }]
                    } else {
                        *p
                    };
                }
            });
        layer.surface.write_region(r, &out);
    }
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

    #[test]
    fn reference_layer_bounds_the_fill_on_another_layer() {
        let mut doc = Document::new(20, 20);
        let black = [0.0, 0.0, 0.0, 1.0];
        // Lineart na camada de baixo, marcada como referência; o balde pinta na de cima.
        for i in 0..20 {
            doc.layers[0].surface.write_pixel(10, i, &black);
        }
        doc.layers[0].reference = true;
        doc.add_layer();
        let r = flood_fill(&mut doc, 2, 2, [1.0, 0.0, 0.0, 1.0], 0.1);
        assert!(!r.is_empty());
        assert_eq!(doc.layers[1].surface.rgba(2, 2), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(doc.layers[1].surface.rgba(15, 2)[3], 0.0);
    }

    #[test]
    fn fill_stays_inside_the_selection() {
        let mut doc = Document::new(20, 20);
        doc.set_selection(crate::Selection::rect([0.0, 0.0], [5.0, 5.0], 20, 20));
        flood_fill(&mut doc, 2, 2, [1.0, 0.0, 0.0, 1.0], 0.1);
        assert_eq!(doc.layers[0].surface.rgba(2, 2)[3], 1.0);
        assert_eq!(doc.layers[0].surface.rgba(12, 12)[3], 0.0);
    }

    #[test]
    fn close_gap_stops_the_fill_at_a_small_opening() {
        let black = [0.0, 0.0, 0.0, 1.0];
        let make = || {
            let mut doc = Document::new(40, 40);
            let s = &mut doc.layers[0].surface;
            // Quadrado de 10..30 com uma abertura de 3 px no lado de cima.
            for i in 10..30 {
                if !(18..21).contains(&i) {
                    s.write_pixel(i, 10, &black);
                }
                s.write_pixel(i, 29, &black);
                s.write_pixel(10, i, &black);
                s.write_pixel(29, i, &black);
            }
            doc
        };
        let red = [1.0, 0.0, 0.0, 1.0];
        let mut leaky = make();
        flood_fill(&mut leaky, 20, 20, red, 0.1);
        assert_eq!(leaky.layers[0].surface.rgba(2, 2), red);
        let mut closed = make();
        let opts = FillOptions {
            close_gap: 5.0,
            ..FillOptions::default()
        };
        flood_fill_with(&mut closed, 20, 20, red, &opts);
        assert_eq!(closed.layers[0].surface.rgba(20, 20), red);
        assert_eq!(closed.layers[0].surface.rgba(2, 2)[3], 0.0);
        // Expandir área passa por baixo da linha.
        let mut grown = make();
        let opts = FillOptions {
            close_gap: 5.0,
            expand: 2.0,
            ..FillOptions::default()
        };
        flood_fill_with(&mut grown, 20, 20, [0.0, 1.0, 0.0, 1.0], &opts);
        assert_eq!(grown.layers[0].surface.rgba(10, 20), [0.0, 1.0, 0.0, 1.0]);
    }
}
