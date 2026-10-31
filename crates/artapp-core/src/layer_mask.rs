//! Máscara de camada: uma camada marcada como máscara, colada logo acima da dona. Pintar nela
//! em preto esconde a dona, em branco (ou apagar) mostra de novo; como é uma camada raster
//! comum, todas as ferramentas funcionam nela. Ocultar a máscara a desliga.

use photocraft_raster::Surface;

use crate::composite::mask_visibility;
use crate::document::Document;

impl Document {
    /// A camada dona de `i` (ela mesma, se não for máscara).
    pub fn owner(&self, i: usize) -> usize {
        if i > 0 && self.layers.get(i).is_some_and(|l| l.mask) {
            i - 1
        } else {
            i
        }
    }

    /// Primeira e última posição do conjunto dona + máscara que contém `i`.
    pub fn unit(&self, i: usize) -> (usize, usize) {
        let o = self.owner(i);
        let end = if self.layers.get(o + 1).is_some_and(|l| l.mask) {
            o + 1
        } else {
            o
        };
        (o, end)
    }

    /// A máscara ligada da camada `i`, se houver.
    pub fn mask_above(&self, i: usize) -> Option<&crate::document::Layer> {
        if self.layers.get(i).is_some_and(|l| l.mask) {
            return None;
        }
        self.layers.get(i + 1).filter(|l| l.mask && l.visible)
    }

    /// A tinta da camada `i` com a máscara dela já aplicada.
    pub fn masked_surface(&self, i: usize) -> Surface {
        let Some(layer) = self.layers.get(i) else {
            return Surface::new(photocraft_color::PixelFormat::RGBA8);
        };
        let Some(m) = self.mask_above(i) else {
            return layer.surface.clone();
        };
        let r = layer.surface.content_bounds().intersect(&self.bounds());
        let mut out = layer.surface.clone();
        if r.is_empty() {
            return out;
        }
        let n = r.width() as usize * r.height() as usize;
        let (mut px, mut mk) = (vec![[0.0f32; 4]; n], vec![[0.0f32; 4]; n]);
        layer.surface.read_rgba_into(r, &mut px);
        m.surface.read_rgba_into(r, &mut mk);
        let data: Vec<f32> = px
            .iter()
            .zip(&mk)
            .flat_map(|(p, k)| [p[0], p[1], p[2], p[3] * mask_visibility(*k)])
            .collect();
        out.write_region(r, &data);
        out.prune();
        out
    }

    /// A camada ativa tem máscara (ou é uma)?
    pub fn active_has_mask(&self) -> bool {
        let (o, end) = self.unit(self.active);
        end > o
    }

    /// Cria uma máscara (vazia: tudo aparece) para a camada ativa e passa a pintar nela. Se já
    /// existe, só seleciona a máscara.
    pub fn add_mask(&mut self) -> bool {
        if self.active >= self.layers.len() {
            return false;
        }
        let (o, end) = self.unit(self.active);
        if end > o {
            self.active = end;
            return false;
        }
        if self.layers[o].folder {
            return false;
        }
        self.checkpoint();
        let mut m = self.make_layer();
        m.name = "Máscara".into();
        m.mask = true;
        m.parent = self.layers[o].parent;
        self.layers.insert(o + 1, m);
        self.active = o + 1;
        true
    }

    /// Aplica a máscara na dona (a tinta escondida some de verdade) e apaga a máscara.
    pub fn apply_mask(&mut self) -> bool {
        let (o, end) = self.unit(self.active);
        if end == o {
            return false;
        }
        self.checkpoint();
        let baked = self.masked_surface(o);
        self.layers[o].surface = baked;
        self.layers[o].vector = None;
        self.layers.remove(end);
        self.active = o;
        true
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::composite::composite_rgba8;
    use photocraft_geom::Rect;

    #[test]
    fn mask_hides_and_moves_with_its_owner() {
        let mut doc = Document::new(20, 10);
        doc.paper = [0.0; 4];
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 20, 10), &[1.0, 0.0, 0.0, 1.0]);
        assert!(doc.add_mask());
        assert_eq!(doc.active, 1);
        // Preto na metade esquerda da máscara.
        doc.layers[1]
            .surface
            .fill_rect(Rect::new(0, 0, 10, 10), &[0.0, 0.0, 0.0, 1.0]);
        let (_, px) = composite_rgba8(&doc, doc.bounds(), false);
        assert_eq!(px[0][3], 0);
        assert_eq!(px[15][3], 255);

        // Camada nova entra acima da máscara, e mover leva dona + máscara juntas.
        doc.active = 0;
        doc.add_layer();
        assert_eq!(doc.active, 2);
        doc.active = 0;
        doc.move_layer(true);
        assert!(!doc.layers[0].mask && doc.layers[2].mask && doc.active == 1);
        doc.move_layer(false);
        assert!(doc.layers[1].mask && doc.active == 0);

        // Ocultar a máscara desliga; aplicar corta a tinta.
        doc.layers[1].visible = false;
        assert_eq!(composite_rgba8(&doc, doc.bounds(), false).1[0][3], 255);
        doc.layers[1].visible = true;
        doc.active = 1;
        assert!(doc.apply_mask());
        assert_eq!(doc.layers.len(), 2);
        assert_eq!(doc.layers[0].surface.rgba(2, 2)[3], 0.0);
        assert_eq!(doc.layers[0].surface.rgba(15, 2)[3], 1.0);
    }

    #[test]
    fn deleting_the_owner_takes_the_mask() {
        let mut doc = Document::new(8, 8);
        doc.add_layer();
        doc.add_mask();
        assert_eq!(doc.layers.len(), 3);
        doc.active = 1;
        doc.delete_layer();
        assert_eq!(doc.layers.len(), 1);
        assert!(!doc.layers[0].mask);
    }
}
