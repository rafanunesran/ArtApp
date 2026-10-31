//! QuickMenu (como o do Procreate): um menu redondo de seis ações que abre onde o dedo está,
//! segurando o dedo na tela (se ligado nos gestos) ou com a tecla F.

use eframe::egui::{self, Align2, Color32, Pos2, Sense, Stroke, Vec2};

use crate::app::{ArtApp, Tool};
use crate::theme::{color, icon, icon_font};

#[derive(Clone, Copy)]
enum Action {
    Undo,
    Redo,
    NewLayer,
    Eyedropper,
    Transform,
    HideUi,
}

const ACTIONS: [Action; 6] = [
    Action::Undo,
    Action::Redo,
    Action::NewLayer,
    Action::Eyedropper,
    Action::Transform,
    Action::HideUi,
];

impl Action {
    fn glyph(self) -> char {
        match self {
            Action::Undo => icon::UNDO,
            Action::Redo => icon::REDO,
            Action::NewLayer => icon::PLUS,
            Action::Eyedropper => icon::PIPETTE,
            Action::Transform => icon::SCALING,
            Action::HideUi => icon::MAXIMIZE,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Action::Undo => "Desfazer",
            Action::Redo => "Refazer",
            Action::NewLayer => "Nova camada",
            Action::Eyedropper => "Conta-gotas",
            Action::Transform => "Transformar",
            Action::HideUi => "Tela cheia",
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
        }
    }
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
            for (k, a) in ACTIONS.iter().enumerate() {
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
