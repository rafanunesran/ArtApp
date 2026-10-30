//! Liquefazer (o "Liquify" do PhotoCraft / a "Deformação de malha" do Clip Studio): empurrar,
//! torcer, contrair, inflar, suavizar e restaurar a camada raster ativa com o pincel.

use photocraft_algo::liquify::{LiquifyField, LiquifyStroke, apply_liquify, auto_cell};
use photocraft_geom::Rect;
use photocraft_raster::Surface;

pub use photocraft_algo::liquify::LiquifyTool;

use crate::document::Document;

/// As ferramentas do pincel de liquefazer, com nome em português.
pub const TOOLS: [(LiquifyTool, &str); 8] = [
    (LiquifyTool::ForwardWarp, "Empurrar"),
    (LiquifyTool::PushLeft, "Deslocar"),
    (LiquifyTool::TwirlCw, "Torcer (horário)"),
    (LiquifyTool::TwirlCcw, "Torcer (anti-horário)"),
    (LiquifyTool::Pucker, "Contrair"),
    (LiquifyTool::Bloat, "Inflar"),
    (LiquifyTool::Smooth, "Suavizar"),
    (LiquifyTool::Reconstruct, "Restaurar"),
];

pub struct LiquifySession {
    stroke: LiquifyStroke,
    field: LiquifyField,
    pre: Surface,
    layer_id: u64,
    last: Option<[f64; 3]>,
    dirty: Rect,
}

impl LiquifySession {
    /// `pressure` 0..1 é a força do pincel.
    pub fn begin(doc: &mut Document, tool: LiquifyTool, size: f32, pressure: f32) -> Option<Self> {
        let layer = doc.active_layer()?;
        if !layer.visible || layer.is_vector() {
            return None;
        }
        let pre = layer.surface.clone();
        let layer_id = layer.id;
        let bounds = doc.bounds();
        let mut stroke = LiquifyStroke::new(tool, f64::from(size.clamp(1.0, 5000.0)));
        stroke.pressure = f64::from(pressure.clamp(0.01, 1.0)) * 100.0;
        doc.checkpoint();
        Some(Self {
            stroke,
            field: LiquifyField::new(bounds, auto_cell(bounds)),
            pre,
            layer_id,
            last: None,
            dirty: Rect::EMPTY,
        })
    }

    /// Acrescenta um ponto (`pressure` 0..1) e refaz a camada. Devolve a área a recompor.
    pub fn add(&mut self, doc: &mut Document, x: f64, y: f64, pressure: f32) -> Rect {
        if !(x.is_finite() && y.is_finite()) {
            return Rect::EMPTY;
        }
        let p = [x, y, f64::from(pressure.clamp(0.0, 1.0))];
        let r = match self.last {
            None => self.field.stroke_begin(&self.stroke, p),
            Some(a) => self.field.stroke_segment(&self.stroke, a, p),
        };
        self.last = Some(p);
        if r.is_empty() {
            return Rect::EMPTY;
        }
        // A área que pode mudar: o que o campo mexeu, mais o quanto os pixels andam.
        let reach = (self.field.max_displacement() * 1.6).ceil() as i32 + 4;
        self.dirty = self.dirty.union(&r.inflate(reach)).intersect(&doc.bounds());
        let out = apply_liquify(&self.pre, &self.field);
        let id = self.layer_id;
        if let Some(l) = doc.layers.iter_mut().find(|l| l.id == id) {
            l.surface = out;
        }
        self.dirty
    }

    pub fn end(self) -> Rect {
        Rect::EMPTY
    }

    pub fn cancel(self, doc: &mut Document) -> Rect {
        let id = self.layer_id;
        if let Some(l) = doc.layers.iter_mut().find(|l| l.id == id) {
            l.surface = self.pre;
        }
        doc.drop_checkpoint();
        doc.bounds()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn forward_warp_pushes_paint() {
        let mut doc = Document::new(120, 60);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 50, 60), &[0.0, 0.0, 0.0, 1.0]);
        let mut s = LiquifySession::begin(&mut doc, LiquifyTool::ForwardWarp, 40.0, 1.0).unwrap();
        for i in 0..30 {
            s.add(&mut doc, 40.0 + i as f64, 30.0, 1.0);
        }
        s.end();
        assert!(doc.layers[0].surface.rgba(56, 30)[3] > 0.5);
        assert!(doc.undo());
        assert_eq!(doc.layers[0].surface.rgba(56, 30)[3], 0.0);
    }
}
