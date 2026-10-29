//! Estado do app e o quadro principal: barra de ferramentas, painéis e a tela de desenho.

use artapp_core::brushes::{self, BrushEntry};
use artapp_core::vector::VectorStyle;
use artapp_core::{Anchor, BrushSettings, Document, NodeId, Rect};
use std::collections::HashMap;

use eframe::egui::{self, Color32, TextureHandle};

use crate::canvas::CanvasView;
use crate::{panels, platform};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Brush,
    Eraser,
    Fill,
    Eyedropper,
    Hand,
    /// Seleciona e move objetos vetoriais.
    Select,
    /// Traço vetorial à mão livre (vira curvas Bézier ajustadas).
    VectorPen,
    /// Caneta Bézier: toque põe um ponto, arrastar puxa as alças.
    Bezier,
    Line,
    Rectangle,
    Ellipse,
}

impl Tool {
    /// Ferramentas raster, sempre visíveis na barra.
    pub const RASTER: [Tool; 5] = [
        Tool::Brush,
        Tool::Eraser,
        Tool::Fill,
        Tool::Eyedropper,
        Tool::Hand,
    ];
    /// Ferramentas vetoriais, no menu "Vetor".
    pub const VECTOR: [Tool; 6] = [
        Tool::Select,
        Tool::VectorPen,
        Tool::Bezier,
        Tool::Line,
        Tool::Rectangle,
        Tool::Ellipse,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Brush => "Pincel",
            Tool::Eraser => "Borracha",
            Tool::Fill => "Balde",
            Tool::Eyedropper => "Conta-gotas",
            Tool::Hand => "Mão",
            Tool::Select => "Selecionar",
            Tool::VectorPen => "Caneta vetorial",
            Tool::Bezier => "Bézier",
            Tool::Line => "Linha",
            Tool::Rectangle => "Retângulo",
            Tool::Ellipse => "Elipse",
        }
    }

    pub fn icon(self) -> char {
        use crate::theme::icon;
        match self {
            Tool::Brush => icon::BRUSH,
            Tool::Eraser => icon::ERASER,
            Tool::Fill => icon::BUCKET,
            Tool::Eyedropper => icon::PIPETTE,
            Tool::Hand => icon::HAND,
            Tool::Select => icon::POINTER,
            Tool::VectorPen => icon::PEN_LINE,
            Tool::Bezier => icon::PEN_TOOL,
            Tool::Line => icon::MINUS,
            Tool::Rectangle => icon::SQUARE,
            Tool::Ellipse => icon::CIRCLE,
        }
    }

    /// Ferramentas que só trabalham em camadas vetoriais.
    pub fn is_vector(self) -> bool {
        Self::VECTOR.contains(&self)
    }

    /// Atalho de teclado (desktop).
    pub fn key(self) -> egui::Key {
        match self {
            Tool::Brush => egui::Key::B,
            Tool::Eraser => egui::Key::E,
            Tool::Fill => egui::Key::G,
            Tool::Eyedropper => egui::Key::I,
            Tool::Hand => egui::Key::H,
            Tool::Select => egui::Key::V,
            Tool::VectorPen => egui::Key::N,
            Tool::Bezier => egui::Key::P,
            Tool::Line => egui::Key::L,
            Tool::Rectangle => egui::Key::M,
            Tool::Ellipse => egui::Key::O,
        }
    }
}

/// Tamanhos de tela oferecidos em "Nova tela".
pub const CANVAS_PRESETS: [(&str, u32, u32); 5] = [
    ("Retrato 1536×2048", 1536, 2048),
    ("Quadrado 2048×2048", 2048, 2048),
    ("Celular 1080×1920", 1080, 1920),
    ("Paisagem 1920×1080", 1920, 1080),
    ("A4 150 dpi", 1240, 1754),
];

pub struct ArtApp {
    pub doc: Document,
    pub view: CanvasView,
    pub tool: Tool,
    /// Ferramenta a voltar depois de usar o conta-gotas.
    pub previous_tool: Tool,
    pub brushes: Vec<BrushEntry>,
    pub brush_index: usize,
    pub brush: BrushSettings,
    pub eraser: BrushSettings,
    pub color: Color32,
    pub recent_colors: Vec<Color32>,
    pub fill_tolerance: f32,
    /// Usa a pressão da caneta/dedo (desligue se o dedo gerar pressão errática).
    pub use_pressure: bool,
    pub show_tools_panel: bool,
    pub show_layers_panel: bool,
    pub show_new_dialog: bool,
    /// Mensagem curta na barra de baixo (ex.: onde a imagem foi salva).
    pub status: String,
    /// Estilo dos objetos vetoriais novos (a cor do traço é `color`).
    pub vector_stroke: bool,
    pub vector_width: f64,
    pub vector_fill: bool,
    pub vector_fill_color: Color32,
    /// Objeto vetorial selecionado (ferramenta Selecionar).
    pub selected: Option<NodeId>,
    /// Pontos do caminho Bézier em construção.
    pub bezier: Vec<Anchor>,
    /// Seletor de cor aberto.
    pub show_color: bool,
    /// Quando `status` mudou (o aviso some depois de alguns segundos).
    pub status_since: f64,
    last_status: String,
    /// Área da tela de desenho (os painéis flutuam por cima dela).
    pub canvas_rect: egui::Rect,
    brush_previews: Vec<TextureHandle>,
    thumbs: HashMap<u64, TextureHandle>,
    thumbs_stale: bool,
    thumbs_at: f64,
    /// Retângulo da tela que precisa ser recomposto no próximo quadro.
    pending: Rect,
}

impl ArtApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::theme::install(&cc.egui_ctx, platform::TOUCH_FIRST);
        let brushes = brushes::all();
        let brush = brushes
            .first()
            .map(|b| b.settings.clone())
            .unwrap_or_default();
        let wide = cc.egui_ctx.content_rect().width() > 900.0;
        Self {
            doc: Document::new(1536, 2048),
            view: CanvasView::default(),
            tool: Tool::Brush,
            previous_tool: Tool::Brush,
            brushes,
            brush_index: 0,
            brush,
            eraser: BrushSettings {
                size: 30.0,
                hardness: 0.8,
                erase: true,
                spacing: 0.05,
                ..Default::default()
            },
            color: Color32::BLACK,
            recent_colors: vec![
                Color32::BLACK,
                Color32::WHITE,
                Color32::from_rgb(220, 50, 50),
                Color32::from_rgb(40, 110, 220),
                Color32::from_rgb(250, 200, 40),
            ],
            fill_tolerance: 0.15,
            use_pressure: true,
            show_tools_panel: wide,
            show_layers_panel: wide,
            show_new_dialog: false,
            status: String::new(),
            vector_stroke: true,
            vector_width: 6.0,
            vector_fill: false,
            vector_fill_color: Color32::from_rgb(250, 200, 40),
            selected: None,
            bezier: Vec::new(),
            show_color: false,
            status_since: 0.0,
            last_status: String::new(),
            canvas_rect: egui::Rect::NOTHING,
            brush_previews: Vec::new(),
            thumbs: HashMap::new(),
            thumbs_stale: true,
            thumbs_at: 0.0,
            pending: Rect::EMPTY,
        }
    }

    /// Marca uma área para recompor.
    pub fn invalidate(&mut self, r: Rect) {
        if !r.is_empty() {
            self.thumbs_stale = true;
        }
        self.pending = self.pending.union(&r);
    }

    pub fn invalidate_all(&mut self) {
        self.thumbs_stale = true;
        self.pending = self.doc.bounds();
    }

    /// Amostras dos pincéis da lista (feitas uma vez, na primeira vez que o painel abre).
    pub fn brush_previews(&mut self, ctx: &egui::Context) -> Vec<TextureHandle> {
        if self.brush_previews.len() != self.brushes.len() {
            let (w, h) = (240u32, 44u32);
            self.brush_previews = self
                .brushes
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    let px = artapp_core::brushes::preview(&b.settings, w, h);
                    let img = egui::ColorImage::from_rgba_unmultiplied(
                        [w as usize, h as usize],
                        px.as_flattened(),
                    );
                    ctx.load_texture(format!("brush-{i}"), img, egui::TextureOptions::LINEAR)
                })
                .collect();
        }
        self.brush_previews.clone()
    }

    /// Miniaturas das camadas, refeitas no máximo duas vezes por segundo enquanto algo muda.
    pub fn layer_thumbnails(&mut self, ctx: &egui::Context) -> HashMap<u64, TextureHandle> {
        let now = ctx.input(|i| i.time);
        let ids: Vec<u64> = self.doc.layers.iter().map(|l| l.id).collect();
        let missing = ids.iter().any(|id| !self.thumbs.contains_key(id));
        if missing || (self.thumbs_stale && now - self.thumbs_at > 0.5) {
            self.thumbs.retain(|id, _| ids.contains(id));
            let (tw, th) = thumb_size(self.doc.width, self.doc.height);
            for layer in &self.doc.layers {
                let px = thumbnail(layer, self.doc.width, self.doc.height, tw, th);
                let img = egui::ColorImage::from_rgba_unmultiplied([tw, th], px.as_flattened());
                match self.thumbs.get_mut(&layer.id) {
                    Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                    None => {
                        let t = ctx.load_texture(
                            format!("thumb-{}", layer.id),
                            img,
                            egui::TextureOptions::LINEAR,
                        );
                        self.thumbs.insert(layer.id, t);
                    }
                }
            }
            self.thumbs_stale = false;
            self.thumbs_at = now;
        } else if self.thumbs_stale {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        self.thumbs.clone()
    }

    pub fn take_pending(&mut self) -> Rect {
        std::mem::replace(&mut self.pending, Rect::EMPTY)
    }

    /// As configurações do traço para a ferramenta atual, com a cor e a pressão aplicadas.
    pub fn stroke_brush(&self) -> BrushSettings {
        let mut b = if self.tool == Tool::Eraser {
            self.eraser.clone()
        } else {
            self.brush.clone()
        };
        if !b.erase {
            b.color = color_to_f32(self.color);
        }
        b
    }

    pub fn select_brush(&mut self, i: usize) {
        if let Some(entry) = self.brushes.get(i) {
            self.brush = entry.settings.clone();
            self.brush_index = i;
            if self.tool != Tool::Brush {
                self.tool = Tool::Brush;
            }
        }
    }

    pub fn set_tool(&mut self, t: Tool) {
        if t == Tool::Eyedropper && self.tool != Tool::Eyedropper {
            self.previous_tool = self.tool;
        }
        if t != Tool::Bezier {
            self.finish_bezier(false);
        }
        if t != Tool::Select {
            self.selected = None;
        }
        self.tool = t;
    }

    /// O estilo vetorial atual.
    pub fn vector_style(&self) -> VectorStyle {
        let rgb = |c: Color32| {
            let [r, g, b, _] = color_to_f32(c);
            [r, g, b]
        };
        VectorStyle {
            stroke: self.vector_stroke.then(|| rgb(self.color)),
            width: self.vector_width,
            fill: self.vector_fill.then(|| rgb(self.vector_fill_color)),
        }
    }

    /// Garante que a camada ativa é vetorial (cria uma acima, como no Clip Studio, se não for).
    pub fn ensure_vector_layer(&mut self) {
        if !self.doc.active_is_vector() {
            self.doc.add_vector_layer();
            self.status = "Nova camada vetorial criada.".into();
        }
    }

    /// Termina o caminho Bézier em construção (`closed` fecha no primeiro ponto).
    pub fn finish_bezier(&mut self, closed: bool) {
        let anchors = std::mem::take(&mut self.bezier);
        if anchors.len() < 2 {
            return;
        }
        let style = self.vector_style();
        let r = self.doc.vector_add(
            &artapp_core::vector::Shape::Bezier(artapp_core::SubPath::new(anchors, closed)),
            &style,
        );
        self.invalidate(r);
        self.remember_color();
    }

    /// Guarda a cor atual entre as recentes.
    pub fn remember_color(&mut self) {
        let c = self.color;
        self.recent_colors.retain(|x| *x != c);
        self.recent_colors.insert(0, c);
        self.recent_colors.truncate(12);
    }

    pub fn undo(&mut self) {
        if self.doc.undo() {
            self.invalidate_all();
        }
    }

    pub fn redo(&mut self) {
        if self.doc.redo() {
            self.invalidate_all();
        }
    }

    pub fn new_canvas(&mut self, w: u32, h: u32) {
        self.doc = Document::new(w, h);
        self.view = CanvasView::default();
        self.invalidate_all();
    }

    pub fn export_png(&mut self, with_paper: bool) {
        self.status = match artapp_core::export::png(&self.doc, with_paper)
            .and_then(|bytes| platform::save_file("png", &bytes))
        {
            Ok(path) => format!("Imagem salva em {}", path.display()),
            Err(e) => format!("Não foi possível salvar: {e}"),
        };
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (undo, redo, tool, bigger, smaller) = ctx.input_mut(|i| {
            let redo = i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::Z,
            ) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y);
            let undo = i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z);
            let tool = Tool::RASTER
                .into_iter()
                .chain(Tool::VECTOR)
                .into_iter()
                .find(|t| i.consume_key(egui::Modifiers::NONE, t.key()));
            let bigger = i.consume_key(egui::Modifiers::NONE, egui::Key::CloseBracket);
            let smaller = i.consume_key(egui::Modifiers::NONE, egui::Key::OpenBracket);
            (undo, redo, tool, bigger, smaller)
        });
        let (enter, escape) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            )
        });
        if enter {
            self.finish_bezier(false);
        }
        if escape {
            self.bezier.clear();
            self.selected = None;
        }
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        if let Some(t) = tool {
            self.set_tool(t);
        }
        let target = if self.tool == Tool::Eraser {
            &mut self.eraser
        } else {
            &mut self.brush
        };
        if bigger {
            target.size = (target.size * 1.2).min(photocraft_max_size());
        }
        if smaller {
            target.size = (target.size / 1.2).max(1.0);
        }
    }
}

/// Tamanho da miniatura (cabe em 80×80 mantendo a proporção da tela).
fn thumb_size(w: u32, h: u32) -> (usize, usize) {
    let s = 80.0 / w.max(h).max(1) as f32;
    (
        ((w as f32 * s) as usize).max(1),
        ((h as f32 * s) as usize).max(1),
    )
}

/// Miniatura de uma camada, amostrando um pixel por ponto da miniatura.
fn thumbnail(layer: &artapp_core::Layer, w: u32, h: u32, tw: usize, th: usize) -> Vec<[u8; 4]> {
    let mut out = Vec::with_capacity(tw * th);
    for y in 0..th {
        for x in 0..tw {
            let sx = ((x as f32 + 0.5) * w as f32 / tw as f32) as i32;
            let sy = ((y as f32 + 0.5) * h as f32 / th as f32) as i32;
            let p = layer.surface.rgba(sx, sy);
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            out.push([q(p[0]), q(p[1]), q(p[2]), q(p[3])]);
        }
    }
    out
}

pub fn photocraft_max_size() -> f32 {
    artapp_core::brushes::MAX_SIZE
}

pub fn color_to_f32(c: Color32) -> [f32; 4] {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    [
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ]
}

pub fn color_from_f32(c: [f32; 4]) -> Color32 {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    Color32::from_rgba_unmultiplied(q(c[0]), q(c[1]), q(c[2]), q(c[3]))
}

impl eframe::App for ArtApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        if self.status != self.last_status {
            self.last_status.clone_from(&self.status);
            self.status_since = ctx.input(|i| i.time);
        }
        let portrait = panels::is_portrait(&ctx);
        if portrait && self.show_tools_panel && self.show_layers_panel {
            // No celular em pé só cabe uma folha de cada vez.
            self.show_tools_panel = false;
        }
        let bar = egui::Frame::new()
            .fill(crate::theme::color::SURFACE)
            .inner_margin(egui::Margin::symmetric(6, 2));
        egui::Panel::top("top_bar")
            .frame(bar)
            .show(ui, |ui| panels::top_bar(self, ui));
        if portrait {
            egui::Panel::bottom("bottom_tools")
                .frame(bar)
                .show(ui, |ui| panels::bottom_tools(self, ui));
        }
        egui::CentralPanel::no_frame().show(ui, |ui| crate::canvas::show(self, ui));

        let canvas = self.canvas_rect;
        if !portrait {
            panels::tool_rail(self, &ctx, canvas);
        }
        panels::side_sliders(self, &ctx, canvas, portrait);
        if self.show_tools_panel {
            panels::brush_panel(self, &ctx, canvas, portrait);
        }
        if self.show_layers_panel {
            panels::layers_panel(self, &ctx, canvas, portrait);
        }
        if self.show_color {
            panels::color_popup(self, &ctx, canvas, portrait);
        }
        panels::toast(self, &ctx, canvas, 0.0);
        if self.show_new_dialog {
            panels::new_canvas_dialog(self, &ctx);
        }
    }
}
