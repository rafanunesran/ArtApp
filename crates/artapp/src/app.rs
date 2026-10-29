//! Estado do app e o quadro principal: barra de ferramentas, painéis e a tela de desenho.

use artapp_core::brushes::{self, BrushEntry};
use artapp_core::vector::VectorStyle;
use artapp_core::{Anchor, BrushSettings, Document, NodeId, Rect};
use eframe::egui::{self, Color32};

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
    /// Retângulo da tela que precisa ser recomposto no próximo quadro.
    pending: Rect,
}

impl ArtApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_style(&cc.egui_ctx);
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
            pending: Rect::EMPTY,
        }
    }

    /// Marca uma área para recompor.
    pub fn invalidate(&mut self, r: Rect) {
        self.pending = self.pending.union(&r);
    }

    pub fn invalidate_all(&mut self) {
        self.pending = self.doc.bounds();
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

/// Controles maiores no Android, para o dedo.
fn setup_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    if platform::TOUCH_FIRST {
        ctx.all_styles_mut(|style| {
            style.spacing.interact_size = egui::vec2(44.0, 40.0);
            style.spacing.button_padding = egui::vec2(12.0, 8.0);
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
            style.spacing.slider_width = 180.0;
            for font in style.text_styles.values_mut() {
                font.size *= 1.15;
            }
        });
    }
}

impl eframe::App for ArtApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        egui::Panel::top("toolbar").show(ui, |ui| panels::toolbar(self, ui));
        egui::Panel::bottom("status").show(ui, |ui| panels::status_bar(self, ui));
        if self.show_tools_panel {
            egui::Panel::left("tools")
                .default_size(260.0)
                .show(ui, |ui| panels::tools_panel(self, ui));
        }
        if self.show_layers_panel {
            egui::Panel::right("layers")
                .default_size(250.0)
                .show(ui, |ui| panels::layers_panel(self, ui));
        }
        egui::CentralPanel::no_frame().show(ui, |ui| crate::canvas::show(self, ui));
        if self.show_new_dialog {
            panels::new_canvas_dialog(self, &ctx);
        }
    }
}
