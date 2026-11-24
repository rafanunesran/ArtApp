//! Várias camadas de uma vez (seleção múltipla do painel): arrastar e soltar para reordenar e
//! para pôr/tirar de pastas, apagar, mesclar e agrupar numa pasta nova. Cada ação é um passo só
//! do histórico. As camadas andam com o bloco delas (a máscara junto; a pasta com tudo dentro).

use photocraft_color::PixelFormat;
use photocraft_raster::Surface;

use crate::document::{Document, Layer};

/// Onde soltar as camadas arrastadas, relativo a uma camada da lista.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropAt {
    /// Logo acima dela (na mesma pasta dela).
    Above(u64),
    /// Logo abaixo dela (na mesma pasta; numa pasta aberta, entra no topo dela).
    Below(u64),
    /// Dentro da pasta, no topo.
    Into(u64),
}

impl Document {
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }

    /// Os blocos (início, fim) das camadas `ids`, em ordem de pilha, sem repetir e sem bloco que
    /// já está dentro de outro (camada de uma pasta que também foi escolhida).
    fn blocks_of(&self, ids: &[u64]) -> Vec<(usize, usize)> {
        let mut v: Vec<(usize, usize)> = ids
            .iter()
            .filter_map(|id| self.index_of(*id))
            .map(|i| self.block(self.owner(i)))
            .collect();
        v.sort_unstable();
        v.dedup();
        let all = v.clone();
        v.retain(|&(s, e)| {
            !all.iter()
                .any(|&(s2, e2)| (s2, e2) != (s, e) && s2 <= s && e <= e2)
        });
        v
    }

    /// Reordena: tira os blocos `moving` e os põe antes da posição `at` (índice antigo), com a
    /// pasta `parent` (as pastas que se movem ficam sempre fora de pasta). `extra` entra logo
    /// acima deles. Devolve se algo mudou; o passo do histórico é feito aqui.
    fn rearrange(
        &mut self,
        moving: &[(usize, usize)],
        at: usize,
        parent: Option<u64>,
        extra: Option<Layer>,
    ) -> bool {
        let n = self.layers.len();
        let mut is_moving = vec![false; n];
        for &(s, e) in moving {
            is_moving[s..=e].iter_mut().for_each(|m| *m = true);
        }
        let rest: Vec<usize> = (0..n).filter(|&i| !is_moving[i]).collect();
        let moved: Vec<usize> = (0..n).filter(|&i| is_moving[i]).collect();
        let pos = rest.iter().filter(|&&i| i < at).count();
        let mut order: Vec<usize> = Vec::with_capacity(n);
        order.extend_from_slice(&rest[..pos]);
        order.extend_from_slice(&moved);
        order.extend_from_slice(&rest[pos..]);
        // O novo pai de cada camada que se move (as de dentro de uma pasta que se move ficam).
        let new_parent = |l: &Layer, folder_moving: bool| {
            if l.folder {
                None
            } else if folder_moving {
                l.parent
            } else {
                parent
            }
        };
        let mut parents: Vec<Option<u64>> = self.layers.iter().map(|l| l.parent).collect();
        for &(s, e) in moving {
            let folder = self.layers[e].folder;
            for (k, p) in parents.iter_mut().enumerate().take(e + 1).skip(s) {
                *p = new_parent(&self.layers[k], folder && k < e);
            }
        }
        let same = extra.is_none()
            && order.iter().enumerate().all(|(a, &b)| a == b)
            && parents
                .iter()
                .zip(&self.layers)
                .all(|(p, l)| *p == l.parent);
        if same {
            return false;
        }
        let active_id = self.layers.get(self.active).map(|l| l.id);
        self.checkpoint();
        let mut old: Vec<Option<Layer>> = std::mem::take(&mut self.layers)
            .into_iter()
            .zip(parents)
            .map(|(mut l, p)| {
                l.parent = p;
                Some(l)
            })
            .collect();
        let mut layers: Vec<Layer> = Vec::with_capacity(n + 1);
        for (k, &i) in order.iter().enumerate() {
            if let Some(l) = old.get_mut(i).and_then(Option::take) {
                layers.push(l);
            }
            // Logo acima do último que se move.
            if k + 1 == pos + moved.len()
                && let Some(x) = &extra
            {
                layers.push(x.clone());
            }
        }
        if moved.is_empty()
            && let Some(x) = extra
        {
            layers.insert(pos, x);
        }
        crate::folders::sanitize(&mut layers);
        self.layers = layers;
        self.active = active_id
            .and_then(|id| self.index_of(id))
            .unwrap_or(0)
            .min(self.layers.len().saturating_sub(1));
        true
    }

    /// Arrasta as camadas `ids` (com máscaras e pastas inteiras) para `to`. Um passo do histórico.
    /// Pasta não entra em pasta: se o grupo tem pasta, tudo fica fora, ao lado da pasta de destino.
    pub fn move_layers(&mut self, ids: &[u64], to: DropAt) -> bool {
        let moving = self.blocks_of(ids);
        let target_id = match to {
            DropAt::Above(t) | DropAt::Below(t) | DropAt::Into(t) => t,
        };
        let Some(t) = self.index_of(target_id) else {
            return false;
        };
        if moving.is_empty() || moving.iter().any(|&(s, e)| (s..=e).contains(&t)) {
            return false;
        }
        let tl = &self.layers[t];
        let to = match to {
            DropAt::Into(_) if !tl.folder => DropAt::Above(target_id),
            DropAt::Below(_) if tl.folder && !tl.collapsed => DropAt::Into(target_id),
            other => other,
        };
        let (mut at, mut parent) = match to {
            DropAt::Into(_) => (t, Some(target_id)),
            DropAt::Above(_) if tl.folder => (t + 1, None),
            DropAt::Above(_) => (self.unit(t).1 + 1, tl.parent),
            DropAt::Below(_) if tl.folder => (self.block(t).0, None),
            DropAt::Below(_) => (self.unit(t).0, tl.parent),
        };
        let has_folder = moving.iter().any(|&(_, e)| self.layers[e].folder);
        if has_folder && parent.is_some() {
            let (s, e) = self.top_block(t);
            at = if matches!(to, DropAt::Below(_)) {
                s
            } else {
                e + 1
            };
            parent = None;
        }
        self.rearrange(&moving, at, parent, None)
    }

    /// Põe as camadas `ids` (pastas ficam de fora) numa pasta nova, no lugar da mais alta delas.
    pub fn group_layers(&mut self, ids: &[u64]) -> bool {
        let moving: Vec<(usize, usize)> = self
            .blocks_of(ids)
            .into_iter()
            .filter(|&(_, e)| !self.layers[e].folder)
            .collect();
        let Some(&(_, top)) = moving.last() else {
            return false;
        };
        // Pasta não entra em pasta: a nova fica acima da pasta da mais alta.
        let at = if self.layers[top].parent.is_some() {
            self.top_block(top).1 + 1
        } else {
            top + 1
        };
        let mut f = self.make_layer();
        f.name = f.name.replace("Camada", "Pasta");
        f.folder = true;
        let fid = f.id;
        let changed = self.rearrange(&moving, at, Some(fid), Some(f));
        if let Some(i) = self.index_of(fid) {
            self.active = i;
        }
        changed
    }

    /// Apaga as camadas `ids` (pastas com tudo dentro). Sempre sobra uma camada.
    pub fn delete_layers(&mut self, ids: &[u64]) -> bool {
        let blocks = self.blocks_of(ids);
        if blocks.is_empty() {
            return false;
        }
        self.checkpoint();
        let total: usize = blocks.iter().map(|(s, e)| e - s + 1).sum();
        if total >= self.layers.len() {
            self.layers.truncate(1);
            let l = &mut self.layers[0];
            l.surface = Surface::new(PixelFormat::RGBA8);
            l.mask = false;
            l.folder = false;
            l.parent = None;
            l.vector = None;
            self.active = 0;
            return true;
        }
        let lowest = blocks[0].0;
        for &(s, e) in blocks.iter().rev() {
            self.layers.drain(s..=e);
        }
        crate::folders::sanitize(&mut self.layers);
        self.active = lowest.saturating_sub(1).min(self.layers.len() - 1);
        true
    }

    /// Mescla as camadas visíveis de `ids` (sem pastas) na mais baixa delas, de baixo para cima.
    pub fn merge_layers(&mut self, ids: &[u64]) -> bool {
        let mut owners: Vec<usize> = ids
            .iter()
            .filter_map(|id| self.index_of(*id))
            .map(|i| self.owner(i))
            .filter(|&i| {
                let l = &self.layers[i];
                !l.folder && !l.mask && l.visible
            })
            .collect();
        owners.sort_unstable();
        owners.dedup();
        let [bottom, rest @ ..] = owners.as_slice() else {
            return false;
        };
        if rest.is_empty() {
            return false;
        }
        let bottom = *bottom;
        let bottom_id = self.layers[bottom].id;
        self.checkpoint();
        let has_mask = self.layers.get(bottom + 1).is_some_and(|l| l.mask);
        let mut surface = if has_mask {
            self.masked_surface(bottom)
        } else {
            self.layers[bottom].surface.clone()
        };
        for &k in rest {
            let mut top = self.layers[k].clone();
            top.surface = self.masked_surface(k);
            crate::composite::merge_into(&mut surface, &top, self.width, self.height);
        }
        {
            let dst = &mut self.layers[bottom];
            dst.surface = surface;
            dst.vector = None;
            if has_mask {
                dst.border = None;
                dst.tone = None;
            }
        }
        let mut gone: Vec<(usize, usize)> = rest.iter().map(|&k| self.unit(k)).collect();
        if has_mask {
            gone.push((bottom + 1, bottom + 1));
        }
        gone.sort_unstable();
        for &(s, e) in gone.iter().rev() {
            self.layers.drain(s..=e);
        }
        crate::folders::sanitize(&mut self.layers);
        self.active = self.index_of(bottom_id).unwrap_or(0);
        true
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// Documento com camadas A (0), B (1), C (2), D (3) e nada mais.
    fn four() -> Document {
        let mut d = Document::new(8, 8);
        for _ in 0..3 {
            d.add_layer();
        }
        for (l, n) in d.layers.iter_mut().zip(["A", "B", "C", "D"]) {
            l.name = n.into();
        }
        d
    }

    fn names(d: &Document) -> String {
        d.layers
            .iter()
            .map(|l| {
                let p = l
                    .parent
                    .and_then(|p| d.layers.iter().find(|f| f.id == p))
                    .map_or(String::new(), |f| format!("<{}", f.name));
                format!("{}{p}", l.name)
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn id(d: &Document, name: &str) -> u64 {
        d.layers.iter().find(|l| l.name == name).unwrap().id
    }

    #[test]
    fn drag_several_reorders_in_one_step() {
        let mut d = four();
        let v = d.history_version();
        let (a, b, d_) = (id(&d, "A"), id(&d, "B"), id(&d, "D"));
        assert!(d.move_layers(&[a, b], DropAt::Above(d_)));
        assert_eq!(names(&d), "C D A B");
        assert!(d.history_version() != v);
        // Soltar no mesmo lugar não muda nada nem gasta histórico.
        let v = d.history_version();
        assert!(!d.move_layers(&[a], DropAt::Above(d_)));
        assert_eq!(d.history_version(), v);
        assert!(d.undo());
        assert_eq!(names(&d), "A B C D");
    }

    #[test]
    fn drop_into_and_out_of_folders() {
        let mut d = four();
        d.active = 3;
        d.new_folder(); // D vai para a pasta F
        let f = d.layers[4].id;
        d.layers[4].name = "F".into();
        let (a, c) = (id(&d, "A"), id(&d, "C"));
        assert!(d.move_layers(&[a, c], DropAt::Into(f)));
        assert_eq!(names(&d), "B D<F A<F C<F F");
        // Abaixo de uma pasta aberta = topo dela; fechada = abaixo do bloco todo.
        let b = id(&d, "B");
        d.layers[4].collapsed = true;
        assert!(!d.move_layers(&[b], DropAt::Below(f)));
        assert!(d.move_layers(&[c], DropAt::Below(b)));
        assert_eq!(names(&d), "C B D<F A<F F");
        // Pasta arrastada para dentro de outra fica ao lado dela.
        d.active = d.index_of(b).unwrap();
        d.new_folder();
        let g = d.layers[d.active].id;
        assert!(d.move_layers(&[g], DropAt::Into(f)));
        assert!(d.layers.iter().all(|l| !l.folder || l.parent.is_none()));
        assert_eq!(d.layers.last().unwrap().id, g);
    }

    #[test]
    fn group_delete_merge_selected() {
        let mut d = four();
        let (a, c, d_) = (id(&d, "A"), id(&d, "C"), id(&d, "D"));
        assert!(d.group_layers(&[a, c]));
        assert!(d.layers[d.active].folder);
        let f = d.layers[d.active].name.clone();
        assert_eq!(names(&d), format!("B A<{f} C<{f} {f} D"));
        assert!(d.undo());
        d.layers[0].surface.fill_rect(
            photocraft_geom::Rect::new(0, 0, 2, 2),
            &[1.0, 0.0, 0.0, 1.0],
        );
        d.layers[3].surface.fill_rect(
            photocraft_geom::Rect::new(4, 4, 6, 6),
            &[0.0, 0.0, 1.0, 1.0],
        );
        assert!(d.merge_layers(&[a, d_]));
        assert_eq!(names(&d), "A B C");
        assert!(d.layers[0].surface.rgba(5, 5)[2] > 0.9);
        assert!(d.layers[0].surface.rgba(1, 1)[0] > 0.9);
        assert!(d.delete_layers(&[a, c]));
        assert_eq!(names(&d), "B");
        let b = id(&d, "B");
        assert!(d.delete_layers(&[b]));
        assert_eq!(d.layers.len(), 1);
    }
}
