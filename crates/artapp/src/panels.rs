//! A interface em volta da tela: barra de cima, trilho de ferramentas, sliders de tamanho e
//! opacidade, painel do pincel, painel de camadas, seletor de cor, avisos e o diálogo de nova tela.
//!
//! Tudo flutua sobre a tela de desenho (que ocupa a janela inteira), como no protótipo feito no
//! Lovable. Em celular na vertical, as ferramentas vão para uma barra embaixo e os painéis viram
//! folhas que sobem de baixo.

use artapp_core::BlendMode;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Id, Order, Pos2, RichText, Sense, Stroke, TextureHandle,
    Vec2,
};

use crate::app::{ArtApp, CANVAS_PRESETS, Tool, photocraft_max_size};
use crate::theme::{self, color, icon, icon_button, panel_frame, section};

const GAP: f32 = 10.0;
const PANEL_W: f32 = 290.0;
const RAIL_W: f32 = 56.0;
/// Altura do trilho em uma coluna: as ferramentas, 3 separadores e as cores.
const RAIL_H: f32 = (Tool::RASTER.len() + Tool::SELECTION.len() + Tool::VECTOR.len() + 1) as f32
    * (theme::TOUCH + 2.0)
    + 50.0;

/// Colunas do trilho: duas quando uma não cabe na altura (como a paleta de ferramentas do Clip
/// Studio em telas baixas).
fn rail_cols(canvas: egui::Rect) -> usize {
    if RAIL_H > canvas.height() - 2.0 * GAP {
        2
    } else {
        1
    }
}

/// Largura ocupada pelo trilho.
fn rail_w(canvas: egui::Rect) -> f32 {
    RAIL_W + (rail_cols(canvas) - 1) as f32 * (theme::TOUCH + 2.0)
}
const THUMB: Vec2 = Vec2::new(40.0, 40.0);

/// Celular em pé: barra de ferramentas embaixo e painéis como folhas inferiores.
pub fn is_portrait(ctx: &egui::Context) -> bool {
    let r = ctx.content_rect();
    r.width() < 720.0 && r.height() > r.width()
}

/// Tela de celular, em pé ou deitado (o lado menor é curto): só cabe um painel aberto por vez e a
/// barra de cima fica enxuta.
pub fn is_compact(ctx: &egui::Context) -> bool {
    let r = ctx.content_rect();
    r.width().min(r.height()) < 600.0
}

// ---------- Barra de cima ----------

pub fn top_bar(app: &mut ArtApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.menu_button(
            RichText::new(icon::MENU.to_string()).font(theme::icon_font(20.0)),
            |ui| file_menu(app, ui),
        );
        ui.add_enabled_ui(app.doc.can_undo(), |ui| {
            if icon_button(ui, icon::UNDO, false, "Desfazer (Ctrl+Z)").clicked() {
                app.undo();
            }
        });
        ui.add_enabled_ui(app.doc.can_redo(), |ui| {
            if icon_button(ui, icon::REDO, false, "Refazer (Ctrl+Shift+Z)").clicked() {
                app.redo();
            }
        });

        let compact = is_compact(ui.ctx());
        let title = if compact {
            format!("{:.0}%", app.view.zoom * 100.0)
        } else {
            format!(
                "{}{}  ·  {} × {}  ·  {:.0}%",
                app.projects.name,
                if app.projects.dirty() { " •" } else { "" },
                app.doc.width,
                app.doc.height,
                app.view.zoom * 100.0
            )
        };
        let right_w = if compact { 3.0 } else { 6.0 } * (theme::TOUCH + 2.0);
        let center_w = (ui.available_width() - right_w).max(0.0);
        ui.allocate_ui_with_layout(
            Vec2::new(center_w, theme::TOUCH),
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                ui.label(RichText::new(title).color(color::TEXT_MUTED).size(13.0));
            },
        );

        if !compact {
            view_buttons(app, ui);
        }
        if icon_button(
            ui,
            icon::MAXIMIZE,
            false,
            "Encaixar a tela (e desfazer o giro)",
        )
        .clicked()
        {
            app.view.request_fit();
        }
        if icon_button(ui, icon::SLIDERS, app.show_tools_panel, "Pincel").clicked() {
            app.show_tools_panel = !app.show_tools_panel;
            if compact && app.show_tools_panel {
                app.show_layers_panel = false;
            }
        }
        if icon_button(ui, icon::LAYERS, app.show_layers_panel, "Camadas").clicked() {
            app.show_layers_panel = !app.show_layers_panel;
            if compact && app.show_layers_panel {
                app.show_tools_panel = false;
            }
        }
    });
}

/// Girar e espelhar a vista (o "Navegador" do Clip Studio). Só muda a visualização.
fn view_buttons(app: &mut ArtApp, ui: &mut egui::Ui) {
    let canvas = app.canvas_rect;
    let step = 15f32.to_radians();
    if icon_button(ui, icon::ROTATE_CCW, false, "Girar a vista para a esquerda").clicked() {
        app.view.rotate_at(canvas, canvas.center(), -step);
    }
    if icon_button(ui, icon::ROTATE_CW, false, "Girar a vista para a direita").clicked() {
        app.view.rotate_at(canvas, canvas.center(), step);
    }
    if icon_button(ui, icon::FLIP_H, app.view.flip, "Espelhar a vista").clicked() {
        app.view.toggle_flip(canvas);
    }
}

/// Ajustes de caneta e gestos (gravados entre as sessões).
fn pen_settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    ui.separator();
    ui.label(
        RichText::new("Caneta e gestos")
            .size(13.0)
            .color(color::TEXT_MUTED),
    );
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Botão da caneta");
        let name = app.pen_button_tool.map_or("Nada", Tool::label);
        egui::ComboBox::from_id_salt("pen_button_tool")
            .selected_text(name)
            .show_ui(ui, |ui| {
                for t in Tool::PEN_BUTTON {
                    changed |= ui
                        .selectable_value(&mut app.pen_button_tool, Some(t), t.label())
                        .changed();
                }
                changed |= ui
                    .selectable_value(&mut app.pen_button_tool, None, "Nada")
                    .changed();
            });
    });
    ui.horizontal(|ui| {
        ui.label("Dedo");
        egui::ComboBox::from_id_salt("finger_mode")
            .selected_text(app.finger_mode.label())
            .show_ui(ui, |ui| {
                for m in crate::app::FingerMode::ALL {
                    changed |= ui
                        .selectable_value(&mut app.finger_mode, m, m.label())
                        .changed();
                }
            });
    })
    .response
    .on_hover_text(
        "Automático: o dedo desenha até a caneta tocar a tela; depois só a caneta desenha e o dedo \
         fica para zoom, giro e desfazer com dois dedos. A mão apoiada enquanto a caneta está na \
         tela é ignorada.",
    );
    for (value, text) in [
        (&mut app.gesture_rotate, "Girar com dois dedos"),
        (&mut app.tap_undo, "2 dedos desfaz, 3 refaz"),
        (&mut app.quick_shape, "Forma rápida (segurar no fim)"),
        (&mut app.hold_eyedropper, "Segurar o dedo pega a cor"),
        (
            &mut app.hold_quickmenu,
            "Segurar o dedo abre o QuickMenu (tecla F)",
        ),
        (&mut app.autoclose_panels, "Fechar painéis ao desenhar"),
    ] {
        changed |= ui
            .add_sized([210.0, 36.0], |ui: &mut egui::Ui| {
                theme::switch(ui, value, text)
            })
            .changed();
    }
    if changed {
        app.save_prefs();
    }
}

fn file_menu(app: &mut ArtApp, ui: &mut egui::Ui) {
    // O menu cresceu: rola quando não cabe na altura da tela.
    let max_h = (ui.ctx().content_rect().height() - 80.0).max(200.0);
    egui::ScrollArea::vertical()
        .max_height(max_h)
        .show(ui, |ui| file_menu_items(app, ui));
}

fn file_menu_items(app: &mut ArtApp, ui: &mut egui::Ui) {
    ui.set_min_width(220.0);
    let item = |ui: &mut egui::Ui, glyph: char, text: &str| {
        ui.add(
            egui::Button::new(RichText::new(format!("{glyph}   {text}")).size(15.0))
                .frame(false)
                .min_size(Vec2::new(210.0, 36.0)),
        )
    };
    if item(ui, icon::FILE_PLUS, "Nova tela…").clicked() {
        app.show_new_dialog = true;
        ui.close();
    }
    if item(ui, icon::FOLDER_OPEN, "Abrir projeto ou PSD…").clicked() {
        app.open_project_dialog();
        ui.close();
    }
    if item(ui, icon::IMAGE, "Importar imagem como camada…").clicked() {
        app.import_image_dialog();
        ui.close();
    }
    if item(ui, icon::SAVE, "Salvar projeto").clicked() {
        app.save_project();
        ui.close();
    }
    if item(ui, icon::SAVE, "Salvar como…").clicked() {
        app.save_project_as();
        ui.close();
    }
    ui.add_sized([210.0, 36.0], |ui: &mut egui::Ui| {
        theme::switch(ui, &mut app.projects.autosave, "Salvar automaticamente")
    });
    ui.separator();
    if item(ui, icon::IMAGE_DOWN, "Exportar PNG").clicked() {
        app.export_png(true);
        ui.close();
    }
    if item(ui, icon::IMAGE, "Exportar PNG transparente").clicked() {
        app.export_png(false);
        ui.close();
    }
    if item(ui, icon::IMAGE_DOWN, "Exportar JPEG").clicked() {
        app.export_jpeg();
        ui.close();
    }
    if item(ui, icon::LAYERS, "Exportar PSD (camadas)…").clicked() {
        app.export_psd();
        ui.close();
    }
    if crate::platform::update::SUPPORTED {
        if item(ui, icon::ROTATE_CW, "Atualizar app").clicked() {
            app.check_update();
            ui.close();
        }
        ui.label(
            RichText::new(format!("Build instalado: {}", app.updater.installed()))
                .size(12.0)
                .color(crate::theme::color::TEXT_MUTED),
        );
    }
    ui.separator();
    let canvas = app.canvas_rect;
    let step = 15f32.to_radians();
    if item(ui, icon::ROTATE_CCW, "Girar vista à esquerda").clicked() {
        app.view.rotate_at(canvas, canvas.center(), -step);
    }
    if item(ui, icon::ROTATE_CW, "Girar vista à direita").clicked() {
        app.view.rotate_at(canvas, canvas.center(), step);
    }
    if item(ui, icon::FLIP_H, "Espelhar vista").clicked() {
        app.view.toggle_flip(canvas);
        ui.close();
    }
    ui.add_sized([210.0, 36.0], |ui: &mut egui::Ui| {
        theme::switch(ui, &mut app.show_navigator, "Navegador")
    });
    ui.add_sized([210.0, 36.0], |ui: &mut egui::Ui| {
        theme::switch(ui, &mut app.show_grid, "Grade")
    });
    if app.show_grid {
        ui.add(
            egui::Slider::new(&mut app.grid_size, 8.0..=512.0)
                .logarithmic(true)
                .suffix(" px")
                .text("Grade"),
        );
    }
    pen_settings(app, ui);
    ui.separator();
    if item(ui, icon::SQUARE_DASHED, "Selecionar tudo").clicked() {
        app.select_all();
        ui.close();
    }
    if app.doc.selection.is_some() {
        if item(ui, icon::DESELECT, "Desmarcar").clicked() {
            app.deselect();
            ui.close();
        }
        if item(ui, icon::INVERT, "Inverter seleção").clicked() {
            app.invert_selection();
            ui.close();
        }
    }
    ui.separator();
    if item(ui, icon::SCISSORS, "Recortar (Ctrl+X)").clicked() {
        app.cut();
        ui.close();
    }
    if item(ui, icon::COPY, "Copiar (Ctrl+C)").clicked() {
        app.copy();
        ui.close();
    }
    if item(ui, icon::PLUS, "Colar em camada nova (Ctrl+V)").clicked() {
        app.paste();
        ui.close();
    }
    if item(ui, icon::PALETTE, "Mudar cor da linha para a principal").clicked() {
        app.line_to_color();
        ui.close();
    }
    if item(ui, icon::SUN_DIM, "Converter brilho em opacidade").clicked() {
        app.brightness_to_opacity();
        ui.close();
    }
    ui.separator();
    if item(ui, icon::SCALING, "Transformar (Ctrl+T)").clicked() {
        app.start_transform();
        ui.close();
    }
    if item(ui, icon::SCALING, "Transformar por valores…").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::transform());
        ui.close();
    }
    if item(ui, icon::SCALING, "Tamanho da tela…").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::canvas_size(
            app.doc.width,
            app.doc.height,
        ));
        ui.close();
    }
    if app.doc.selection.is_some() && item(ui, icon::CROP, "Cortar na seleção").clicked() {
        app.crop_to_selection();
        ui.close();
    }
    if item(ui, icon::SPARKLES, "Filtros…").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::filter(0));
        ui.close();
    }
    if item(ui, icon::CONTRAST, "Correção de cor…").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::adjust());
        ui.close();
    }
}

// ---------- Ferramentas ----------

fn tool_buttons(app: &mut ArtApp, ui: &mut egui::Ui, vertical: bool, cols: usize) {
    let separator = |ui: &mut egui::Ui| {
        if vertical {
            ui.add(egui::Separator::default().horizontal().spacing(6.0));
        } else {
            ui.add(egui::Separator::default().vertical().spacing(6.0));
        }
    };
    for (i, group) in [&Tool::RASTER[..], &Tool::SELECTION[..], &Tool::VECTOR[..]]
        .into_iter()
        .enumerate()
    {
        if i > 0 {
            separator(ui);
        }
        let mut buttons = |ui: &mut egui::Ui| {
            for &t in group {
                if icon_button(ui, t.icon(), app.tool == t, t.label()).clicked() {
                    if app.tool == t {
                        // Tocar de novo na ferramenta escolhida abre (ou fecha) os ajustes dela.
                        app.show_tools_panel = !app.show_tools_panel;
                        if app.show_tools_panel && is_compact(ui.ctx()) {
                            app.show_layers_panel = false;
                        }
                    } else {
                        app.set_tool(t);
                    }
                }
            }
        };
        if vertical && cols > 1 {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(2.0);
                buttons(ui)
            });
        } else {
            buttons(ui);
        }
    }
}

/// As cores principal/secundária/transparente. Fica fora da rolagem das ferramentas para estar
/// sempre à vista: tocar na principal abre o seletor de cor.
fn color_button(app: &mut ArtApp, ui: &mut egui::Ui) {
    match theme::color_chips(ui, app.color, app.sub_color, app.transparent) {
        Some(theme::ColorChip::Main) => {
            if app.transparent {
                app.transparent = false;
            } else if app.color_float {
                // Com a roda flutuante, a cor principal recolhe/abre a roda.
                app.color_float_collapsed = !app.color_float_collapsed;
                app.save_prefs();
            } else {
                app.show_color = !app.show_color;
            }
        }
        Some(theme::ColorChip::Sub) => app.swap_colors(),
        Some(theme::ColorChip::Transparent) => app.transparent = !app.transparent,
        Some(theme::ColorChip::Drop(at)) => crate::canvas::color_drop(app, at),
        None => {}
    }
}

/// Trilho vertical de ferramentas, à esquerda (tablet/desktop e celular deitado). As cores ficam
/// no topo, fixas; as ferramentas rolam quando não cabem na altura.
pub fn tool_rail(app: &mut ArtApp, ctx: &egui::Context, canvas: egui::Rect) {
    let cols = rail_cols(canvas);
    // Fileiras de botões por grupo, mais as cores, os separadores e as margens.
    let rows: usize = [
        Tool::RASTER.len(),
        Tool::SELECTION.len(),
        Tool::VECTOR.len(),
    ]
    .iter()
    .map(|n| n.div_ceil(cols))
    .sum();
    let rail_h = (rows + 1) as f32 * (theme::TOUCH + 2.0) + 3.0 * 16.0 + 16.0;
    let h = rail_h.min(canvas.height() - 2.0 * GAP);
    // Centralizado na altura, como no protótipo; encosta no topo se a tela for baixa.
    let y = (canvas.center().y - h / 2.0).max(canvas.min.y + GAP);
    egui::Area::new(Id::unique("rail"))
        .order(Order::Foreground)
        .fixed_pos(Pos2::new(canvas.min.x + GAP, y))
        .show(ctx, |ui| {
            panel_frame()
                .inner_margin(egui::Margin::same(4))
                .show(ui, |ui| {
                    ui.set_width(rail_w(canvas) - 8.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.vertical_centered(|ui| {
                        color_button(app, ui);
                        ui.add(egui::Separator::default().horizontal().spacing(6.0));
                        egui::ScrollArea::vertical()
                            .max_height((h - theme::TOUCH - 24.0).max(theme::TOUCH))
                            .show(ui, |ui| tool_buttons(app, ui, true, cols));
                    });
                });
        });
}

/// Barra de ferramentas horizontal embaixo (celular em pé). As cores ficam fixas à esquerda e as
/// ferramentas rolam para o lado.
pub fn bottom_tools(app: &mut ArtApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        color_button(app, ui);
        ui.add(egui::Separator::default().vertical().spacing(6.0));
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                tool_buttons(app, ui, false, 1);
            });
        });
    });
}

/// Altura dos sliders laterais: menores quando a tela é baixa (celular deitado).
fn slider_h(canvas: egui::Rect) -> f32 {
    ((canvas.height() - 2.0 * GAP - 12.0) / 2.0).clamp(70.0, 144.0)
}

/// Sliders verticais de tamanho e opacidade colados na borda esquerda, como no Procreate.
pub fn side_sliders(app: &mut ArtApp, ctx: &egui::Context, canvas: egui::Rect, portrait: bool) {
    if !matches!(app.tool, Tool::Brush | Tool::Eraser | Tool::Blend) && !app.tool.is_vector() {
        return;
    }
    let h = slider_h(canvas);
    let x = if portrait {
        canvas.min.x + 6.0
    } else {
        canvas.min.x + GAP + rail_w(canvas) + 8.0
    };
    let y = if portrait {
        canvas.min.y + GAP
    } else {
        (canvas.center().y - h - 6.0).max(canvas.min.y + GAP)
    };
    egui::Area::new(Id::unique("sliders"))
        .order(Order::Foreground)
        .fixed_pos(Pos2::new(x, y))
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 12.0;
            let max = photocraft_max_size().min(1000.0);
            if app.tool.is_vector() {
                let mut w = app.vector_width as f32;
                if theme::vslider(ui, &mut w, 0.5..=200.0, true, h, "Espessura", |v| {
                    format!("{v:.1}")
                })
                .changed()
                {
                    app.vector_width = f64::from(w);
                }
                return;
            }
            if app.tool == Tool::Blend {
                theme::vslider(
                    ui,
                    &mut app.mix_brush.size,
                    1.0..=max,
                    true,
                    h,
                    "Tamanho",
                    |v| format!("{v:.0} px"),
                );
                let mut pct = app.mix_strength * 100.0;
                if theme::vslider(ui, &mut pct, 1.0..=100.0, false, h, "Força", |v| {
                    format!("{v:.0}%")
                })
                .changed()
                {
                    app.mix_strength = pct / 100.0;
                }
                return;
            }
            let b = if app.tool == Tool::Eraser {
                &mut app.eraser
            } else {
                &mut app.brush
            };
            theme::vslider(ui, &mut b.size, 1.0..=max, true, h, "Tamanho", |v| {
                format!("{v:.0} px")
            });
            let mut pct = b.opacity * 100.0;
            if theme::vslider(ui, &mut pct, 1.0..=100.0, false, h, "Opacidade", |v| {
                format!("{v:.0}%")
            })
            .changed()
            {
                b.opacity = pct / 100.0;
            }
        });
}

// ---------- Painéis ----------

/// Posição e tamanho de um painel: ao lado do trilho/à direita no tablet, folha de baixo no celular
/// em pé. Todo painel tem título com botão de fechar, e o conteúdo rola quando não cabe na altura
/// (celular deitado). `add` recebe a altura útil do painel. Devolve `true` se pediram para fechar.
#[allow(clippy::too_many_arguments)]
fn panel_area(
    id: &str,
    ctx: &egui::Context,
    canvas: egui::Rect,
    portrait: bool,
    right: bool,
    glyph: char,
    title: &str,
    add: impl FnOnce(&mut egui::Ui, f32),
) -> bool {
    let (pos, width, max_h) = if portrait {
        let h = canvas.height() * 0.55;
        (
            Pos2::new(canvas.min.x + 6.0, canvas.max.y - h - 6.0),
            canvas.width() - 12.0,
            h - 24.0,
        )
    } else if right {
        let w = PANEL_W.min(canvas.width() * 0.45);
        (
            Pos2::new(canvas.max.x - w - GAP, canvas.min.y + GAP),
            w,
            canvas.height() - 2.0 * GAP - 24.0,
        )
    } else {
        // Ao lado do trilho e dos sliders.
        let x = canvas.min.x + GAP + rail_w(canvas) + 56.0;
        (
            Pos2::new(x, canvas.min.y + GAP),
            PANEL_W.min(canvas.max.x - x - GAP),
            canvas.height() - 2.0 * GAP - 24.0,
        )
    };
    let max_h = max_h.max(120.0);
    let mut close = false;
    egui::Area::new(Id::unique(id))
        .order(Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            panel_frame().show(ui, |ui| {
                let inner = width - 24.0;
                ui.set_min_width(inner);
                ui.set_max_width(inner);
                ui.set_max_height(max_h);
                ui.spacing_mut().slider_width = (inner - 160.0).max(80.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(glyph.to_string())
                            .font(theme::icon_font(18.0))
                            .color(color::ACCENT),
                    );
                    ui.heading(title);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close =
                            theme::icon_button_sized(ui, icon::X, false, "Fechar", 36.0).clicked();
                    });
                });
                ui.separator();
                let body_h = max_h - 48.0;
                egui::ScrollArea::vertical()
                    .id_salt(id)
                    .max_height(body_h)
                    .auto_shrink([false, true])
                    .show(ui, |ui| add(ui, body_h));
            });
        });
    close
}

pub fn brush_panel(app: &mut ArtApp, ctx: &egui::Context, canvas: egui::Rect, portrait: bool) {
    let close = panel_area(
        "brush_panel",
        ctx,
        canvas,
        portrait,
        false,
        app.tool.icon(),
        app.tool.label(),
        |ui, _| {
            if app.tool == Tool::VectorKit {
                kit_settings(app, ui);
            } else if app.tool.is_vector() {
                vector_settings(app, ui);
            } else if app.tool == Tool::Fill {
                section(ui, "Balde");
                theme::switch(ui, &mut app.fill_recolor, "Recolorir (troca a cor tocada)");
                if app.fill_recolor {
                    theme::switch(ui, &mut app.wand_contiguous, "Só a área ligada ao toque");
                }
                ui.add(egui::Slider::new(&mut app.fill_tolerance, 0.0..=1.0).text("Tolerância"));
                ui.add(
                    egui::Slider::new(&mut app.fill_close_gap, 0.0..=40.0)
                        .suffix(" px")
                        .text("Fechar lacunas"),
                );
                ui.add(
                    egui::Slider::new(&mut app.fill_expand, -10.0..=10.0)
                        .suffix(" px")
                        .text("Expandir área"),
                );
                ui.label(RichText::new("Pinta a área de cor parecida, olhando todas as camadas. Em camada vetorial, pinta o preenchimento da forma tocada.").size(12.0).color(color::TEXT_MUTED));
            } else if app.tool == Tool::Blend {
                mix_settings(app, ui);
            } else if app.tool == Tool::Liquify {
                liquify_settings(app, ui);
            } else if app.tool == Tool::Ruler {
                ruler_settings(app, ui);
            } else if app.tool == Tool::Gradient {
                gradient_settings(app, ui);
            } else if app.tool == Tool::MagicWand {
                wand_settings(app, ui);
            } else if app.tool.is_selection() || app.tool == Tool::MoveLayer {
                selection_settings(app, ui);
            } else if matches!(app.tool, Tool::Eyedropper | Tool::Hand) {
                ui.label(RichText::new(if app.tool == Tool::Hand { "Arraste para mover a tela. Dois dedos: zoom e mover em qualquer ferramenta." } else { "Toque na tela para pegar a cor." }).color(color::TEXT_MUTED));
            } else {
                brush_settings(app, ui);
            }
        },
    );
    if close {
        app.show_tools_panel = false;
    }
}

fn mix_settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    use artapp_core::mix::MixKind;
    section(ui, "Modo");
    ui.horizontal_wrapped(|ui| {
        for (kind, name) in [
            (MixKind::Smudge, "Dedo"),
            (MixKind::Blur, "Desfocar"),
            (MixKind::Sharpen, "Nitidez"),
            (MixKind::Dodge, "Clarear"),
            (MixKind::Burn, "Escurecer"),
            (MixKind::Saturate, "Saturar"),
            (MixKind::Desaturate, "Dessaturar"),
            (MixKind::Clone, "Carimbo"),
            (MixKind::Heal, "Corretivo"),
            (MixKind::BgErase, "Borracha de fundo"),
        ] {
            ui.selectable_value(&mut app.mix_kind, kind, name);
        }
    });
    ui.label(
        RichText::new(match app.mix_kind {
            MixKind::Smudge => "Arrasta a tinta pelo caminho, como o dedo.",
            MixKind::Blur => "Suaviza as cores e as bordas embaixo do pincel.",
            MixKind::Sharpen => "Realça as bordas e os detalhes embaixo do pincel.",
            MixKind::Dodge => "Clareia os meios-tons, mantendo o matiz (subexpor).",
            MixKind::Burn => "Escurece os meios-tons, mantendo o matiz (superexpor).",
            MixKind::Saturate => "Deixa as cores mais vivas (esponja).",
            MixKind::Desaturate => "Puxa as cores para o cinza (esponja).",
            MixKind::Clone => "Toque para marcar a origem; depois pinte para copiar dela.",
            MixKind::Heal => {
                "Pinte por cima da mancha; ao soltar, ela some, refeita com a textura em volta."
            }
            MixKind::BgErase => {
                "Apaga a cor que está sob o centro do pincel e as parecidas (a força é a tolerância), deixando as outras."
            }
        })
        .size(12.5)
        .color(color::TEXT_MUTED),
    );
    if app.mix_kind == MixKind::Clone
        && ui
            .selectable_label(
                app.clone_picking,
                format!("{}  Marcar nova origem", icon::STAMP),
            )
            .clicked()
    {
        app.clone_picking = !app.clone_picking;
    }
    section(ui, "Ajustes");
    let max = photocraft_max_size().min(1000.0);
    ui.add(
        egui::Slider::new(&mut app.mix_brush.size, 1.0..=max)
            .logarithmic(true)
            .text("Tamanho"),
    );
    let mut pct = app.mix_strength * 100.0;
    if ui
        .add(
            egui::Slider::new(&mut pct, 1.0..=100.0)
                .suffix("%")
                .text("Força"),
        )
        .changed()
    {
        app.mix_strength = pct / 100.0;
    }
    ui.add(egui::Slider::new(&mut app.mix_brush.hardness, 0.0..=1.0).text("Dureza"));
    let mut none = artapp_core::Taper::default();
    crate::brush_editor::show(ui, &mut app.mix_brush, &mut none);
}

/// Comandos do VectorCraft que valem um botão, sem diálogo: (id, nome).
const KIT_COMMANDS: &[(&str, &[(&str, &str)])] = &[
    (
        "Objeto",
        &[
            ("object.group", "Agrupar"),
            ("object.ungroup", "Desagrupar"),
            ("edit.duplicate", "Duplicar"),
            ("edit.clear", "Apagar"),
            ("select.all", "Selecionar tudo"),
            ("select.none", "Desmarcar"),
            ("object.transformAgain", "Repetir transformação"),
        ],
    ),
    (
        "Ordem",
        &[
            ("object.arrange.bringToFront", "Para a frente"),
            ("object.arrange.bringForward", "Avançar"),
            ("object.arrange.sendBackward", "Recuar"),
            ("object.arrange.sendToBack", "Para trás"),
        ],
    ),
    (
        "Combinar formas",
        &[
            ("object.pathfinder.unite", "Unir"),
            ("object.pathfinder.minusFront", "Subtrair"),
            ("object.pathfinder.intersect", "Interseção"),
            ("object.pathfinder.exclude", "Excluir sobreposição"),
            ("object.pathfinder.divide", "Dividir"),
            ("object.pathfinder.outline", "Contorno"),
        ],
    ),
    (
        "Traçado",
        &[
            ("path.join", "Unir pontas"),
            ("object.path.outlineStroke", "Traço em forma"),
            ("object.compoundPath.make", "Traçado composto"),
            ("object.compoundPath.release", "Soltar composto"),
            ("object.clippingMask.make", "Máscara de recorte"),
            ("object.clippingMask.release", "Soltar máscara"),
            ("object.expand", "Expandir"),
            ("effect.expandAppearance", "Expandir aparência"),
        ],
    ),
];

fn kit_settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    use artapp_core::vectorkit::{ToolKey, groups, label_pt};
    ui.label(
        RichText::new(
            "Todas as ferramentas do VectorCraft. Trabalham na camada vetorial ativa (uma é criada se precisar). A cor do traço é a principal; preenchimento e espessura vêm de Vetor.",
        )
        .size(12.5)
        .color(color::TEXT_MUTED),
    );
    ui.horizontal_wrapped(|ui| {
        if ui.button("Concluir").clicked() {
            app.kit_key(ToolKey::Enter);
        }
        if ui.button("Cancelar").clicked() {
            app.kit_key(ToolKey::Escape);
        }
        if ui.button(format!("{}  Apagar", icon::TRASH)).clicked() {
            app.kit_key(ToolKey::Delete);
        }
    });
    let options = app.kit.options();
    if let Some(map) = options
        .as_object()
        .filter(|m| m.values().any(|v| v.is_number() || v.is_boolean()))
    {
        section(ui, "Opções da ferramenta");
        for (key, v) in map {
            let mut changed = None;
            match v {
                serde_json::Value::Bool(b) => {
                    let mut b = *b;
                    if ui.checkbox(&mut b, key.as_str()).changed() {
                        changed = Some(serde_json::Value::Bool(b));
                    }
                }
                serde_json::Value::Number(n) => {
                    let mut x = n.as_f64().unwrap_or(0.0);
                    ui.horizontal(|ui| {
                        if ui.add(egui::DragValue::new(&mut x).speed(0.5)).changed() {
                            changed =
                                serde_json::Number::from_f64(x).map(serde_json::Value::Number);
                        }
                        ui.label(key.as_str());
                    });
                }
                _ => {}
            }
            if let Some(v) = changed {
                app.kit.set_option(key, &v);
            }
        }
    }
    for group in groups() {
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
            for t in group {
                let name = label_pt(t.id);
                if name.is_empty() {
                    continue;
                }
                if ui.selectable_label(app.kit_tool == t.id, name).clicked() {
                    app.set_kit_tool(t.id);
                }
            }
        });
        ui.separator();
    }
    for (title, cmds) in KIT_COMMANDS {
        section(ui, title);
        ui.horizontal_wrapped(|ui| {
            for (id, name) in *cmds {
                if ui.button(*name).clicked() {
                    app.kit_command(id, serde_json::json!({}));
                }
            }
        });
    }
}

/// Com a interface escondida: um botão discreto no canto para trazê-la de volta.
pub fn show_ui_button(app: &mut ArtApp, ctx: &egui::Context, canvas: egui::Rect) {
    egui::Area::new(Id::unique("show_ui"))
        .order(Order::Foreground)
        .fixed_pos(canvas.min + Vec2::splat(8.0))
        .show(ctx, |ui| {
            ui.set_opacity(0.55);
            if icon_button(
                ui,
                icon::MAXIMIZE,
                false,
                "Mostrar a interface (Tab ou 4 dedos)",
            )
            .clicked()
            {
                app.hide_ui = false;
            }
        });
}

/// Navegador no canto de baixo à direita, ao lado do painel de camadas se ele estiver aberto.
pub fn navigator(app: &mut ArtApp, ctx: &egui::Context, canvas: egui::Rect, portrait: bool) {
    let corner = if portrait {
        // Em retrato os painéis abrem embaixo: o navegador fica no alto.
        Pos2::new(canvas.max.x - GAP, canvas.min.y + GAP + 196.0)
    } else if app.show_layers_panel {
        Pos2::new(canvas.max.x - 2.0 * GAP - PANEL_W, canvas.max.y - GAP)
    } else {
        Pos2::new(canvas.max.x - GAP, canvas.max.y - GAP)
    };
    crate::canvas::navigator(app, ctx, corner);
}

fn ruler_settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    section(ui, "Tipo de régua");
    ui.horizontal_wrapped(|ui| {
        for (i, name) in ["Reta", "Paralelas", "Concêntrica", "Perspectiva"]
            .into_iter()
            .enumerate()
        {
            ui.selectable_value(&mut app.ruler_kind, i, name);
        }
    });
    ui.label(
        RichText::new(match app.ruler_kind {
            0 => "Arraste para traçar a reta. Arraste as pontas para ajustar.",
            1 => "Arraste na direção das linhas paralelas.",
            2 => "Toque no centro dos círculos. Arraste o centro para mover.",
            _ => "Cada toque põe um ponto de fuga (até três). Arraste para mover.",
        })
        .size(12.0)
        .color(color::TEXT_MUTED),
    );
    section(ui, "Uso");
    ui.checkbox(&mut app.ruler_snap, "Grudar o pincel na régua");
    ui.label(
        RichText::new("Vale para pincel, borracha e misturar. Desligue para desenhar livre sem perder a régua.")
            .size(12.0)
            .color(color::TEXT_MUTED),
    );
    if ui
        .add_enabled(app.ruler.is_some(), egui::Button::new("Apagar régua"))
        .clicked()
    {
        app.ruler = None;
    }
}

fn liquify_settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    section(ui, "Modo");
    ui.horizontal_wrapped(|ui| {
        for (tool, name) in artapp_core::liquify::TOOLS {
            ui.selectable_value(&mut app.liquify_tool, tool, name);
        }
    });
    section(ui, "Ajustes");
    ui.add(
        egui::Slider::new(&mut app.liquify_size, 5.0..=1000.0)
            .logarithmic(true)
            .text("Tamanho"),
    );
    let mut pct = app.liquify_pressure * 100.0;
    if ui
        .add(
            egui::Slider::new(&mut pct, 1.0..=100.0)
                .suffix("%")
                .text("Força"),
        )
        .changed()
    {
        app.liquify_pressure = pct / 100.0;
    }
    ui.label(
        RichText::new("Arraste sobre a camada raster ativa. Restaurar desfaz a deformação aos poucos onde passar.")
            .size(12.5)
            .color(color::TEXT_MUTED),
    );
}

fn gradient_settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    use artapp_core::edit::GradientShape;
    section(ui, "Forma");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.gradient_shape, GradientShape::Linear, "Linear");
        ui.selectable_value(&mut app.gradient_shape, GradientShape::Radial, "Radial");
    });
    section(ui, "Cores");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.gradient_to_sub, true, "Principal e secundária");
        ui.selectable_value(&mut app.gradient_to_sub, false, "Principal e transparente");
    });
    ui.label(
        RichText::new(
            "Arraste na tela do início ao fim. Com seleção ativa, só preenche dentro dela.",
        )
        .size(12.5)
        .color(color::TEXT_MUTED),
    );
}

fn wand_settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    section(ui, "Varinha mágica");
    let mut pct = app.wand_tolerance * 100.0;
    if ui
        .add(
            egui::Slider::new(&mut pct, 0.0..=100.0)
                .suffix("%")
                .text("Tolerância"),
        )
        .changed()
    {
        app.wand_tolerance = pct / 100.0;
    }
    theme::switch(ui, &mut app.wand_all_layers, "Olhar todas as camadas");
    theme::switch(ui, &mut app.wand_contiguous, "Só áreas contíguas");
    ui.label(
        RichText::new("Toque numa cor para selecionar a área parecida.")
            .size(12.5)
            .color(color::TEXT_MUTED),
    );
    ui.add_space(4.0);
    selection_settings(app, ui);
}

fn symmetry_settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    use artapp_core::paint::symmetry::SymmetryMode;
    section(ui, "Simetria");
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut app.symmetry, None, "Desligada");
        for (mode, name) in [
            (SymmetryMode::Vertical, "Vertical"),
            (SymmetryMode::Horizontal, "Horizontal"),
            (SymmetryMode::Dual, "Quatro lados"),
            (SymmetryMode::Diagonal, "Diagonal"),
        ] {
            ui.selectable_value(&mut app.symmetry, Some(mode), name);
        }
    });
    if app.symmetry.is_some() {
        let mut deg = app.symmetry_angle.to_degrees();
        if ui
            .add(
                egui::Slider::new(&mut deg, -180.0..=180.0)
                    .suffix("°")
                    .text("Ângulo"),
            )
            .changed()
        {
            app.symmetry_angle = deg.to_radians();
        }
    }
}

fn selection_settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    let hint = match app.tool {
        Tool::MoveLayer => "Arraste para mover o conteúdo da camada ativa.",
        Tool::Lasso => "Contorne a área com o dedo. Pincel, balde e limpar só agem dentro dela.",
        _ => {
            "Arraste para marcar a área. Um toque sem arrastar desmarca. Pincel, balde e limpar só agem dentro dela."
        }
    };
    ui.label(RichText::new(hint).size(12.5).color(color::TEXT_MUTED));
    if app.tool == Tool::MoveLayer && ui.button("Transformar com alças").clicked() {
        app.start_transform();
    }
    if app.tool != Tool::MoveLayer {
        use artapp_core::selection::SelectMode;
        section(ui, "Modo");
        ui.horizontal_wrapped(|ui| {
            for (mode, name) in [
                (SelectMode::New, "Nova"),
                (SelectMode::Add, "Adicionar"),
                (SelectMode::Subtract, "Subtrair"),
                (SelectMode::Intersect, "Interseção"),
            ] {
                ui.selectable_value(&mut app.select_mode, mode, name);
            }
        });
    }
    section(ui, "Seleção");
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(format!("{}  Tudo", icon::SQUARE_DASHED))
            .clicked()
        {
            app.select_all();
        }
        let has = app.doc.selection.is_some();
        if ui
            .add_enabled(
                has,
                egui::Button::new(format!("{}  Desmarcar", icon::DESELECT)),
            )
            .clicked()
        {
            app.deselect();
        }
        if ui
            .add_enabled(
                has,
                egui::Button::new(format!("{}  Inverter", icon::INVERT)),
            )
            .clicked()
        {
            app.invert_selection();
        }
        if ui.button(format!("{}  Da camada", icon::LAYERS)).clicked() {
            app.doc.select_layer_content();
        }
    });
    if app.doc.selection.is_some() {
        section(ui, "Ajustar seleção");
        ui.add(
            egui::Slider::new(&mut app.select_amount, 1.0..=100.0)
                .logarithmic(true)
                .suffix(" px")
                .text("Quanto"),
        );
        ui.horizontal_wrapped(|ui| {
            let n = app.select_amount;
            if ui.button("Expandir").clicked() {
                app.doc.grow_selection(n);
            }
            if ui.button("Contrair").clicked() {
                app.doc.grow_selection(-n);
            }
            if ui.button("Suavizar borda").clicked() {
                app.doc.feather_selection(n);
            }
        });
    }
}

/// Barra flutuante embaixo da seleção, como o "lançador de seleção" do Clip Studio.
pub fn selection_launcher(app: &mut ArtApp, ctx: &egui::Context, canvas: egui::Rect) {
    let Some(sel) = &app.doc.selection else {
        return;
    };
    let b = sel.bounds;
    let pts = [
        app.view.doc_to_screen(canvas, b.x0 as f32, b.y0 as f32),
        app.view.doc_to_screen(canvas, b.x1 as f32, b.y0 as f32),
        app.view.doc_to_screen(canvas, b.x1 as f32, b.y1 as f32),
        app.view.doc_to_screen(canvas, b.x0 as f32, b.y1 as f32),
    ];
    let bottom = pts.iter().map(|p| p.y).fold(f32::MIN, f32::max);
    let cx = pts.iter().map(|p| p.x).sum::<f32>() / 4.0;
    let w = 5.0 * (theme::TOUCH + 2.0) + 16.0;
    let h = theme::TOUCH + 16.0;
    let x = (cx - w / 2.0).clamp(
        canvas.min.x + GAP,
        (canvas.max.x - w - GAP).max(canvas.min.x),
    );
    let y = (bottom + 12.0).clamp(
        canvas.min.y + GAP,
        (canvas.max.y - h - GAP).max(canvas.min.y),
    );
    egui::Area::new(Id::unique("selection_launcher"))
        .order(Order::Foreground)
        .fixed_pos(Pos2::new(x, y))
        .show(ctx, |ui| {
            panel_frame()
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if icon_button(ui, icon::DESELECT, false, "Desmarcar (Ctrl+D)").clicked() {
                            app.deselect();
                        }
                        if icon_button(ui, icon::INVERT, false, "Inverter seleção (Ctrl+Shift+I)")
                            .clicked()
                        {
                            app.invert_selection();
                        }
                        if icon_button(
                            ui,
                            icon::SCISSORS,
                            false,
                            "Limpar dentro da seleção (Delete)",
                        )
                        .clicked()
                        {
                            app.clear_selection();
                        }
                        if icon_button(ui, icon::BUCKET, false, "Preencher com a cor").clicked() {
                            app.fill_selection();
                        }
                        if icon_button(ui, icon::MOVE, app.tool == Tool::MoveLayer, "Mover camada")
                            .clicked()
                        {
                            app.set_tool(Tool::MoveLayer);
                        }
                    });
                });
        });
}

fn brush_settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    let max = photocraft_max_size().min(1000.0);
    let use_pressure = &mut app.use_pressure;
    let b = if app.tool == Tool::Eraser {
        &mut app.eraser
    } else {
        &mut app.brush
    };
    section(ui, "Ajustes");
    ui.add(
        egui::Slider::new(&mut b.size, 1.0..=max)
            .logarithmic(true)
            .text("Tamanho"),
    );
    // Tamanhos prontos, como a paleta "Tamanho do pincel" do Clip Studio.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(3.0);
        for s in [
            1.0, 2.0, 3.0, 5.0, 8.0, 12.0, 20.0, 30.0, 50.0, 80.0, 120.0, 200.0,
        ] {
            if s <= max
                && ui
                    .selectable_label((b.size - s).abs() < 0.5, format!("{s}"))
                    .clicked()
            {
                b.size = s;
            }
        }
    });
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
    let pressure_changed = theme::switch(ui, use_pressure, "Usar pressão da caneta").changed();
    if pressure_changed {
        app.save_prefs();
    }
    ui.add_space(4.0);
    symmetry_settings(app, ui);
    if app.tool == Tool::Eraser {
        section(ui, "Borracha mágica");
        theme::switch(ui, &mut app.eraser_magic, "Toque apaga a cor parecida");
        if app.eraser_magic {
            ui.add(egui::Slider::new(&mut app.wand_tolerance, 0.0..=1.0).text("Tolerância"));
            ui.checkbox(&mut app.wand_contiguous, "Só a área contígua");
        }
        let mut none = artapp_core::Taper::default();
        crate::brush_editor::show(ui, &mut app.eraser, &mut none);
        return;
    }
    ui.add_space(4.0);
    let mut group = String::new();
    let mut picked = None;
    let mut remove = None;
    let previews = app.brush_previews(ui.ctx());
    for (i, entry) in app.brushes.iter().enumerate() {
        if entry.group != group {
            group.clone_from(&entry.group);
            section(ui, &group);
        }
        let selected = app.brush_index == i;
        let row = brush_row(ui, &entry.name, previews.get(i), selected);
        if row.clicked() {
            picked = Some(i);
        }
        if entry.group == artapp_core::brushes::CUSTOM_GROUP {
            row.context_menu(|ui| {
                if ui
                    .button(format!("{}  Apagar pincel", icon::TRASH))
                    .clicked()
                {
                    remove = Some(i);
                    ui.close();
                }
            });
        }
    }
    if let Some(i) = picked {
        app.select_brush(i);
    }
    if let Some(i) = remove {
        app.delete_brush(i);
    }
    ui.add_space(6.0);
    section(ui, "Detalhes do pincel");
    crate::brush_editor::show(ui, &mut app.brush, &mut app.taper);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.new_brush_name)
                .hint_text("Nome do pincel")
                .desired_width(ui.available_width() - 110.0),
        );
        if ui.button(format!("{}  Salvar", icon::PLUS)).clicked() {
            app.save_brush();
        }
    });
    if ui.button("Restaurar ajustes do preset").clicked() {
        app.reset_brush();
    }
}

fn brush_row(
    ui: &mut egui::Ui,
    name: &str,
    preview: Option<&TextureHandle>,
    selected: bool,
) -> egui::Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 46.0), Sense::click());
    let p = ui.painter();
    let bg = if selected {
        color::ACCENT.gamma_multiply(0.22)
    } else if resp.hovered() {
        color::SURFACE_HOVER
    } else {
        Color32::TRANSPARENT
    };
    p.rect_filled(rect, CornerRadius::same(8), bg);
    if selected {
        p.rect_stroke(
            rect,
            CornerRadius::same(8),
            Stroke::new(1.0, color::ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    p.text(
        rect.left_top() + Vec2::new(10.0, 5.0),
        Align2::LEFT_TOP,
        name,
        egui::FontId::proportional(12.5),
        color::TEXT,
    );
    if let Some(tex) = preview {
        let r = egui::Rect::from_min_size(
            rect.left_top() + Vec2::new(8.0, 20.0),
            Vec2::new(w - 16.0, 22.0),
        );
        p.image(
            tex.id(),
            r,
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    resp
}

fn vector_settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    section(ui, "Traço e preenchimento");
    ui.horizontal(|ui| {
        ui.add_sized([150.0, 36.0], |ui: &mut egui::Ui| {
            theme::switch(ui, &mut app.vector_stroke, "Traço")
        });
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
    });
    ui.add(
        egui::Slider::new(&mut app.vector_width, 0.5..=200.0)
            .logarithmic(true)
            .text("Espessura"),
    );
    ui.horizontal(|ui| {
        ui.add_sized([150.0, 36.0], |ui: &mut egui::Ui| {
            theme::switch(ui, &mut app.vector_fill, "Preenchimento")
        });
        egui::color_picker::color_edit_button_srgba(
            ui,
            &mut app.vector_fill_color,
            egui::color_picker::Alpha::Opaque,
        );
    });
    ui.separator();
    let hint = |ui: &mut egui::Ui, t: &str| {
        ui.label(RichText::new(t).size(12.5).color(color::TEXT_MUTED));
    };
    match app.tool {
        Tool::Bezier => {
            hint(
                ui,
                "Toque para pôr pontos; arraste para curvar. Toque no primeiro ponto para fechar.",
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        app.bezier.len() >= 2,
                        egui::Button::new(format!("{}  Concluir", icon::CHECK)),
                    )
                    .clicked()
                {
                    app.finish_bezier(false);
                }
                if ui
                    .add_enabled(
                        !app.bezier.is_empty(),
                        egui::Button::new(format!("{}  Cancelar", icon::X)),
                    )
                    .clicked()
                {
                    app.bezier.clear();
                }
            });
        }
        Tool::Select => {
            hint(ui, "Toque num objeto para selecionar; arraste para mover.");
            let has = app.selected.is_some();
            ui.horizontal_wrapped(|ui| {
                let restyle = ui
                    .add_enabled(
                        has,
                        egui::Button::new(format!("{}  Aplicar estilo", icon::PALETTE)),
                    )
                    .clicked();
                let delete = ui
                    .add_enabled(has, egui::Button::new(format!("{}  Apagar", icon::TRASH)))
                    .clicked();
                if let Some(id) = app.selected.filter(|_| restyle) {
                    let style = app.vector_style();
                    let r = app.doc.vector_restyle(id, &style);
                    app.invalidate(r);
                }
                if let Some(id) = app.selected.filter(|_| delete) {
                    app.selected = None;
                    let r = app.doc.vector_delete(id);
                    app.invalidate(r);
                }
            });
        }
        Tool::VectorPen => hint(ui, "Desenhe à mão livre: o traço vira curvas editáveis."),
        _ => hint(ui, "Arraste na tela para desenhar a forma."),
    }
}

pub fn layers_panel(app: &mut ArtApp, ctx: &egui::Context, canvas: egui::Rect, portrait: bool) {
    let close = panel_area(
        "layers_panel",
        ctx,
        canvas,
        portrait,
        true,
        icon::LAYERS,
        "Camadas",
        |ui, body_h| {
            let mut changed = false;
            let list_h = (body_h - 210.0).max(120.0);
            egui::ScrollArea::vertical()
                .id_salt("layer_list")
                .max_height(list_h)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let thumbs = app.layer_thumbnails(ui.ctx());
                    let active = app.doc.active;
                    let mut select = None;
                    let mut toggle = None;
                    for i in (0..app.doc.layers.len()).rev() {
                        let Some(layer) = app.doc.layers.get(i) else {
                            continue;
                        };
                        let (sel, tog) = layer_row(ui, layer, thumbs.get(&layer.id), i == active);
                        if sel {
                            select = Some(i);
                        }
                        if tog {
                            toggle = Some(i);
                        }
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
                        app.selected = None;
                    }
                });
            ui.separator();
            changed |= active_layer_settings(app, ui);
            ui.separator();
            let n = app.doc.layers.len();
            let is_vector = app.doc.active_is_vector();
            let b = |ui: &mut egui::Ui, g: char, tip: &str, enabled: bool| {
                ui.add_enabled_ui(enabled, |ui| icon_button(ui, g, false, tip))
                    .inner
                    .clicked()
            };
            // Duas fileiras de botões, para caber na largura do painel.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if b(ui, icon::PLUS, "Nova camada", true) {
                    app.doc.add_layer();
                }
                if b(ui, icon::SQUARE_PEN, "Nova camada vetorial", true) {
                    app.doc.add_vector_layer();
                }
                if b(ui, icon::COPY, "Duplicar", true) {
                    app.doc.duplicate_layer();
                    changed = true;
                }
                if b(ui, icon::MERGE_DOWN, "Mesclar abaixo", app.doc.active > 0) {
                    app.doc.merge_down();
                    changed = true;
                }
                if b(ui, icon::TRASH, "Apagar camada", true) {
                    app.doc.delete_layer();
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if b(ui, icon::CHEVRON_UP, "Subir", app.doc.active + 1 < n) {
                    app.doc.move_layer(true);
                    changed = true;
                }
                if b(ui, icon::CHEVRON_DOWN, "Descer", app.doc.active > 0) {
                    app.doc.move_layer(false);
                    changed = true;
                }
                if b(ui, icon::IMAGE, "Rasterizar camada vetorial", is_vector) {
                    app.doc.rasterize_layer();
                }
                if b(ui, icon::ERASER, "Limpar camada", true) {
                    app.doc.clear_layer();
                    changed = true;
                }
            });
            if changed {
                app.invalidate_all();
            }
        },
    );
    if close {
        app.show_layers_panel = false;
    }
}

/// Uma linha da lista de camadas. Devolve (selecionar, alternar visibilidade).
fn layer_row(
    ui: &mut egui::Ui,
    layer: &artapp_core::Layer,
    thumb: Option<&TextureHandle>,
    active: bool,
) -> (bool, bool) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 52.0), Sense::click());
    let p = ui.painter();
    let bg = if active {
        color::SELECTION.gamma_multiply(0.25)
    } else if resp.hovered() {
        color::SURFACE_HOVER
    } else {
        Color32::TRANSPARENT
    };
    p.rect_filled(rect, CornerRadius::same(8), bg);
    if active {
        p.rect_stroke(
            rect,
            CornerRadius::same(8),
            Stroke::new(1.0, color::SELECTION),
            egui::StrokeKind::Inside,
        );
    }
    // Camada recortada: recuada, com uma barra ligando-a à base embaixo (como no Clip Studio).
    // Máscara: recuada também, com a barra azul da seleção.
    let indent = if layer.clip || layer.mask { 14.0 } else { 0.0 };
    if layer.clip || layer.mask {
        p.rect_filled(
            egui::Rect::from_min_size(rect.left_top() + Vec2::new(8.0, 4.0), Vec2::new(3.0, 48.0)),
            CornerRadius::same(1),
            if layer.mask {
                color::SELECTION
            } else {
                color::ACCENT
            },
        );
    }
    let thumb_rect =
        egui::Rect::from_min_size(rect.left_top() + Vec2::new(6.0 + indent, 6.0), THUMB);
    p.rect_filled(thumb_rect, CornerRadius::same(4), Color32::WHITE);
    if let Some(t) = thumb {
        p.image(
            t.id(),
            thumb_rect,
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    p.rect_stroke(
        thumb_rect,
        CornerRadius::same(4),
        Stroke::new(1.0, color::BORDER),
        egui::StrokeKind::Outside,
    );
    let text_x = thumb_rect.max.x + 10.0;
    let fg = if layer.visible {
        color::TEXT
    } else {
        color::TEXT_MUTED
    };
    p.text(
        Pos2::new(text_x, rect.center().y - 8.0),
        Align2::LEFT_CENTER,
        &layer.name,
        egui::FontId::proportional(14.0),
        fg,
    );
    let mut sub = format!("{:.0}%  ·  {}", layer.opacity * 100.0, layer.blend.label());
    for (on, glyph) in [
        (layer.lock_alpha, icon::LOCK),
        (layer.reference, icon::REFERENCE),
        (layer.draft, icon::DRAFT),
    ] {
        if on {
            sub.push_str(&format!("  {glyph}"));
        }
    }
    p.text(
        Pos2::new(text_x, rect.center().y + 10.0),
        Align2::LEFT_CENTER,
        sub,
        egui::FontId::proportional(11.0),
        color::TEXT_MUTED,
    );
    if layer.is_vector() {
        let badge = egui::Rect::from_min_size(
            Pos2::new(rect.max.x - 96.0, rect.center().y - 9.0),
            Vec2::new(40.0, 18.0),
        );
        p.rect_filled(
            badge,
            CornerRadius::same(9),
            color::SELECTION.gamma_multiply(0.35),
        );
        p.text(
            badge.center(),
            Align2::CENTER_CENTER,
            "vetor",
            egui::FontId::proportional(10.5),
            color::TEXT,
        );
    }
    let eye_rect = egui::Rect::from_center_size(
        Pos2::new(rect.max.x - 26.0, rect.center().y),
        Vec2::splat(theme::TOUCH),
    );
    let eye = ui.interact(
        eye_rect,
        ui.scope_id().with(("eye", layer.id)),
        Sense::click(),
    );
    let eye_col = if eye.hovered() {
        color::TEXT
    } else {
        color::TEXT_MUTED
    };
    ui.painter().text(
        eye_rect.center(),
        Align2::CENTER_CENTER,
        if layer.visible {
            icon::EYE
        } else {
            icon::EYE_OFF
        },
        theme::icon_font(18.0),
        eye_col,
    );
    (resp.clicked() && !eye.clicked(), eye.clicked())
}

/// Opacidade, mesclagem e bloqueio da camada ativa. Devolve se algo mudou.
fn active_layer_settings(app: &mut ArtApp, ui: &mut egui::Ui) -> bool {
    let mut changed = false;
    let mut checkpoint = false;
    let mask_now = app.doc.active_has_mask();
    let Some(layer) = app.doc.layers.get_mut(app.doc.active) else {
        return false;
    };
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(ui.available_width()));
    });
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
    let mut lock = layer.lock_alpha;
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("blend")
            .selected_text(blend.label())
            .width(150.0)
            .show_ui(ui, |ui| {
                for m in BlendMode::layer_modes().filter(|m| *m != BlendMode::Dissolve) {
                    ui.selectable_value(&mut blend, m, m.label());
                }
            });
        let lock_icon = if lock { icon::LOCK } else { icon::UNLOCK };
        if icon_button(ui, lock_icon, lock, "Bloquear transparência").clicked() {
            lock = !lock;
        }
    });
    // Os três recursos de camada do Clip Studio que mudam o jeito de pintar.
    let (mut clip, mut reference, mut draft) = (layer.clip, layer.reference, layer.draft);
    let (mut add_mask, mut apply_mask) = (false, false);
    if layer.mask {
        ui.label(
            RichText::new("Pintando na máscara: preto esconde, branco ou borracha mostra.")
                .size(12.0)
                .color(color::TEXT_MUTED),
        );
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        if icon_button(ui, icon::CLIP, clip, "Recortar na camada abaixo").clicked() {
            clip = !clip;
        }
        if icon_button(
            ui,
            icon::CIRCLE_DASHED,
            mask_now,
            "Máscara de camada (pinte de preto para esconder)",
        )
        .clicked()
        {
            add_mask = true;
        }
        if mask_now && icon_button(ui, icon::CHECK, false, "Aplicar a máscara").clicked() {
            apply_mask = true;
        }
        if icon_button(
            ui,
            icon::REFERENCE,
            reference,
            "Camada de referência (o balde olha só ela)",
        )
        .clicked()
        {
            reference = !reference;
        }
        if icon_button(
            ui,
            icon::DRAFT,
            draft,
            "Camada de rascunho (fica fora da exportação)",
        )
        .clicked()
        {
            draft = !draft;
        }
    });
    if add_mask {
        app.doc.add_mask();
        return true;
    }
    if apply_mask {
        app.doc.apply_mask();
        return true;
    }
    if blend != layer.blend
        || lock != layer.lock_alpha
        || (clip, reference, draft) != (layer.clip, layer.reference, layer.draft)
    {
        checkpoint = true;
    }
    if checkpoint {
        app.doc.checkpoint();
        if let Some(layer) = app.doc.active_layer_mut() {
            layer.blend = blend;
            layer.lock_alpha = lock;
            layer.clip = clip;
            layer.reference = reference;
            layer.draft = draft;
        }
        changed = true;
    }
    changed
}

// ---------- Cor, avisos e diálogos ----------

pub fn color_popup(app: &mut ArtApp, ctx: &egui::Context, canvas: egui::Rect, portrait: bool) {
    // Altura do seletor inteiro; em tela baixa ele encolhe e rola.
    let full_h: f32 = 540.0;
    let max_h = full_h.min(canvas.height() - 12.0);
    let pos = if portrait {
        Pos2::new(canvas.min.x + 6.0, canvas.max.y - max_h - 6.0)
    } else {
        Pos2::new(
            canvas.min.x + GAP + rail_w(canvas) + 8.0,
            (canvas.min.y + 300.0)
                .min(canvas.max.y - max_h - 6.0)
                .max(canvas.min.y + 6.0),
        )
    };
    let mut close = false;
    egui::Area::new(Id::unique("color_popup"))
        .order(Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            panel_frame().show(ui, |ui| {
                ui.set_width(270.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(icon::PALETTE.to_string())
                            .font(theme::icon_font(18.0))
                            .color(color::ACCENT),
                    );
                    ui.heading("Cor");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close =
                            theme::icon_button_sized(ui, icon::X, false, "Fechar", 36.0).clicked();
                    });
                });
                egui::ScrollArea::vertical()
                    .id_salt("color_popup")
                    .max_height(max_h - 72.0)
                    .show(ui, |ui| color_body(app, ui));
            });
        });
    if close {
        app.show_color = false;
    }
}

/// A roda de cores flutuante: fica na tela o tempo todo (não fecha ao desenhar), arrasta pela
/// barra, muda de tamanho pelo canto e recolhe. Fica dentro da tela de desenho, sem cobrir as
/// barras nem o trilho de ferramentas.
pub fn floating_color(app: &mut ArtApp, ctx: &egui::Context, canvas: egui::Rect, portrait: bool) {
    const HEADER: f32 = 36.0;
    const MIN: f32 = 140.0;
    if !canvas.is_positive() {
        // Primeiro quadro: ainda sem tela, e a posição inicial ficaria errada.
        return;
    }
    let mut bounds = canvas.shrink(4.0);
    if !portrait {
        bounds.min.x = canvas.min.x + GAP + rail_w(canvas) + 4.0;
    }
    // Margens da moldura (8 de cada lado) e a barra de título.
    let max = (bounds.width() - 16.0)
        .min(bounds.height() - HEADER - 26.0)
        .max(MIN);
    let size = app.color_float_size.clamp(MIN, max);
    let width = size.max(4.0 * HEADER + 30.0);
    // Começa em cima à direita: os painéis abrem embaixo (em pé), à esquerda (deitado) ou
    // ocupam a direita só no tablet, e os sliders ficam à esquerda.
    let pos = app
        .color_float_pos
        .unwrap_or_else(|| Pos2::new(bounds.max.x - width - 16.0, bounds.min.y));
    let mut save = false;
    let mut close = false;
    let resp = egui::Area::new(Id::unique("color_float"))
        .order(Order::Foreground)
        .sense(Sense::drag())
        .current_pos(pos)
        .constrain_to(bounds)
        .show(ctx, |ui| {
            panel_frame()
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.horizontal(|ui| {
                        // A cor atual (a barra serve para arrastar).
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(HEADER), Sense::hover());
                        ui.painter().circle_filled(r.center(), 11.0, app.color);
                        ui.painter().circle_stroke(
                            r.center(),
                            11.0,
                            Stroke::new(1.5, color::BORDER),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            close = theme::icon_button_sized(
                                ui,
                                icon::X,
                                false,
                                "Tirar a roda flutuante",
                                HEADER,
                            )
                            .clicked();
                            let (glyph, tip) = if app.color_float_collapsed {
                                (icon::CHEVRON_DOWN, "Abrir a roda")
                            } else {
                                (icon::CHEVRON_UP, "Recolher a roda")
                            };
                            if theme::icon_button_sized(ui, glyph, false, tip, HEADER).clicked() {
                                app.color_float_collapsed = !app.color_float_collapsed;
                                save = true;
                            }
                            if theme::icon_button_sized(
                                ui,
                                icon::SLIDERS,
                                app.show_color,
                                "Painel de cor completo",
                                HEADER,
                            )
                            .clicked()
                            {
                                app.show_color = !app.show_color;
                            }
                            let (glyph, tip) = if app.color_state.triangle {
                                (icon::TRIANGLE, "Meio em triângulo (toque para quadrado)")
                            } else {
                                (icon::SQUARE, "Meio em quadrado (toque para triângulo)")
                            };
                            if theme::icon_button_sized(ui, glyph, false, tip, HEADER).clicked() {
                                app.color_state.triangle = !app.color_state.triangle;
                                save = true;
                            }
                        });
                    });
                    if app.color_float_collapsed {
                        return;
                    }
                    ui.add_space(6.0);
                    ui.vertical_centered(|ui| {
                        crate::color_panel::show_wheel(app, ui, size);
                    });
                    // Canto para mudar o tamanho.
                    let r = ui.min_rect();
                    let grip = egui::Rect::from_min_max(r.max - Vec2::splat(30.0), r.max);
                    let g = ui.interact(grip, ui.scope_id().with("grip"), Sense::drag());
                    if g.dragged() {
                        let d = g.drag_delta();
                        app.color_float_size = (size + (d.x + d.y) / 2.0).clamp(MIN, max);
                    }
                    if g.drag_stopped() {
                        save = true;
                    }
                    let fg = if g.hovered() || g.dragged() {
                        color::ACCENT
                    } else {
                        color::TEXT_MUTED
                    };
                    for k in [8.0, 14.0, 20.0] {
                        ui.painter().line_segment(
                            [
                                Pos2::new(grip.max.x - k, grip.max.y - 2.0),
                                Pos2::new(grip.max.x - 2.0, grip.max.y - k),
                            ],
                            Stroke::new(1.5, fg),
                        );
                    }
                });
        });
    // A posição é nossa (não da memória do egui) para valer desde o primeiro quadro e entre
    // aberturas do app; o egui só a prende dentro de `bounds`.
    if resp.response.dragged() {
        app.color_float_pos = Some(resp.response.rect.min + resp.response.drag_delta());
    }
    if close {
        app.color_float = false;
        save = true;
    }
    if save || resp.response.drag_stopped() {
        app.color_float_pos = app.color_float_pos.map(|_| resp.response.rect.min);
        app.save_prefs();
    }
}

fn color_body(app: &mut ArtApp, ui: &mut egui::Ui) {
    ui.spacing_mut().slider_width = 170.0;
    crate::color_panel::show(app, ui);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), app.sub_color);
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(6),
            Stroke::new(1.0, color::BORDER),
            egui::StrokeKind::Outside,
        );
        if ui
            .button(format!("{}  Trocar com a secundária (X)", icon::SWAP))
            .clicked()
        {
            app.swap_colors();
        }
    });
    section(ui, "Recentes");
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(6.0);
        for rc in app.recent_colors.clone() {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
            ui.painter().rect_filled(rect, CornerRadius::same(6), rc);
            let edge = if rc == app.color {
                Stroke::new(2.0, color::ACCENT)
            } else {
                Stroke::new(1.0, color::BORDER)
            };
            ui.painter()
                .rect_stroke(rect, CornerRadius::same(6), edge, egui::StrokeKind::Outside);
            if resp.clicked() {
                app.color = rc;
            }
        }
    });
}

/// Aviso curto embaixo, que some sozinho.
pub fn toast(app: &ArtApp, ctx: &egui::Context, canvas: egui::Rect, bottom_offset: f32) {
    let now = ctx.input(|i| i.time);
    let age = now - app.status_since;
    if app.status.is_empty() || age > 4.0 {
        return;
    }
    let alpha = (((4.0 - age) / 0.4).clamp(0.0, 1.0)) as f32;
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
    egui::Area::new(Id::unique("toast"))
        .order(Order::Tooltip)
        .anchor(
            Align2::CENTER_BOTTOM,
            Vec2::new(0.0, -(GAP + bottom_offset)),
        )
        .constrain_to(canvas)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_opacity(alpha);
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(20, 21, 24, 235))
                .corner_radius(CornerRadius::same(20))
                .inner_margin(egui::Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    ui.label(RichText::new(&app.status).size(13.5).color(color::TEXT));
                });
        });
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
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(panel_frame())
        .vscroll(true)
        .max_height(ctx.content_rect().height() - 60.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(
                    "O desenho atual será descartado. Salve o projeto antes se quiser guardá-lo.",
                )
                .color(color::TEXT_MUTED),
            );
            ui.add_space(4.0);
            for (name, w, h) in CANVAS_PRESETS {
                let fits = w <= max && h <= max;
                let label = name.to_string();
                if ui
                    .add_enabled(
                        fits,
                        egui::Button::new(RichText::new(label).size(15.0))
                            .min_size(Vec2::new(260.0, 40.0)),
                    )
                    .clicked()
                {
                    create = Some((w, h));
                }
            }
            ui.label(
                RichText::new(format!("Tamanho máximo neste aparelho: {max} px"))
                    .size(12.0)
                    .color(color::TEXT_MUTED),
            );
        });
    if let Some((w, h)) = create {
        app.new_canvas(w, h);
        app.show_new_dialog = false;
    } else if !open {
        app.show_new_dialog = false;
    }
}
