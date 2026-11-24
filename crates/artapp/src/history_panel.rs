//! Painel de histórico (como o do Clip Studio): a lista dos passos do desfazer. Tocar num passo
//! volta (ou avança) o desenho até ele; os passos depois do atual ficam apagados até virem a
//! ser refeitos ou um traço novo os descartar.

use eframe::egui::{self, Align2, RichText, Vec2};

use crate::app::ArtApp;
use crate::theme::{color, panel_frame};

pub fn show(app: &mut ArtApp, ctx: &egui::Context) {
    if !app.history_open {
        return;
    }
    let (names, now) = app.doc.history_steps();
    let mut open = true;
    let mut jump = None;
    egui::Window::new("Histórico")
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-12.0, -12.0))
        .frame(panel_frame())
        .default_width(240.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(240.0);
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() * 0.5).max(160.0))
                .show(ui, |ui| {
                    for (i, name) in names.iter().enumerate() {
                        let text = RichText::new(name).size(13.0).color(if i > now {
                            color::TEXT_MUTED
                        } else {
                            color::TEXT
                        });
                        let r = ui.selectable_label(i == now, text);
                        if i == now {
                            r.scroll_to_me(None);
                        }
                        if r.clicked() && i != now {
                            jump = Some(i);
                        }
                    }
                });
        });
    if let Some(i) = jump {
        app.jump_history(i);
    }
    app.history_open = open;
}
