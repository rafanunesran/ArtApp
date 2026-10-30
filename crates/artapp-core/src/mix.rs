//! Ferramenta Misturar (o "Misturar" do Clip Studio): desfocar e dedo (borrar), feitas marca a marca
//! com a maquinaria de retoque do PhotoCraft. Cada marca lê os pixels de baixo dela já com as marcas
//! anteriores aplicadas, então o dedo arrasta a tinta pelo caminho.

use photocraft_geom::Rect;
use photocraft_paint::retouch::{Region, Smudge, alpha_index};
use photocraft_paint::{BrushContext, BrushSettings, Stroke, StrokePoint, dabs};
use photocraft_raster::Surface;

use crate::document::Document;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixKind {
    /// Suaviza (média dos vizinhos).
    Blur,
    /// Arrasta a tinta, como o dedo.
    Smudge,
}

pub struct MixSession {
    kind: MixKind,
    brush: BrushSettings,
    /// 0..1: quanto cada marca age.
    strength: f32,
    points: Vec<StrokePoint>,
    /// Marcas já aplicadas.
    done: usize,
    smudge: Smudge,
    pre: Surface,
    layer_id: u64,
    lock_alpha: bool,
    selection: Option<Surface>,
}

impl MixSession {
    pub fn begin(
        doc: &mut Document,
        kind: MixKind,
        brush: &BrushSettings,
        strength: f32,
    ) -> Option<Self> {
        let layer = doc.active_layer()?;
        if !layer.visible || layer.is_vector() {
            return None;
        }
        let pre = layer.surface.clone();
        let (layer_id, lock_alpha) = (layer.id, layer.lock_alpha);
        let selection = doc.selection.as_ref().map(|s| s.mask.clone());
        let brush = brush.bounded_for_render();
        let smudge = Smudge::new(strength, None, brush.size).with_alpha(alpha_index(&pre.format()));
        doc.checkpoint();
        Some(Self {
            kind,
            brush,
            strength: strength.clamp(0.0, 1.0),
            points: Vec::new(),
            done: 0,
            smudge,
            pre,
            layer_id,
            lock_alpha,
            selection,
        })
    }

    /// Acrescenta pontos e aplica as marcas novas. Devolve o retângulo alterado.
    pub fn add(&mut self, doc: &mut Document, pts: &[StrokePoint]) -> Rect {
        self.points.extend(
            pts.iter()
                .copied()
                .filter(|p| p.x.is_finite() && p.y.is_finite()),
        );
        let stroke = Stroke {
            brush: self.brush.clone(),
            points: self.points.clone(),
        };
        let all = dabs(&stroke);
        let ctx = BrushContext::new(&self.brush);
        let canvas = doc.bounds();
        let id = self.layer_id;
        let Some(layer) = doc.layers.iter_mut().find(|l| l.id == id) else {
            return Rect::EMPTY;
        };
        let mut dirty = Rect::EMPTY;
        let halo = (self.brush.size / 2.0).ceil() as i32 + 3;
        for (i, d) in all.iter().enumerate().skip(self.done) {
            let fp = ctx.footprint(d, i);
            let rect = fp.rect.intersect(&canvas);
            if rect.is_empty() {
                continue;
            }
            let outer = fp.rect.inflate(halo).intersect(&canvas);
            let orig = Region::read(&layer.surface, outer);
            let mut work = orig.clone();
            match self.kind {
                MixKind::Smudge => self.smudge.dab(&mut work, &fp),
                MixKind::Blur => blur_dab(&mut work, &orig, &fp, self.strength),
            }
            // Devolve só o retângulo da marca, respeitando seleção e bloqueio de transparência.
            let mut out = orig.crop(rect);
            let sel = self.selection.as_ref().map(|s| s.read_region(rect));
            let a = alpha_index(&layer.surface.format());
            let n = out.ch;
            for y in rect.y0..rect.y1 {
                for x in rect.x0..rect.x1 {
                    let i = out.index(x, y);
                    let k = sel.as_ref().map_or(1.0, |m| m[i / n]);
                    if k <= 0.0 {
                        continue;
                    }
                    let w = work.px(x, y).to_vec();
                    let px = out.px_mut(x, y);
                    for c in 0..n {
                        if self.lock_alpha && Some(c) == a {
                            continue;
                        }
                        px[c] += (w[c] - px[c]) * k;
                    }
                }
            }
            out.write(&mut layer.surface);
            dirty = dirty.union(&rect);
        }
        self.done = all.len();
        dirty
    }

    pub fn end(self) -> Rect {
        Rect::EMPTY
    }

    /// Desfaz o traço inteiro.
    pub fn cancel(self, doc: &mut Document) -> Rect {
        let id = self.layer_id;
        if let Some(l) = doc.layers.iter_mut().find(|l| l.id == id) {
            l.surface = self.pre;
        }
        doc.drop_checkpoint();
        doc.bounds()
    }
}

/// Média dos vizinhos (raio que cresce com a marca), misturada pela cobertura da marca.
fn blur_dab(
    work: &mut Region,
    orig: &Region,
    fp: &photocraft_paint::retouch::Footprint,
    strength: f32,
) {
    let wr = work.rect;
    let n = work.ch;
    let r = ((fp.dab.radius / 6.0).round() as i32).clamp(1, 6);
    let src = orig.clone();
    let mut acc = vec![0.0f32; n];
    for y in fp.rect.y0.max(wr.y0)..fp.rect.y1.min(wr.y1) {
        for x in fp.rect.x0.max(wr.x0)..fp.rect.x1.min(wr.x1) {
            let k = fp.at(x, y) * strength;
            if k <= 0.0 {
                continue;
            }
            acc.iter_mut().for_each(|v| *v = 0.0);
            let mut count = 0.0;
            for yy in (y - r).max(wr.y0)..=(y + r).min(wr.y1 - 1) {
                for xx in (x - r).max(wr.x0)..=(x + r).min(wr.x1 - 1) {
                    for (c, v) in src.px(xx, yy).iter().enumerate() {
                        acc[c] += v;
                    }
                    count += 1.0;
                }
            }
            let px = work.px_mut(x, y);
            for c in 0..n {
                px[c] += (acc[c] / count - px[c]) * k;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn smudge_drags_paint_and_blur_softens_the_edge() {
        for kind in [MixKind::Smudge, MixKind::Blur] {
            let mut doc = Document::new(100, 40);
            doc.layers[0]
                .surface
                .fill_rect(Rect::new(0, 0, 50, 40), &[0.0, 0.0, 0.0, 1.0]);
            let brush = BrushSettings {
                size: 16.0,
                ..Default::default()
            };
            let mut s = MixSession::begin(&mut doc, kind, &brush, 0.9).unwrap();
            let pts: Vec<_> = (0..30)
                .map(|i| StrokePoint::new(40.0 + i as f64, 20.0, 1.0))
                .collect();
            assert!(!s.add(&mut doc, &pts).is_empty());
            s.end();
            // Tinta passou da borda (x = 50) para a área vazia.
            assert!(doc.layers[0].surface.rgba(52, 20)[3] > 0.05, "{kind:?}");
            assert!(doc.undo());
            assert_eq!(doc.layers[0].surface.rgba(52, 20)[3], 0.0);
        }
    }
}
