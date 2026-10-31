//! Composição das camadas para exibição e exportação, com os modos de mesclagem do PhotoCraft.

use photocraft_color::blend;
use photocraft_geom::Rect;
use photocraft_raster::Surface;

use crate::document::{Document, Layer};

/// Quais camadas entram na composição.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    /// Tudo que está visível (a tela de desenho, o conta-gotas).
    Display,
    /// Sem as camadas de rascunho (exportar, balde olhando todas as camadas).
    Final,
    /// Só as camadas de referência (balde quando alguma camada é de referência).
    Reference,
}

/// Compõe `r` (recortado à tela) em RGBA 8 bits não pré-multiplicado, linha a linha.
/// Com `with_paper = false` o papel fica transparente (exportar com fundo transparente).
/// Devolve o retângulo efetivamente composto e os pixels.
pub fn composite_rgba8(doc: &Document, r: Rect, with_paper: bool) -> (Rect, Vec<[u8; 4]>) {
    composite_pass(doc, r, with_paper, Pass::Display)
}

/// Como [`composite_rgba8`], escolhendo as camadas pelo `pass`. Com a animação ligada, a tela
/// (`Pass::Display`) mostra só o quadro atual e a pele de cebola.
pub fn composite_pass(
    doc: &Document,
    r: Rect,
    with_paper: bool,
    pass: Pass,
) -> (Rect, Vec<[u8; 4]>) {
    let a = &doc.animation;
    let view = (pass == Pass::Display && a.enabled).then_some((a.current, !a.pages));
    composite_view(doc, r, with_paper, pass, view)
}

/// O quadro `k` inteiro da animação, sem pele de cebola (exportar).
pub fn composite_frame(doc: &Document, k: usize, with_paper: bool) -> Vec<[u8; 4]> {
    composite_view(doc, doc.bounds(), with_paper, Pass::Final, Some((k, false))).1
}

/// `view`: quadro mostrado e se há pele de cebola.
pub(crate) fn composite_view(
    doc: &Document,
    r: Rect,
    with_paper: bool,
    pass: Pass,
    view: Option<(usize, bool)>,
) -> (Rect, Vec<[u8; 4]>) {
    use crate::animation::FrameLook;
    let r = r.intersect(&doc.bounds());
    if r.is_empty() {
        return (r, Vec::new());
    }
    let n = r.width() as usize * r.height() as usize;
    let base = if with_paper { doc.paper } else { [0.0; 4] };
    let mut acc = vec![base; n];
    let mut row = vec![[0.0f32; 4]; n];
    // Alfa da camada base do grupo de recorte atual (`None` = a base não aparece).
    let mut clip_base: Option<Vec<f32>> = None;
    let mut mask_row = Vec::new();
    for (i, layer) in doc.layers.iter().enumerate() {
        // A máscara entra junto com a camada de baixo, não sozinha.
        if layer.mask {
            continue;
        }
        let included = match pass {
            Pass::Display => true,
            Pass::Final => !layer.draft,
            Pass::Reference => layer.reference,
        };
        // Pastas não têm tinta: só escondem e esmaecem o que está dentro.
        if layer.folder {
            continue;
        }
        let fold = doc.folder_mix(i);
        let look = view.map_or(FrameLook::Shown, |(k, onion)| doc.frame_look(i, k, onion));
        let (look_k, tint) = match look {
            FrameLook::Shown => (1.0, None),
            FrameLook::Onion(o, t) => (o, t),
            FrameLook::Hidden => (0.0, None),
        };
        let opacity = layer.opacity * fold.unwrap_or(0.0) * look_k;
        let shown = included && layer.visible && opacity > 0.0;
        if !layer.clip {
            clip_base = None;
        }
        let reach = layer.border.map_or(r, |b| r.inflate(b.pad()));
        if !shown || !layer.surface.has_tiles_in(reach) {
            if !layer.clip && shown {
                // Base vazia neste trecho: as camadas recortadas nela também somem aqui.
                clip_base = Some(vec![0.0; n]);
            }
            continue;
        }
        match &layer.border {
            Some(b) => {
                crate::border::read_with_border(&layer.surface, b, r, doc.bounds(), &mut row)
            }
            None => layer.surface.read_rgba_into(r, &mut row),
        }
        if let Some(t) = &layer.tone {
            crate::tone::apply(t, r, &mut row);
        }
        if let Some(c) = tint {
            // Pele de cebola colorida: a tinta vira a cor do lado (mantendo o alfa).
            for p in row.iter_mut() {
                p[0] = c[0];
                p[1] = c[1];
                p[2] = c[2];
            }
        }
        if let Some(m) = doc.mask_above(i) {
            mask_row.resize(n, [0.0f32; 4]);
            m.surface.read_rgba_into(r, &mut mask_row);
            for (p, k) in row.iter_mut().zip(&mask_row) {
                p[3] *= mask_visibility(*k);
            }
        }
        if layer.clip {
            let Some(mask) = &clip_base else {
                continue;
            };
            for ((dst, src), m) in acc.iter_mut().zip(&row).zip(mask) {
                let a = src[3] * m;
                if a > 0.0 {
                    *dst =
                        blend::composite(layer.blend, *dst, [src[0], src[1], src[2], a], opacity);
                }
            }
            continue;
        }
        clip_base = Some(row.iter().map(|p| p[3]).collect());
        for (dst, src) in acc.iter_mut().zip(&row) {
            if src[3] > 0.0 {
                *dst = blend::composite(layer.blend, *dst, *src, opacity);
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

/// Quanto um pixel da máscara deixa ver: o cinza dele sobre branco (sem tinta = tudo aparece,
/// preto = some).
pub fn mask_visibility(p: [f32; 4]) -> f32 {
    let l = 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
    (l * p[3] + 1.0 - p[3]).clamp(0.0, 1.0)
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
        // Uma camada recortada mesclada na base só fica onde a base tem tinta.
        let s = if top.clip {
            [s[0], s[1], s[2], s[3] * d[3]]
        } else {
            *s
        };
        let p = if s[3] > 0.0 {
            blend::composite(top.blend, *d, s, top.opacity)
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
    fn clipped_layer_shows_only_over_its_base() {
        let mut doc = Document::new(4, 1);
        paint(&mut doc, 0, Rect::new(0, 0, 2, 1), [0.0, 0.0, 1.0, 1.0]);
        doc.add_layer();
        paint(&mut doc, 1, Rect::new(0, 0, 4, 1), [1.0, 0.0, 0.0, 1.0]);
        doc.layers[1].clip = true;
        let px = composite_rgba8(&doc, doc.bounds(), true).1;
        assert_eq!(px[0], [255, 0, 0, 255]);
        assert_eq!(px[3], [255, 255, 255, 255]);
        // Base oculta: o recorte some junto.
        doc.layers[0].visible = false;
        assert_eq!(
            composite_rgba8(&doc, doc.bounds(), true).1[0],
            [255, 255, 255, 255]
        );
    }

    #[test]
    fn drafts_stay_out_of_the_final_image() {
        let mut doc = Document::new(2, 1);
        let b = doc.bounds();
        paint(&mut doc, 0, b, [0.0, 0.0, 0.0, 1.0]);
        doc.layers[0].draft = true;
        assert_eq!(
            composite_rgba8(&doc, doc.bounds(), true).1[0],
            [0, 0, 0, 255]
        );
        assert_eq!(
            composite_pass(&doc, doc.bounds(), true, Pass::Final).1[0],
            [255, 255, 255, 255]
        );
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
