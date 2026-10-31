//! Desfazer/refazer por instantâneos da pilha de camadas. Como as superfícies são copy-on-write,
//! um instantâneo custa só os ponteiros dos tiles; os tiles que um traço altera são os únicos
//! copiados de verdade.

use crate::document::Layer;

/// Quantos passos de desfazer guardar.
pub const MAX_UNDO: usize = 100;

/// Camadas, camada ativa e tamanho da tela (cortar e redimensionar também se desfazem).
type Snapshot = (Vec<Layer>, usize, (u32, u32));

#[derive(Default)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl History {
    pub fn push(&mut self, snap: Snapshot) {
        self.undo.push(snap);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Troca o estado atual pelo último instantâneo, guardando o atual para refazer.
    pub fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let prev = self.undo.pop()?;
        self.redo.push(current);
        Some(prev)
    }

    pub fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }

    pub fn drop_last(&mut self) {
        self.undo.pop();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
