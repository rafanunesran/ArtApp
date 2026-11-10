//! Janela do relatório de erros (ver `crash`): aparece sozinha quando a sessão anterior terminou
//! com erro, e pelo menu Arquivo › Relatório de erros.

use eframe::egui::{self, Align2, RichText, Vec2};

use crate::app::ArtApp;
use crate::theme::{color, panel_frame};

pub fn show(app: &mut ArtApp, ctx: &egui::Context) {
    let Some(report) = app.crash_report.clone() else {
        return;
    };
    let mut open = true;
    let mut close = false;
    egui::Window::new("Relatório de erros")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .order(egui::Order::Foreground)
        .frame(panel_frame())
        .show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.label(
                RichText::new(
                    "O ArtApp teve um erro. Toque em Copiar (ou Compartilhar) e mande na conversa com quem está \
                     corrigindo o app: isso mostra onde foi o problema.",
                )
                .size(13.0)
                .color(color::TEXT_MUTED),
            );
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .max_height(ctx.content_rect().height() * 0.5)
                .show(ui, |ui| {
                    ui.label(RichText::new(report.as_str()).monospace().size(11.0));
                });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Copiar").clicked() {
                    ctx.copy_text(report.clone());
                    app.status = "Relatório copiado.".into();
                }
                if crate::platform::share::SUPPORTED && ui.button("Compartilhar").clicked() {
                    let path = crate::crash::dir().join("relatorio-artapp.txt");
                    let r = std::fs::write(&path, &report)
                        .map_err(|e| e.to_string())
                        .and_then(|()| {
                            crate::platform::share::file(
                                &path,
                                "text/plain",
                                "Relatório de erros do ArtApp",
                            )
                        });
                    if let Err(e) = r {
                        app.status = format!("Não deu para compartilhar: {e}");
                    }
                }
                if ui.button("Fechar").clicked() {
                    close = true;
                }
            });
        });
    if !open || close {
        app.crash_report = None;
    }
}

/// Abre a janela com o relatório mais recente (menu).
pub fn open_latest(app: &mut ArtApp) {
    app.crash_report = Some(crate::crash::latest().unwrap_or_else(|| {
        "Nenhum erro registrado. Se o app fechou sozinho sem aparecer nada aqui, provavelmente \
         foi falta de memória: na próxima abertura deve aparecer um aviso."
            .into()
    }));
}
