//! Ferramenta Misturar (o "Misturar" do Clip Studio): desfocar e dedo (borrar), feitas marca a marca
//! com a maquinaria de retoque do PhotoCraft. Cada marca lê os pixels de baixo dela já com as marcas
//! anteriores aplicadas, então o dedo arrasta a tinta pelo caminho.

use photocraft_algo::retouch;
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
    /// Clareia (subexpor), puxando os meios-tons.
    Dodge,
    /// Escurece (superexpor).
    Burn,
    /// Satura as cores (esponja).
    Saturate,
    /// Tira saturação (esponja).
    Desaturate,
    /// Realça as bordas (nitidez).
    Sharpen,
    /// Carimbo: copia de outro lugar da camada (a origem fica em `MixSession::with_source`).
    Clone,
    /// Corretivo: refaz a área pintada a partir da textura em volta (fica em `heal`).
    Heal,
    /// Borracha de fundo: apaga a cor que está sob o centro do pincel (e as parecidas, pela
    /// força como tolerância), deixando as outras cores.
    BgErase,
}

type DabEffect = Box<dyn FnMut(&mut Region, &photocraft_paint::retouch::Footprint)>;

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
    /// Carimbo: o ponto de origem e, depois do primeiro ponto, o deslocamento até ela.
    source: Option<[f64; 2]>,
    offset: Option<(i32, i32)>,
    /// Borracha de fundo: o efeito de cada marca (lê a camada de antes do traço).
    bg: Option<DabEffect>,
}

impl MixSession {
    /// De onde o carimbo copia (coordenadas do documento).
    pub fn with_source(mut self, source: [f64; 2]) -> Self {
        self.source = Some(source);
        self
    }

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
        let bg = (kind == MixKind::BgErase)
            .then(|| {
                let bs = photocraft_paint::bg_erase::BgEraseSettings {
                    tolerance: strength.clamp(0.02, 1.0),
                    ..Default::default()
                };
                photocraft_paint::bg_erase::background_eraser_effect(&pre, &bs)
                    .map(|e| Box::new(e) as DabEffect)
            })
            .flatten();
        doc.checkpoint();
        Some(Self {
            bg,
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
            source: None,
            offset: None,
        })
    }

    /// Acrescenta pontos e aplica as marcas novas. Devolve o retângulo alterado.
    pub fn add(&mut self, doc: &mut Document, pts: &[StrokePoint]) -> Rect {
        self.points.extend(
            pts.iter()
                .copied()
                .filter(|p| p.x.is_finite() && p.y.is_finite()),
        );
        if self.offset.is_none()
            && let (Some(src), Some(first)) = (self.source, self.points.first())
        {
            self.offset = Some((
                (src[0] - first.x).round() as i32,
                (src[1] - first.y).round() as i32,
            ));
        }
        if self.kind == MixKind::Clone && self.offset.is_none() {
            return Rect::EMPTY;
        }
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
            let k = self.strength;
            match self.kind {
                MixKind::Smudge => self.smudge.dab(&mut work, &fp),
                MixKind::Blur => blur_dab(&mut work, &orig, &fp, k),
                MixKind::Sharpen => sharpen_dab(&mut work, &orig, &fp, k),
                MixKind::Dodge | MixKind::Burn => {
                    let burn = self.kind == MixKind::Burn;
                    tone_dab(&mut work, &fp, k, |c, e| {
                        retouch::dodge_burn(c, e, retouch::ToneRange::Midtones, burn, true)
                    })
                }
                MixKind::Saturate | MixKind::Desaturate => {
                    let up = self.kind == MixKind::Saturate;
                    tone_dab(&mut work, &fp, k, |c, e| retouch::sponge(c, e, up, true))
                }
                MixKind::Heal => {}
                MixKind::BgErase => {
                    if let Some(e) = &mut self.bg {
                        e(&mut work, &fp);
                    }
                }
                MixKind::Clone => {
                    let (dx, dy) = self.offset.unwrap_or_default();
                    clone_dab(&mut work, &self.pre, &fp, k, dx, dy)
                }
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

/// Aplica `f` (cor sem alfa, intensidade) a cada pixel da marca, na medida da cobertura.
fn tone_dab(
    work: &mut Region,
    fp: &photocraft_paint::retouch::Footprint,
    strength: f32,
    f: impl Fn([f32; 3], f32) -> [f32; 3],
) {
    let wr = work.rect;
    if work.ch < 3 {
        return;
    }
    for y in fp.rect.y0.max(wr.y0)..fp.rect.y1.min(wr.y1) {
        for x in fp.rect.x0.max(wr.x0)..fp.rect.x1.min(wr.x1) {
            let k = fp.at(x, y) * strength;
            if k <= 0.0 {
                continue;
            }
            let px = work.px_mut(x, y);
            let out = f([px[0], px[1], px[2]], k);
            px[..3].copy_from_slice(&out);
        }
    }
}

/// Nitidez: afasta cada pixel da média dos vizinhos.
fn sharpen_dab(
    work: &mut Region,
    orig: &Region,
    fp: &photocraft_paint::retouch::Footprint,
    strength: f32,
) {
    let mut blurred = orig.clone();
    blur_dab(&mut blurred, orig, fp, 1.0);
    let wr = work.rect;
    let a = if work.ch == 4 { Some(3) } else { None };
    for y in fp.rect.y0.max(wr.y0)..fp.rect.y1.min(wr.y1) {
        for x in fp.rect.x0.max(wr.x0)..fp.rect.x1.min(wr.x1) {
            let k = fp.at(x, y) * strength;
            if k <= 0.0 {
                continue;
            }
            let o = orig.px(x, y).to_vec();
            let b = blurred.px(x, y).to_vec();
            let px = work.px_mut(x, y);
            for c in 0..px.len() {
                if Some(c) == a {
                    continue;
                }
                px[c] = (o[c] + (o[c] - b[c]) * k * 1.5).clamp(0.0, 1.0);
            }
        }
    }
}

/// Carimbo: copia os pixels de antes do traço, deslocados por `(dx, dy)`.
fn clone_dab(
    work: &mut Region,
    pre: &Surface,
    fp: &photocraft_paint::retouch::Footprint,
    strength: f32,
    dx: i32,
    dy: i32,
) {
    let wr = work.rect;
    let src = Region::read(
        pre,
        Rect::new(wr.x0 + dx, wr.y0 + dy, wr.x1 + dx, wr.y1 + dy),
    );
    for y in fp.rect.y0.max(wr.y0)..fp.rect.y1.min(wr.y1) {
        for x in fp.rect.x0.max(wr.x0)..fp.rect.x1.min(wr.x1) {
            let k = (fp.at(x, y) * strength).min(1.0);
            if k <= 0.0 {
                continue;
            }
            let s = src.px(x + dx, y + dy).to_vec();
            let px = work.px_mut(x, y);
            for c in 0..px.len() {
                px[c] += (s[c] - px[c]) * k;
            }
        }
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

    #[test]
    fn background_eraser_keeps_other_colours() {
        let mut doc = Document::new(60, 40);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 60, 40), &[1.0, 1.0, 1.0, 1.0]);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(28, 0, 32, 40), &[0.0, 0.0, 0.0, 1.0]);
        let brush = BrushSettings {
            size: 30.0,
            ..Default::default()
        };
        let mut s = MixSession::begin(&mut doc, MixKind::BgErase, &brush, 0.3).unwrap();
        let pts: Vec<_> = (0..10)
            .map(|i| StrokePoint::new(10.0 + f64::from(i) * 0.5, 20.0, 1.0))
            .collect();
        s.add(&mut doc, &pts);
        assert!(doc.layers[0].surface.rgba(12, 20)[3] < 0.1);
        assert!(doc.layers[0].surface.rgba(29, 20)[3] > 0.9);
    }

    #[test]
    fn retouch_kinds_change_colour() {
        let run = |kind: MixKind, fill: [f32; 4], source: Option<[f64; 2]>| {
            let mut doc = Document::new(60, 60);
            doc.layers[0]
                .surface
                .fill_rect(Rect::new(0, 0, 60, 30), &fill);
            let brush = BrushSettings {
                size: 12.0,
                ..Default::default()
            };
            let mut s = MixSession::begin(&mut doc, kind, &brush, 1.0).unwrap();
            if let Some(src) = source {
                s = s.with_source(src);
            }
            let pts: Vec<_> = (0..10)
                .map(|i| StrokePoint::new(20.0 + i as f64, 15.0, 1.0))
                .collect();
            s.add(&mut doc, &pts);
            doc.layers[0].surface.rgba(25, 15)
        };
        let grey = [0.5, 0.5, 0.5, 1.0];
        assert!(run(MixKind::Dodge, grey, None)[0] > 0.55);
        assert!(run(MixKind::Burn, grey, None)[0] < 0.45);
        let red = [0.8, 0.3, 0.3, 1.0];
        let d = run(MixKind::Desaturate, red, None);
        assert!(d[0] - d[1] < 0.45);
        // Carimbo de cima (tinta) para baixo (vazio): a tinta aparece onde não havia.
        let mut doc = Document::new(60, 60);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 60, 30), &red);
        let brush = BrushSettings {
            size: 12.0,
            ..Default::default()
        };
        let mut s = MixSession::begin(&mut doc, MixKind::Clone, &brush, 1.0)
            .unwrap()
            .with_source([20.0, 10.0]);
        let pts: Vec<_> = (0..10)
            .map(|i| StrokePoint::new(20.0 + i as f64, 45.0, 1.0))
            .collect();
        s.add(&mut doc, &pts);
        assert!(doc.layers[0].surface.rgba(25, 45)[3] > 0.5);
    }
}
