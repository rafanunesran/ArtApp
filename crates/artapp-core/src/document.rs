//! Documento: tamanho da tela, cor do papel e a pilha de camadas.

use photocraft_color::{BlendMode, PixelFormat};
use photocraft_geom::Rect;
use photocraft_raster::Surface;

use crate::history::History;

/// Maior lado aceito para uma tela nova (texturas de GPU de celular costumam parar em 4096 ou 8192).
pub const MAX_CANVAS_SIDE: u32 = 8192;

/// Camada raster. A superfície é RGBA de 8 bits, alfa não pré-multiplicado.
#[derive(Clone)]
pub struct Layer {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    /// 0..1.
    pub opacity: f32,
    pub blend: BlendMode,
    /// Pinta só onde já há tinta (o "bloquear pixels transparentes").
    pub lock_alpha: bool,
    /// "Recortar na camada abaixo": só aparece onde a camada base (a primeira sem recorte
    /// embaixo) tem tinta.
    pub clip: bool,
    /// Camada de referência: o balde olha só as camadas marcadas assim (ex.: o lineart).
    pub reference: bool,
    /// Camada de rascunho: aparece na tela, mas fica fora da exportação e do balde.
    pub draft: bool,
    /// Máscara da camada logo abaixo: o cinza dela esconde (preto) ou mostra (branco, ou sem
    /// tinta) a camada dona. Fica sempre colada acima da dona.
    pub mask: bool,
    /// Pasta (grupo): não tem tinta; as camadas de dentro ficam logo abaixo dela na pilha.
    pub folder: bool,
    /// Pasta fechada no painel (só a aparência da lista).
    pub collapsed: bool,
    /// A pasta em que a camada está (id), se estiver numa.
    pub parent: Option<u64>,
    /// Borda em volta da tinta (propriedade de camada do Clip Studio).
    pub border: Option<crate::border::Border>,
    /// Camadas vetoriais: a arte editável. `surface` guarda a cópia rasterizada dela.
    pub vector: Option<crate::vector::VectorArt>,
    pub surface: Surface,
}

impl Layer {
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            visible: true,
            opacity: 1.0,
            blend: BlendMode::Normal,
            lock_alpha: false,
            clip: false,
            reference: false,
            draft: false,
            mask: false,
            folder: false,
            collapsed: false,
            parent: None,
            border: None,
            vector: None,
            surface: Surface::new(PixelFormat::RGBA8),
        }
    }
}

pub struct Document {
    pub width: u32,
    pub height: u32,
    /// Cor do papel embaixo de todas as camadas (RGBA sRGB 0..1, alfa 0 = transparente).
    pub paper: [f32; 4],
    pub layers: Vec<Layer>,
    /// Índice da camada ativa em `layers` (0 = a de baixo).
    pub active: usize,
    /// Área selecionada (máscara de 8 bits, 1 = selecionado). `None` = nada selecionado, tudo
    /// pode ser pintado.
    pub selection: Option<crate::selection::Selection>,
    pub(crate) history: History,
    next_id: u64,
}

impl Document {
    /// Tela nova com uma camada vazia. O tamanho é limitado a 1..=[`MAX_CANVAS_SIDE`].
    pub fn new(width: u32, height: u32) -> Self {
        let mut doc = Self {
            width: width.clamp(1, MAX_CANVAS_SIDE),
            height: height.clamp(1, MAX_CANVAS_SIDE),
            paper: [1.0, 1.0, 1.0, 1.0],
            layers: Vec::new(),
            active: 0,
            selection: None,
            history: History::default(),
            next_id: 1,
        };
        let layer = doc.make_layer();
        doc.layers.push(layer);
        doc
    }

    /// Documento montado a partir de camadas prontas (abrir um projeto ou um PSD). Sem camadas,
    /// ganha uma vazia; o histórico começa limpo.
    pub fn from_parts(
        width: u32,
        height: u32,
        paper: [f32; 4],
        layers: Vec<Layer>,
        active: usize,
    ) -> Self {
        let mut layers = layers;
        // Máscara só vale colada numa camada que não é máscara.
        for i in 0..layers.len() {
            if layers[i].mask && (i == 0 || layers[i - 1].mask) {
                layers[i].mask = false;
            }
        }
        crate::folders::sanitize(&mut layers);
        let next_id = layers.iter().map(|l| l.id).max().unwrap_or(0) + 1;
        let mut doc = Self {
            width: width.clamp(1, MAX_CANVAS_SIDE),
            height: height.clamp(1, MAX_CANVAS_SIDE),
            paper,
            layers,
            active: 0,
            selection: None,
            history: History::default(),
            next_id,
        };
        if doc.layers.is_empty() {
            let layer = doc.make_layer();
            doc.layers.push(layer);
        }
        doc.active = active.min(doc.layers.len() - 1);
        doc
    }

    /// Cópia das camadas sem o histórico (para salvar em segundo plano).
    pub fn snapshot(&self) -> Self {
        Self::from_parts(
            self.width,
            self.height,
            self.paper,
            self.layers.clone(),
            self.active,
        )
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.width as i32, self.height as i32)
    }

    pub fn active_layer(&self) -> Option<&Layer> {
        self.layers.get(self.active)
    }

    pub fn active_layer_mut(&mut self) -> Option<&mut Layer> {
        self.layers.get_mut(self.active)
    }

    pub(crate) fn make_layer(&mut self) -> Layer {
        let id = self.next_id;
        self.next_id += 1;
        Layer::new(id, format!("Camada {id}"))
    }

    /// Nova camada acima da ativa, que passa a ser a ativa.
    pub fn add_layer(&mut self) {
        self.checkpoint();
        let mut layer = self.make_layer();
        // Acima da camada ativa e da máscara dela (nunca entre as duas), na mesma pasta. Com uma
        // pasta ativa, entra nela, em cima das outras.
        let (at, parent) = match self.layers.get(self.active) {
            Some(l) if l.folder => (self.active, Some(l.id)),
            Some(l) => (self.unit(self.active).1 + 1, l.parent),
            None => (self.layers.len(), None),
        };
        layer.parent = parent;
        let at = at.min(self.layers.len());
        self.layers.insert(at, layer);
        self.active = at;
    }

    /// Duplica a camada ativa (a cópia fica acima e passa a ser a ativa).
    pub fn duplicate_layer(&mut self) {
        let Some(src) = self.layers.get(self.active).cloned() else {
            return;
        };
        if src.mask || src.folder {
            return;
        }
        let (_, end) = self.unit(self.active);
        let mask = (end > self.active).then(|| self.layers[end].clone());
        self.checkpoint();
        let mut copy = self.make_layer();
        copy.name = format!("{} cópia", src.name);
        copy.surface = src.surface;
        copy.opacity = src.opacity;
        copy.blend = src.blend;
        copy.clip = src.clip;
        copy.draft = src.draft;
        copy.vector = src.vector;
        copy.parent = src.parent;
        let at = end + 1;
        self.layers.insert(at, copy);
        if let Some(mut m) = mask {
            m.id = self.next_id;
            self.next_id += 1;
            self.layers.insert(at + 1, m);
        }
        self.active = at;
    }

    /// Apaga a camada ativa. A última camada nunca é apagada (só limpa).
    pub fn delete_layer(&mut self) {
        if self.active >= self.layers.len() {
            return;
        }
        self.checkpoint();
        if self.layers.len() == 1 {
            if let Some(l) = self.layers.first_mut() {
                l.surface = Surface::new(PixelFormat::RGBA8);
            }
            return;
        }
        // Apagar a dona leva a máscara junto.
        let (start, end) = if self.layers[self.active].mask {
            (self.active, self.active)
        } else {
            self.block(self.active)
        };
        if end - start + 1 >= self.layers.len() {
            self.layers.truncate(1);
            self.layers[0].surface = Surface::new(PixelFormat::RGBA8);
            self.layers[0].mask = false;
            self.layers[0].folder = false;
            self.layers[0].parent = None;
            self.active = 0;
            return;
        }
        self.layers.drain(start..=end);
        self.active = start.saturating_sub(1).min(self.layers.len() - 1);
    }

    /// Move a camada ativa (com a máscara dela, ou a pasta com tudo dentro) para cima (`up`) ou
    /// para baixo, passando por cima da vizinha inteira. Na beira da pasta, a camada sai dela.
    pub fn move_layer(&mut self, up: bool) {
        if self.layers.get(self.active).is_none_or(|l| l.mask) {
            return;
        }
        let (start, end) = self.block(self.active);
        let len = end - start + 1;
        // A pasta de quem está se movendo (a da própria pasta, se é uma pasta que se move).
        let head = if self.layers[end].folder { end } else { start };
        let parent = self.layers[head].parent;
        let offset = self.active - start;
        if up {
            let above = end + 1;
            let Some(next) = self.layers.get(above) else {
                return;
            };
            let exits = parent.is_some() && next.folder && Some(next.id) == parent;
            self.checkpoint();
            if exits {
                // Topo da pasta: sai por cima dela.
                for l in &mut self.layers[start..=end] {
                    l.parent = None;
                }
                self.layers[start..=above].rotate_left(len);
                self.active = start + 1 + offset;
                return;
            }
            let (_, other_end) = if parent.is_none() {
                self.top_block(above)
            } else {
                self.block(above)
            };
            self.layers[start..=other_end].rotate_left(len);
            self.active = other_end + 1 - len + offset;
        } else {
            if start == 0 {
                return;
            }
            let below = start - 1;
            self.checkpoint();
            if parent.is_some() && self.layers[below].parent != parent {
                // Fundo da pasta: sai por baixo (fica onde está, mas fora dela).
                for l in &mut self.layers[start..=end] {
                    l.parent = None;
                }
                return;
            }
            let (other_start, _) = if parent.is_none() {
                self.top_block(below)
            } else {
                self.block(below)
            };
            self.layers[other_start..=end].rotate_right(len);
            self.active = other_start + offset;
        }
    }

    /// Mescla a camada ativa na de baixo (respeitando opacidade e modo de mesclagem).
    pub fn merge_down(&mut self) {
        if self.active == 0 || self.active >= self.layers.len() {
            return;
        }
        if self.layers[self.active].mask {
            self.apply_mask();
            return;
        }
        if self.layers[self.active].folder {
            self.flatten_folder();
            return;
        }
        // Mescla na camada (dona) de baixo, nunca na máscara dela, nem fora da pasta.
        let below = self.owner(self.active - 1);
        let b = &self.layers[below];
        if b.mask || b.folder || b.parent != self.layers[self.active].parent {
            return;
        }
        self.checkpoint();
        let mut top = self.layers[self.active].clone();
        top.surface = self.masked_surface(self.active);
        let (start, end) = self.unit(self.active);
        self.layers.drain(start..=end);
        // A dona de baixo tem máscara: ela entra na tinta antes de mesclar.
        if self.layers.get(below + 1).is_some_and(|l| l.mask) {
            let baked = self.masked_surface(below);
            self.layers[below].surface = baked;
            // A borda foi junto para a tinta.
            self.layers[below].border = None;
            self.layers.remove(below + 1);
        }
        if let Some(dst) = self.layers.get_mut(below) {
            // Mesclar numa camada vetorial a transforma em raster.
            dst.vector = None;
            crate::composite::merge_into(&mut dst.surface, &top, self.width, self.height);
        }
        self.active = below;
    }

    /// Limpa a camada ativa.
    pub fn clear_layer(&mut self) {
        if self.active >= self.layers.len() {
            return;
        }
        self.checkpoint();
        if let Some(l) = self.layers.get_mut(self.active) {
            l.surface = Surface::new(PixelFormat::RGBA8);
        }
    }

    /// Guarda o estado atual no histórico antes de uma alteração.
    pub fn checkpoint(&mut self) {
        self.history
            .push((self.layers.clone(), self.active, (self.width, self.height)));
    }

    /// Desfaz o último passo. Devolve `true` se algo mudou.
    pub fn undo(&mut self) -> bool {
        let current = (self.layers.clone(), self.active, (self.width, self.height));
        let Some((layers, active, (w, h))) = self.history.undo(current) else {
            return false;
        };
        self.layers = layers;
        self.active = active;
        if (w, h) != (self.width, self.height) {
            self.width = w;
            self.height = h;
            self.selection = None;
        }
        true
    }

    /// Refaz o último passo desfeito. Devolve `true` se algo mudou.
    pub fn redo(&mut self) -> bool {
        let current = (self.layers.clone(), self.active, (self.width, self.height));
        let Some((layers, active, (w, h))) = self.history.redo(current) else {
            return false;
        };
        self.layers = layers;
        self.active = active;
        if (w, h) != (self.width, self.height) {
            self.width = w;
            self.height = h;
            self.selection = None;
        }
        true
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Descarta o último ponto do histórico sem aplicá-lo (um traço cancelado).
    pub fn drop_checkpoint(&mut self) {
        self.history.drop_last();
    }

    /// Muda a cada passo guardado, desfeito ou refeito: diz se o desenho mudou desde que foi lido.
    pub fn history_version(&self) -> u64 {
        self.history.version()
    }

    /// Linha do tempo para o painel de histórico: o nome de cada passo (o primeiro é "Início") e
    /// a posição do estado atual nela.
    pub fn history_steps(&self) -> (Vec<String>, usize) {
        let (undo, redo) = self.history.states();
        let current = (self.layers.clone(), self.active, (self.width, self.height));
        let states: Vec<&crate::history::Snapshot> = undo
            .iter()
            .chain(std::iter::once(&current))
            .chain(redo.iter().rev())
            .collect();
        let mut names = vec!["Início".to_string()];
        names.extend(
            states
                .windows(2)
                .map(|w| crate::history::describe(w[0], w[1])),
        );
        (names, undo.len())
    }

    /// Desfaz ou refaz até chegar ao passo `step` da linha do tempo. Devolve se algo mudou.
    pub fn jump_history(&mut self, step: usize) -> bool {
        let (_, now) = self.history_steps();
        let mut changed = false;
        for _ in step..now {
            changed |= self.undo();
        }
        for _ in now..step {
            changed |= self.redo();
        }
        changed
    }
}
