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

    fn make_layer(&mut self) -> Layer {
        let id = self.next_id;
        self.next_id += 1;
        Layer::new(id, format!("Camada {id}"))
    }

    /// Nova camada acima da ativa, que passa a ser a ativa.
    pub fn add_layer(&mut self) {
        self.checkpoint();
        let layer = self.make_layer();
        let at = (self.active + 1).min(self.layers.len());
        self.layers.insert(at, layer);
        self.active = at;
    }

    /// Duplica a camada ativa (a cópia fica acima e passa a ser a ativa).
    pub fn duplicate_layer(&mut self) {
        let Some(src) = self.layers.get(self.active).cloned() else {
            return;
        };
        self.checkpoint();
        let mut copy = self.make_layer();
        copy.name = format!("{} cópia", src.name);
        copy.surface = src.surface;
        copy.opacity = src.opacity;
        copy.blend = src.blend;
        copy.clip = src.clip;
        copy.draft = src.draft;
        copy.vector = src.vector;
        let at = self.active + 1;
        self.layers.insert(at, copy);
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
        self.layers.remove(self.active);
        self.active = self.active.min(self.layers.len().saturating_sub(1));
    }

    /// Move a camada ativa para cima (`up`) ou para baixo na pilha.
    pub fn move_layer(&mut self, up: bool) {
        let to = if up {
            self.active + 1
        } else {
            self.active.wrapping_sub(1)
        };
        if to >= self.layers.len() || self.active >= self.layers.len() {
            return;
        }
        self.checkpoint();
        self.layers.swap(self.active, to);
        self.active = to;
    }

    /// Mescla a camada ativa na de baixo (respeitando opacidade e modo de mesclagem).
    pub fn merge_down(&mut self) {
        if self.active == 0 || self.active >= self.layers.len() {
            return;
        }
        self.checkpoint();
        let top = self.layers.remove(self.active);
        let below = self.active - 1;
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
        self.history.push(self.layers.clone(), self.active);
    }

    /// Desfaz o último passo. Devolve `true` se algo mudou.
    pub fn undo(&mut self) -> bool {
        let Some((layers, active)) = self.history.undo(self.layers.clone(), self.active) else {
            return false;
        };
        self.layers = layers;
        self.active = active;
        true
    }

    /// Refaz o último passo desfeito. Devolve `true` se algo mudou.
    pub fn redo(&mut self) -> bool {
        let Some((layers, active)) = self.history.redo(self.layers.clone(), self.active) else {
            return false;
        };
        self.layers = layers;
        self.active = active;
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
}
