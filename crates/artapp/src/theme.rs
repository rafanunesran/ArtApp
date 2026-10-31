//! Visual do ArtApp: cores (tokens), fontes, ícones (Lucide) e os widgets básicos da interface.
//!
//! Os valores seguem o protótipo de interface feito no Lovable ("ArtApp Studio"): tema escuro, a
//! tela de desenho ocupando tudo e painéis flutuantes translúcidos com cantos de 12 px.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Response, Sense, Stroke, Vec2,
};

/// Tokens de cor.
pub mod color {
    use eframe::egui::Color32;

    /// Fundo atrás da folha.
    pub const BACKDROP: Color32 = Color32::from_rgb(0x1e, 0x1f, 0x22);
    /// Painéis.
    pub const PANEL: Color32 = Color32::from_rgb(0x2a, 0x2b, 0x2f);
    /// Barra de cima e controles.
    pub const SURFACE: Color32 = Color32::from_rgb(0x23, 0x24, 0x28);
    pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x34, 0x35, 0x3a);
    pub const BORDER: Color32 = Color32::from_rgb(0x3a, 0x3b, 0x40);
    pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xea);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x9a, 0x9b, 0xa1);
    /// Ferramenta/estado ativo.
    pub const ACCENT: Color32 = Color32::from_rgb(0xf2, 0xb1, 0x34);
    /// Texto/ícone sobre o acento.
    pub const ON_ACCENT: Color32 = Color32::from_rgb(0x1e, 0x1f, 0x22);
    /// Seleção (objetos vetoriais, camada ativa).
    pub const SELECTION: Color32 = Color32::from_rgb(0x4a, 0xa3, 0xff);
}

pub const RADIUS: u8 = 12;
/// Lado mínimo de um alvo de toque, em pontos.
pub const TOUCH: f32 = 44.0;

/// Ícones (pontos de código da fonte Lucide incluída em `assets/fonts/lucide-artapp.ttf`).
/// Alguns ficam reservados para as próximas ferramentas.
#[allow(dead_code)]
pub mod icon {
    pub const MENU: char = '\u{e119}';
    pub const UNDO: char = '\u{e2a1}';
    pub const REDO: char = '\u{e2a0}';
    pub const BRUSH: char = '\u{e1d3}';
    pub const ERASER: char = '\u{e28f}';
    pub const BUCKET: char = '\u{e2e6}';
    pub const PIPETTE: char = '\u{e13f}';
    pub const HAND: char = '\u{e1d7}';
    pub const POINTER: char = '\u{e1c3}';
    pub const PEN_TOOL: char = '\u{e135}';
    pub const PEN_LINE: char = '\u{e134}';
    pub const MINUS: char = '\u{e120}';
    pub const SQUARE: char = '\u{e16b}';
    pub const CIRCLE: char = '\u{e07a}';
    pub const LAYERS: char = '\u{e104}';
    pub const SLIDERS: char = '\u{e29a}';
    pub const PLUS: char = '\u{e141}';
    pub const COPY: char = '\u{e0a2}';
    pub const MERGE_DOWN: char = '\u{e45a}';
    pub const TRASH: char = '\u{e18e}';
    pub const EYE: char = '\u{e0be}';
    pub const EYE_OFF: char = '\u{e0bf}';
    pub const LOCK: char = '\u{e10f}';
    pub const UNLOCK: char = '\u{e110}';
    pub const CHECK: char = '\u{e070}';
    pub const X: char = '\u{e1b2}';
    pub const IMAGE_DOWN: char = '\u{e541}';
    pub const MAXIMIZE: char = '\u{e116}';
    pub const CHEVRON_UP: char = '\u{e074}';
    pub const CHEVRON_DOWN: char = '\u{e071}';
    pub const PALETTE: char = '\u{e1dd}';
    pub const FILE_PLUS: char = '\u{e0cd}';
    pub const IMAGE: char = '\u{e0f9}';
    pub const SQUARE_PEN: char = '\u{e176}';
    // Acrescentados da Lucide atual (sem conflito com os pontos de código acima).
    pub const LASSO: char = '\u{e1ce}';
    pub const SQUARE_DASHED: char = '\u{e1cb}';
    pub const CIRCLE_DASHED: char = '\u{e4b0}';
    pub const MOVE: char = '\u{e121}';
    pub const ROTATE_CW: char = '\u{e149}';
    pub const ROTATE_CCW: char = '\u{e148}';
    pub const FLIP_H: char = '\u{e35e}';
    pub const SWAP: char = '\u{e24a}';
    pub const CLIP: char = '\u{e12d}';
    pub const REFERENCE: char = '\u{e180}';
    pub const DRAFT: char = '\u{e596}';
    pub const INVERT: char = '\u{e3db}';
    pub const DESELECT: char = '\u{e175}';
    pub const SCISSORS: char = '\u{e14e}';
    pub const DROPLET: char = '\u{e0b4}';
    pub const DROPLETS: char = '\u{e0b5}';
    pub const WAND_SPARKLES: char = '\u{e357}';
    pub const SCALING: char = '\u{e2ec}';
    pub const SYMMETRY: char = '\u{e3b6}';
    pub const TYPE: char = '\u{e198}';
    pub const MOUSE_POINTER: char = '\u{e11f}';
    pub const GRADIENT: char = '\u{e59c}';
    pub const SUN_DIM: char = '\u{e299}';
    pub const STAMP: char = '\u{e3bb}';
    pub const CROP: char = '\u{e0ab}';
    pub const CONTRAST: char = '\u{e09d}';
    pub const HEXAGON: char = '\u{e0f3}';
    pub const TRIANGLE: char = '\u{e192}';
    pub const GRID_3X3: char = '\u{e0e9}';
    pub const COMBINE: char = '\u{e44c}';
    pub const ROTATE_3D: char = '\u{e2ea}';
    pub const RULER: char = '\u{e14b}';
    pub const SAVE: char = '\u{e14d}';
    pub const FOLDER_OPEN: char = '\u{e247}';
    pub const SPLINE_POINTER: char = '\u{e64f}';
    pub const SPARKLES: char = '\u{e412}';
    pub const PAINT_ROLLER: char = '\u{e59e}';
    pub const FLIP_H_SIMPLE: char = '\u{e35d}';
    pub const SCISSORS_DASHED: char = '\u{e4e9}';
    pub const SPLINE: char = '\u{e38b}';
    pub const PLAY: char = '\u{e13c}';
    pub const PAUSE: char = '\u{e12e}';
    pub const SETTINGS: char = '\u{e154}';
    pub const FILM: char = '\u{e0d0}';
}

const ICON_FONT: &[u8] = include_bytes!("../../../assets/fonts/lucide-artapp.ttf");

/// Nome da família de fonte só com os ícones.
pub fn icon_family() -> FontFamily {
    FontFamily::Name("icons".into())
}

pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, icon_family())
}

/// Fontes e estilo do app inteiro.
pub fn install(ctx: &egui::Context, touch: bool) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "lucide".into(),
        std::sync::Arc::new(egui::FontData::from_static(ICON_FONT)),
    );
    fonts.families.insert(icon_family(), vec!["lucide".into()]);
    // Também como último recurso das famílias normais, para ícones dentro de textos.
    for fam in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(fam).or_default().push("lucide".into());
    }
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        let v = &mut style.visuals;
        v.panel_fill = color::SURFACE;
        v.window_fill = color::PANEL;
        v.extreme_bg_color = Color32::from_rgb(0x18, 0x19, 0x1c);
        v.window_stroke = Stroke::new(1.0, color::BORDER);
        v.window_corner_radius = CornerRadius::same(RADIUS);
        v.menu_corner_radius = CornerRadius::same(RADIUS);
        v.selection.bg_fill = color::ACCENT;
        v.selection.stroke = Stroke::new(1.0, color::ON_ACCENT);
        v.hyperlink_color = color::SELECTION;
        v.override_text_color = Some(color::TEXT);
        let r = CornerRadius::same(8);
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = r;
        }
        v.widgets.inactive.weak_bg_fill = color::SURFACE;
        v.widgets.inactive.bg_fill = color::SURFACE_HOVER;
        v.widgets.hovered.weak_bg_fill = color::SURFACE_HOVER;
        v.widgets.active.weak_bg_fill = color::ACCENT;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, color::BORDER);
        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(8.0, 8.0);
        s.button_padding = egui::vec2(10.0, 6.0);
        s.slider_width = 150.0;
        // A barra de rolagem fica meio visível mesmo parada: no toque não há "passar o mouse"
        // para descobrir que um painel ou o trilho de ferramentas rola.
        s.scroll.dormant_handle_opacity = 0.5;
        s.scroll.dormant_background_opacity = 0.2;
        if touch {
            s.interact_size = egui::vec2(TOUCH, 36.0);
            s.slider_width = 170.0;
            for font in style.text_styles.values_mut() {
                font.size *= 1.1;
            }
        }
    });
}

/// Moldura dos painéis flutuantes.
pub fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(color::PANEL)
        .stroke(Stroke::new(1.0, color::BORDER))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::same(12))
        .shadow(egui::epaint::Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: Color32::from_black_alpha(110),
        })
}

/// Botão quadrado com ícone (44×44 por padrão). `active` pinta com a cor de destaque.
pub fn icon_button(ui: &mut egui::Ui, glyph: char, active: bool, tip: &str) -> Response {
    icon_button_sized(ui, glyph, active, tip, TOUCH)
}

pub fn icon_button_sized(
    ui: &mut egui::Ui,
    glyph: char,
    active: bool,
    tip: &str,
    side: f32,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
    if ui.is_rect_visible(rect) {
        let enabled = ui.is_enabled();
        let (bg, fg) = if active {
            (color::ACCENT, color::ON_ACCENT)
        } else if resp.hovered() && enabled {
            (color::SURFACE_HOVER, color::TEXT)
        } else {
            (
                Color32::TRANSPARENT,
                if enabled {
                    color::TEXT
                } else {
                    color::TEXT_MUTED.gamma_multiply(0.6)
                },
            )
        };
        let p = ui.painter();
        p.rect_filled(rect.shrink(2.0), CornerRadius::same(10), bg);
        p.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            glyph,
            icon_font(side * 0.45),
            fg,
        );
    }
    if tip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tip)
    }
}

/// Título pequeno de seção dentro de um painel.
pub fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(11.5)
            .color(color::TEXT_MUTED)
            .strong(),
    );
}

/// Slider vertical em pílula, estilo Procreate: preenche de baixo para cima e mostra o valor num
/// balão ao lado enquanto é arrastado. `log` usa escala logarítmica (tamanho do pincel); `height`
/// encolhe em telas baixas.
pub fn vslider(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    log: bool,
    height: f32,
    label: &str,
    format: impl Fn(f32) -> String,
) -> Response {
    let (lo, hi) = (*range.start(), *range.end());
    let to_t = |v: f32| {
        if log {
            (v.max(lo).ln() - lo.ln()) / (hi.ln() - lo.ln())
        } else {
            (v - lo) / (hi - lo)
        }
    };
    let from_t = |t: f32| {
        if log {
            (lo.ln() + t * (hi.ln() - lo.ln())).exp()
        } else {
            lo + t * (hi - lo)
        }
    };
    let size = Vec2::new(36.0, height);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    // Arraste relativo: encostar não faz o valor pular, só o movimento muda (como no Procreate).
    if resp.dragged() {
        let dy = resp.drag_delta().y;
        let t = (to_t(*value) - dy / (rect.height() - 8.0)).clamp(0.0, 1.0);
        let v = from_t(t).clamp(lo, hi);
        if (v - *value).abs() > f32::EPSILON {
            *value = v;
            resp.mark_changed();
        }
    }
    let t = to_t(*value).clamp(0.0, 1.0);
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(18), color::PANEL);
    p.rect_stroke(
        rect,
        CornerRadius::same(18),
        Stroke::new(1.0, color::BORDER),
        egui::StrokeKind::Inside,
    );
    let inner = rect.shrink(4.0);
    let fill_h = (inner.height() * t).max(0.0);
    let fill =
        egui::Rect::from_min_max(egui::pos2(inner.left(), inner.bottom() - fill_h), inner.max);
    p.rect_filled(fill, CornerRadius::same(14), Color32::from_white_alpha(38));
    let thumb_y = inner.bottom() - fill_h.max(5.0).min(inner.height() - 3.0);
    let thumb = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, thumb_y + 3.0),
        Vec2::new(20.0, 6.0),
    );
    p.rect_filled(thumb, CornerRadius::same(3), color::TEXT);
    if resp.dragged() || resp.is_pointer_button_down_on() {
        let text = format!("{label} {}", format(*value));
        let galley =
            ui.painter()
                .layout_no_wrap(text, FontId::proportional(12.5), color::ON_ACCENT);
        let bubble = egui::Rect::from_min_size(
            egui::pos2(rect.right() + 10.0, thumb.center().y - 12.0),
            galley.size() + Vec2::new(18.0, 10.0),
        );
        let fg = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::unique("vslider_bubble"),
        ));
        fg.rect_filled(bubble, CornerRadius::same(8), color::ACCENT);
        fg.galley(bubble.min + Vec2::new(9.0, 5.0), galley, color::ON_ACCENT);
    }
    resp.on_hover_text(label)
}

/// Interruptor (liga/desliga) com o texto à esquerda.
pub fn switch(ui: &mut egui::Ui, on: &mut bool, text: &str) -> Response {
    let w = ui.available_width();
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(w, 36.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let p = ui.painter();
    p.text(
        egui::pos2(rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        FontId::proportional(14.0),
        color::TEXT,
    );
    let track = egui::Rect::from_center_size(
        egui::pos2(rect.right() - 22.0, rect.center().y),
        Vec2::new(40.0, 24.0),
    );
    let anim = ui.ctx().animate_bool(resp.id, *on);
    let bg = if *on {
        color::ACCENT
    } else {
        Color32::from_rgb(0x1a, 0x1b, 0x1e)
    };
    p.rect_filled(track, CornerRadius::same(12), bg);
    p.rect_stroke(
        track,
        CornerRadius::same(12),
        Stroke::new(1.0, color::BORDER),
        egui::StrokeKind::Inside,
    );
    let x = egui::lerp(track.left() + 12.0..=track.right() - 12.0, anim);
    p.circle_filled(egui::pos2(x, track.center().y), 9.0, color::TEXT);
    resp
}

/// Parte tocada do par de cores.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ColorChip {
    Main,
    Sub,
    Transparent,
    /// A cor principal foi arrastada e solta neste ponto da tela (ColorDrop do Procreate).
    Drop(egui::Pos2),
}

/// Cores principal e secundária e a cor transparente, como no pé da paleta de ferramentas do Clip
/// Studio: a principal na frente (abre o seletor), a secundária atrás (troca as duas) e o xadrez
/// pequeno (pinta transparência). A escolhida ganha o contorno de destaque.
pub fn color_chips(
    ui: &mut egui::Ui,
    main: Color32,
    sub: Color32,
    transparent: bool,
) -> Option<ColorChip> {
    let side = TOUCH;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(side), Sense::click_and_drag());
    let p = ui.painter();
    let main_c = rect.min + Vec2::new(17.0, 17.0);
    let sub_c = rect.min + Vec2::new(30.0, 30.0);
    let tr_c = rect.min + Vec2::new(9.0, 36.0);
    let ring = |c: egui::Pos2, r: f32, on: bool| {
        let edge = if on { color::ACCENT } else { color::BORDER };
        p.circle_filled(c, r + 2.5, edge);
        p.circle_filled(c, r + 1.0, Color32::WHITE);
    };
    ring(sub_c, 9.0, false);
    p.circle_filled(sub_c, 9.0, sub);
    ring(main_c, 12.0, !transparent);
    p.circle_filled(main_c, 12.0, main);
    ring(tr_c, 6.0, transparent);
    for (i, q) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
        .into_iter()
        .enumerate()
    {
        let shade = if i == 0 || i == 3 { 200 } else { 140 };
        let r = egui::Rect::from_two_pos(tr_c, tr_c + Vec2::new(q.0 * 4.2, q.1 * 4.2));
        p.rect_filled(r, 0.0, Color32::from_gray(shade));
    }
    // Arrastar a cor principal leva uma bolinha da cor; soltar na tela preenche ali.
    if resp.dragged()
        && let Some(at) = resp.interact_pointer_pos()
    {
        let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("color-drop"));
        let p = ui.ctx().layer_painter(layer);
        p.circle_filled(at, 15.0, Color32::WHITE);
        p.circle_filled(at, 13.0, main);
    }
    if resp.drag_stopped() {
        let at = resp
            .interact_pointer_pos()
            .or_else(|| ui.ctx().input(|i| i.pointer.latest_pos()));
        return at.filter(|p| !rect.contains(*p)).map(ColorChip::Drop);
    }
    let resp = resp.on_hover_text(
        "Cor (toque na de trás para trocar; xadrez = transparente; arraste até a tela para preencher)",
    );
    if !resp.clicked() {
        return None;
    }
    let at = resp.interact_pointer_pos()?;
    Some(if at.distance(tr_c) <= 9.0 {
        ColorChip::Transparent
    } else if at.distance(main_c) <= 14.0 {
        ColorChip::Main
    } else {
        ColorChip::Sub
    })
}
