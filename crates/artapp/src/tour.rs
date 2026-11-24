//! Tour curto da primeira abertura: cinco cartões com o básico do app (pincéis, cor, camadas,
//! gestos e salvar). Volta pelo menu ☰ › Mostrar o tour.

use eframe::egui::{self, Align2, RichText, Vec2};

use crate::app::ArtApp;
use crate::theme::{color, panel_frame};

/// Título e texto de cada passo.
pub const STEPS: [(&str, &str); 5] = [
    (
        "Pincéis",
        "Na ferramenta Pincel, a aba Pincéis do painel de ferramentas mostra a biblioteca, por \
         categoria, com Recentes e Favoritos no topo. Segure um pincel para favoritar, duplicar ou renomear.",
    ),
    (
        "Ajustar o pincel",
        "As abas Geral, Forma, Granulado, Dinâmica, Suavização e Avançado mudam o traço. A faixa \
         no topo mostra o traço na hora. Salve o ajuste como pincel novo em Meus pincéis.",
    ),
    (
        "Cor e camadas",
        "O círculo de cor abre a roda, os conjuntos e o histórico (que fica salvo no desenho). \
         Segure o dedo parado na tela para pegar uma cor. As camadas ficam no painel de camadas.",
    ),
    (
        "Gestos",
        "Dois dedos: zoom, girar e mover; toque com dois dedos desfaz e com três refaz. Quatro \
         dedos escondem a interface. Sem caneta, ligue \"Pressão pela velocidade\" na aba Geral.",
    ),
    (
        "Salvar",
        "O botão de salvar fica no topo; o ponto amarelo avisa que há mudanças. O app guarda uma \
         cópia de recuperação sozinho, e o Backup (menu ☰ › Arquivo) copia para uma pasta ou o \
         Google Drive.",
    ),
];

/// Mostra o passo atual do tour, se estiver aberto.
pub fn show(app: &mut ArtApp, ctx: &egui::Context) {
    let Some(step) = app.tour_step else {
        return;
    };
    let Some((title, text)) = STEPS.get(step) else {
        finish(app);
        return;
    };
    let mut next = None;
    let mut done = false;
    egui::Window::new("tour")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .order(egui::Order::Foreground)
        .frame(panel_frame())
        .show(ctx, |ui| {
            ui.set_max_width(360.0);
            ui.label(
                RichText::new(format!("{} de {}", step + 1, STEPS.len()))
                    .size(12.0)
                    .color(color::TEXT_MUTED),
            );
            ui.label(RichText::new(*title).size(18.0).strong());
            ui.add_space(4.0);
            ui.label(*text);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Pular").clicked() {
                    done = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let last = step + 1 == STEPS.len();
                    let label = if last {
                        "Começar a desenhar"
                    } else {
                        "Próximo"
                    };
                    if ui
                        .add(
                            egui::Button::new(RichText::new(label).strong())
                                .min_size(Vec2::new(0.0, 36.0)),
                        )
                        .clicked()
                    {
                        if last {
                            done = true;
                        } else {
                            next = Some(step + 1);
                        }
                    }
                    if step > 0 && ui.button("Voltar").clicked() {
                        next = Some(step - 1);
                    }
                });
            });
        });
    if done {
        finish(app);
    } else if let Some(n) = next {
        app.tour_step = Some(n);
    }
}

fn finish(app: &mut ArtApp) {
    app.tour_step = None;
    if !app.tour_done {
        app.tour_done = true;
        app.save_prefs();
    }
}
