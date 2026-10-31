//! Linha do tempo do assistente de animação: quadros, reproduzir, pele de cebola e exportar.

use artapp_core::animation::{FPS_RANGE, ONION_MAX, Playback};
use eframe::egui::{self, RichText};

use crate::app::ArtApp;
use crate::theme::{color, icon, icon_button_sized};

/// Avança a reprodução ou segue a camada ativa. Chamado a cada quadro da interface.
pub fn tick(app: &mut ArtApp, ctx: &egui::Context) {
    if !app.doc.animation.enabled {
        app.anim_playing = false;
        return;
    }
    let before = app.doc.animation.current;
    if app.anim_playing {
        let now = ctx.input(|i| i.time);
        let step = 1.0
            / f64::from(
                app.doc
                    .animation
                    .fps
                    .clamp(*FPS_RANGE.start(), *FPS_RANGE.end()),
            );
        if now >= app.anim_next {
            match app.doc.next_frame(before, app.anim_dir) {
                Some((k, dir)) => {
                    app.doc.animation.current = k;
                    app.anim_dir = dir;
                }
                None => app.anim_playing = false,
            }
            app.anim_next = now + step;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(
            (app.anim_next - now).max(0.0),
        ));
    } else {
        app.doc.follow_active_frame();
    }
    let n = app.doc.frames().len();
    if n > 0 && app.doc.animation.current >= n {
        app.doc.animation.current = n - 1;
    }
    if app.doc.animation.current != before {
        app.redraw_canvas();
    }
}

/// A barra embaixo da tela, com a animação ligada.
pub fn show(app: &mut ArtApp, ctx: &egui::Context, canvas: egui::Rect) {
    if !app.doc.animation.enabled {
        return;
    }
    let before = app.doc.animation.clone();
    egui::Area::new(egui::Id::unique("anim_bar"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(canvas.center_bottom() - egui::vec2(0.0, 10.0))
        .show(ctx, |ui| {
            crate::theme::panel_frame().show(ui, |ui| {
                ui.set_max_width((canvas.width() - 24.0).clamp(200.0, 560.0));
                timeline(app, ui);
                if app.anim_settings_open {
                    ui.separator();
                    settings(app, ui);
                }
            });
        });
    if app.doc.animation != before {
        app.redraw_canvas();
    }
}

fn timeline(app: &mut ArtApp, ui: &mut egui::Ui) {
    let frames = app.doc.frames();
    let cur = app.doc.animation.current;
    ui.horizontal(|ui| {
        let play = if app.anim_playing {
            icon::PAUSE
        } else {
            icon::PLAY
        };
        if icon_button_sized(ui, play, app.anim_playing, "Reproduzir", 34.0).clicked() {
            app.anim_playing = !app.anim_playing;
            app.anim_dir = 1;
            app.anim_next = 0.0;
            if !app.anim_playing {
                // Parou: a camada ativa passa a ser a do quadro que ficou na tela.
                if let Some(&i) = frames.get(app.doc.animation.current) {
                    app.doc.active = i;
                }
            }
        }
        egui::ScrollArea::horizontal()
            .max_width(ui.available_width() - 90.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (k, &i) in frames.iter().enumerate() {
                        let label = RichText::new(format!("{}", k + 1)).size(15.0);
                        let resp = ui
                            .add(
                                egui::Button::new(label)
                                    .selected(k == cur)
                                    .min_size(egui::vec2(34.0, 30.0)),
                            )
                            .on_hover_text(app.doc.layers.get(i).map_or("", |l| l.name.as_str()));
                        if resp.clicked() {
                            app.anim_playing = false;
                            app.doc.active = i;
                            app.doc.animation.current = k;
                            app.redraw_canvas();
                        }
                    }
                });
            });
        if icon_button_sized(ui, icon::PLUS, false, "Novo quadro", 34.0).clicked() {
            app.commit_transform();
            app.anim_playing = false;
            app.doc.add_frame();
            app.invalidate_all();
        }
        if icon_button_sized(
            ui,
            icon::SETTINGS,
            app.anim_settings_open,
            "Ajustes da animação",
            34.0,
        )
        .clicked()
        {
            app.anim_settings_open = !app.anim_settings_open;
        }
    });
}

fn settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    let a = &mut app.doc.animation;
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut a.playback, Playback::Loop, "Repetir");
        ui.selectable_value(&mut a.playback, Playback::PingPong, "Ida e volta");
        ui.selectable_value(&mut a.playback, Playback::Once, "Uma vez");
    });
    ui.add(
        egui::Slider::new(&mut a.fps, FPS_RANGE)
            .step_by(1.0)
            .text("quadros por segundo"),
    );
    ui.add(egui::Slider::new(&mut a.onion_before, 0..=ONION_MAX).text("pele de cebola antes"));
    ui.add(egui::Slider::new(&mut a.onion_after, 0..=ONION_MAX).text("pele de cebola depois"));
    ui.add(egui::Slider::new(&mut a.onion_opacity, 0.05..=1.0).text("opacidade da pele"));
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut a.onion_colors, "Pele colorida");
        ui.checkbox(&mut a.background, "1º quadro é fundo");
        ui.checkbox(&mut a.foreground, "Último é primeiro plano");
    });
    ui.horizontal_wrapped(|ui| {
        if ui.button("Exportar GIF").clicked() {
            app.export_animation(false);
        }
        if ui.button("Exportar PNG animado").clicked() {
            app.export_animation(true);
        }
    });
    ui.label(
        RichText::new("Cada camada (ou pasta) é um quadro, de baixo para cima. O lado maior sai com até 1080 px.")
            .size(12.0)
            .color(color::TEXT_MUTED),
    );
}
