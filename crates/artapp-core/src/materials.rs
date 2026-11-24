//! Materiais (a biblioteca de materiais do Clip Studio): camadas guardadas para reusar em
//! outros desenhos — um objeto desenhado, um fundo, uma retícula, arte vetorial. Cada material
//! é um projeto `.artapp` de uma camada só (com extensão própria), então guarda tudo que a
//! camada tem: tinta, vetor, borda, retícula, modo de mesclagem.

use photocraft_geom::Rect;

use crate::document::Document;
use crate::project;

/// Extensão dos arquivos de material.
pub const EXT: &str = "artmat";

/// O material feito da camada ativa (vazia, pasta ou máscara não viram material).
pub fn from_active(doc: &Document) -> Option<Document> {
    let layer = doc.active_layer()?;
    if layer.folder || layer.mask {
        return None;
    }
    let empty = layer.surface.content_bounds().is_empty();
    if empty && layer.vector.is_none() {
        return None;
    }
    let mut m = doc.snapshot();
    let mut l = layer.clone();
    l.parent = None;
    l.clip = false;
    if l.vector.is_none() {
        // Tinta: o material fica do tamanho dela (a miniatura mostra só o desenho).
        let c = l.surface.content_bounds();
        l.surface = l.surface.translated(-c.x0, -c.y0, c);
        m.width = c.width().max(1);
        m.height = c.height().max(1);
    }
    m.layers = vec![l];
    m.active = 0;
    Some(m)
}

/// Grava o material (os bytes do arquivo).
pub fn encode(m: &Document) -> Result<Vec<u8>, String> {
    project::save(m)
}

/// Lê um material.
pub fn decode(bytes: &[u8]) -> Result<Document, String> {
    let m = project::load(bytes)?;
    if m.layers.is_empty() {
        return Err("material vazio".into());
    }
    Ok(m)
}

impl Document {
    /// Põe o material numa camada nova acima da ativa. A tinta fica centralizada na tela (o
    /// material pode vir de um desenho de outro tamanho); a arte vetorial fica onde estava.
    pub fn place_material(&mut self, m: &Document, name: &str) -> Rect {
        let Some(src) = m.layers.iter().find(|l| !l.folder && !l.mask) else {
            return Rect::EMPTY;
        };
        self.add_layer();
        let (w, h) = (self.width, self.height);
        let bounds = self.bounds();
        let Some(layer) = self.active_layer_mut() else {
            return Rect::EMPTY;
        };
        let (id, parent) = (layer.id, layer.parent);
        *layer = src.clone();
        layer.id = id;
        layer.parent = parent;
        layer.clip = false;
        layer.name = name.to_string();
        if let Some(art) = layer.vector.as_mut() {
            art.rev = crate::vector::next_rev();
            layer.surface = crate::vector::rasterize(art, w, h);
            return bounds;
        }
        let content = layer.surface.content_bounds();
        if content.is_empty() {
            return Rect::EMPTY;
        }
        let dx = (w as i32 - content.width() as i32) / 2 - content.x0;
        let dy = (h as i32 - content.height() as i32) / 2 - content.y0;
        if (dx, dy) != (0, 0) {
            layer.surface = layer.surface.translated(dx, dy, content);
        }
        content.translate(dx, dy).intersect(&bounds)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn layer_round_trips_into_another_drawing() {
        let mut a = Document::new(200, 100);
        let px = vec![255u8; 20 * 20 * 4];
        let r = Rect::new(10, 10, 30, 30);
        a.layers[a.active].surface = photocraft_raster::Surface::from_interleaved(
            photocraft_color::PixelFormat::RGBA8,
            r,
            &px,
        );
        a.layers[a.active].name = "Estrela".into();
        let m = from_active(&a).unwrap();
        let bytes = encode(&m).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(back.layers.len(), 1);
        assert_eq!((back.width, back.height), (20, 20), "recortado na tinta");

        let mut b = Document::new(400, 400);
        let n = b.layers.len();
        let dirty = b.place_material(&back, "Estrela");
        assert_eq!(b.layers.len(), n + 1);
        let content = b.layers[b.active].surface.content_bounds();
        // 20×20 de tinta, no meio da tela nova.
        assert_eq!((content.width(), content.height()), (20, 20));
        assert_eq!((content.x0, content.y0), (190, 190));
        assert_eq!(dirty, content);
        assert_eq!(b.layers[b.active].name, "Estrela");
        assert!(b.can_undo());

        // Camada vazia não vira material.
        let empty = Document::new(50, 50);
        assert!(from_active(&empty).is_none());
    }
}
