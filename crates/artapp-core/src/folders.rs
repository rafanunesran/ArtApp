//! Pastas de camada (grupos): uma camada-pasta sem tinta, com as camadas de dentro logo abaixo
//! dela na pilha (`Layer::parent` = id da pasta). Ocultar a pasta esconde tudo dentro; a
//! opacidade dela multiplica a das camadas de dentro. Um nível só (pasta dentro de pasta não).

use photocraft_color::PixelFormat;
use photocraft_raster::Surface;

use crate::document::{Document, Layer};

/// Desfaz pastas inválidas (de um arquivo estranho): a camada de dentro precisa estar logo
/// abaixo da pasta dela, sem nada de fora no meio, e pasta não entra em pasta.
pub(crate) fn sanitize(layers: &mut [Layer]) {
    for i in 0..layers.len() {
        if layers[i].folder {
            layers[i].parent = None;
            layers[i].mask = false;
            layers[i].clip = false;
            layers[i].reference = false;
            layers[i].draft = false;
        }
        let Some(p) = layers[i].parent else {
            continue;
        };
        let ok = (i + 1..layers.len())
            .find(|&k| layers[k].parent != Some(p))
            .is_some_and(|k| layers[k].folder && layers[k].id == p);
        if !ok {
            layers[i].parent = None;
        }
    }
}

impl Document {
    /// Posições (primeira, última) do bloco de `i`: a pasta com tudo dentro, ou a camada com a
    /// máscara dela.
    pub fn block(&self, i: usize) -> (usize, usize) {
        let Some(l) = self.layers.get(i) else {
            return (i, i);
        };
        if !l.folder {
            return self.unit(i);
        }
        let mut start = i;
        while start > 0 && self.layers[start - 1].parent == Some(l.id) {
            start -= 1;
        }
        (start, i)
    }

    /// O bloco de primeiro nível que contém `i` (a pasta inteira, se `i` está numa).
    pub fn top_block(&self, i: usize) -> (usize, usize) {
        match self.layers.get(i).and_then(|l| l.parent) {
            Some(p) => match self.layers.iter().position(|l| l.id == p) {
                Some(f) => self.block(f),
                None => self.block(i),
            },
            None => self.block(i),
        }
    }

    /// Quanto a pasta deixa a camada `i` aparecer: `None` se a pasta está oculta, senão a
    /// opacidade dela (1 fora de pasta).
    pub fn folder_mix(&self, i: usize) -> Option<f32> {
        let Some(p) = self.layers.get(i).and_then(|l| l.parent) else {
            return Some(1.0);
        };
        match self.layers.iter().find(|l| l.id == p) {
            Some(f) if !f.visible => None,
            Some(f) => Some(f.opacity),
            None => Some(1.0),
        }
    }

    /// Põe a camada ativa (com a máscara) numa pasta nova. Não cria pasta dentro de pasta.
    pub fn new_folder(&mut self) -> bool {
        let Some(l) = self.layers.get(self.active) else {
            return false;
        };
        if l.folder || l.parent.is_some() {
            return false;
        }
        let (start, end) = self.block(self.active);
        self.checkpoint();
        let mut f = self.make_layer();
        f.name = f.name.replace("Camada", "Pasta");
        f.folder = true;
        for l in &mut self.layers[start..=end] {
            l.parent = Some(f.id);
        }
        self.layers.insert(end + 1, f);
        self.active = end + 1;
        true
    }

    /// Tira tudo da pasta ativa e apaga a pasta.
    pub fn ungroup(&mut self) -> bool {
        if !self.layers.get(self.active).is_some_and(|l| l.folder) {
            return false;
        }
        let (start, end) = self.block(self.active);
        self.checkpoint();
        for l in &mut self.layers[start..end] {
            l.parent = None;
        }
        self.layers.remove(end);
        self.active = end.saturating_sub(1).min(self.layers.len() - 1);
        true
    }

    /// Achata a pasta ativa numa camada só (com as máscaras aplicadas).
    pub fn flatten_folder(&mut self) -> bool {
        if !self.layers.get(self.active).is_some_and(|l| l.folder) {
            return false;
        }
        let (start, end) = self.block(self.active);
        self.checkpoint();
        let mut surface = Surface::new(PixelFormat::RGBA8);
        for k in start..end {
            let l = &self.layers[k];
            if l.mask || !l.visible {
                continue;
            }
            let mut top = l.clone();
            top.surface = self.masked_surface(k);
            crate::composite::merge_into(&mut surface, &top, self.width, self.height);
        }
        surface.prune();
        let folder = &self.layers[end];
        let mut flat = Layer::new(folder.id, folder.name.clone());
        flat.visible = folder.visible;
        flat.opacity = folder.opacity;
        flat.surface = surface;
        self.layers.splice(start..=end, [flat]);
        self.active = start;
        true
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::composite::composite_rgba8;
    use photocraft_geom::Rect;

    fn names(d: &Document) -> Vec<(String, Option<u64>)> {
        d.layers
            .iter()
            .map(|l| (l.name.clone(), l.parent))
            .collect()
    }

    #[test]
    fn folder_groups_moves_and_hides() {
        let mut d = Document::new(10, 10);
        d.paper = [0.0; 4];
        d.layers[0].name = "A".into();
        d.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 10, 10), &[1.0, 0.0, 0.0, 1.0]);
        d.add_layer();
        d.layers[1].name = "B".into();
        // B numa pasta; nova camada dentro da pasta.
        assert!(d.new_folder());
        let fid = d.layers[2].id;
        assert!(d.layers[2].folder && d.layers[1].parent == Some(fid));
        d.add_layer();
        assert_eq!(d.active, 2);
        assert_eq!(d.layers[2].parent, Some(fid));
        d.layers[2].name = "C".into();
        // C sobe: sai da pasta por cima.
        d.move_layer(true);
        assert_eq!(names(&d)[3], ("C".into(), None));
        assert!(d.layers[2].folder);
        // A pasta desce inteira, passando por A.
        d.active = 2;
        d.move_layer(false);
        assert!(d.layers[1].folder && d.layers[0].parent == Some(fid));
        assert_eq!(d.layers[2].name, "A");
        // Ocultar a pasta some com o que está dentro.
        d.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 10, 10), &[0.0, 0.0, 1.0, 1.0]);
        d.layers[1].visible = false;
        let (_, px) = composite_rgba8(&d, d.bounds(), false);
        assert_eq!(px[0], [255, 0, 0, 255]);
        d.layers[1].visible = true;
        let (_, px) = composite_rgba8(&d, d.bounds(), false);
        assert_eq!(px[0], [255, 0, 0, 255], "A está por cima da pasta");
        // Achatar vira uma camada com a tinta de dentro.
        d.active = 1;
        assert!(d.flatten_folder());
        assert!(!d.layers[0].folder && d.layers[0].surface.rgba(1, 1)[2] > 0.9);
        assert!(d.undo());
        assert!(d.layers[1].folder);
        assert!(d.ungroup());
        assert!(d.layers.iter().all(|l| !l.folder && l.parent.is_none()));
    }

    #[test]
    fn deleting_a_folder_takes_its_layers() {
        let mut d = Document::new(8, 8);
        d.add_layer();
        d.new_folder();
        d.add_layer();
        assert_eq!(d.layers.len(), 4);
        d.active = 3;
        d.delete_layer();
        assert_eq!(d.layers.len(), 1);
    }

    #[test]
    fn sanitize_drops_broken_parents() {
        let mut d = Document::new(8, 8);
        d.add_layer();
        d.layers[0].parent = Some(999);
        let d = Document::from_parts(8, 8, d.paper, d.layers.clone(), 0);
        assert!(d.layers.iter().all(|l| l.parent.is_none()));
    }
}
