//! Desfazer/refazer por instantâneos da pilha de camadas. Como as superfícies são copy-on-write,
//! um instantâneo custa só os ponteiros dos tiles; os tiles que um traço altera são os únicos
//! copiados de verdade.

use crate::document::Layer;

/// Quantos passos de desfazer guardar.
pub const MAX_UNDO: usize = 100;

/// Camadas, camada ativa e tamanho da tela (cortar e redimensionar também se desfazem).
pub type Snapshot = (Vec<Layer>, usize, (u32, u32));

#[derive(Default)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Muda a cada passo guardado, desfeito ou refeito.
    version: u64,
}

impl History {
    pub fn push(&mut self, snap: Snapshot) {
        self.version += 1;
        self.undo.push(snap);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Troca o estado atual pelo último instantâneo, guardando o atual para refazer.
    pub fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let prev = self.undo.pop()?;
        self.version += 1;
        self.redo.push(current);
        Some(prev)
    }

    pub fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let next = self.redo.pop()?;
        self.version += 1;
        self.undo.push(current);
        Some(next)
    }

    pub fn drop_last(&mut self) {
        self.version += 1;
        self.undo.pop();
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Os estados guardados: os de desfazer (do mais antigo ao mais novo) e os de refazer (do
    /// mais distante ao mais próximo do estado atual).
    pub fn states(&self) -> (&[Snapshot], &[Snapshot]) {
        (&self.undo, &self.redo)
    }
}

/// Nome curto do passo que levou de `a` para `b`, para o painel de histórico.
pub fn describe(a: &Snapshot, b: &Snapshot) -> String {
    let (la, lb) = (&a.0, &b.0);
    let name = |l: &Layer| l.name.clone();
    if a.2 != b.2 {
        return format!("Tamanho da tela {}×{}", b.2.0, b.2.1);
    }
    if let Some(l) = lb.iter().find(|l| la.iter().all(|o| o.id != l.id)) {
        return if l.folder {
            format!("Nova pasta: {}", name(l))
        } else if l.mask {
            "Nova máscara".into()
        } else {
            format!("Nova camada: {}", name(l))
        };
    }
    if let Some(l) = la.iter().find(|l| lb.iter().all(|o| o.id != l.id)) {
        return format!("Camada removida: {}", name(l));
    }
    if la.iter().zip(lb).any(|(x, y)| x.id != y.id) {
        return "Ordem das camadas".into();
    }
    fn props(l: &Layer) -> impl PartialEq + '_ {
        (
            &l.name,
            l.visible,
            l.opacity,
            l.blend,
            (l.lock_alpha, l.clip, l.reference, l.draft),
            (l.collapsed, l.parent),
        )
    }
    if let Some(l) = la
        .iter()
        .zip(lb)
        .find(|(x, y)| props(x) != props(y))
        .map(|(_, y)| y)
    {
        return format!("Propriedades: {}", name(l));
    }
    // O resto mexe na tinta; quase sempre na camada ativa de antes.
    la.get(a.1)
        .map_or_else(|| "Edição".into(), |l| format!("Pintura: {}", name(l)))
}

#[cfg(test)]
mod tests {
    use crate::document::Document;

    #[test]
    fn timeline_names_steps_and_jumps() {
        let mut doc = Document::new(16, 16);
        doc.add_layer();
        doc.checkpoint();
        doc.layers[1].opacity = 0.5;
        let (names, now) = doc.history_steps();
        assert_eq!(names.len(), 3);
        assert!(names[1].starts_with("Nova camada"));
        assert!(names[2].starts_with("Propriedades"));
        assert_eq!(now, 2);
        assert!(doc.jump_history(0));
        assert_eq!(doc.layers.len(), 1);
        assert_eq!(doc.history_steps().1, 0);
        assert!(doc.jump_history(2));
        assert_eq!(doc.layers[1].opacity, 0.5);
    }
}
