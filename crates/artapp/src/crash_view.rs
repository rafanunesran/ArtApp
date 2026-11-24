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
                    "O ArtApp teve um erro. Este relatório mostra onde foi o problema, para ele ser \
                     corrigido.",
                )
                .size(13.0)
                .color(color::TEXT_MUTED),
            );
            ui.add_space(6.0);
            upload_ui(app, ui);
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

/// Pergunta (uma vez) se pode enviar os relatórios ao site, ou mostra como está.
fn upload_ui(app: &mut ArtApp, ui: &mut egui::Ui) {
    const WHAT: &str = "Vai só a parte técnica (versão do app, sistema, onde o erro aconteceu). \
                        Não vai o desenho, nomes de arquivos nem dados pessoais.";
    match app.crash_upload.consent {
        None => {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "Enviar os relatórios de erro automaticamente para o site do ArtApp?",
                    )
                    .strong(),
                );
                ui.label(RichText::new(WHAT).size(12.0).color(color::TEXT_MUTED));
                ui.horizontal(|ui| {
                    if ui
                        .button(RichText::new("Sim, enviar sempre").strong())
                        .clicked()
                    {
                        app.set_crash_upload(true);
                        app.status = "Obrigado! Os erros vão direto para serem corrigidos.".into();
                    }
                    if ui.button("Não enviar").clicked() {
                        app.set_crash_upload(false);
                    }
                });
            });
        }
        Some(on) => {
            let mut on2 = on;
            ui.checkbox(
                &mut on2,
                "Enviar relatórios de erro automaticamente para o site",
            )
            .on_hover_text(WHAT);
            if on2 != on {
                app.set_crash_upload(on2);
            }
            if on {
                let u = &app.crash_upload;
                let pending = crate::crash::queued().len();
                let msg = if u.sending() {
                    "Enviando…".to_string()
                } else if pending == 0 {
                    "✓ Enviado. Não precisa copiar nem mandar.".to_string()
                } else if let Some(e) = &u.last_error {
                    format!(
                        "{pending} esperando para enviar (sem conexão? {e}). Tenta de novo sozinho."
                    )
                } else {
                    format!("{pending} esperando para enviar.")
                };
                ui.label(RichText::new(msg).size(12.0).color(color::TEXT_MUTED));
            }
        }
    }
}

/// Na abertura: há relatórios esperando e a pessoa ainda não disse se pode enviar. Mostra a
/// janela (com a pergunta), mesmo que o erro tenha sido contornado.
pub fn ask_if_pending(app: &mut ArtApp) {
    if app.crash_report.is_none()
        && app.crash_upload.consent.is_none()
        && !crate::crash::queued().is_empty()
    {
        app.crash_report = crate::crash::latest();
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
