//! QuickMenu (como o do Procreate): um menu redondo de seis ações que abre onde o dedo está,
//! segurando o dedo na tela (se ligado nos gestos) ou com a tecla F.

use eframe::egui::{self, Align2, Color32, Pos2, Sense, Stroke, Vec2};

use crate::app::{ArtApp, Tool};
use crate::theme::{color, icon, icon_font, panel_frame};

/// O que cada botão do QuickMenu faz; a pessoa escolhe seis nas preferências.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Undo,
    Redo,
    NewLayer,
    Eyedropper,
    Transform,
    HideUi,
    Brush,
    Eraser,
    Fill,
    Copy,
    Paste,
    ClearLayer,
    MergeDown,
    DuplicateLayer,
    SelectAll,
    Deselect,
    FlipView,
    SwapColors,
    History,
}

/// Os seis de fábrica.
pub const DEFAULT: [Action; 6] = [
    Action::Undo,
    Action::Redo,
    Action::NewLayer,
    Action::Eyedropper,
    Action::Transform,
    Action::HideUi,
];

impl Action {
    pub const ALL: [Action; 19] = [
        Action::Undo,
        Action::Redo,
        Action::NewLayer,
        Action::Eyedropper,
        Action::Transform,
        Action::HideUi,
        Action::Brush,
        Action::Eraser,
        Action::Fill,
        Action::Copy,
        Action::Paste,
        Action::ClearLayer,
        Action::MergeDown,
        Action::DuplicateLayer,
        Action::SelectAll,
        Action::Deselect,
        Action::FlipView,
        Action::SwapColors,
        Action::History,
    ];

    /// Pelo nome gravado nas preferências.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| format!("{a:?}") == name)
    }

    fn glyph(self) -> char {
        match self {
            Action::Undo => icon::UNDO,
            Action::Redo => icon::REDO,
            Action::NewLayer => icon::PLUS,
            Action::Eyedropper => icon::PIPETTE,
            Action::Transform => icon::SCALING,
            Action::HideUi => icon::MAXIMIZE,
            Action::Brush => icon::BRUSH,
            Action::Eraser => icon::ERASER,
            Action::Fill => icon::BUCKET,
            Action::Copy => icon::COPY,
            Action::Paste => icon::IMAGE_DOWN,
            Action::ClearLayer => icon::TRASH,
            Action::MergeDown => icon::MERGE_DOWN,
            Action::DuplicateLayer => icon::LAYERS,
            Action::SelectAll => icon::SQUARE_DASHED,
            Action::Deselect => icon::DESELECT,
            Action::FlipView => icon::FLIP_H,
            Action::SwapColors => icon::SWAP,
            Action::History => icon::ROTATE_CCW,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Action::Undo => "Desfazer",
            Action::Redo => "Refazer",
            Action::NewLayer => "Nova camada",
            Action::Eyedropper => "Conta-gotas",
            Action::Transform => "Transformar",
            Action::HideUi => "Tela cheia",
            Action::Brush => "Pincel",
            Action::Eraser => "Borracha",
            Action::Fill => "Balde",
            Action::Copy => "Copiar",
            Action::Paste => "Colar",
            Action::ClearLayer => "Limpar camada",
            Action::MergeDown => "Mesclar abaixo",
            Action::DuplicateLayer => "Duplicar camada",
            Action::SelectAll => "Selecionar tudo",
            Action::Deselect => "Tirar a seleção",
            Action::FlipView => "Espelhar a vista",
            Action::SwapColors => "Trocar as cores",
            Action::History => "Histórico",
        }
    }

    fn run(self, app: &mut ArtApp) {
        match self {
            Action::Undo => app.undo(),
            Action::Redo => app.redo(),
            Action::NewLayer => {
                app.doc.add_layer();
                app.invalidate_all();
            }
            Action::Eyedropper => app.set_tool(Tool::Eyedropper),
            Action::Transform => app.start_transform(),
            Action::HideUi => app.hide_ui = !app.hide_ui,
            Action::Brush => app.set_tool(Tool::Brush),
            Action::Eraser => app.set_tool(Tool::Eraser),
            Action::Fill => app.set_tool(Tool::Fill),
            Action::Copy => app.copy(),
            Action::Paste => app.paste(),
            Action::ClearLayer => {
                app.doc.clear_layer();
                app.invalidate_all();
            }
            Action::MergeDown => {
                app.doc.merge_down();
                app.invalidate_all();
            }
            Action::DuplicateLayer => {
                app.doc.duplicate_layer();
                app.invalidate_all();
            }
            Action::SelectAll => app.select_all(),
            Action::Deselect => app.deselect(),
            Action::FlipView => app.view.flip = !app.view.flip,
            Action::SwapColors => app.swap_colors(),
            Action::History => app.history_open = true,
        }
    }
}

/// Escolha das seis ações (nas preferências de gestos).
pub fn settings(app: &mut ArtApp, ui: &mut egui::Ui) -> bool {
    let mut changed = false;
    ui.label(
        egui::RichText::new("Botões do QuickMenu")
            .size(12.0)
            .color(color::TEXT_MUTED),
    );
    // Setas em vez de uma lista suspensa: a lista abriria por cima do menu e o fecharia.
    let n = Action::ALL.len();
    for k in 0..app.quick_actions.len() {
        let a = app.quick_actions[k];
        let at = Action::ALL.iter().position(|b| *b == a).unwrap_or(0);
        let mut next = None;
        ui.horizontal(|ui| {
            if ui.small_button(" < ").clicked() {
                next = Some(Action::ALL[(at + n - 1) % n]);
            }
            if ui.small_button(" > ").clicked() {
                next = Some(Action::ALL[(at + 1) % n]);
            }
            ui.label(
                egui::RichText::new(format!("{}  {}", a.glyph(), a.label()))
                    .size(13.0)
                    .color(color::TEXT),
            );
        });
        if let Some(b) = next {
            app.quick_actions[k] = b;
            changed = true;
        }
    }
    changed
}

const RADIUS: f32 = 74.0;
const BUTTON: f32 = 30.0;

/// Desenha o menu aberto e executa o que for tocado. Tocar fora fecha.
pub fn show(app: &mut ArtApp, ctx: &egui::Context) {
    let Some(center) = app.quick_menu else {
        return;
    };
    // Mantém o menu inteiro dentro da tela.
    let screen = ctx.content_rect().shrink(RADIUS + BUTTON + 4.0);
    let center = Pos2::new(
        center
            .x
            .clamp(screen.left(), screen.right().max(screen.left())),
        center
            .y
            .clamp(screen.top(), screen.bottom().max(screen.top())),
    );
    let size = Vec2::splat((RADIUS + BUTTON) * 2.0);
    let actions = app.quick_actions;
    let mut picked = None;
    let mut close = false;
    egui::Area::new(egui::Id::unique("quick_menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(center - size / 2.0)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            let p = ui.painter();
            p.circle_filled(
                rect.center(),
                RADIUS + BUTTON + 2.0,
                Color32::from_black_alpha(90),
            );
            for (k, a) in actions.iter().enumerate() {
                let ang = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::TAU / 6.0;
                let c = rect.center() + Vec2::angled(ang) * RADIUS;
                let r = egui::Rect::from_center_size(c, Vec2::splat(BUTTON * 2.0));
                let resp = ui
                    .interact(r, ui.scope_id().with(k), Sense::click())
                    .on_hover_text(a.label());
                let fill = if resp.hovered() {
                    color::ACCENT
                } else {
                    color::PANEL
                };
                let p = ui.painter();
                p.circle_filled(c, BUTTON, fill);
                p.circle_stroke(c, BUTTON, Stroke::new(1.0, color::BORDER));
                p.text(
                    c,
                    Align2::CENTER_CENTER,
                    a.glyph(),
                    icon_font(20.0),
                    color::TEXT,
                );
                if resp.clicked() {
                    picked = Some(*a);
                }
            }
            let mid = egui::Rect::from_center_size(rect.center(), Vec2::splat(36.0));
            let resp = ui.interact(mid, ui.scope_id().with("close"), Sense::click());
            let p = ui.painter();
            p.circle_filled(rect.center(), 18.0, color::SURFACE);
            p.text(
                rect.center(),
                Align2::CENTER_CENTER,
                icon::X,
                icon_font(16.0),
                color::TEXT_MUTED,
            );
            close |= resp.clicked();
        });
    let clicked_out = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| p.distance(center) > RADIUS + BUTTON)
    });
    if let Some(a) = picked {
        a.run(app);
        close = true;
    }
    if close || clicked_out || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.quick_menu = None;
    }
}

/// Uma ação de menu.
type Run = fn(&mut ArtApp);

/// Menu de copiar e colar (deslizar três dedos para baixo, como no Procreate).
pub fn copy_paste(app: &mut ArtApp, ctx: &egui::Context) {
    let Some(at) = app.copy_menu else {
        return;
    };
    let mut picked: Option<Run> = None;
    let area = egui::Area::new(egui::Id::unique("copy_menu"))
        .order(egui::Order::Foreground)
        .pivot(Align2::CENTER_TOP)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            panel_frame().show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Copiar e colar")
                        .size(12.0)
                        .color(color::TEXT_MUTED),
                );
                let items: [(&str, Run); 5] = [
                    ("Recortar", ArtApp::cut),
                    ("Copiar", ArtApp::copy),
                    ("Colar", ArtApp::paste),
                    ("Recortar e colar", |app| {
                        app.cut();
                        app.paste();
                    }),
                    ("Duplicar camada", |app| {
                        app.doc.duplicate_layer();
                        app.invalidate_all();
                    }),
                ];
                for (label, run) in items {
                    let enabled = label != "Colar" || app.clipboard.is_some();
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(label).min_size(Vec2::new(180.0, 36.0)),
                        )
                        .clicked()
                    {
                        picked = Some(run);
                    }
                }
            });
        });
    let clicked_out = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !area.response.rect.contains(p))
    });
    if let Some(run) = picked {
        run(app);
    }
    if picked.is_some() || clicked_out || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.copy_menu = None;
    }
}
