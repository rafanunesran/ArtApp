//! Barra de ferramentas, painel de pincel/cor, painel de camadas e o diálogo de nova tela.

use artapp_core::BlendMode;
use eframe::egui::{self, Color32, RichText};

use crate::app::{ArtApp, CANVAS_PRESETS, Tool, photocraft_max_size};

pub fn toolbar(app: &mut ArtApp, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.menu_button("Arquivo", |ui| {
            if ui.button("Nova tela…").clicked() {
                app.show_new_dialog = true;
                ui.close();
            }
            if ui.button("Exportar PNG").clicked() {
                app.export_png(true);
                ui.close();
            }
            if ui.button("Exportar PNG transparente").clicked() {
                app.export_png(false);
                ui.close();
            }
        });
        ui.separator();
        if ui
            .add_enabled(app.doc.can_undo(), egui::Button::new("Desfazer"))
            .clicked()
        {
            app.undo();
        }
        if ui
            .add_enabled(app.doc.can_redo(), egui::Button::new("Refazer"))
            .clicked()
        {
            app.redo();
        }
        ui.separator();
        for t in Tool::ALL {
            if ui.selectable_label(app.tool == t, t.label()).clicked() {
                app.set_tool(t);
            }
        }
        ui.separator();
        let mut c = app.color;
        if egui::color_picker::color_edit_button_srgba(
            ui,
            &mut c,
            egui::color_picker::Alpha::Opaque,
        )
        .changed()
        {
            app.color = c;
        }
        ui.separator();
        ui.toggle_value(&mut app.show_tools_panel, "Pincel");
        ui.toggle_value(&mut app.show_layers_panel, "Camadas");
        if ui
            .button("Ajustar")
            .on_hover_text("Encaixar a tela na janela")
            .clicked()
        {
            app.view.request_fit();
        }
    });
}

pub fn status_bar(app: &mut ArtApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(format!("{} × {} px", app.doc.width, app.doc.height));
        ui.separator();
        ui.label(format!("Zoom {:.0}%", app.view.zoom * 100.0));
        ui.separator();
        let layer = app
            .doc
            .active_layer()
            .map(|l| l.name.clone())
            .unwrap_or_default();
        ui.label(layer);
        if !app.status.is_empty() {
            ui.separator();
            ui.label(&app.status);
        }
    });
}

pub fn tools_panel(app: &mut ArtApp, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading(if app.tool == Tool::Eraser {
            "Borracha"
        } else {
            "Pincel"
        });
        let max = photocraft_max_size();
        let use_pressure = &mut app.use_pressure;
        let b = if app.tool == Tool::Eraser {
            &mut app.eraser
        } else {
            &mut app.brush
        };
        ui.add(
            egui::Slider::new(&mut b.size, 1.0..=max.min(1000.0))
                .logarithmic(true)
                .text("Tamanho"),
        );
        ui.add(egui::Slider::new(&mut b.opacity, 0.01..=1.0).text("Opacidade"));
        ui.add(egui::Slider::new(&mut b.flow, 0.01..=1.0).text("Fluxo"));
        ui.add(egui::Slider::new(&mut b.hardness, 0.0..=1.0).text("Dureza"));
        let s = &mut b.smoothing;
        if ui
            .add(egui::Slider::new(&mut s.amount, 0.0..=1.0).text("Estabilizador"))
            .changed()
        {
            s.enabled = s.amount > 0.0;
        }
        ui.checkbox(use_pressure, "Usar pressão da caneta");

        ui.separator();
        ui.heading("Cor");
        let mut c = app.color;
        egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque);
        app.color = c;
        ui.horizontal_wrapped(|ui| {
            for rc in app.recent_colors.clone() {
                let (rect, resp) =
                    ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
                ui.painter().rect_filled(rect, 4.0, rc);
                if rc == app.color {
                    ui.painter().rect_stroke(
                        rect,
                        4.0,
                        egui::Stroke::new(2.0, Color32::WHITE),
                        egui::StrokeKind::Outside,
                    );
                }
                if resp.clicked() {
                    app.color = rc;
                }
            }
        });

        if app.tool == Tool::Fill {
            ui.separator();
            ui.add(
                egui::Slider::new(&mut app.fill_tolerance, 0.0..=1.0).text("Tolerância do balde"),
            );
        }

        ui.separator();
        ui.heading("Pincéis");
        let mut group = String::new();
        let mut picked = None;
        for (i, entry) in app.brushes.iter().enumerate() {
            if entry.group != group {
                group.clone_from(&entry.group);
                ui.label(RichText::new(&group).strong());
            }
            let selected = app.tool == Tool::Brush && app.brush_index == i;
            if ui.selectable_label(selected, &entry.name).clicked() {
                picked = Some(i);
            }
        }
        if let Some(i) = picked {
            app.select_brush(i);
        }
    });
}

pub fn layers_panel(app: &mut ArtApp, ui: &mut egui::Ui) {
    ui.heading("Camadas");
    ui.horizontal_wrapped(|ui| {
        if ui.button("+ Nova").clicked() {
            app.doc.add_layer();
        }
        if ui.button("Duplicar").clicked() {
            app.doc.duplicate_layer();
            app.invalidate_all();
        }
        if ui.button("Apagar").clicked() {
            app.doc.delete_layer();
            app.invalidate_all();
        }
        if ui.button("Subir").clicked() {
            app.doc.move_layer(true);
            app.invalidate_all();
        }
        if ui.button("Descer").clicked() {
            app.doc.move_layer(false);
            app.invalidate_all();
        }
        if ui
            .add_enabled(app.doc.active > 0, egui::Button::new("Mesclar abaixo"))
            .clicked()
        {
            app.doc.merge_down();
            app.invalidate_all();
        }
        if ui.button("Limpar").clicked() {
            app.doc.clear_layer();
            app.invalidate_all();
        }
    });
    ui.separator();

    let mut changed = false;
    let mut select = None;
    let mut toggle = None;
    let active = app.doc.active;
    // A camada de cima aparece primeiro, como em todo app de desenho.
    for i in (0..app.doc.layers.len()).rev() {
        let Some(layer) = app.doc.layers.get(i) else {
            continue;
        };
        ui.horizontal(|ui| {
            if ui
                .selectable_label(layer.visible, if layer.visible { "Ver" } else { "Oculta" })
                .on_hover_text("Mostrar/ocultar")
                .clicked()
            {
                toggle = Some(i);
            }
            if ui.selectable_label(i == active, &layer.name).clicked() {
                select = Some(i);
            }
        });
    }
    if let Some(i) = toggle {
        app.doc.checkpoint();
        if let Some(l) = app.doc.layers.get_mut(i) {
            l.visible = !l.visible;
        }
        changed = true;
    }
    if let Some(i) = select {
        app.doc.active = i;
    }

    ui.separator();
    let mut checkpoint = false;
    if let Some(layer) = app.doc.layers.get_mut(app.doc.active) {
        ui.label(RichText::new("Camada ativa").strong());
        ui.text_edit_singleline(&mut layer.name);
        let mut pct = layer.opacity * 100.0;
        let r = ui.add(
            egui::Slider::new(&mut pct, 0.0..=100.0)
                .suffix("%")
                .text("Opacidade"),
        );
        checkpoint |= r.drag_started();
        if r.changed() {
            layer.opacity = pct / 100.0;
            changed = true;
        }
        let mut blend = layer.blend;
        egui::ComboBox::from_label("Mesclagem")
            .selected_text(blend.label())
            .show_ui(ui, |ui| {
                for m in BlendMode::layer_modes().filter(|m| *m != BlendMode::Dissolve) {
                    ui.selectable_value(&mut blend, m, m.label());
                }
            });
        if blend != layer.blend {
            checkpoint = true;
        }
        let mut lock = layer.lock_alpha;
        ui.checkbox(&mut lock, "Bloquear transparência");
        if checkpoint || lock != layer.lock_alpha {
            // O histórico guarda o estado de antes da mudança.
            let (b, l) = (blend, lock);
            app.doc.checkpoint();
            if let Some(layer) = app.doc.active_layer_mut() {
                layer.blend = b;
                layer.lock_alpha = l;
            }
            changed = true;
        }
    }
    if changed {
        app.invalidate_all();
    }
}

pub fn new_canvas_dialog(app: &mut ArtApp, ctx: &egui::Context) {
    let max = ctx
        .input(|i| i.max_texture_side)
        .min(artapp_core::document::MAX_CANVAS_SIDE as usize) as u32;
    let mut open = true;
    let mut create = None;
    egui::Window::new("Nova tela")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("O desenho atual será descartado. Exporte antes se quiser guardá-lo.");
            for (name, w, h) in CANVAS_PRESETS {
                let fits = w <= max && h <= max;
                if ui.add_enabled(fits, egui::Button::new(name)).clicked() {
                    create = Some((w, h));
                }
            }
            ui.label(format!("Tamanho máximo neste aparelho: {max} px"));
        });
    if let Some((w, h)) = create {
        app.new_canvas(w, h);
        app.show_new_dialog = false;
    } else if !open {
        app.show_new_dialog = false;
    }
}
