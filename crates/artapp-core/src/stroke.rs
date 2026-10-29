//! Um traço de pincel em andamento, desenhado ao vivo com o `StrokeRenderer` do PhotoCraft.

use photocraft_geom::Rect;
use photocraft_paint::{BrushSettings, StrokePoint, StrokeRenderer};
use photocraft_raster::Surface;

use crate::document::Document;

pub struct PaintSession {
    renderer: StrokeRenderer,
    /// A camada como estava antes do traço (o renderizador compõe sempre a partir dela).
    pre: Surface,
    layer_id: u64,
    lock_alpha: bool,
    points: usize,
}

impl PaintSession {
    /// Começa um traço na camada ativa. `zoom` é o zoom da vista (a suavização usa).
    /// Devolve `None` se a camada ativa estiver oculta, for vetorial ou não existir.
    pub fn begin(doc: &mut Document, brush: &BrushSettings, zoom: f32) -> Option<Self> {
        let layer = doc.active_layer()?;
        if !layer.visible || layer.is_vector() {
            return None;
        }
        let (pre, layer_id, lock_alpha) = (layer.surface.clone(), layer.id, layer.lock_alpha);
        let renderer = StrokeRenderer::new(&brush.bounded_for_render(), Some(pre.format()), zoom);
        doc.checkpoint();
        Some(Self {
            renderer,
            pre,
            layer_id,
            lock_alpha,
            points: 0,
        })
    }

    /// Quantos pontos o traço já recebeu.
    pub fn points(&self) -> usize {
        self.points
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
        self.renderer.push(&pts);
        self.composite(doc)
    }

    /// Termina o traço. Devolve o retângulo alterado.
    pub fn end(mut self, doc: &mut Document) -> Rect {
        self.renderer.finish();
        self.composite(doc)
    }

    /// Desfaz o traço inteiro (ex.: o segundo dedo encostou, era um gesto de zoom).
    pub fn cancel(self, doc: &mut Document) -> Rect {
        let bounds = self.renderer.bounds();
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
        let r =
            self.renderer
                .composite(&self.pre, &mut layer.surface, None, self.lock_alpha, false);
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
