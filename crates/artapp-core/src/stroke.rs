//! Um traço de pincel em andamento, desenhado ao vivo com o `StrokeRenderer` do PhotoCraft.

use photocraft_geom::Rect;
use photocraft_paint::symmetry::PresetSymmetry;
use photocraft_paint::{BrushSettings, StrokePoint, StrokeRenderer, UnionLive};
use photocraft_raster::Surface;

use crate::document::Document;

/// Afinar o começo e o fim do traço, em pixels do documento (o "início e fim" do Clip Studio).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Taper {
    pub start: f32,
    pub end: f32,
}

impl Taper {
    /// Fator 0..1 da pressão a `d` pixels do começo e `rest` pixels do fim.
    fn factor(&self, d: f64, rest: Option<f64>) -> f32 {
        let ramp = |len: f32, x: f64| {
            if len > 0.0 {
                ((x / f64::from(len)).clamp(0.0, 1.0) as f32).max(0.05)
            } else {
                1.0
            }
        };
        ramp(self.start, d) * rest.map_or(1.0, |r| ramp(self.end, r))
    }
}

/// Opções de um traço além do pincel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StrokeOptions {
    pub taper: Taper,
    /// Régua de simetria: o traço é repetido espelhado nos eixos.
    pub symmetry: Option<PresetSymmetry>,
}

pub struct PaintSession {
    renderer: StrokeRenderer,
    /// Um renderizador por espelho da simetria.
    mirrors: Vec<StrokeRenderer>,
    live: UnionLive,
    /// Ainda não compôs nada (a primeira composição com espelhos redesenha tudo).
    first: bool,
    brush: BrushSettings,
    zoom: f32,
    opts: StrokeOptions,
    /// Pontos recebidos (sem o afinamento), para refazer o traço com o fim afinado.
    raw: Vec<StrokePoint>,
    /// Distância percorrida até o último ponto.
    travelled: f64,
    /// A camada como estava antes do traço (o renderizador compõe sempre a partir dela).
    pre: Surface,
    layer_id: u64,
    lock_alpha: bool,
    /// Máscara da seleção no começo do traço (o traço só pinta dentro dela).
    selection: Option<Surface>,
    points: usize,
}

impl PaintSession {
    /// Começa um traço na camada ativa. `zoom` é o zoom da vista (a suavização usa).
    /// Devolve `None` se a camada ativa estiver oculta, for vetorial ou não existir.
    pub fn begin(doc: &mut Document, brush: &BrushSettings, zoom: f32) -> Option<Self> {
        Self::begin_with(doc, brush, zoom, StrokeOptions::default())
    }

    /// Como [`Self::begin`], afinando o começo e o fim do traço.
    pub fn begin_tapered(
        doc: &mut Document,
        brush: &BrushSettings,
        zoom: f32,
        taper: Taper,
    ) -> Option<Self> {
        Self::begin_with(
            doc,
            brush,
            zoom,
            StrokeOptions {
                taper,
                symmetry: None,
            },
        )
    }

    /// Como [`Self::begin`], com afinamento e simetria.
    pub fn begin_with(
        doc: &mut Document,
        brush: &BrushSettings,
        zoom: f32,
        opts: StrokeOptions,
    ) -> Option<Self> {
        let layer = doc.active_layer()?;
        if !layer.visible || layer.is_vector() {
            return None;
        }
        let (pre, layer_id, lock_alpha) = (layer.surface.clone(), layer.id, layer.lock_alpha);
        let selection = doc.selection.as_ref().map(|s| s.mask.clone());
        let make = || StrokeRenderer::new(&brush.bounded_for_render(), Some(pre.format()), zoom);
        let renderer = make();
        let mirrors = opts.symmetry.map_or(0, |s| s.mirror_count());
        let mirrors = (0..mirrors).map(|_| make()).collect();
        doc.checkpoint();
        Some(Self {
            renderer,
            mirrors,
            live: UnionLive::default(),
            first: true,
            brush: brush.clone(),
            zoom,
            opts,
            raw: Vec::new(),
            travelled: 0.0,
            pre,
            layer_id,
            lock_alpha,
            selection,
            points: 0,
        })
    }

    /// Quantos pontos o traço já recebeu.
    pub fn points(&self) -> usize {
        self.points
    }

    /// Manda pontos para o traço e para os espelhos.
    fn push(&mut self, pts: &[StrokePoint]) {
        self.renderer.push(pts);
        if let Some(sym) = self.opts.symmetry {
            for (m, pass) in self.mirrors.iter_mut().zip(sym.reflected_passes(pts)) {
                m.push(&pass);
            }
        }
    }

    /// Acrescenta pontos e desenha. Devolve o retângulo alterado (já recortado à tela).
    pub fn add(&mut self, doc: &mut Document, pts: &[StrokePoint]) -> Rect {
        let pts: Vec<StrokePoint> = pts
            .iter()
            .copied()
            .filter(|p| p.x.is_finite() && p.y.is_finite())
            .collect();
        if pts.is_empty() {
            return Rect::EMPTY;
        }
        self.points += pts.len();
        let mut live = Vec::with_capacity(pts.len());
        for p in pts {
            if let Some(last) = self.raw.last() {
                self.travelled += (p.x - last.x).hypot(p.y - last.y);
            }
            self.raw.push(p);
            live.push(StrokePoint {
                pressure: p.pressure * self.opts.taper.factor(self.travelled, None),
                ..p
            });
        }
        self.push(&live);
        self.composite(doc)
    }

    /// Termina o traço. Devolve o retângulo alterado.
    pub fn end(mut self, doc: &mut Document) -> Rect {
        if self.opts.taper.end > 0.0 && self.raw.len() > 1 {
            return self.redo_with_end_taper(doc);
        }
        self.renderer.finish();
        self.mirrors.iter_mut().for_each(StrokeRenderer::finish);
        self.composite(doc)
    }

    /// Refaz o traço inteiro com o fim afinado (só dá para saber onde é o fim quando ele acaba).
    fn redo_with_end_taper(mut self, doc: &mut Document) -> Rect {
        let before = self.bounds();
        let total = self.travelled;
        let mut d = 0.0;
        let pts: Vec<StrokePoint> = self
            .raw
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if i > 0 {
                    let q = self.raw[i - 1];
                    d += (p.x - q.x).hypot(p.y - q.y);
                }
                StrokePoint {
                    pressure: p.pressure * self.opts.taper.factor(d, Some(total - d)),
                    ..*p
                }
            })
            .collect();
        let make = || {
            StrokeRenderer::new(
                &self.brush.bounded_for_render(),
                Some(self.pre.format()),
                self.zoom,
            )
        };
        self.renderer = make();
        self.mirrors = self.mirrors.iter().map(|_| make()).collect();
        self.push(&pts);
        self.renderer.finish();
        self.mirrors.iter_mut().for_each(StrokeRenderer::finish);
        let canvas = doc.bounds();
        let id = self.layer_id;
        let Some(layer) = doc.layers.iter_mut().find(|l| l.id == id) else {
            return Rect::EMPTY;
        };
        layer.surface = self.pre.clone();
        let r = self.renderer.composite_union_many(
            self.mirrors.iter(),
            &self.pre,
            &mut layer.surface,
            self.selection.as_ref(),
            self.lock_alpha,
        );
        r.union(&before.inflate(2)).intersect(&canvas)
    }

    fn bounds(&self) -> Rect {
        self.mirrors
            .iter()
            .fold(self.renderer.bounds(), |r, m| r.union(&m.bounds()))
    }

    /// Desfaz o traço inteiro (ex.: o segundo dedo encostou, era um gesto de zoom).
    pub fn cancel(self, doc: &mut Document) -> Rect {
        let bounds = self.bounds();
        let id = self.layer_id;
        if let Some(l) = doc.layers.iter_mut().find(|l| l.id == id) {
            l.surface = self.pre;
        }
        doc.drop_checkpoint();
        bounds.inflate(2).intersect(&doc.bounds())
    }

    fn composite(&mut self, doc: &mut Document) -> Rect {
        let canvas = doc.bounds();
        let id = self.layer_id;
        let Some(layer) = doc.layers.iter_mut().find(|l| l.id == id) else {
            return Rect::EMPTY;
        };
        let r = if self.mirrors.is_empty() {
            self.renderer.composite(
                &self.pre,
                &mut layer.surface,
                self.selection.as_ref(),
                self.lock_alpha,
                false,
            )
        } else {
            let mut passes: Vec<&mut StrokeRenderer> = std::iter::once(&mut self.renderer)
                .chain(self.mirrors.iter_mut())
                .collect();
            let full = std::mem::replace(&mut self.first, false);
            StrokeRenderer::composite_union_live(
                &mut passes,
                &mut self.live,
                full,
                &self.pre,
                &mut layer.surface,
                self.selection.as_ref(),
                self.lock_alpha,
            )
        };
        r.intersect(&canvas)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn stroke_paints_and_undo_restores() {
        let mut doc = Document::new(200, 200);
        let brush = BrushSettings {
            size: 10.0,
            color: [1.0, 0.0, 0.0, 1.0],
            ..Default::default()
        };
        let mut s = PaintSession::begin(&mut doc, &brush, 1.0).unwrap();
        let pts: Vec<_> = (0..50)
            .map(|i| StrokePoint::new(20.0 + i as f64 * 3.0, 100.0, 1.0))
            .collect();
        let dirty = s.add(&mut doc, &pts);
        assert!(!dirty.is_empty());
        s.end(&mut doc);
        let px = doc.layers[0].surface.rgba(80, 100);
        assert!(px[0] > 0.9 && px[3] > 0.9, "{px:?}");
        assert!(doc.undo());
        assert_eq!(doc.layers[0].surface.rgba(80, 100)[3], 0.0);
        assert!(doc.redo());
        assert!(doc.layers[0].surface.rgba(80, 100)[3] > 0.9);
    }

    #[test]
    fn tapered_stroke_is_thin_at_both_ends() {
        let mut doc = Document::new(300, 100);
        let brush = BrushSettings {
            size: 20.0,
            ..Default::default()
        };
        let taper = Taper {
            start: 80.0,
            end: 80.0,
        };
        let mut s = PaintSession::begin_tapered(&mut doc, &brush, 1.0, taper).unwrap();
        let pts: Vec<_> = (0..=100)
            .map(|i| StrokePoint::new(50.0 + i as f64 * 2.0, 50.0, 1.0))
            .collect();
        s.add(&mut doc, &pts);
        s.end(&mut doc);
        let surf = &doc.layers[0].surface;
        // No meio o traço tem a largura toda; perto das pontas, bem menos.
        assert!(surf.rgba(150, 57)[3] > 0.5);
        assert!(surf.rgba(54, 57)[3] < 0.1);
        assert!(surf.rgba(246, 57)[3] < 0.1);
    }

    #[test]
    fn symmetry_mirrors_the_stroke() {
        use photocraft_paint::symmetry::SymmetryMode;
        let mut doc = Document::new(200, 100);
        let opts = StrokeOptions {
            symmetry: PresetSymmetry::new(SymmetryMode::Vertical, [100.0, 50.0], 0.0),
            ..Default::default()
        };
        let mut s =
            PaintSession::begin_with(&mut doc, &BrushSettings::default(), 1.0, opts).unwrap();
        let pts: Vec<_> = (0..20)
            .map(|i| StrokePoint::new(20.0 + i as f64, 50.0, 1.0))
            .collect();
        s.add(&mut doc, &pts);
        s.end(&mut doc);
        let surf = &doc.layers[0].surface;
        assert!(surf.rgba(30, 50)[3] > 0.5);
        // Espelho no eixo vertical x = 100: o ponto 30 vira 170.
        assert!(surf.rgba(170, 50)[3] > 0.5);
    }

    #[test]
    fn cancel_leaves_no_trace_or_history() {
        let mut doc = Document::new(100, 100);
        let mut s = PaintSession::begin(&mut doc, &BrushSettings::default(), 1.0).unwrap();
        s.add(
            &mut doc,
            &[
                StrokePoint::new(50.0, 50.0, 1.0),
                StrokePoint::new(60.0, 50.0, 1.0),
            ],
        );
        s.cancel(&mut doc);
        assert_eq!(doc.layers[0].surface.rgba(55, 50)[3], 0.0);
        assert!(!doc.can_undo());
    }

    #[test]
    fn eraser_removes_paint() {
        let mut doc = Document::new(100, 100);
        let line: Vec<_> = (0..30)
            .map(|i| StrokePoint::new(20.0 + i as f64 * 2.0, 50.0, 1.0))
            .collect();
        let mut s = PaintSession::begin(
            &mut doc,
            &BrushSettings {
                size: 12.0,
                ..Default::default()
            },
            1.0,
        )
        .unwrap();
        s.add(&mut doc, &line);
        s.end(&mut doc);
        assert!(doc.layers[0].surface.rgba(50, 50)[3] > 0.9);
        let eraser = BrushSettings {
            size: 20.0,
            erase: true,
            ..Default::default()
        };
        let mut s = PaintSession::begin(&mut doc, &eraser, 1.0).unwrap();
        s.add(&mut doc, &line);
        s.end(&mut doc);
        assert!(doc.layers[0].surface.rgba(50, 50)[3] < 0.05);
    }
}
