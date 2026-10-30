//! Estado do app e o quadro principal: barra de ferramentas, painéis e a tela de desenho.

use artapp_core::brushes::{self, BrushEntry};
use artapp_core::vector::VectorStyle;
use artapp_core::{Anchor, BrushSettings, Document, NodeId, Rect};
use std::collections::HashMap;

use eframe::egui::{self, Color32, TextureHandle};

use crate::canvas::CanvasView;
use crate::{panels, platform};

/// O que um dedo faz na tela quando há caneta.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FingerMode {
    /// O dedo desenha como a caneta.
    Draw,
    /// Só a caneta desenha; o dedo move a tela.
    PenOnly,
    /// O dedo desenha até a caneta tocar a tela pela primeira vez; depois, só a caneta.
    #[default]
    Auto,
}

impl FingerMode {
    pub const ALL: [FingerMode; 3] = [FingerMode::Auto, FingerMode::PenOnly, FingerMode::Draw];

    pub fn label(self) -> &'static str {
        match self {
            FingerMode::Draw => "Dedo desenha",
            FingerMode::PenOnly => "Só caneta desenha",
            FingerMode::Auto => "Automático",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Brush,
    Eraser,
    Fill,
    Eyedropper,
    /// Misturar: desfocar ou dedo.
    Blend,
    /// Degradê da cor principal para a secundária (ou transparente).
    Gradient,
    /// Seleção automática (varinha).
    MagicWand,
    Hand,
    /// Move o conteúdo da camada ativa.
    MoveLayer,
    /// Seleção retangular, elíptica e à mão livre (laço).
    SelectRect,
    SelectEllipse,
    Lasso,
    /// Seleciona e move objetos vetoriais.
    Select,
    /// Traço vetorial à mão livre (vira curvas Bézier ajustadas).
    VectorPen,
    /// Caneta Bézier: toque põe um ponto, arrastar puxa as alças.
    Bezier,
    Line,
    Rectangle,
    Ellipse,
    /// Liquefazer: empurrar, torcer, inflar… a camada raster.
    Liquify,
    /// Todas as ferramentas do VectorCraft (a escolhida fica em `ArtApp::kit_tool`).
    VectorKit,
    /// Réguas especiais: cria e ajusta a régua em que o pincel gruda.
    Ruler,
}

impl Tool {
    /// Ferramentas raster, sempre visíveis na barra.
    pub const RASTER: [Tool; 8] = [
        Tool::Brush,
        Tool::Eraser,
        Tool::Blend,
        Tool::Liquify,
        Tool::Fill,
        Tool::Gradient,
        Tool::Eyedropper,
        Tool::Hand,
    ];
    /// Mover camada e seleção, como o grupo "Operação/Seleção" do Clip Studio.
    pub const SELECTION: [Tool; 6] = [
        Tool::MoveLayer,
        Tool::Ruler,
        Tool::SelectRect,
        Tool::SelectEllipse,
        Tool::Lasso,
        Tool::MagicWand,
    ];
    /// Ferramentas vetoriais, no menu "Vetor".
    pub const VECTOR: [Tool; 7] = [
        Tool::VectorKit,
        Tool::Select,
        Tool::VectorPen,
        Tool::Bezier,
        Tool::Line,
        Tool::Rectangle,
        Tool::Ellipse,
    ];

    /// O que o botão lateral da caneta pode virar (ferramentas sem estado pela metade).
    pub const PEN_BUTTON: [Tool; 9] = [
        Tool::Eraser,
        Tool::Eyedropper,
        Tool::Hand,
        Tool::Blend,
        Tool::Brush,
        Tool::Fill,
        Tool::Lasso,
        Tool::SelectRect,
        Tool::MoveLayer,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Brush => "Pincel",
            Tool::Eraser => "Borracha",
            Tool::Fill => "Balde",
            Tool::Eyedropper => "Conta-gotas",
            Tool::Blend => "Misturar",
            Tool::Gradient => "Degradê",
            Tool::MagicWand => "Seleção automática",
            Tool::Hand => "Mão",
            Tool::MoveLayer => "Mover camada",
            Tool::SelectRect => "Seleção retangular",
            Tool::SelectEllipse => "Seleção elíptica",
            Tool::Lasso => "Laço",
            Tool::Select => "Selecionar",
            Tool::VectorPen => "Caneta vetorial",
            Tool::Bezier => "Bézier",
            Tool::Line => "Linha",
            Tool::Rectangle => "Retângulo",
            Tool::Ellipse => "Elipse",
            Tool::VectorKit => "Vetor pro",
            Tool::Liquify => "Liquefazer",
            Tool::Ruler => "Régua",
        }
    }

    pub fn icon(self) -> char {
        use crate::theme::icon;
        match self {
            Tool::Brush => icon::BRUSH,
            Tool::Eraser => icon::ERASER,
            Tool::Fill => icon::BUCKET,
            Tool::Eyedropper => icon::PIPETTE,
            Tool::Blend => icon::DROPLETS,
            Tool::Gradient => icon::GRADIENT,
            Tool::MagicWand => icon::WAND_SPARKLES,
            Tool::Hand => icon::HAND,
            Tool::MoveLayer => icon::MOVE,
            Tool::SelectRect => icon::SQUARE_DASHED,
            Tool::SelectEllipse => icon::CIRCLE_DASHED,
            Tool::Lasso => icon::LASSO,
            Tool::Select => icon::POINTER,
            Tool::VectorPen => icon::PEN_LINE,
            Tool::Bezier => icon::PEN_TOOL,
            Tool::Line => icon::MINUS,
            Tool::Rectangle => icon::SQUARE,
            Tool::Ellipse => icon::CIRCLE,
            Tool::VectorKit => icon::SPLINE_POINTER,
            Tool::Liquify => icon::SPLINE,
            Tool::Ruler => icon::RULER,
        }
    }

    /// Ferramentas que só trabalham em camadas vetoriais.
    pub fn is_vector(self) -> bool {
        Self::VECTOR.contains(&self)
    }

    pub fn is_selection(self) -> bool {
        matches!(
            self,
            Tool::SelectRect | Tool::SelectEllipse | Tool::Lasso | Tool::MagicWand
        )
    }

    /// Todas, na ordem da barra.
    pub fn all() -> impl Iterator<Item = Tool> {
        Self::RASTER
            .into_iter()
            .chain(Self::SELECTION)
            .chain(Self::VECTOR)
    }

    /// Atalho de teclado (desktop).
    pub fn key(self) -> egui::Key {
        match self {
            Tool::Brush => egui::Key::B,
            Tool::Eraser => egui::Key::E,
            Tool::Fill => egui::Key::G,
            Tool::Eyedropper => egui::Key::I,
            Tool::Blend => egui::Key::J,
            Tool::Gradient => egui::Key::D,
            Tool::MagicWand => egui::Key::W,
            Tool::Hand => egui::Key::H,
            Tool::MoveLayer => egui::Key::K,
            Tool::SelectRect => egui::Key::S,
            Tool::SelectEllipse => egui::Key::U,
            Tool::Lasso => egui::Key::A,
            Tool::Select => egui::Key::V,
            Tool::VectorPen => egui::Key::N,
            Tool::Bezier => egui::Key::P,
            Tool::Line => egui::Key::L,
            Tool::Rectangle => egui::Key::M,
            Tool::Ellipse => egui::Key::O,
            Tool::VectorKit => egui::Key::Y,
            Tool::Liquify => egui::Key::Q,
            Tool::Ruler => egui::Key::R,
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
    /// Afinar o começo e o fim dos traços do pincel.
    pub taper: artapp_core::Taper,
    /// Nome para "Salvar como pincel".
    pub new_brush_name: String,
    pub eraser: BrushSettings,
    /// Ferramenta Misturar: modo, ponta e força.
    pub mix_kind: artapp_core::mix::MixKind,
    pub mix_brush: BrushSettings,
    pub mix_strength: f32,
    pub liquify_tool: artapp_core::liquify::LiquifyTool,
    pub liquify_size: f32,
    pub liquify_pressure: f32,
    /// Carimbo: a origem, e se o próximo toque escolhe uma nova.
    pub clone_source: Option<[f64; 2]>,
    pub clone_picking: bool,
    /// Cor principal (a de pintar).
    pub color: Color32,
    /// Cor secundária (troca com a principal pelo X ou tocando nela).
    pub sub_color: Color32,
    /// Cor transparente escolhida: pincel e balde apagam.
    pub transparent: bool,
    pub recent_colors: Vec<Color32>,
    pub fill_tolerance: f32,
    /// Balde: fechar lacunas e expandir área, em pixels.
    pub fill_close_gap: f32,
    pub fill_expand: f32,
    /// Como uma seleção nova se junta à atual.
    pub select_mode: artapp_core::selection::SelectMode,
    /// Quanto expandir/contrair/suavizar a seleção (px).
    pub select_amount: f32,
    /// Área de transferência interna (recortar/copiar/colar).
    pub clipboard: Option<artapp_core::edit::Clip>,
    /// Grade por cima da tela (Visualizar › Grade).
    pub show_grid: bool,
    /// Painel Navegador (miniatura com a vista).
    pub show_navigator: bool,
    /// Estado do painel de cor (aba, HSV, conjuntos, cor intermediária).
    pub color_state: crate::color_panel::ColorState,
    pub grid_size: f32,
    /// Régua especial atual (só uma, como a régua ativa do Clip Studio).
    pub ruler: Option<artapp_core::ruler::Ruler>,
    /// Que régua a ferramenta Régua cria: 0 reta, 1 paralelas, 2 concêntrica, 3 perspectiva.
    pub ruler_kind: usize,
    /// O pincel gruda na régua.
    pub ruler_snap: bool,
    /// O traço em andamento preso à régua.
    pub ruler_live: Option<artapp_core::ruler::Snap>,
    /// Degradê: forma e se termina na secundária (senão, em transparente).
    pub gradient_shape: artapp_core::edit::GradientShape,
    pub gradient_to_sub: bool,
    /// Seleção automática: tolerância, olhar todas as camadas, só a área contígua.
    pub wand_tolerance: f32,
    pub wand_all_layers: bool,
    pub wand_contiguous: bool,
    /// Régua de simetria (`None` = desligada) e o giro dela em graus.
    pub symmetry: Option<artapp_core::paint::symmetry::SymmetryMode>,
    pub symmetry_angle: f64,
    /// Diálogo aberto no momento (transformar, filtros, ajustes).
    pub dialog: Option<crate::dialogs::Dialog>,
    /// O motor do VectorCraft e a ferramenta dele escolhida.
    pub kit: artapp_core::vectorkit::VectorKit,
    pub kit_tool: &'static str,
    /// O que a ferramenta do kit desenha por cima, recalculado a cada quadro.
    pub kit_overlays: Vec<artapp_core::vectorkit::Overlay>,
    /// Usa a pressão da caneta/dedo (desligue se o dedo gerar pressão errática).
    pub use_pressure: bool,
    /// Girar a vista com dois dedos (além de zoom e mover).
    pub gesture_rotate: bool,
    /// Ferramenta usada enquanto o botão lateral da caneta fica apertado (`None` = desligado).
    pub pen_button_tool: Option<Tool>,
    /// Fecha os painéis e o seletor de cor ao começar a desenhar.
    pub autoclose_panels: bool,
    /// Toque rápido com dois dedos desfaz; com três, refaz.
    pub tap_undo: bool,
    /// Se o dedo usa a ferramenta ou só mexe na vista (rejeição de palma).
    pub finger_mode: FingerMode,
    pub show_tools_panel: bool,
    pub show_layers_panel: bool,
    /// Painel do pincel, camadas e cor no quadro anterior, para saber qual acabou de abrir.
    panels_before: [bool; 3],
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
    /// Arquivo do projeto, salvamento automático e diálogos de abrir/salvar.
    pub projects: crate::projects::Projects,
}

impl ArtApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::theme::install(&cc.egui_ctx, platform::TOUCH_FIRST);
        let mut brushes = brushes::all();
        brushes.extend(brushes::load_custom(&brushes_path()));
        let brush = brushes
            .first()
            .map(|b| b.settings.clone())
            .unwrap_or_default();
        // No Android o tamanho ainda não está em pontos ao criar o app (um celular parecia "largo"
        // e abria com os painéis por cima do desenho): lá eles começam fechados.
        let wide = !platform::TOUCH_FIRST && cc.egui_ctx.content_rect().width() > 900.0;
        let mut app = Self {
            doc: Document::new(1536, 2048),
            view: CanvasView::default(),
            tool: Tool::Brush,
            previous_tool: Tool::Brush,
            brushes,
            brush_index: 0,
            brush,
            taper: artapp_core::Taper::default(),
            new_brush_name: String::new(),
            mix_kind: artapp_core::mix::MixKind::Smudge,
            mix_brush: BrushSettings {
                size: 40.0,
                hardness: 0.4,
                spacing: 0.08,
                ..Default::default()
            },
            mix_strength: 0.6,
            liquify_tool: artapp_core::liquify::LiquifyTool::ForwardWarp,
            liquify_size: 120.0,
            liquify_pressure: 0.6,
            clone_source: None,
            clone_picking: false,
            eraser: BrushSettings {
                size: 30.0,
                hardness: 0.8,
                erase: true,
                spacing: 0.05,
                ..Default::default()
            },
            color: Color32::BLACK,
            sub_color: Color32::WHITE,
            transparent: false,
            recent_colors: vec![
                Color32::BLACK,
                Color32::WHITE,
                Color32::from_rgb(220, 50, 50),
                Color32::from_rgb(40, 110, 220),
                Color32::from_rgb(250, 200, 40),
            ],
            fill_tolerance: 0.15,
            fill_close_gap: 0.0,
            fill_expand: 0.0,
            select_mode: artapp_core::selection::SelectMode::New,
            select_amount: 4.0,
            clipboard: None,
            show_grid: false,
            show_navigator: false,
            color_state: Default::default(),
            grid_size: 64.0,
            ruler: None,
            ruler_kind: 0,
            ruler_snap: true,
            ruler_live: None,
            gradient_shape: artapp_core::edit::GradientShape::Linear,
            gradient_to_sub: true,
            wand_tolerance: 0.1,
            wand_all_layers: true,
            wand_contiguous: true,
            symmetry: None,
            symmetry_angle: 0.0,
            dialog: None,
            kit: artapp_core::vectorkit::VectorKit::new(),
            kit_tool: "directSelection",
            kit_overlays: Vec::new(),
            use_pressure: true,
            gesture_rotate: true,
            pen_button_tool: Some(Tool::Eraser),
            autoclose_panels: true,
            tap_undo: true,
            finger_mode: FingerMode::Auto,
            show_tools_panel: wide,
            show_layers_panel: wide,
            panels_before: [false; 3],
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
            projects: crate::projects::Projects::default(),
        };
        app.load_prefs();
        app
    }

    /// Lê os ajustes de caneta e gestos gravados (se houver).
    fn load_prefs(&mut self) {
        let Ok(text) = std::fs::read_to_string(prefs_path()) else {
            return;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            return;
        };
        let flag = |k: &str| v.get(k).and_then(serde_json::Value::as_bool);
        if let Some(b) = flag("use_pressure") {
            self.use_pressure = b;
        }
        if let Some(b) = flag("gesture_rotate") {
            self.gesture_rotate = b;
        }
        if let Some(b) = flag("autoclose_panels") {
            self.autoclose_panels = b;
        }
        if let Some(b) = flag("tap_undo") {
            self.tap_undo = b;
        }
        if let Some(name) = v.get("finger_mode").and_then(serde_json::Value::as_str) {
            self.finger_mode = FingerMode::ALL
                .into_iter()
                .find(|m| format!("{m:?}") == name)
                .unwrap_or_default();
        }
        if let Some(name) = v.get("pen_button_tool").and_then(serde_json::Value::as_str) {
            self.pen_button_tool = Tool::PEN_BUTTON
                .into_iter()
                .find(|t| format!("{t:?}") == name);
        }
    }

    /// Grava os ajustes de caneta e gestos.
    pub fn save_prefs(&mut self) {
        let v = serde_json::json!({
            "use_pressure": self.use_pressure,
            "gesture_rotate": self.gesture_rotate,
            "autoclose_panels": self.autoclose_panels,
            "tap_undo": self.tap_undo,
            "finger_mode": format!("{:?}", self.finger_mode),
            "pen_button_tool": self.pen_button_tool.map(|t| format!("{t:?}")),
        });
        let path = prefs_path();
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, v.to_string()));
        if let Err(e) = result {
            self.status = format!("Não foi possível gravar os ajustes: {e}");
        }
    }

    /// Fecha os painéis flutuantes (ao começar a desenhar).
    pub fn close_panels(&mut self) {
        if self.autoclose_panels {
            self.show_tools_panel = false;
            self.show_layers_panel = false;
            self.show_color = false;
        }
    }

    /// Marca uma área para recompor.
    pub fn invalidate(&mut self, r: Rect) {
        if !r.is_empty() {
            self.thumbs_stale = true;
            self.projects.touch();
        }
        self.pending = self.pending.union(&r);
    }

    pub fn invalidate_all(&mut self) {
        self.thumbs_stale = true;
        self.projects.touch();
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
        if self.transparent && self.tool == Tool::Brush {
            // Cor transparente: o pincel apaga com a própria forma.
            b.erase = true;
        }
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

    /// A cor para o balde e o "preencher seleção" (alfa 0 com a cor transparente).
    pub fn paint_color(&self) -> [f32; 4] {
        if self.transparent {
            [0.0; 4]
        } else {
            color_to_f32(self.color)
        }
    }

    /// A régua de simetria atual, no centro da tela.
    pub fn symmetry_ruler(&self) -> Option<artapp_core::paint::symmetry::PresetSymmetry> {
        let mode = self.symmetry?;
        artapp_core::paint::symmetry::PresetSymmetry::new(
            mode,
            [self.doc.width as f64 / 2.0, self.doc.height as f64 / 2.0],
            self.symmetry_angle,
        )
    }

    /// Troca a cor principal com a secundária.
    pub fn swap_colors(&mut self) {
        std::mem::swap(&mut self.color, &mut self.sub_color);
        self.transparent = false;
    }

    /// Ações do lançador da seleção.
    pub fn deselect(&mut self) {
        self.doc.set_selection(None);
    }

    pub fn select_all(&mut self) {
        let (w, h) = (self.doc.width, self.doc.height);
        self.doc.set_selection(artapp_core::Selection::rect(
            [0.0, 0.0],
            [w as f64, h as f64],
            w,
            h,
        ));
    }

    pub fn invert_selection(&mut self) {
        self.doc.invert_selection();
    }

    pub fn copy(&mut self) {
        match self.doc.copy_pixels() {
            Some(c) => {
                self.clipboard = Some(c);
                self.status = "Copiado.".into();
            }
            None => self.status = "Nada para copiar na camada ativa.".into(),
        }
    }

    pub fn cut(&mut self) {
        match self.doc.cut_pixels() {
            Some((c, dirty)) => {
                self.clipboard = Some(c);
                self.invalidate(dirty);
            }
            None => self.status = "Nada para recortar na camada ativa.".into(),
        }
    }

    /// Cola numa camada nova e já deixa a ferramenta de mover pronta.
    pub fn paste(&mut self) {
        let Some(clip) = self.clipboard.clone() else {
            self.status = "A área de transferência está vazia.".into();
            return;
        };
        let r = self.doc.paste_layer(&clip);
        self.invalidate(r);
        self.set_tool(Tool::MoveLayer);
    }

    pub fn line_to_color(&mut self) {
        let [r, g, b, _] = color_to_f32(self.color);
        let d = self.doc.line_to_color([r, g, b]);
        self.invalidate(d);
    }

    pub fn brightness_to_opacity(&mut self) {
        let [r, g, b, _] = color_to_f32(self.color);
        let d = self.doc.brightness_to_opacity([r, g, b]);
        self.invalidate(d);
    }

    pub fn clear_selection(&mut self) {
        let r = self.doc.clear_selection();
        if r.is_empty() {
            self.status = "Escolha uma camada raster visível para limpar.".into();
        }
        self.invalidate(r);
    }

    pub fn fill_selection(&mut self) {
        let r = self.doc.fill_selection(self.paint_color());
        if r.is_empty() {
            self.status = "Escolha uma camada raster visível para preencher.".into();
        } else {
            self.remember_color();
        }
        self.invalidate(r);
    }

    /// Guarda o pincel atual (com todos os ajustes) em "Meus pincéis".
    pub fn save_brush(&mut self) {
        let name = self.new_brush_name.trim();
        let name = if name.is_empty() {
            format!("Pincel {}", self.brushes.len() + 1)
        } else {
            name.to_string()
        };
        let entry = BrushEntry {
            name: name.clone(),
            group: brushes::CUSTOM_GROUP.into(),
            settings: self.brush.clone(),
        };
        match self
            .brushes
            .iter()
            .position(|b| b.group == brushes::CUSTOM_GROUP && b.name == name)
        {
            Some(i) => {
                if let Some(b) = self.brushes.get_mut(i) {
                    *b = entry;
                }
                self.brush_index = i;
            }
            None => {
                self.brushes.push(entry);
                self.brush_index = self.brushes.len() - 1;
            }
        }
        self.brush_previews.clear();
        self.new_brush_name.clear();
        self.status = match brushes::save_custom(&brushes_path(), &self.brushes) {
            Ok(()) => format!("Pincel \"{name}\" salvo."),
            Err(e) => format!("Pincel criado, mas não foi possível gravar: {e}"),
        };
    }

    /// Apaga um pincel de "Meus pincéis".
    pub fn delete_brush(&mut self, i: usize) {
        if self
            .brushes
            .get(i)
            .is_none_or(|b| b.group != brushes::CUSTOM_GROUP)
        {
            return;
        }
        self.brushes.remove(i);
        self.brush_index = self.brush_index.min(self.brushes.len().saturating_sub(1));
        self.brush_previews.clear();
        if let Err(e) = brushes::save_custom(&brushes_path(), &self.brushes) {
            self.status = format!("Não foi possível gravar: {e}");
        }
    }

    /// Volta o pincel atual para os ajustes do preset.
    pub fn reset_brush(&mut self) {
        if let Some(entry) = self.brushes.get(self.brush_index) {
            self.brush = entry.settings.clone();
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
        if t == Tool::VectorKit && self.kit.tool_id() != self.kit_tool {
            self.kit.select_tool(self.kit_tool);
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
        self.selected = None;
        self.invalidate_all();
        self.forget_project();
    }

    pub fn export_png(&mut self, with_paper: bool) {
        self.status = match artapp_core::export::png(&self.doc, with_paper)
            .and_then(|bytes| platform::save_file("png", &bytes))
        {
            Ok(path) => format!("Imagem salva em {}", path.display()),
            Err(e) => format!("Não foi possível salvar: {e}"),
        };
    }

    /// Escolhe uma ferramenta do VectorCraft.
    pub fn set_kit_tool(&mut self, id: &'static str) {
        self.kit_tool = id;
        self.kit.select_tool(id);
        self.set_tool(Tool::VectorKit);
    }

    /// Manda uma tecla para a ferramenta do kit (Enter conclui, Esc cancela…).
    pub fn kit_key(&mut self, key: artapp_core::vectorkit::ToolKey) {
        let r = self.kit.key(&mut self.doc, key);
        self.invalidate(r);
    }

    /// Roda um comando do VectorCraft na camada vetorial ativa.
    pub fn kit_command(&mut self, id: &str, params: serde_json::Value) {
        self.ensure_vector_layer();
        let r = self.kit.execute(&mut self.doc, id, &params);
        self.invalidate(r);
        if let Some(msg) = self.kit.status.take() {
            self.status = msg;
        }
    }

    /// Com a ferramenta de texto do kit digitando, o teclado vai todo para ela.
    fn kit_typing(&mut self, ctx: &egui::Context) -> bool {
        use artapp_core::vectorkit::ToolKey;
        if self.tool != Tool::VectorKit || ctx.egui_wants_keyboard_input() {
            return false;
        }
        let typing = self.kit.wants_text();
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                egui::Event::Text(t) if typing => {
                    let r = self.kit.text(&mut self.doc, &t);
                    self.invalidate(r);
                }
                egui::Event::Key {
                    key, pressed: true, ..
                } => {
                    let k = match key {
                        egui::Key::Enter => ToolKey::Enter,
                        egui::Key::Escape => ToolKey::Escape,
                        egui::Key::Backspace => ToolKey::Backspace,
                        egui::Key::Delete => ToolKey::Delete,
                        egui::Key::ArrowUp => ToolKey::Up,
                        egui::Key::ArrowDown => ToolKey::Down,
                        egui::Key::ArrowLeft => ToolKey::Left,
                        egui::Key::ArrowRight => ToolKey::Right,
                        egui::Key::Tab => ToolKey::Tab,
                        egui::Key::Home => ToolKey::Home,
                        egui::Key::End => ToolKey::End,
                        _ => continue,
                    };
                    self.kit_key(k);
                }
                _ => {}
            }
        }
        // O que foi para o kit não vira atalho do app (Delete não limpa a camada, por exemplo).
        ctx.input_mut(|i| {
            i.events.retain(|e| match e {
                egui::Event::Text(_) => !typing,
                egui::Event::Key { key, .. } => {
                    !typing
                        && !matches!(
                            key,
                            egui::Key::Enter
                                | egui::Key::Escape
                                | egui::Key::Backspace
                                | egui::Key::Delete
                                | egui::Key::ArrowUp
                                | egui::Key::ArrowDown
                                | egui::Key::ArrowLeft
                                | egui::Key::ArrowRight
                                | egui::Key::Tab
                                | egui::Key::Home
                                | egui::Key::End
                        )
                }
                _ => true,
            })
        });
        typing
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.kit_typing(ctx) {
            return;
        }
        let (undo, redo, tool, bigger, smaller) = ctx.input_mut(|i| {
            let redo = i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::Z,
            ) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y);
            let undo = i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z);
            let tool = Tool::all().find(|t| i.consume_key(egui::Modifiers::NONE, t.key()));
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
        // Atalhos do Clip Studio para cor e seleção.
        // O egui entrega Ctrl+C/X/V como eventos próprios, não como teclas.
        let (copy, cut, paste) = ctx.input(|i| {
            let has = |f: fn(&egui::Event) -> bool| i.events.iter().any(f);
            (
                has(|e| matches!(e, egui::Event::Copy)),
                has(|e| matches!(e, egui::Event::Cut)),
                has(|e| matches!(e, egui::Event::Paste(_))),
            )
        });
        if copy {
            self.copy();
        }
        if cut {
            self.cut();
        }
        if paste {
            self.paste();
        }
        let (swap, deselect, invert, all, clear) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::X),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::D),
                i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::I,
                ),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::A),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Delete),
            )
        });
        let (save_as, save, open) = ctx.input_mut(|i| {
            (
                i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::S,
                ),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::S),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::O),
            )
        });
        if save_as {
            self.save_project_as();
        } else if save {
            self.save_project();
        }
        if open {
            self.open_project_dialog();
        }
        if swap {
            self.swap_colors();
        }
        if deselect {
            self.deselect();
        }
        if invert {
            self.invert_selection();
        }
        if all {
            self.select_all();
        }
        if clear && self.doc.selection.is_some() {
            self.clear_selection();
        }
        if enter {
            self.finish_bezier(false);
        }
        if escape {
            self.bezier.clear();
            self.selected = None;
            self.deselect();
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
        let target = match self.tool {
            Tool::Eraser => &mut self.eraser,
            Tool::Blend => &mut self.mix_brush,
            _ => &mut self.brush,
        };
        if bigger {
            target.size = (target.size * 1.2).min(photocraft_max_size());
        }
        if smaller {
            target.size = (target.size / 1.2).max(1.0);
        }
    }
}

impl ArtApp {
    /// Gesto/botão "voltar" do Android: fecha o que estiver aberto por cima, um de cada vez.
    fn back_gesture(&mut self, ctx: &egui::Context) {
        if !ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::BrowserBack)) {
            return;
        }
        if self.dialog.is_some() {
            self.dialog = None;
        } else if self.show_new_dialog {
            self.show_new_dialog = false;
        } else if self.show_color {
            self.show_color = false;
        } else if self.show_layers_panel {
            self.show_layers_panel = false;
        } else if self.show_tools_panel {
            self.show_tools_panel = false;
        }
    }

    /// No celular só cabe um painel de cada vez: o que acabou de abrir fecha os outros.
    fn one_panel_at_a_time(&mut self) {
        let now = [
            self.show_tools_panel,
            self.show_layers_panel,
            self.show_color,
        ];
        let keep = (0..3)
            .rev()
            .find(|&i| now[i] && !self.panels_before[i])
            .or_else(|| (0..3).rev().find(|&i| now[i]));
        if let Some(keep) = keep {
            self.show_tools_panel = keep == 0;
            self.show_layers_panel = keep == 1;
            self.show_color = keep == 2;
        }
        self.panels_before = [
            self.show_tools_panel,
            self.show_layers_panel,
            self.show_color,
        ];
    }
}

fn prefs_path() -> std::path::PathBuf {
    platform::data_dir().join("prefs.json")
}

fn brushes_path() -> std::path::PathBuf {
    platform::data_dir().join("brushes.json")
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
        self.projects_tick(&ctx);
        if self.status != self.last_status {
            self.last_status.clone_from(&self.status);
            self.status_since = ctx.input(|i| i.time);
        }
        self.back_gesture(&ctx);
        let portrait = panels::is_portrait(&ctx);
        if panels::is_compact(&ctx) {
            self.one_panel_at_a_time();
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
        if self.doc.selection.is_some() {
            panels::selection_launcher(self, &ctx, canvas);
        }
        if self.show_navigator {
            panels::navigator(self, &ctx, canvas, portrait);
        }
        panels::toast(self, &ctx, canvas, 0.0);
        crate::dialogs::show(self, &ctx);
        crate::projects::show(self, &ctx);
        if self.show_new_dialog {
            panels::new_canvas_dialog(self, &ctx);
        }
    }
}
