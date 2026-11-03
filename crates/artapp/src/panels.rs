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
/// Largura dos sliders laterais.
const SLIDER_W: f32 = 36.0;

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
        // Clicar dentro não fecha o menu (os itens fecham sozinhos): assim as chaves e as setas
        // das preferências podem ser tocadas várias vezes seguidas.
        let (menu_btn, _) = egui::containers::menu::MenuButton::new(
            RichText::new(icon::MENU.to_string()).font(theme::icon_font(20.0)),
        )
        .config(
            egui::containers::menu::MenuConfig::new()
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
        )
        .ui(ui, |ui| file_menu(app, ui));
        // Cada vez que o menu abre, ele começa na lista de grupos.
        if menu_btn.clicked() {
            reset_menu_group(ui.ctx());
        }
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
        save_button(app, ui);

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

/// Atalho de "Salvar projeto" na barra de cima. Sem arquivo ainda, pergunta o nome (como
/// "Salvar como"); segurar ou clicar com o botão direito sempre abre "Salvar como". Um ponto
/// amarelo marca alterações não salvas e um ✓ aparece por um instante quando termina de gravar.
fn save_button(app: &mut ArtApp, ui: &mut egui::Ui) {
    let now = ui.input(|i| i.time);
    let busy = app.projects.busy();
    let just_saved = app.projects.saved_at.is_some_and(|t| now - t < 1.5);
    if just_saved {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
    let glyph = if just_saved { icon::CHECK } else { icon::SAVE };
    let tip = if app.projects.path.is_some() {
        "Salvar (Ctrl+S) · segure para Salvar como"
    } else {
        "Salvar projeto (Ctrl+S)"
    };
    let resp = ui
        .add_enabled_ui(!busy, |ui| icon_button(ui, glyph, just_saved, tip))
        .inner;
    if app.projects.dirty() && !just_saved {
        let c = resp.rect.right_top() + Vec2::new(-10.0, 10.0);
        ui.painter().circle_filled(c, 4.0, color::ACCENT);
    }
    if resp.secondary_clicked() || resp.long_touched() {
        app.save_project_as();
    } else if resp.clicked() && app.projects.dialog.is_none() {
        // Depois de segurar, o soltar também conta como clique: não salva por cima do diálogo.
        app.save_project();
    }
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
    changed |= crate::quick_menu::settings(app, ui);
    if changed {
        app.save_prefs();
    }
}

/// Preferências gerais: interface, desfazer e papel.
fn app_settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    let mut changed = false;
    ui.label("Tamanho da interface");
    ui.horizontal_wrapped(|ui| {
        for pct in [80u32, 90, 100, 115, 130, 150] {
            let v = pct as f32 / 100.0;
            if ui
                .selectable_label((app.ui_scale - v).abs() < 0.01, format!("{pct}%"))
                .clicked()
            {
                app.ui_scale = v.clamp(crate::app::UI_SCALE.0, crate::app::UI_SCALE.1);
                changed = true;
            }
        }
    });
    changed |= ui
        .add_sized([210.0, 36.0], |ui: &mut egui::Ui| {
            theme::switch(ui, &mut app.left_handed, "Ferramentas à direita (canhoto)")
        })
        .changed();
    changed |= ui
        .add_sized([210.0, 36.0], |ui: &mut egui::Ui| {
            theme::switch(ui, &mut app.show_perf, "Mostrar desempenho (ms por quadro)")
        })
        .changed();
    changed |= ui
        .add_sized([210.0, 36.0], |ui: &mut egui::Ui| {
            theme::switch(ui, &mut app.simple_mode, "Modo simples (menos ferramentas)")
        })
        .changed();
    changed |= ui
        .add_sized([210.0, 36.0], |ui: &mut egui::Ui| {
            theme::switch(ui, &mut app.low_power, "Modo econômico (aparelhos mais fracos)")
        })
        .on_hover_text("Mostra só pincel, borracha, mistura, balde, conta-gotas, mão, mover e laço, e as abas Pincéis, Rápido e Geral.")
        .changed();
    let r = artapp_core::history::UNDO_RANGE;
    let resp = ui.add(
        egui::Slider::new(&mut app.undo_steps, *r.start()..=*r.end())
            .logarithmic(true)
            .text("Passos de desfazer"),
    );
    changed |= resp.drag_stopped() || (resp.changed() && !resp.dragged());
    ui.horizontal(|ui| {
        ui.label("Papel das telas novas");
        let mut rgb = [app.paper_color[0], app.paper_color[1], app.paper_color[2]];
        if ui.color_edit_button_rgb(&mut rgb).changed() {
            app.paper_color = [rgb[0], rgb[1], rgb[2], app.paper_color[3]];
            changed = true;
        }
        let mut clear = app.paper_color[3] == 0.0;
        if ui.checkbox(&mut clear, "Transparente").changed() {
            app.paper_color[3] = if clear { 0.0 } else { 1.0 };
            changed = true;
        }
    });
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

/// Grupos do menu principal. O menu abre na lista de grupos; tocar num grupo mostra os itens
/// dele com um "Voltar" no topo (navegação em níveis, melhor que submenus de passar o mouse
/// para quem usa o dedo ou a caneta).
#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuGroup {
    File,
    Edit,
    Canvas,
    Layer,
    Selection,
    Filters,
    Comic,
    View,
    Pen,
    Prefs,
}

impl MenuGroup {
    const ALL: [MenuGroup; 10] = [
        MenuGroup::File,
        MenuGroup::Edit,
        MenuGroup::Canvas,
        MenuGroup::Layer,
        MenuGroup::Selection,
        MenuGroup::Filters,
        MenuGroup::Comic,
        MenuGroup::View,
        MenuGroup::Pen,
        MenuGroup::Prefs,
    ];

    fn label(self) -> &'static str {
        match self {
            MenuGroup::File => "Arquivo",
            MenuGroup::Edit => "Editar",
            MenuGroup::Canvas => "Tela",
            MenuGroup::Layer => "Camada",
            MenuGroup::Selection => "Seleção",
            MenuGroup::Filters => "Filtros e efeitos",
            MenuGroup::Comic => "Quadrinho",
            MenuGroup::View => "Visualização",
            MenuGroup::Pen => "Caneta e gestos",
            MenuGroup::Prefs => "Preferências",
        }
    }

    fn icon(self) -> char {
        match self {
            MenuGroup::File => icon::FOLDER_OPEN,
            MenuGroup::Edit => icon::SCISSORS,
            MenuGroup::Canvas => icon::CROP,
            MenuGroup::Layer => icon::LAYERS,
            MenuGroup::Selection => icon::SQUARE_DASHED,
            MenuGroup::Filters => icon::SPARKLES,
            MenuGroup::Comic => icon::GRID_3X3,
            MenuGroup::View => icon::EYE,
            MenuGroup::Pen => icon::PEN_TOOL,
            MenuGroup::Prefs => icon::SLIDERS,
        }
    }
}

fn menu_group_id() -> Id {
    Id::unique("main_menu_group")
}

/// Volta o menu para a lista de grupos (chamado quando o menu é aberto).
fn reset_menu_group(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<MenuGroup>(menu_group_id()));
}

fn menu_item(ui: &mut egui::Ui, glyph: char, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(format!("{glyph}   {text}")).size(15.0))
            .frame(false)
            .min_size(Vec2::new(210.0, 36.0)),
    )
}

fn file_menu_items(app: &mut ArtApp, ui: &mut egui::Ui) {
    ui.set_min_width(220.0);
    let group = ui.data(|d| d.get_temp::<MenuGroup>(menu_group_id()));
    let Some(group) = group else {
        for g in MenuGroup::ALL {
            let resp = ui.add(
                egui::Button::new(
                    RichText::new(format!("{}   {}", g.icon(), g.label())).size(15.0),
                )
                .right_text(RichText::new("›").size(18.0).color(color::TEXT_MUTED))
                .frame(false)
                .min_size(Vec2::new(210.0, 40.0)),
            );
            if resp.clicked() {
                ui.data_mut(|d| d.insert_temp(menu_group_id(), g));
            }
        }
        ui.separator();
        if menu_item(ui, icon::PLAY, "Mostrar o tour").clicked() {
            app.tour_step = Some(0);
            ui.close();
        }
        update_menu_item(app, ui);
        return;
    };
    ui.horizontal(|ui| {
        if ui
            .add(
                egui::Button::new(RichText::new("‹  Voltar").size(15.0))
                    .frame(false)
                    .min_size(Vec2::new(90.0, 36.0)),
            )
            .clicked()
        {
            reset_menu_group(ui.ctx());
        }
        ui.label(
            RichText::new(group.label())
                .size(15.0)
                .strong()
                .color(color::ACCENT),
        );
    });
    ui.separator();
    match group {
        MenuGroup::File => file_group(app, ui),
        MenuGroup::Edit => edit_group(app, ui),
        MenuGroup::Canvas => canvas_group(app, ui),
        MenuGroup::Layer => layer_group(app, ui),
        MenuGroup::Selection => selection_group(app, ui),
        MenuGroup::Filters => filters_group(app, ui),
        MenuGroup::Comic => comic_group(app, ui),
        MenuGroup::View => view_group(app, ui),
        MenuGroup::Pen => pen_settings(app, ui),
        MenuGroup::Prefs => prefs_group(app, ui),
    }
}

/// Arquivo: abrir, salvar, importar e exportar.
fn file_group(app: &mut ArtApp, ui: &mut egui::Ui) {
    let item = menu_item;
    if item(ui, icon::FILE_PLUS, "Nova tela…").clicked() {
        app.show_new_dialog = true;
        ui.close();
    }
    if item(ui, icon::FOLDER_OPEN, "Abrir projeto ou PSD…").clicked() {
        app.open_project_dialog();
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
    if item(ui, icon::COPY, "Backup…").clicked() {
        app.backup.window = true;
        ui.close();
    }
    ui.separator();
    if item(ui, icon::IMAGE, "Importar imagem como camada…").clicked() {
        app.import_image_dialog();
        ui.close();
    }
    if item(ui, icon::BRUSH, "Instalar pincéis (Clip, Photoshop)…").clicked() {
        app.pick_image_for(crate::projects::ImageUse::Brushes);
        ui.close();
    }
    if item(ui, icon::PALETTE, "Paleta de uma imagem…").clicked() {
        app.pick_image_for(crate::projects::ImageUse::Palette);
        ui.close();
    }
    ui.separator();
    if crate::platform::share::SUPPORTED && item(ui, icon::IMAGE, "Compartilhar imagem…").clicked()
    {
        app.share_png();
        ui.close();
    }
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
    if item(ui, icon::ROTATE_3D, "Abrir modelo 3D (OBJ)…").clicked() {
        app.pick_image_for(crate::projects::ImageUse::Model);
        ui.close();
    }
    if item(ui, icon::IMAGE_DOWN, "Exportar PDF").clicked() {
        app.export_pdf();
        ui.close();
    }
    if item(ui, icon::FILM, "Timelapse…").clicked() {
        app.timelapse.open = true;
        ui.close();
    }
}

/// Editar: área de transferência, transformar e histórico.
fn edit_group(app: &mut ArtApp, ui: &mut egui::Ui) {
    let item = menu_item;
    if item(ui, icon::UNDO, "Histórico").clicked() {
        app.history_open = true;
        ui.close();
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
    ui.separator();
    if item(ui, icon::SCALING, "Transformar (Ctrl+T)").clicked() {
        app.start_transform();
        ui.close();
    }
    if item(ui, icon::SCALING, "Transformar por valores…").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::transform());
        ui.close();
    }
}

/// Tela: tamanho e corte do documento.
fn canvas_group(app: &mut ArtApp, ui: &mut egui::Ui) {
    let item = menu_item;
    if item(ui, icon::SCALING, "Tamanho da tela…").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::canvas_size(
            app.doc.width,
            app.doc.height,
            app.doc.dpi,
        ));
        ui.close();
    }
    if item(ui, icon::CROP, "Recorte com perspectiva").clicked() {
        let (w, h) = (f64::from(app.doc.width), f64::from(app.doc.height));
        let (mx, my) = (w * 0.1, h * 0.1);
        app.persp_crop = Some([[mx, my], [w - mx, my], [w - mx, h - my], [mx, h - my]]);
        ui.close();
    }
    let has_sel = app.doc.selection.is_some();
    if ui
        .add_enabled_ui(has_sel, |ui| item(ui, icon::CROP, "Cortar na seleção"))
        .inner
        .on_disabled_hover_text("Faça uma seleção primeiro")
        .clicked()
    {
        app.crop_to_selection();
        ui.close();
    }
}

/// Camada: operações sobre a camada atual (texto, linha, cor).
fn layer_group(app: &mut ArtApp, ui: &mut egui::Ui) {
    let item = menu_item;
    if item(ui, icon::FOLDER_PLUS, "Materiais…").clicked() {
        app.materials.open = true;
        ui.close();
    }
    let editing = app.doc.active_layer().is_some_and(|l| l.pose.is_some());
    let text = if editing {
        "Editar boneco 3D…"
    } else {
        "Boneco 3D de pose…"
    };
    if item(ui, icon::PERSON, text).clicked() {
        app.open_pose();
        ui.close();
    }
    if item(ui, icon::SQUARE_PEN, "Texto…").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::Text {
            text: String::new(),
            size: 64.0,
            editable: false,
        });
        ui.close();
    }
    if item(ui, icon::PEN_LINE, "Ajustar largura da linha…").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::LineWidth { delta: 1.0 });
        ui.close();
    }
    if item(ui, icon::SPLINE, "Correção de linha (vetor)…").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::LineFix {
            connect: false,
            amount: 2.0,
        });
        ui.close();
    }
    if item(ui, icon::SPLINE_POINTER, "Vetorizar camada…").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::Trace(0));
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
}

/// Seleção: selecionar tudo, desmarcar e inverter.
fn selection_group(app: &mut ArtApp, ui: &mut egui::Ui) {
    let item = menu_item;
    if item(ui, icon::SQUARE_DASHED, "Selecionar tudo").clicked() {
        app.select_all();
        ui.close();
    }
    let has_sel = app.doc.selection.is_some();
    ui.add_enabled_ui(has_sel, |ui| {
        if item(ui, icon::DESELECT, "Desmarcar").clicked() {
            app.deselect();
            ui.close();
        }
        if item(ui, icon::INVERT, "Inverter seleção").clicked() {
            app.invert_selection();
            ui.close();
        }
    });
}

/// Filtros, efeitos e correção de cor.
fn filters_group(app: &mut ArtApp, ui: &mut egui::Ui) {
    let item = menu_item;
    if item(ui, icon::SPARKLES, "Filtros…").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::filter(0));
        ui.close();
    }
    if item(ui, icon::SUN_DIM, "Efeitos (brilho, falha, aberração)…").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::Effect(
            artapp_core::effects::Effect::Bloom {
                radius: 24.0,
                intensity: 1.0,
                threshold: 0.6,
            },
        ));
        ui.close();
    }
    if item(ui, icon::CONTRAST, "Correção de cor…").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::adjust());
        ui.close();
    }
}

/// Quadrinho: requadros, linhas de efeito e balões.
fn comic_group(app: &mut ArtApp, ui: &mut egui::Ui) {
    let item = menu_item;
    if item(ui, icon::GRID_3X3, "Requadros…").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::Frames(Default::default()));
        ui.close();
    }
    if item(ui, icon::SUN_DIM, "Linhas de efeito…").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::MangaLines(Default::default()));
        ui.close();
    }
    if item(ui, icon::SCISSORS, "Dividir quadro").clicked() {
        app.set_tool(Tool::FrameSplit);
        ui.close();
    }
    if item(ui, icon::MESSAGE_CIRCLE, "Balão de fala").clicked() {
        app.set_tool(Tool::Balloon);
        ui.close();
    }
}

/// Visualização: girar/espelhar a vista, navegador, grade e referência.
fn view_group(app: &mut ArtApp, ui: &mut egui::Ui) {
    let item = menu_item;
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
    if item(ui, icon::IMAGE, "Janela de referência").clicked() {
        app.reference_open = true;
        ui.close();
    }
    ui.separator();
    // Animação e páginas usam a mesma linha do tempo: uma liga, a outra desliga.
    let a = &app.doc.animation;
    let was = (a.enabled && !a.pages, a.enabled && a.pages);
    let (mut anim, mut pages) = was;
    ui.add_sized([210.0, 36.0], |ui: &mut egui::Ui| {
        theme::switch(ui, &mut anim, "Animação")
    });
    ui.add_sized([210.0, 36.0], |ui: &mut egui::Ui| {
        theme::switch(ui, &mut pages, "Páginas")
    });
    if (anim, pages) != was {
        let a = &mut app.doc.animation;
        if anim != was.0 {
            a.enabled = anim;
            a.pages = false;
        } else {
            a.enabled = pages;
            a.pages = pages;
        }
        app.doc.follow_active_frame();
        app.anim_playing = false;
        app.redraw_canvas();
        ui.close();
    }
    ui.add_sized([210.0, 36.0], |ui: &mut egui::Ui| {
        theme::switch(ui, &mut app.show_navigator, "Navegador")
    });
    ui.add_sized([210.0, 36.0], |ui: &mut egui::Ui| {
        theme::switch(ui, &mut app.show_grid, "Grade")
    });
    if app.show_grid {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut app.grid_iso, false, "Quadrada");
            ui.selectable_value(&mut app.grid_iso, true, "Isométrica");
        });
        ui.add(
            egui::Slider::new(&mut app.grid_size, 8.0..=512.0)
                .logarithmic(true)
                .suffix(" px")
                .text("Grade"),
        );
    }
    ui.separator();
    crate::workspace::menu(app, ui);
}

/// Preferências gerais e atualização do app.
fn prefs_group(app: &mut ArtApp, ui: &mut egui::Ui) {
    app_settings(app, ui);
}

/// "Atualizar app" fica direto na lista principal do menu, fora dos grupos.
fn update_menu_item(app: &mut ArtApp, ui: &mut egui::Ui) {
    if !crate::platform::update::SUPPORTED {
        return;
    }
    ui.separator();
    if menu_item(ui, icon::ROTATE_CW, "Atualizar app").clicked() {
        app.check_update();
        ui.close();
    }
    ui.label(
        RichText::new(if app.updater.is_fast() {
            format!(
                "Build em uso: {} (atualização rápida)",
                app.updater.installed()
            )
        } else {
            format!("Build instalado: {}", app.updater.installed())
        })
        .size(12.0)
        .color(crate::theme::color::TEXT_MUTED),
    );
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
    for (i, group) in Tool::groups(app.simple_mode).into_iter().enumerate() {
        if i > 0 {
            separator(ui);
        }
        let mut buttons = |ui: &mut egui::Ui| {
            for &t in group {
                if t == Tool::Brush {
                    kit_buttons(app, ui);
                    continue;
                }
                let r = icon_button(ui, t.icon(), app.tool == t, t.label());
                // Segurar (ou botão direito): usa a ferramenta num traço só e volta.
                if r.long_touched() || r.secondary_clicked() {
                    app.start_oneshot(t);
                } else if r.clicked() {
                    app.oneshot = None;
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

/// Lápis, Caneta, Spray, Pincel e Texturas e especiais no lugar do botão único de pincel: cada
/// um volta ao último pincel usado nele. Tocar de novo abre os pincéis; segurar usa num traço só.
fn kit_buttons(app: &mut ArtApp, ui: &mut egui::Ui) {
    let current = (app.tool == Tool::Brush).then(|| app.current_kit());
    for kit in crate::app::BrushKit::ALL {
        let on = current == Some(kit);
        let r = icon_button(ui, kit.icon(), on, kit.label());
        if r.long_touched() || r.secondary_clicked() {
            let back = app.oneshot.unwrap_or(app.tool);
            app.choose_kit(kit);
            if back != Tool::Brush {
                app.oneshot = Some(back);
            }
        } else if r.clicked() {
            app.oneshot = None;
            if on {
                app.show_tools_panel = !app.show_tools_panel;
                if app.show_tools_panel && is_compact(ui.ctx()) {
                    app.show_layers_panel = false;
                }
            } else {
                app.choose_kit(kit);
            }
        }
    }
}

/// Quantos botões o grupo mostra (o pincel vira as cinco ferramentas prontas).
fn group_buttons(group: &[Tool]) -> usize {
    group.len()
        + if group.contains(&Tool::Brush) {
            crate::app::BrushKit::ALL.len() - 1
        } else {
            0
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
    let groups = Tool::groups(app.simple_mode);
    let rows: usize = groups.iter().map(|g| group_buttons(g).div_ceil(cols)).sum();
    let rail_h = (rows + 1) as f32 * (theme::TOUCH + 2.0) + groups.len() as f32 * 16.0 + 16.0;
    let h = rail_h.min(canvas.height() - 2.0 * GAP);
    // Centralizado na altura, como no protótipo; encosta no topo se a tela for baixa.
    let y = (canvas.center().y - h / 2.0).max(canvas.min.y + GAP);
    egui::Area::new(Id::unique("rail"))
        .order(Order::Foreground)
        .fixed_pos(Pos2::new(
            if app.left_handed {
                canvas.max.x - GAP - rail_w(canvas)
            } else {
                canvas.min.x + GAP
            },
            y,
        ))
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
    // Canhoto: as ferramentas e os sliders vão para a direita.
    let x = match (portrait, app.left_handed) {
        (true, false) => canvas.min.x + 6.0,
        (true, true) => canvas.max.x - 6.0 - SLIDER_W,
        (false, false) => canvas.min.x + GAP + rail_w(canvas) + 8.0,
        (false, true) => canvas.max.x - GAP - rail_w(canvas) - 8.0 - SLIDER_W,
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
/// Onde um painel lateral aparece: flutuando por cima da tela ou encaixado numa lateral.
pub enum Place<'a> {
    Float {
        ctx: &'a egui::Context,
        canvas: egui::Rect,
        portrait: bool,
        right: bool,
    },
    Dock {
        ui: &'a mut egui::Ui,
        right: bool,
        width: f32,
    },
}

/// O que foi tocado no cabeçalho do painel.
#[derive(Default)]
pub struct PanelEvents {
    pub close: bool,
    /// Encaixar ou soltar.
    pub dock: bool,
    /// Trocar de lado.
    pub swap: bool,
}

/// Cabeçalho (ícone, título, encaixar, trocar de lado, fechar) e corpo rolável.
fn panel_body(
    ui: &mut egui::Ui,
    id: &str,
    docked: Option<bool>,
    glyph: char,
    title: &str,
    max_h: f32,
    add: impl FnOnce(&mut egui::Ui, f32),
) -> PanelEvents {
    let mut ev = PanelEvents::default();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(glyph.to_string())
                .font(theme::icon_font(18.0))
                .color(color::ACCENT),
        );
        ui.heading(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ev.close = theme::icon_button_sized(ui, icon::X, false, "Fechar", 36.0).clicked();
            if let Some(docked) = docked {
                ev.swap =
                    theme::icon_button_sized(ui, icon::SWAP_SIDE, false, "Trocar de lado", 36.0)
                        .clicked();
                let tip = if docked {
                    "Soltar (flutuar sobre a tela)"
                } else {
                    "Encaixar na lateral"
                };
                ev.dock = theme::icon_button_sized(ui, icon::PIN, docked, tip, 36.0).clicked();
            }
        });
    });
    ui.separator();
    let body_h = max_h - 48.0;
    egui::ScrollArea::vertical()
        .id_salt(id)
        .max_height(body_h)
        .auto_shrink([false, true])
        .show(ui, |ui| add(ui, body_h));
    ev
}

fn panel_area(
    id: &str,
    place: Place<'_>,
    glyph: char,
    title: &str,
    add: impl FnOnce(&mut egui::Ui, f32),
) -> PanelEvents {
    let (ctx, canvas, portrait, right) = match place {
        Place::Dock { ui, right, width } => {
            let panel = if right {
                egui::Panel::right(format!("{id}_dock"))
            } else {
                egui::Panel::left(format!("{id}_dock"))
            };
            let frame = egui::Frame::new()
                .fill(color::PANEL)
                .inner_margin(egui::Margin::same(12));
            return panel
                .exact_size(width)
                .resizable(false)
                .frame(frame)
                .show(ui, |ui| {
                    let inner = width - 24.0;
                    ui.set_min_width(inner);
                    ui.set_max_width(inner);
                    ui.spacing_mut().slider_width = (inner - 160.0).max(80.0);
                    let h = ui.available_height();
                    panel_body(ui, id, Some(true), glyph, title, h, add)
                })
                .inner;
        }
        Place::Float {
            ctx,
            canvas,
            portrait,
            right,
        } => (ctx, canvas, portrait, right),
    };
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
    let docked = (!portrait).then_some(false);
    egui::Area::new(Id::unique(id))
        .order(Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            panel_frame()
                .show(ui, |ui| {
                    let inner = width - 24.0;
                    ui.set_min_width(inner);
                    ui.set_max_width(inner);
                    ui.set_max_height(max_h);
                    ui.spacing_mut().slider_width = (inner - 160.0).max(80.0);
                    panel_body(ui, id, docked, glyph, title, max_h, add)
                })
                .inner
        })
        .inner
}

/// Aplica o que foi tocado no cabeçalho de um painel (`tools`: o da ferramenta).
fn panel_events(app: &mut ArtApp, ev: PanelEvents, tools: bool) {
    let w = &mut app.workspace;
    let (docked, right) = if tools {
        (&mut w.tools_docked, &mut w.tools_right)
    } else {
        (&mut w.layers_docked, &mut w.layers_right)
    };
    if ev.dock {
        *docked = !*docked;
    }
    if ev.swap {
        *right = !*right;
    }
    if ev.close {
        if tools {
            app.show_tools_panel = false;
        } else {
            app.show_layers_panel = false;
        }
    }
    if ev.dock || ev.swap {
        app.save_prefs();
    }
}

pub fn brush_panel(app: &mut ArtApp, place: Place<'_>) {
    let ev = panel_area(
        "brush_panel",
        place,
        if app.tool == Tool::Brush {
            app.current_kit().icon()
        } else {
            app.tool.icon()
        },
        if app.tool == Tool::Brush {
            app.current_kit().title()
        } else {
            app.tool.label()
        },
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
                // Grava ao soltar o controle (não a cada quadro do arrasto).
                let done = |r: egui::Response| r.drag_stopped() || (r.changed() && !r.dragged());
                let mut save = false;
                save |=
                    done(ui.add(
                        egui::Slider::new(&mut app.fill_tolerance, 0.0..=1.0).text("Tolerância"),
                    ));
                save |= done(
                    ui.add(
                        egui::Slider::new(&mut app.fill_close_gap, 0.0..=40.0)
                            .suffix(" px")
                            .text("Fechar lacunas"),
                    ),
                );
                save |= done(
                    ui.add(
                        egui::Slider::new(&mut app.fill_expand, -10.0..=10.0)
                            .suffix(" px")
                            .text("Expandir / contrair"),
                    ),
                );
                let aa = app.fill_antialias;
                theme::switch(ui, &mut app.fill_antialias, "Borda suave (antisserrilhado)");
                let reference = app.fill_ref;
                egui::ComboBox::from_label("Olhar")
                    .selected_text(app.fill_ref.label())
                    .show_ui(ui, |ui| {
                        for r in artapp_core::fill::FillRef::ALL {
                            ui.selectable_value(&mut app.fill_ref, r, r.label());
                        }
                    });
                ui.label(RichText::new("Pinta a área de cor parecida. \"Fechar lacunas\" impede a tinta de escorrer por falhas do traço até esse tamanho; vale também para soltar a cor da roda. Em camada vetorial, pinta o preenchimento da forma tocada.").size(12.0).color(color::TEXT_MUTED));
                if save || aa != app.fill_antialias || reference != app.fill_ref {
                    app.save_prefs();
                }
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
            } else if app.tool == Tool::Eyedropper {
                eyedropper_settings(app, ui);
            } else if app.tool == Tool::Hand {
                ui.label(RichText::new("Arraste para mover a tela. Dois dedos: zoom e mover em qualquer ferramenta.").color(color::TEXT_MUTED));
            } else {
                brush_settings(app, ui);
            }
        },
    );
    panel_events(app, ev, true);
}

/// Opções do conta-gotas (valem também para segurar o dedo parado na tela).
fn eyedropper_settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    use artapp_core::composite::SampleFrom;
    ui.label(RichText::new("Toque na tela para pegar a cor.").color(color::TEXT_MUTED));
    section(ui, "Pegar a cor de");
    let before = (app.eyedropper_from, app.eyedropper_erase);
    ui.horizontal_wrapped(|ui| {
        for (m, name) in [
            (SampleFrom::All, "Todas as camadas"),
            (SampleFrom::NoReference, "Sem as de referência"),
            (SampleFrom::Active, "Só a camada atual"),
        ] {
            ui.selectable_value(&mut app.eyedropper_from, m, name);
        }
    });
    ui.checkbox(
        &mut app.eyedropper_erase,
        "Lugar transparente escolhe a cor transparente (apagar)",
    );
    ui.label(
        RichText::new("As cores pegas entram no histórico, que fica salvo junto com o desenho.")
            .size(12.0)
            .color(color::TEXT_MUTED),
    );
    if before != (app.eyedropper_from, app.eyedropper_erase) {
        app.save_prefs();
    }
}

fn mix_settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    use crate::brush_editor::{self, BrushTab};
    use artapp_core::mix::MixKind;
    let tab = brush_editor::tab_bar(ui, "mix", &BrushTab::SETTINGS);
    if tab != BrushTab::General {
        let mut none = artapp_core::Taper::default();
        brush_editor::show_tab(ui, tab, &mut app.mix_brush, &mut none);
        if tab == BrushTab::Advanced {
            brush_editor::blend_mode(ui, &mut app.mix_brush);
        }
        return;
    }
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
            (MixKind::History, "Pincel histórico"),
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
            MixKind::History => {
                "Pinta de volta a camada como ela estava no passo escolhido do histórico, só onde você passar."
            }
            MixKind::Paint => "Pinta com a cor escolhida misturando com a tinta da tela.",
        })
        .size(12.5)
        .color(color::TEXT_MUTED),
    );
    if app.mix_kind == MixKind::History {
        let (names, now) = app.doc.history_steps();
        let last = names.len().saturating_sub(1);
        app.history_brush_step = app.history_brush_step.min(last);
        let label = |i: usize| {
            let n = names.get(i).cloned().unwrap_or_default();
            if i == now {
                format!("{i}. {n} (atual)")
            } else {
                format!("{i}. {n}")
            }
        };
        egui::ComboBox::from_id_salt("history_brush_step")
            .selected_text(label(app.history_brush_step))
            .width(260.0)
            .show_ui(ui, |ui| {
                for i in 0..=last {
                    ui.selectable_value(&mut app.history_brush_step, i, label(i));
                }
            });
    }
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
    kit_pathfinder(app, ui);
    kit_align(app, ui);
    kit_stroke_style(app, ui);
    kit_appearance(app, ui);
    kit_symbols(app, ui);
}

/// Pathfinder: modos de forma (destrutivos ou como forma composta editável) e os recortes.
fn kit_pathfinder(app: &mut ArtApp, ui: &mut egui::Ui) {
    use serde_json::json;
    section(ui, "Combinar formas (pathfinder)");
    theme::switch(
        ui,
        &mut app.kit_live_shapes,
        "Manter editável (forma composta)",
    );
    let modes = [
        ("Unir", "add", "object.pathfinder.unite"),
        ("Subtrair", "subtract", "object.pathfinder.minusFront"),
        ("Interseção", "intersect", "object.pathfinder.intersect"),
        (
            "Excluir sobreposição",
            "exclude",
            "object.pathfinder.exclude",
        ),
    ];
    ui.horizontal_wrapped(|ui| {
        for (name, mode, id) in modes {
            if ui.button(name).clicked() {
                if app.kit_live_shapes {
                    app.kit_command("object.compoundShape.make", json!({ "mode": mode }));
                } else {
                    app.kit_command(id, json!({}));
                }
            }
        }
    });
    if app.kit_live_shapes {
        ui.horizontal_wrapped(|ui| {
            if ui.button("Expandir forma composta").clicked() {
                app.kit_command("object.compoundShape.expand", json!({}));
            }
            if ui.button("Soltar forma composta").clicked() {
                app.kit_command("object.compoundShape.release", json!({}));
            }
        });
    }
    let cuts = [
        ("Dividir", "object.pathfinder.divide"),
        ("Aparar", "object.pathfinder.trim"),
        ("Mesclar", "object.pathfinder.merge"),
        ("Cortar", "object.pathfinder.crop"),
        ("Contorno", "object.pathfinder.outline"),
        ("Menos o de trás", "object.pathfinder.minusBack"),
    ];
    ui.horizontal_wrapped(|ui| {
        for (name, id) in cuts {
            if ui.button(name).clicked() {
                app.kit_command(id, json!({}));
            }
        }
    });
}

/// Aparência do objeto selecionado: vários preenchimentos e traços empilhados.
fn kit_appearance(app: &mut ArtApp, ui: &mut egui::Ui) {
    use serde_json::json;
    section(ui, "Aparência");
    let Some(rows) = app.kit.selected_appearance(&app.doc) else {
        ui.label(
            RichText::new("Selecione um objeto para ver e somar traços e preenchimentos.")
                .size(12.5)
                .color(color::TEXT_MUTED),
        );
        return;
    };
    let hex = |c: [f32; 3]| {
        let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
    };
    let mut cmd: Option<(&str, serde_json::Value)> = None;
    // De cima para baixo, como o painel do Illustrator; o índice continua o da pintura.
    for (i, row) in rows.iter().enumerate().rev() {
        ui.horizontal(|ui| {
            let mut visible = row.visible;
            if ui.checkbox(&mut visible, "").changed() {
                cmd = Some((
                    "appearance.setItem",
                    json!({"index": i, "visible": visible}),
                ));
            }
            ui.label(if row.fill { "Preenchimento" } else { "Traço" });
            if let Some(c) = row.color {
                let mut c32 = Color32::from_rgb(
                    (c[0] * 255.0) as u8,
                    (c[1] * 255.0) as u8,
                    (c[2] * 255.0) as u8,
                );
                if egui::color_picker::color_edit_button_srgba(
                    ui,
                    &mut c32,
                    egui::color_picker::Alpha::Opaque,
                )
                .changed()
                {
                    let f = |v: u8| f32::from(v) / 255.0;
                    let c = hex([f(c32.r()), f(c32.g()), f(c32.b())]);
                    cmd = Some(("appearance.setItem", json!({"index": i, "color": c})));
                }
            } else if ui
                .small_button("Cor")
                .on_hover_text("Pinta com a cor principal")
                .clicked()
            {
                let [r, g, b, _] = crate::app::color_to_f32(app.color);
                cmd = Some((
                    "appearance.setItem",
                    json!({"index": i, "color": hex([r, g, b])}),
                ));
            }
            if !row.fill {
                let mut w = row.width;
                if ui
                    .add(
                        egui::DragValue::new(&mut w)
                            .range(0.0..=500.0)
                            .speed(0.2)
                            .suffix(" pt"),
                    )
                    .changed()
                {
                    cmd = Some(("appearance.setItem", json!({"index": i, "weight": w})));
                }
            }
            if ui
                .small_button(icon::X.to_string())
                .on_hover_text("Tirar")
                .clicked()
            {
                cmd = Some(("appearance.removeItem", json!({ "index": i })));
            }
        });
    }
    ui.horizontal_wrapped(|ui| {
        if ui.button("+ Traço").clicked() {
            cmd = Some(("appearance.addStroke", json!({})));
        }
        if ui.button("+ Preenchimento").clicked() {
            cmd = Some(("appearance.addFill", json!({})));
        }
        if ui.button("Só o básico").clicked() {
            cmd = Some(("appearance.reduceToBasic", json!({})));
        }
    });
    if let Some((id, p)) = cmd {
        app.kit_command(id, p);
    }
}

/// Símbolos: guardar a seleção como símbolo e pôr cópias ligadas (mudar o símbolo muda todas).
fn kit_symbols(app: &mut ArtApp, ui: &mut egui::Ui) {
    use serde_json::json;
    section(ui, "Símbolos");
    let mut cmd: Option<(&str, serde_json::Value)> = None;
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                app.kit.has_selection(),
                egui::Button::new("Novo da seleção"),
            )
            .clicked()
        {
            cmd = Some(("symbol.new", json!({})));
        }
        if ui
            .add_enabled(app.kit.has_selection(), egui::Button::new("Soltar cópia"))
            .on_hover_text("A cópia vira arte comum, sem ligação com o símbolo.")
            .clicked()
        {
            cmd = Some(("symbol.breakLink", json!({})));
        }
    });
    app.refresh_symbols();
    for name in app.kit_symbols.clone() {
        ui.horizontal(|ui| {
            ui.label(&name);
            if ui.small_button("Pôr").clicked() {
                cmd = Some(("symbol.place", json!({ "name": name })));
            }
            if ui
                .small_button(icon::TRASH.to_string())
                .on_hover_text("Apagar o símbolo")
                .clicked()
            {
                cmd = Some(("symbol.delete", json!({ "name": name })));
            }
        });
    }
    if let Some((id, p)) = cmd {
        app.kit_command(id, p);
    }
}

/// Alinhar e distribuir os objetos selecionados (painel Alinhar do VectorCraft).
fn kit_align(app: &mut ArtApp, ui: &mut egui::Ui) {
    use serde_json::json;
    section(ui, "Alinhar e distribuir");
    ui.checkbox(&mut app.align_to_canvas, "Em relação à tela")
        .on_hover_text("Desligado: alinha os objetos selecionados entre si.");
    let to = if app.align_to_canvas {
        "artboard"
    } else {
        "selection"
    };
    let aligns = [
        ("Esquerda", json!({"horizontal": "left", "to": to})),
        ("Centro H", json!({"horizontal": "center", "to": to})),
        ("Direita", json!({"horizontal": "right", "to": to})),
        ("Topo", json!({"vertical": "top", "to": to})),
        ("Centro V", json!({"vertical": "center", "to": to})),
        ("Base", json!({"vertical": "bottom", "to": to})),
    ];
    ui.horizontal_wrapped(|ui| {
        for (name, p) in aligns {
            if ui.button(name).clicked() {
                app.kit_command("object.align", p);
            }
        }
    });
    let distrib = [
        (
            "Distribuir na horizontal",
            "object.distribute",
            json!({"horizontal": "center", "to": to}),
        ),
        (
            "Distribuir na vertical",
            "object.distribute",
            json!({"vertical": "center", "to": to}),
        ),
        (
            "Espaço igual (H)",
            "object.distributeSpacing",
            json!({"axis": "horizontal", "to": to}),
        ),
        (
            "Espaço igual (V)",
            "object.distributeSpacing",
            json!({"axis": "vertical", "to": to}),
        ),
    ];
    ui.horizontal_wrapped(|ui| {
        for (name, id, p) in distrib {
            if ui.button(name).clicked() {
                app.kit_command(id, p);
            }
        }
    });
}

/// Pontas, cantos e tracejado do traço dos objetos selecionados.
fn kit_stroke_style(app: &mut ArtApp, ui: &mut egui::Ui) {
    use serde_json::json;
    section(ui, "Estilo do traço");
    let rows: [&[(&str, serde_json::Value)]; 3] = [
        &[
            ("Ponta reta", json!({"cap": "butt"})),
            ("Ponta redonda", json!({"cap": "round"})),
            ("Ponta quadrada", json!({"cap": "square"})),
        ],
        &[
            ("Canto vivo", json!({"join": "miter"})),
            ("Canto redondo", json!({"join": "round"})),
            ("Canto chanfrado", json!({"join": "bevel"})),
        ],
        &[
            ("Contínuo", json!({"dash": null})),
            ("Tracejado", json!({"dash": [12, 6]})),
            ("Pontilhado", json!({"dash": [1, 8], "cap": "round"})),
            (
                "Traço e ponto",
                json!({"dash": [14, 5, 1, 5], "cap": "round"}),
            ),
        ],
    ];
    for row in rows {
        ui.horizontal_wrapped(|ui| {
            for (name, p) in row {
                if ui.button(*name).clicked() {
                    app.kit_command("stroke.set", p.clone());
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
        if ui
            .selectable_label(app.symmetry.is_none() && !app.symmetry_radial, "Desligada")
            .clicked()
        {
            app.symmetry = None;
            app.symmetry_radial = false;
        }
        for (mode, name) in [
            (SymmetryMode::Vertical, "Vertical"),
            (SymmetryMode::Horizontal, "Horizontal"),
            (SymmetryMode::Dual, "Quatro lados"),
            (SymmetryMode::Diagonal, "Diagonal"),
        ] {
            if ui
                .selectable_label(!app.symmetry_radial && app.symmetry == Some(mode), name)
                .clicked()
            {
                app.symmetry = Some(mode);
                app.symmetry_radial = false;
            }
        }
        if ui.selectable_label(app.symmetry_radial, "Radial").clicked() {
            app.symmetry_radial = true;
        }
    });
    if app.symmetry_radial {
        ui.add(
            egui::Slider::new(&mut app.radial_count, 2..=artapp_core::symmetry::RADIAL_MAX)
                .text("Fatias"),
        );
        theme::switch(ui, &mut app.radial_mirror, "Espelhar dentro das fatias");
    }
    if app.symmetry.is_some() || app.symmetry_radial {
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
        if ui
            .button(format!("{}  Assunto", icon::WAND_SPARKLES))
            .on_hover_text("Tudo que não é o fundo liso em volta (usa a tolerância)")
            .clicked()
        {
            let sel = app.doc.select_subject(app.wand_tolerance);
            if sel.is_none() {
                app.status = "Não achei um assunto: a imagem está vazia ou sem fundo liso.".into();
            }
            app.doc.combine_selection(sel, app.select_mode);
        }
        if ui
            .button(format!("{}  Pela cor", icon::DROPLET))
            .on_hover_text("As cores perto da cor principal, na imagem toda (usa a tolerância)")
            .clicked()
        {
            let [r, g, b, _] = crate::app::color_to_f32(app.color);
            let sel = app
                .doc
                .select_color_range([r, g, b], app.wand_tolerance.max(0.05));
            if sel.is_none() {
                app.status = "Nenhum pixel com essa cor.".into();
            }
            app.doc.combine_selection(sel, app.select_mode);
        }
    });
    saved_selections(app, ui);
    if app.tool != Tool::MagicWand {
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
    }
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

/// Guardar a seleção atual e trazer de volta as guardadas (com o modo escolhido).
fn saved_selections(app: &mut ArtApp, ui: &mut egui::Ui) {
    if app.doc.selection.is_some()
        && ui
            .button(format!("{}  Guardar seleção", icon::SAVE))
            .clicked()
        && let Some(sel) = app.doc.selection.clone()
    {
        let name = format!("Seleção {}", app.saved_selections.len() + 1);
        app.saved_selections.push((name, sel));
    }
    let mut load = None;
    let mut remove = None;
    for (i, (name, _)) in app.saved_selections.iter().enumerate() {
        ui.horizontal(|ui| {
            if ui.button(name.as_str()).on_hover_text("Carregar").clicked() {
                load = Some(i);
            }
            if ui
                .small_button(icon::X.to_string())
                .on_hover_text("Esquecer")
                .clicked()
            {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = load
        && let Some((_, sel)) = app.saved_selections.get(i)
    {
        let sel = sel.clone();
        app.doc.combine_selection(Some(sel), app.select_mode);
    }
    if let Some(i) = remove {
        app.saved_selections.remove(i);
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

/// Editor da curva de pressão: arraste os pontos; toque num lugar vazio para pôr um ponto;
/// arraste um ponto para fora da caixa para tirá-lo. Devolve se é hora de gravar.
fn pressure_curve(ui: &mut egui::Ui, points: &mut Vec<[f32; 2]>) -> bool {
    use artapp_core::pressure;
    let mut save = false;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Curva de pressão")
                .size(12.0)
                .color(color::TEXT_MUTED),
        );
        if !points.is_empty() && ui.small_button("Redefinir").clicked() {
            points.clear();
            save = true;
        }
    });
    let side = ui.available_width().min(220.0);
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(side, side * 0.62), Sense::click_and_drag());
    let to_screen = |p: [f32; 2]| {
        Pos2::new(
            rect.left() + p[0] * rect.width(),
            rect.bottom() - p[1] * rect.height(),
        )
    };
    let from_screen = |q: Pos2| {
        [
            (q.x - rect.left()) / rect.width(),
            (rect.bottom() - q.y) / rect.height(),
        ]
    };
    let drag_id = ui.scope_id().with("pcurve_drag");
    if resp.drag_started()
        && let Some(q) = resp.interact_pointer_pos()
    {
        let near = points.iter().position(|p| to_screen(*p).distance(q) < 16.0);
        let idx = near.or_else(|| {
            (points.len() < pressure::MAX_POINTS).then(|| {
                points.push(from_screen(q));
                pressure::sanitize(points);
                let x = from_screen(q)[0];
                points
                    .iter()
                    .enumerate()
                    .min_by(|a, b| (a.1[0] - x).abs().total_cmp(&(b.1[0] - x).abs()))
                    .map_or(0, |(i, _)| i)
            })
        });
        ui.ctx().memory_mut(|m| m.data.insert_temp(drag_id, idx));
    }
    let dragging = ui
        .ctx()
        .memory(|m| m.data.get_temp::<Option<usize>>(drag_id))
        .flatten();
    let mut outside = false;
    if let (Some(i), Some(q)) = (dragging, resp.interact_pointer_pos())
        && i < points.len()
    {
        outside = !rect.expand(24.0).contains(q);
        // O ponto fica entre os vizinhos, então a curva nunca desce enquanto arrasta.
        let lo = if i == 0 { [0.0, 0.0] } else { points[i - 1] };
        let hi = points.get(i + 1).copied().unwrap_or([1.0, 1.0]);
        let p = from_screen(q);
        points[i] = [
            p[0].clamp(lo[0] + 0.02, hi[0] - 0.02),
            p[1].clamp(lo[1], hi[1]),
        ];
    }
    if resp.drag_stopped() {
        if let Some(i) = dragging
            && outside
            && i < points.len()
        {
            points.remove(i);
        }
        pressure::sanitize(points);
        ui.ctx()
            .memory_mut(|m| m.data.remove::<Option<usize>>(drag_id));
        save = true;
    }
    if resp.clicked()
        && let Some(q) = resp.interact_pointer_pos()
        && points.len() < pressure::MAX_POINTS
        && !points.iter().any(|p| to_screen(*p).distance(q) < 16.0)
    {
        points.push(from_screen(q));
        pressure::sanitize(points);
        save = true;
    }
    let painter = ui.painter_at(rect.expand(8.0));
    painter.rect_filled(rect, 6.0, color::SURFACE);
    for k in 1..4 {
        let t = k as f32 / 4.0;
        let stroke = Stroke::new(1.0, color::BORDER);
        painter.line_segment([to_screen([t, 0.0]), to_screen([t, 1.0])], stroke);
        painter.line_segment([to_screen([0.0, t]), to_screen([1.0, t])], stroke);
    }
    let curve: Vec<Pos2> = (0..=48)
        .map(|k| {
            let x = k as f32 / 48.0;
            to_screen([x, pressure::apply(points, x)])
        })
        .collect();
    painter.add(egui::Shape::line(curve, Stroke::new(2.0, color::ACCENT)));
    for (i, p) in points.iter().enumerate() {
        let fill = if dragging == Some(i) && outside {
            color::TEXT_MUTED
        } else {
            color::ACCENT
        };
        painter.circle(to_screen(*p), 6.0, fill, Stroke::new(1.5, color::TEXT));
    }
    save
}

fn brush_settings(app: &mut ArtApp, ui: &mut egui::Ui) {
    use crate::brush_editor::{self, BrushTab};
    let eraser = app.tool == Tool::Eraser;
    let tab = if eraser {
        const TABS: [BrushTab; 7] = [
            BrushTab::Quick,
            BrushTab::General,
            BrushTab::Shape,
            BrushTab::Grain,
            BrushTab::Dynamics,
            BrushTab::Smoothing,
            BrushTab::Advanced,
        ];
        brush_editor::tab_bar(ui, "eraser", &TABS)
    } else {
        const SIMPLE: [BrushTab; 3] = [BrushTab::Library, BrushTab::Quick, BrushTab::General];
        const TABS: [BrushTab; 8] = [
            BrushTab::Library,
            BrushTab::Quick,
            BrushTab::General,
            BrushTab::Shape,
            BrushTab::Grain,
            BrushTab::Dynamics,
            BrushTab::Smoothing,
            BrushTab::Advanced,
        ];
        if app.simple_mode {
            brush_editor::tab_bar(ui, "brush-simple", &SIMPLE)
        } else {
            brush_editor::tab_bar(ui, "brush", &TABS)
        }
    };
    if tab != BrushTab::Library {
        let b = if eraser { &app.eraser } else { &app.brush };
        live_preview(ui, b);
    }
    match tab {
        BrushTab::Library => brush_library(app, ui),
        BrushTab::General => brush_general(app, ui),
        BrushTab::Quick => {
            let max = photocraft_max_size().min(1000.0);
            let mut none = artapp_core::Taper::default();
            let (b, taper) = if eraser {
                (&mut app.eraser, &mut none)
            } else {
                (&mut app.brush, &mut app.taper)
            };
            if brush_editor::quick_tab(ui, b, taper, max, &mut app.brush_quick) {
                app.save_prefs();
            }
        }
        BrushTab::Advanced => {
            let b = if eraser {
                &mut app.eraser
            } else {
                &mut app.brush
            };
            brush_editor::show_tab(ui, tab, b, &mut app.taper);
            ui.add_space(4.0);
            symmetry_settings(app, ui);
        }
        _ => {
            if eraser {
                let mut none = artapp_core::Taper::default();
                brush_editor::show_tab(ui, tab, &mut app.eraser, &mut none);
            } else {
                brush_editor::show_tab(ui, tab, &mut app.brush, &mut app.taper);
            }
        }
    }
    let pick = Id::unique(brush_editor::PICK_EXTRA_TIP);
    if ui
        .ctx()
        .data_mut(|d| d.remove_temp::<bool>(pick))
        .unwrap_or(false)
    {
        app.pick_image_for(crate::projects::ImageUse::ExtraTip);
    }
    if !eraser && tab != BrushTab::Library {
        ui.add_space(6.0);
        brush_save_row(app, ui);
    }
}

/// Faixa de rascunho no topo das abas de ajuste: o traço do pincel como está agora, refeito a
/// cada mudança (como a área de teste do Brush Studio do Procreate).
fn live_preview(ui: &mut egui::Ui, b: &artapp_core::BrushSettings) {
    let id = Id::unique("brush-live-preview");
    let w = (ui.available_width() - 4.0).clamp(120.0, 600.0);
    let (pw, ph) = (w.round() as u32, 54u32);
    let cached: Option<(artapp_core::BrushSettings, u32, TextureHandle)> =
        ui.ctx().data(|d| d.get_temp(id));
    let tex = match cached {
        Some((s, cw, t)) if s == *b && cw == pw => t,
        _ => {
            let px = artapp_core::brushes::preview(b, pw, ph);
            let img = egui::ColorImage::from_rgba_unmultiplied(
                [pw as usize, ph as usize],
                px.as_flattened(),
            );
            let t = ui
                .ctx()
                .load_texture("brush-live", img, egui::TextureOptions::LINEAR);
            ui.ctx()
                .data_mut(|d| d.insert_temp(id, (b.clone(), pw, t.clone())));
            t
        }
    };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, ph as f32), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(8), color::SURFACE_HOVER);
    ui.painter().image(
        tex.id(),
        rect,
        egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
    ui.add_space(4.0);
}

/// Aba Geral: tamanho, opacidade, fluxo, dureza, mesclagem e pressão (e a borracha mágica).
fn brush_general(app: &mut ArtApp, ui: &mut egui::Ui) {
    let max = photocraft_max_size().min(1000.0);
    let eraser = app.tool == Tool::Eraser;
    {
        let b = if eraser {
            &mut app.eraser
        } else {
            &mut app.brush
        };
        crate::brush_editor::basics(ui, b, max);
        crate::brush_editor::blend_mode(ui, b);
    }
    section(ui, "Pressão da caneta");
    if theme::switch(ui, &mut app.use_pressure, "Usar pressão da caneta").changed() {
        app.save_prefs();
    }
    if app.use_pressure {
        let key = app.current_brush_key();
        if let Some(k) = &key {
            let mut own = app.brush_curves.contains_key(k);
            if theme::switch(ui, &mut own, "Curva própria deste pincel").changed() {
                if own {
                    app.brush_curves
                        .insert(k.clone(), app.pressure_curve.clone());
                } else {
                    app.brush_curves.remove(k);
                }
                app.save_prefs();
            }
        }
        let curve = match key.filter(|k| app.brush_curves.contains_key(k)) {
            Some(k) => app.brush_curves.get_mut(&k),
            None => Some(&mut app.pressure_curve),
        };
        if curve.is_some_and(|c| pressure_curve(ui, c)) {
            app.save_prefs();
        }
    }
    if theme::switch(
        ui,
        &mut app.speed_pressure,
        "Pressão pela velocidade (dedo)",
    )
    .changed()
    {
        app.save_prefs();
    }
    if app.speed_pressure {
        ui.label(
            RichText::new("Sem caneta: devagar faz o traço cheio, rápido faz fino.")
                .size(12.0)
                .color(color::TEXT_MUTED),
        );
    }
    if eraser {
        section(ui, "Borracha mágica");
        theme::switch(ui, &mut app.eraser_magic, "Toque apaga a cor parecida");
        if app.eraser_magic {
            ui.add(egui::Slider::new(&mut app.wand_tolerance, 0.0..=1.0).text("Tolerância"));
            ui.checkbox(&mut app.wand_contiguous, "Só a área contígua");
        }
    }
}

/// Ícone de cada categoria da biblioteca.
fn category_icon(group: &str) -> char {
    match group {
        "Esboço" => icon::PEN_LINE,
        "Arte-final" => icon::PEN_TOOL,
        "Marcadores" => icon::SQUARE_PEN,
        "Caligrafia" => icon::TYPE,
        "Pintura" => icon::BRUSH,
        "Aquarela" => icon::DROPLET,
        "Aerógrafo" => icon::DROPLETS,
        "Mistura" => icon::PAINT_ROLLER,
        "Texturas" => icon::SQUARE_DASHED,
        "Decoração" => icon::SPARKLES,
        "Quadrinhos" => icon::FILM,
        "Borrachas" => icon::ERASER,
        _ => icon::STAMP,
    }
}

/// Grupos da biblioteca que não são categorias: montados a partir das preferências.
const RECENT: &str = "Recentes";
const FAVORITES: &str = "Favoritos";
/// Estrela dos favoritos (da fonte padrão do egui; a de ícones não tem estrela).
const STAR: char = '★';

/// Aba Pincéis: busca, categorias em botões grandes (fácil no dedo) e os pincéis da categoria.
/// No topo ficam Recentes e Favoritos, como no Procreate.
fn brush_library(app: &mut ArtApp, ui: &mut egui::Ui) {
    use artapp_core::brushes::{CATEGORIES, CUSTOM_GROUP};
    let key = Id::unique("brush-category");
    let search_key = Id::unique("brush-search");
    let current_group = app
        .brushes
        .get(app.brush_index)
        .map(|b| b.group.clone())
        .unwrap_or_default();
    let mut cat: String = ui
        .ctx()
        .data_mut(|d| d.get_persisted::<String>(key))
        .unwrap_or_else(|| current_group.clone());
    let mut search: String = ui
        .ctx()
        .data_mut(|d| d.get_temp::<String>(search_key))
        .unwrap_or_default();
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut search)
                .hint_text("Buscar: nome, categoria ou jeito (pressão, textura…)")
                .desired_width(ui.available_width() - 40.0),
        );
        if !search.is_empty() && ui.button(icon::X.to_string()).clicked() {
            search.clear();
        }
    });
    ui.ctx()
        .data_mut(|d| d.insert_temp(search_key, search.clone()));

    // Só as categorias da ferramenta pronta em uso (Lápis, Caneta…); a busca olha todas.
    let kit = app.current_kit();
    let in_kit = |g: &str| crate::app::BrushKit::of(g) == kit;
    let recent: Vec<usize> = app
        .brushes_by_keys(&app.brush_recent)
        .into_iter()
        .filter(|&i| in_kit(&app.brushes[i].group))
        .collect();
    let favorites: Vec<usize> = app
        .brushes_by_keys(&app.brush_favorites)
        .into_iter()
        .filter(|&i| in_kit(&app.brushes[i].group))
        .collect();
    let mut cats: Vec<String> = Vec::new();
    if !recent.is_empty() {
        cats.push(RECENT.into());
    }
    if !favorites.is_empty() {
        cats.push(FAVORITES.into());
    }
    cats.extend(
        CATEGORIES
            .iter()
            .filter(|c| in_kit(c))
            .filter(|c| **c == CUSTOM_GROUP || app.brushes.iter().any(|b| b.group == **c))
            .map(|c| c.to_string()),
    );
    for b in &app.brushes {
        if in_kit(&b.group) && !cats.contains(&b.group) {
            cats.push(b.group.clone());
        }
    }
    if !cats.contains(&cat) {
        cat = if cats.contains(&current_group) {
            current_group.clone()
        } else {
            cats.first().cloned().unwrap_or_default()
        };
    }
    let query = search.trim().to_lowercase();
    if query.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(4.0);
            for c in &cats {
                let on = *c == cat;
                let glyph = match c.as_str() {
                    RECENT => icon::ROTATE_CCW,
                    FAVORITES => STAR,
                    _ => category_icon(c),
                };
                let mut text = RichText::new(format!("{glyph}  {c}")).size(13.5);
                if on {
                    text = text.strong().color(color::ON_ACCENT);
                } else if *c == current_group {
                    // Mostra onde está o pincel em uso.
                    text = text.color(color::TEXT);
                }
                if ui
                    .add(
                        egui::Button::new(text)
                            .selected(on)
                            .min_size(Vec2::new(0.0, 40.0)),
                    )
                    .clicked()
                {
                    cat.clone_from(c);
                }
            }
        });
        ui.ctx().data_mut(|d| d.insert_persisted(key, cat.clone()));
        if cat == CUSTOM_GROUP {
            brush_folder_row(app, ui);
        }
    }
    ui.separator();

    // Os pincéis mostrados, na ordem certa.
    let shown: Vec<usize> = if !query.is_empty() {
        (0..app.brushes.len())
            .filter(|&i| {
                let b = &app.brushes[i];
                b.matches(&query)
            })
            .collect()
    } else if cat == RECENT {
        recent
    } else if cat == FAVORITES {
        favorites
    } else {
        let folder = (cat == CUSTOM_GROUP).then_some(app.brush_folder.as_str());
        (0..app.brushes.len())
            .filter(|&i| {
                let b = &app.brushes[i];
                b.group == cat && folder.is_none_or(|f| f.is_empty() || b.folder == f)
            })
            .collect()
    };

    let mut picked = None;
    let mut remove = None;
    let mut star = None;
    let mut dup = None;
    let mut rename = None;
    let mut move_to = None;
    let previews = app.brush_previews(ui.ctx());
    for &i in &shown {
        let entry = &app.brushes[i];
        let fav = app.is_favorite(entry);
        let label = match (fav, !query.is_empty()) {
            (true, true) => format!("{STAR}  {}  ·  {}", entry.name, entry.group),
            (true, false) => format!("{STAR}  {}", entry.name),
            (false, true) => format!("{}  ·  {}", entry.name, entry.group),
            (false, false) => entry.name.clone(),
        };
        // Em "Meus pincéis" sem pasta escolhida, mostra a pasta de cada um.
        let label = if query.is_empty()
            && cat == CUSTOM_GROUP
            && app.brush_folder.is_empty()
            && !entry.folder.is_empty()
        {
            format!("{label}  ·  {}", entry.folder)
        } else {
            label
        };
        let selected = app.brush_index == i;
        let row = brush_row(ui, &label, previews.get(i), selected);
        if row.clicked() {
            picked = Some(i);
        }
        let custom = entry.group == CUSTOM_GROUP;
        // Segurar o dedo (ou botão direito) abre as opções do pincel.
        row.context_menu(|ui| {
            let text = if fav {
                "☆  Tirar dos favoritos".to_string()
            } else {
                format!("{STAR}  Favoritar")
            };
            if ui.button(text).clicked() {
                star = Some(i);
                ui.close();
            }
            if ui.button(format!("{}  Duplicar", icon::COPY)).clicked() {
                dup = Some(i);
                ui.close();
            }
            if custom {
                let id = Id::unique(("brush-rename", i));
                let mut text: String = ui
                    .ctx()
                    .data_mut(|d| d.get_temp(id))
                    .unwrap_or_else(|| entry.name.clone());
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(140.0));
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (ui.button("Renomear").clicked() || enter) && !text.trim().is_empty() {
                        rename = Some((i, text.trim().to_string()));
                        ui.close();
                    }
                });
                ui.ctx().data_mut(|d| d.insert_temp(id, text));
                let id = Id::unique(("brush-folder", i));
                let mut text: String = ui
                    .ctx()
                    .data_mut(|d| d.get_temp(id))
                    .unwrap_or_else(|| entry.folder.clone());
                ui.horizontal(|ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut text)
                            .hint_text("Pasta")
                            .desired_width(140.0),
                    );
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let label = if text.trim().is_empty() {
                        "Tirar da pasta"
                    } else {
                        "Mover para pasta"
                    };
                    if ui.button(label).clicked() || enter {
                        move_to = Some((i, text.trim().to_string()));
                        ui.close();
                    }
                });
                ui.ctx().data_mut(|d| d.insert_temp(id, text));
            }
            if custom
                && ui
                    .button(format!("{}  Apagar pincel", icon::TRASH))
                    .clicked()
            {
                remove = Some(i);
                ui.close();
            }
        });
    }
    if shown.is_empty() {
        let msg = if query.is_empty() {
            "Nenhum pincel aqui ainda. Ajuste um pincel e toque em Salvar, ou importe pincéis."
        } else {
            "Nenhum pincel com esse nome."
        };
        ui.label(RichText::new(msg).size(12.0).color(color::TEXT_MUTED));
    } else if query.is_empty() && cat != FAVORITES {
        ui.label(
            RichText::new("Segure um pincel para favoritar ou duplicar.")
                .size(11.5)
                .color(color::TEXT_MUTED),
        );
    }
    if let Some(d) = app
        .brushes
        .get(app.brush_index)
        .and_then(|b| artapp_core::brushes::describe(&b.name))
    {
        ui.label(RichText::new(d).size(12.0).color(color::TEXT_MUTED));
    }
    if let Some(i) = star {
        app.toggle_favorite(i);
    }
    if let Some(i) = dup {
        app.duplicate_brush(i);
    }
    if let Some((i, name)) = rename {
        ui.ctx()
            .data_mut(|d| d.remove::<String>(Id::unique("brush-rename")));
        app.rename_brush(i, name);
    }
    if let Some((i, folder)) = move_to {
        app.move_brush_to_folder(i, folder);
    }
    if let Some(i) = picked {
        app.select_brush(i);
    }
    if let Some(i) = remove {
        app.delete_brush(i);
    }
    ui.add_space(6.0);
    brush_save_row(app, ui);
}

/// Pastas de "Meus pincéis": Todas, as que existem e uma nova. Pincel salvo ou duplicado entra
/// na pasta aberta.
fn brush_folder_row(app: &mut ArtApp, ui: &mut egui::Ui) {
    let mut folders = artapp_core::brushes::folders(&app.brushes);
    if !app.brush_folder.is_empty() && !folders.contains(&app.brush_folder) {
        // Pasta nova, ainda sem pincel.
        folders.push(app.brush_folder.clone());
    }
    let new_key = Id::unique("brush-new-folder");
    let mut new_name: Option<String> = ui.ctx().data_mut(|d| d.get_temp(new_key));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(4.0);
        ui.label(
            RichText::new(icon::FOLDER_OPEN.to_string())
                .size(13.0)
                .color(color::TEXT_MUTED),
        );
        let all = std::iter::once(String::new()).chain(folders);
        for f in all {
            let on = app.brush_folder == f;
            let text = if f.is_empty() { "Todas" } else { f.as_str() };
            let mut text = RichText::new(text).size(12.5);
            if on {
                text = text.strong().color(color::ON_ACCENT);
            }
            if ui
                .add(
                    egui::Button::new(text)
                        .selected(on)
                        .min_size(Vec2::new(0.0, 32.0)),
                )
                .clicked()
            {
                app.brush_folder = f;
            }
        }
        match &mut new_name {
            None => {
                if ui
                    .add(
                        egui::Button::new(format!("{}  Pasta", icon::PLUS))
                            .min_size(Vec2::new(0.0, 32.0)),
                    )
                    .on_hover_text("Cria uma pasta; os pincéis que você salvar entram nela.")
                    .clicked()
                {
                    new_name = Some(String::new());
                }
            }
            Some(name) => {
                let r = ui.add(
                    egui::TextEdit::singleline(name)
                        .hint_text("Nome da pasta")
                        .desired_width(120.0),
                );
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (ui.button("Criar").clicked() || enter) && !name.trim().is_empty() {
                    app.brush_folder = name.trim().to_string();
                    new_name = None;
                } else if ui.button(icon::X.to_string()).clicked() {
                    new_name = None;
                }
            }
        }
    });
    ui.ctx().data_mut(|d| match new_name {
        Some(n) => {
            d.insert_temp(new_key, n);
        }
        None => d.remove::<String>(new_key),
    });
}

/// Salvar como pincel novo, restaurar o preset e importar.
fn brush_save_row(app: &mut ArtApp, ui: &mut egui::Ui) {
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
    ui.horizontal_wrapped(|ui| {
        if ui.button("Restaurar ajustes do preset").clicked() {
            app.reset_brush();
        }
        if ui
            .button(format!("{}  Instalar pincéis…", icon::FOLDER_OPEN))
            .on_hover_text("Pincéis baixados: Clip Studio (.sut), Photoshop (.abr), pacotes .zip com eles, o pacote do ArtApp (.artbrushes) ou uma imagem, que vira a ponta. Começa na pasta Downloads.")
            .clicked()
        {
            app.pick_image_for(crate::projects::ImageUse::Brushes);
        }
        if ui
            .button("Exportar meus pincéis")
            .on_hover_text("Grava os pincéis de Meus pincéis num arquivo .artbrushes.")
            .clicked()
        {
            app.export_brushes();
        }
    });
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
    if ui
        .add(egui::Slider::new(&mut app.vector_opacity, 0.01..=1.0).text("Opacidade"))
        .changed()
    {
        app.save_prefs();
    }
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
        Tool::VectorPen => {
            section(ui, "Pressão da caneta");
            let mut changed = false;
            let pen = &mut app.vector_pen;
            changed |= theme::switch(ui, &mut pen.size, "Pressão muda a espessura").changed();
            if pen.size {
                changed |= ui
                    .add(egui::Slider::new(&mut pen.min_size, 0.0..=1.0).text("Espessura mínima"))
                    .changed();
            }
            changed |= theme::switch(ui, &mut pen.opacity, "Pressão muda a opacidade").changed();
            if pen.opacity {
                changed |= ui
                    .add(
                        egui::Slider::new(&mut pen.min_opacity, 0.0..=1.0).text("Opacidade mínima"),
                    )
                    .changed();
            }
            if !app.use_pressure {
                hint(
                    ui,
                    "A pressão está desligada no painel do pincel (Usar pressão da caneta).",
                );
            }
            if changed {
                app.save_prefs();
            }
            hint(
                ui,
                "Desenhe à mão livre: o traço vira curvas editáveis, com a largura da pressão.",
            );
        }
        Tool::FrameSplit => {
            section(ui, "Dividir quadro");
            ui.add(egui::Slider::new(&mut app.frame_gap, 0.0..=200.0).text("Calha (px)"));
            theme::switch(ui, &mut app.frame_snap, "Reta na horizontal/vertical");
            hint(
                ui,
                "Arraste atravessando um quadro da camada de requadros: ele vira dois, com a \
                 calha da cor do papel entre eles.",
            );
        }
        Tool::Balloon => {
            section(ui, "Balão");
            ui.horizontal_wrapped(|ui| {
                for shape in artapp_core::balloon::BalloonShape::ALL {
                    ui.selectable_value(&mut app.balloon_shape, shape, shape.label());
                }
            });
            hint(
                ui,
                "Arraste para desenhar o balão; depois toque onde o rabicho aponta. O traço usa \
                 a espessura acima e o balão fica branco por dentro.",
            );
            if app.balloon_pending.is_some()
                && ui.button(format!("{}  Sem rabicho", icon::CHECK)).clicked()
            {
                app.finish_balloon(None);
            }
        }
        _ => hint(ui, "Arraste na tela para desenhar a forma."),
    }
}

/// Lado dos botões da barra do painel de camadas: menores que os do resto da interface, todos no
/// topo, como na paleta de camadas do Clip Studio.
const LBTN: f32 = 32.0;
/// Altura de uma linha da lista de camadas.
const ROW_H: f32 = 48.0;

pub fn layers_panel(app: &mut ArtApp, place: Place<'_>) {
    let ev = panel_area(
        "layers_panel",
        place,
        icon::LAYERS,
        "Camadas",
        |ui, body_h| {
            let top = ui.cursor().min.y;
            let mut changed = layer_toolbar(app, ui);
            if app.layers_more {
                ui.separator();
                changed |= layer_extras(app, ui);
            }
            ui.separator();
            let used = ui.cursor().min.y - top;
            let list_h = (body_h - used - 4.0).max(120.0);
            changed |= layer_list(app, ui, list_h);
            if changed {
                app.invalidate_all();
            }
        },
    );
    panel_events(app, ev, false);
}

/// As camadas escolhidas: a ativa mais as marcadas, em ordem de pilha (de baixo para cima).
fn chosen_layers(app: &ArtApp) -> Vec<u64> {
    let active = app.doc.layers.get(app.doc.active).map(|l| l.id);
    app.doc
        .layers
        .iter()
        .map(|l| l.id)
        .filter(|id| Some(*id) == active || app.layer_sel.contains(id))
        .collect()
}

fn small_button(ui: &mut egui::Ui, g: char, tip: &str, on: bool, enabled: bool) -> bool {
    ui.add_enabled_ui(enabled, |ui| theme::icon_button_sized(ui, g, on, tip, LBTN))
        .inner
        .clicked()
}

/// A barra de cima: criar, agrupar, duplicar, mesclar, apagar, selecionar várias e "mais"; embaixo,
/// o modo de mesclagem, os travamentos e a opacidade. Com várias escolhidas, vale para todas.
fn layer_toolbar(app: &mut ArtApp, ui: &mut egui::Ui) -> bool {
    let mut changed = false;
    let chosen = chosen_layers(app);
    let many = chosen.len() > 1;
    let (is_folder, in_folder, is_mask) =
        app.doc.active_layer().map_or((false, false, false), |l| {
            (l.folder, l.parent.is_some(), l.mask)
        });
    let b = small_button;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        if b(ui, icon::PLUS, "Nova camada", false, true) {
            app.doc.add_layer();
            changed = true;
        }
        if b(ui, icon::SQUARE_PEN, "Nova camada vetorial", false, true) {
            app.doc.add_vector_layer();
            changed = true;
        }
        let tip = if many {
            "Agrupar as escolhidas numa pasta"
        } else {
            "Pôr numa pasta nova"
        };
        if b(
            ui,
            icon::FOLDER_PLUS,
            tip,
            false,
            many || (!is_folder && !in_folder && !is_mask),
        ) {
            changed |= if many {
                app.doc.group_layers(&chosen)
            } else {
                app.doc.new_folder()
            };
            app.layer_sel.clear();
        }
        if b(ui, icon::COPY, "Duplicar", false, !many) {
            app.doc.duplicate_layer();
            changed = true;
        }
        let tip = if many {
            "Mesclar as escolhidas (na mais baixa)"
        } else if is_folder {
            "Achatar a pasta"
        } else {
            "Mesclar abaixo"
        };
        if b(ui, icon::MERGE_DOWN, tip, false, many || app.doc.active > 0) {
            if many {
                changed |= app.doc.merge_layers(&chosen);
                app.layer_sel.clear();
            } else {
                app.doc.merge_down();
                changed = true;
            }
        }
        let tip = if many {
            "Apagar as escolhidas"
        } else {
            "Apagar camada"
        };
        if b(ui, icon::TRASH, tip, false, true) {
            if many {
                app.doc.delete_layers(&chosen);
                app.layer_sel.clear();
            } else {
                app.doc.delete_layer();
            }
            changed = true;
        }
        if b(
            ui,
            icon::CHECK,
            "Selecionar várias (toque marca e desmarca; também Ctrl/Shift ou segurar a camada)",
            app.layer_multi,
            true,
        ) {
            app.layer_multi = !app.layer_multi;
            if !app.layer_multi {
                app.layer_sel.clear();
            }
        }
        if b(
            ui,
            icon::ELLIPSIS,
            "Mais: nome, borda, retícula, subir/descer…",
            app.layers_more,
            true,
        ) {
            app.layers_more = !app.layers_more;
        }
    });
    changed | layer_props(app, ui, &chosen)
}

/// Modo de mesclagem, travamentos, máscara e opacidade (da ativa, ou de todas as escolhidas).
fn layer_props(app: &mut ArtApp, ui: &mut egui::Ui, chosen: &[u64]) -> bool {
    let many = chosen.len() > 1;
    let mask_now = app.doc.active_has_mask();
    let Some(l) = app.doc.active_layer() else {
        return false;
    };
    let folder = l.folder;
    let before = (
        l.blend,
        l.lock_alpha,
        l.clip,
        l.reference,
        l.draft,
        l.visible,
    );
    let (mut blend, mut lock, mut clip, mut reference, mut draft, mut visible) = before;
    let mut pct = l.opacity * 100.0;
    let (mut add_mask, mut apply_mask) = (false, false);
    let (mut drag_start, mut op_changed) = (false, false);
    let b = small_button;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
        if !folder {
            egui::ComboBox::from_id_salt("blend")
                .selected_text(blend.label())
                .width(92.0)
                .show_ui(ui, |ui| {
                    for m in BlendMode::layer_modes().filter(|m| *m != BlendMode::Dissolve) {
                        ui.selectable_value(&mut blend, m, m.label());
                    }
                });
            ui.add_space(4.0);
            let g = if lock { icon::LOCK } else { icon::UNLOCK };
            if b(ui, g, "Bloquear transparência", lock, true) {
                lock = !lock;
            }
            if b(ui, icon::CLIP, "Recortar na camada abaixo", clip, true) {
                clip = !clip;
            }
            if b(
                ui,
                icon::CIRCLE_DASHED,
                "Máscara de camada (pinte de preto para esconder)",
                mask_now,
                !many,
            ) {
                add_mask = true;
            }
            if mask_now && !many && b(ui, icon::CHECK, "Aplicar a máscara", false, true) {
                apply_mask = true;
            }
            if b(
                ui,
                icon::REFERENCE,
                "Camada de referência (o balde olha só ela)",
                reference,
                true,
            ) {
                reference = !reference;
            }
            if b(
                ui,
                icon::DRAFT,
                "Camada de rascunho (fica fora da exportação)",
                draft,
                true,
            ) {
                draft = !draft;
            }
        }
        if many {
            let g = if visible { icon::EYE } else { icon::EYE_OFF };
            if b(ui, g, "Mostrar ou ocultar as escolhidas", false, true) {
                visible = !visible;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            RichText::new(icon::CONTRAST.to_string())
                .font(theme::icon_font(14.0))
                .color(color::TEXT_MUTED),
        )
        .on_hover_text("Opacidade");
        ui.spacing_mut().slider_width = (ui.available_width() - 64.0).max(60.0);
        let r = ui.add(
            egui::Slider::new(&mut pct, 0.0..=100.0)
                .suffix("%")
                .integer(),
        );
        drag_start = r.drag_started();
        op_changed = r.changed();
    });
    if add_mask {
        return app.doc.add_mask();
    }
    if apply_mask {
        return app.doc.apply_mask();
    }
    let now = (blend, lock, clip, reference, draft, visible);
    let toggled = now != before;
    if !toggled && !op_changed {
        return false;
    }
    // Um passo do histórico por toque; no arraste da opacidade, só no começo dele.
    if toggled || drag_start {
        app.doc.checkpoint();
    }
    let active = app.doc.active_layer().map(|l| l.id);
    for l in app
        .doc
        .layers
        .iter_mut()
        .filter(|l| Some(l.id) == active || (many && chosen.contains(&l.id)))
    {
        if op_changed {
            l.opacity = pct / 100.0;
        }
        if visible != before.5 {
            l.visible = visible;
        }
        if l.folder {
            continue;
        }
        if blend != before.0 && !l.mask {
            l.blend = blend;
        }
        if lock != before.1 {
            l.lock_alpha = lock;
        }
        if clip != before.2 && !l.mask {
            l.clip = clip;
        }
        if reference != before.3 && !l.mask {
            l.reference = reference;
        }
        if draft != before.4 && !l.mask {
            l.draft = draft;
        }
    }
    true
}

/// Ajustes que aparecem com "mais": nome, borda, retícula e as ações menos usadas.
fn layer_extras(app: &mut ArtApp, ui: &mut egui::Ui) -> bool {
    let mut changed = false;
    let mut checkpoint = false;
    let n = app.doc.layers.len();
    let is_vector = app.doc.active_is_vector();
    let [mr, mg, mb, _] = crate::app::color_to_f32(app.color);
    let Some(layer) = app.doc.layers.get_mut(app.doc.active) else {
        return false;
    };
    let is_folder = layer.folder;
    ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(ui.available_width()));
    if layer.mask {
        ui.label(
            RichText::new("Pintando na máscara: preto esconde, branco ou borracha mostra.")
                .size(12.0)
                .color(color::TEXT_MUTED),
        );
    }
    // Borda da camada: liga com a cor principal; a espessura e a cor mudam ao vivo.
    let mut border = layer.border;
    let mut tone = layer.tone;
    if !layer.mask && !is_folder {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if small_button(ui, icon::HEXAGON, "Borda da camada", border.is_some(), true) {
                border = match border {
                    Some(_) => None,
                    None => Some(artapp_core::border::Border {
                        width: 4.0,
                        color: [mr, mg, mb],
                    }),
                };
                checkpoint = true;
            }
            if let Some(b) = &mut border {
                let r = ui.add(
                    egui::Slider::new(&mut b.width, 1.0..=artapp_core::border::MAX_WIDTH)
                        .integer()
                        .suffix(" px"),
                );
                checkpoint |= r.drag_started();
                ui.color_edit_button_rgb(&mut b.color);
            } else {
                ui.label(RichText::new("Borda").size(12.0).color(color::TEXT_MUTED));
            }
        });
        // Retícula de quadrinho: a tinta vira pontos.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if small_button(
                ui,
                icon::GRID_3X3,
                "Retícula (quadrinho)",
                tone.is_some(),
                true,
            ) {
                tone = match tone {
                    Some(_) => None,
                    None => Some(artapp_core::tone::Tone::default()),
                };
                checkpoint = true;
            }
            if let Some(t) = &mut tone {
                let r = ui.add(
                    egui::Slider::new(
                        &mut t.period,
                        artapp_core::tone::MIN_PERIOD..=artapp_core::tone::MAX_PERIOD,
                    )
                    .integer()
                    .suffix(" px"),
                );
                checkpoint |= r.drag_started();
                let r = ui.add(
                    egui::DragValue::new(&mut t.angle)
                        .range(0.0..=90.0)
                        .suffix("°"),
                );
                checkpoint |= r.drag_started();
            } else {
                ui.label(
                    RichText::new("Retícula")
                        .size(12.0)
                        .color(color::TEXT_MUTED),
                );
            }
        });
    }
    if border != layer.border || tone != layer.tone {
        changed = true;
        if !checkpoint {
            layer.border = border;
            layer.tone = tone;
        }
    }
    if checkpoint {
        app.doc.checkpoint();
        if let Some(layer) = app.doc.active_layer_mut() {
            layer.border = border;
            layer.tone = tone;
        }
        changed = true;
    }
    let b = small_button;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        if b(ui, icon::CHEVRON_UP, "Subir", false, app.doc.active + 1 < n) {
            app.doc.move_layer(true);
            changed = true;
        }
        if b(ui, icon::CHEVRON_DOWN, "Descer", false, app.doc.active > 0) {
            app.doc.move_layer(false);
            changed = true;
        }
        if b(
            ui,
            icon::IMAGE,
            "Rasterizar camada vetorial",
            false,
            is_vector,
        ) {
            app.doc.rasterize_layer();
            changed = true;
        }
        if b(ui, icon::ERASER, "Limpar camada", false, !is_folder) {
            app.doc.clear_layer();
            changed = true;
        }
        if is_folder
            && b(
                ui,
                icon::FOLDER_OPEN,
                "Desfazer a pasta (tira as camadas)",
                false,
                true,
            )
        {
            app.doc.ungroup();
            changed = true;
        }
    });
    changed
}

/// Uma linha da lista na hora de soltar: retângulo, id, se é pasta.
struct RowHit {
    rect: egui::Rect,
    id: u64,
    folder: bool,
    index: usize,
    thumb: egui::Rect,
    grip: egui::Rect,
}

/// Um toque (ou clique) na lista de camadas, do encostar ao soltar.
#[derive(Clone, Copy, PartialEq)]
enum PressMode {
    /// Ainda não se mexeu: pode virar toque, segurar, rolar ou arrastar.
    Pending,
    /// Segurou parado (marcou a camada); mexer agora arrasta.
    Held,
    /// Deslizou logo: rola a lista.
    Scroll,
    /// Arrastando camadas.
    Drag,
}

#[derive(Clone, Copy)]
struct LayerPress {
    index: usize,
    at: Pos2,
    time: f64,
    mode: PressMode,
    on_grip: bool,
    on_thumb: bool,
    folder: bool,
    touch: bool,
}

/// Segurar parado este tempo marca a camada (modo de várias) e deixa arrastar.
const LAYER_HOLD: f64 = 0.45;
/// Antes disso, deslizar rola a lista; depois (ou pela alça, ou com o mouse), arrasta.
const LAYER_DRAG_AFTER: f64 = 0.25;
/// Quanto o dedo anda antes de contar como movimento.
const LAYER_SLOP: f32 = 8.0;

/// Onde as camadas arrastadas cairiam com o dedo/ponteiro em `p`, e a marca a desenhar (uma
/// linha, ou o contorno da pasta quando entram nela).
fn drop_target(
    rows: &[RowHit],
    moving: &[u64],
    p: Pos2,
) -> Option<(artapp_core::layer_batch::DropAt, egui::Rect)> {
    use artapp_core::layer_batch::DropAt;
    let line = |r: egui::Rect, y: f32| {
        egui::Rect::from_min_max(Pos2::new(r.left(), y - 1.5), Pos2::new(r.right(), y + 1.5))
    };
    let (first, last) = (rows.first()?, rows.last()?);
    if p.y < first.rect.top() {
        return (!moving.contains(&first.id))
            .then(|| (DropAt::Above(first.id), line(first.rect, first.rect.top())));
    }
    if p.y > last.rect.bottom() {
        return (!moving.contains(&last.id))
            .then(|| (DropAt::Below(last.id), line(last.rect, last.rect.bottom())));
    }
    let row = rows
        .iter()
        .find(|r| p.y >= r.rect.top() - 2.0 && p.y <= r.rect.bottom() + 2.0)?;
    if moving.contains(&row.id) {
        return None;
    }
    let f = (p.y - row.rect.top()) / row.rect.height();
    Some(if row.folder && (0.28..=0.72).contains(&f) {
        (DropAt::Into(row.id), row.rect)
    } else if f < 0.5 {
        (DropAt::Above(row.id), line(row.rect, row.rect.top()))
    } else {
        (DropAt::Below(row.id), line(row.rect, row.rect.bottom()))
    })
}

/// A lista. Tocar escolhe a camada (no modo de várias, Ctrl ou Shift, marca). Para mudar a
/// ordem: segurar a camada um instante e arrastar (ou arrastar direto pela alça ≡, ou com o
/// mouse). Deslizar rápido rola a lista. Segurar parado sem arrastar marca a camada (várias).
fn layer_list(app: &mut ArtApp, ui: &mut egui::Ui, list_h: f32) -> bool {
    let mut changed = false;
    if !app.layer_sel.is_empty() {
        let ids: Vec<u64> = app.doc.layers.iter().map(|l| l.id).collect();
        app.layer_sel.retain(|id| ids.contains(id));
    }
    let press_id = Id::unique("layer_press");
    let mut press: Option<LayerPress> = ui.ctx().data(|d| d.get_temp(press_id));
    let (pressed, down, pos, delta, now, touch, mods) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.interact_pos(),
            i.pointer.delta(),
            i.time,
            i.any_touches(),
            i.modifiers,
        )
    });
    let mut rows: Vec<RowHit> = Vec::new();
    let mut toggle = None;
    let mut clip = egui::Rect::NOTHING;
    let moving_now = app.layer_drag.clone();
    egui::ScrollArea::vertical()
        .id_salt("layer_list")
        .max_height(list_h)
        .auto_shrink([false, true])
        // Rolar com o dedo é feito aqui embaixo (para não brigar com segurar e arrastar).
        .scroll_source(egui::containers::scroll_area::ScrollSource {
            drag: egui::containers::scroll_area::DragScroll::Never,
            ..Default::default()
        })
        .show(ui, |ui| {
            clip = ui.clip_rect();
            let thumbs = app.layer_thumbnails(ui.ctx());
            let active = app.doc.active;
            let closed: Vec<u64> = app
                .doc
                .layers
                .iter()
                .filter(|l| l.folder && l.collapsed)
                .map(|l| l.id)
                .collect();
            for i in (0..app.doc.layers.len()).rev() {
                let Some(layer) = app.doc.layers.get(i) else {
                    continue;
                };
                // Pasta fechada: as de dentro não aparecem na lista.
                if layer.parent.is_some_and(|p| closed.contains(&p)) {
                    continue;
                }
                let state = RowState {
                    active: i == active,
                    picked: app.layer_sel.contains(&layer.id),
                    multi: app.layer_multi,
                    moving: moving_now.as_ref().is_some_and(|d| d.contains(&layer.id)),
                };
                let out = layer_row(ui, layer, thumbs.get(&layer.id), state);
                if out.eye {
                    toggle = Some(i);
                }
                rows.push(RowHit {
                    rect: out.rect,
                    id: layer.id,
                    folder: layer.folder,
                    index: i,
                    thumb: out.thumb,
                    grip: out.grip,
                });
            }
            // Encostou numa camada (não no olho, nem por baixo de outro painel).
            if pressed
                && let Some(p) = pos
                && clip.contains(p)
                && ui.ctx().layer_id_at(p) == Some(ui.layer_id())
                && let Some(r) = rows.iter().find(|r| r.rect.contains(p))
                && toggle.is_none()
                && !egui::Rect::from_center_size(
                    Pos2::new(r.rect.max.x - 20.0, r.rect.center().y),
                    Vec2::new(36.0, ROW_H),
                )
                .contains(p)
            {
                press = Some(LayerPress {
                    index: r.index,
                    at: p,
                    time: now,
                    mode: PressMode::Pending,
                    on_grip: r.grip.contains(p),
                    on_thumb: r.thumb.contains(p),
                    folder: r.folder,
                    touch,
                });
            }
            let Some(pr) = press.as_mut() else {
                return;
            };
            if !down {
                return;
            }
            let moved = pos.is_some_and(|p| p.distance(pr.at) > LAYER_SLOP);
            let held = now - pr.time;
            match pr.mode {
                PressMode::Pending if moved => {
                    pr.mode = if pr.on_grip || !pr.touch || held >= LAYER_DRAG_AFTER {
                        PressMode::Drag
                    } else {
                        PressMode::Scroll
                    };
                }
                PressMode::Pending if pr.touch && held >= LAYER_HOLD => {
                    // Segurou parado: marca a camada; arrastar agora leva as marcadas.
                    pr.mode = PressMode::Held;
                    pick_layer(app, pr.index, true, mods);
                }
                PressMode::Held if moved => pr.mode = PressMode::Drag,
                PressMode::Scroll => {
                    ui.scroll_with_delta(Vec2::new(0.0, delta.y));
                }
                _ => {}
            }
            if pr.mode == PressMode::Drag && app.layer_drag.is_none() {
                let id = app.doc.layers.get(pr.index).map(|l| l.id);
                let mut set = chosen_layers(app);
                if !id.is_some_and(|id| set.contains(&id)) {
                    set = id.into_iter().collect();
                }
                app.layer_drag = Some(set);
            }
            if pr.mode == PressMode::Pending {
                // Para notar o "segurou" mesmo sem o dedo se mexer.
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(50));
            }
            // Arrastando perto da beira: a lista rola sozinha.
            if pr.mode == PressMode::Drag
                && let Some(p) = pos
            {
                let edge = 32.0;
                let step = if p.y < clip.top() + edge {
                    8.0
                } else if p.y > clip.bottom() - edge {
                    -8.0
                } else {
                    0.0
                };
                if step != 0.0 {
                    ui.scroll_with_delta(Vec2::new(0.0, step));
                    ui.ctx().request_repaint();
                }
            }
        });
    if let Some(i) = toggle {
        app.doc.checkpoint();
        if let Some(l) = app.doc.layers.get_mut(i) {
            l.visible = !l.visible;
        }
        changed = true;
    }
    // Soltou.
    let released = press.is_some() && !down;
    let target = match (&app.layer_drag, pos) {
        (Some(moving), Some(p)) => drop_target(&rows, moving, p),
        _ => None,
    };
    if released && let Some(pr) = press.take() {
        match pr.mode {
            PressMode::Pending if pr.folder && pr.on_thumb && !app.layer_multi => {
                if let Some(l) = app.doc.layers.get_mut(pr.index) {
                    l.collapsed = !l.collapsed;
                }
            }
            PressMode::Pending => pick_layer(app, pr.index, false, mods),
            PressMode::Drag => {
                if let (Some(moving), Some((at, _))) = (app.layer_drag.take(), target) {
                    changed |= app.doc.move_layers(&moving, at);
                }
            }
            _ => {}
        }
        app.layer_drag = None;
    }
    if press.is_none() {
        app.layer_drag = None;
    }
    ui.ctx().data_mut(|d| {
        if let Some(p) = press {
            d.insert_temp(press_id, p);
        } else {
            d.remove::<LayerPress>(press_id);
        }
    });
    // Desenha onde vai cair e a etiqueta do que está sendo arrastado.
    let Some(moving) = app.layer_drag.clone() else {
        return changed;
    };
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        Order::Tooltip,
        Id::unique("layer_drop_mark"),
    ));
    if let Some((at, mark)) = target {
        if matches!(at, artapp_core::layer_batch::DropAt::Into(_)) {
            painter.rect_stroke(
                mark,
                CornerRadius::same(8),
                Stroke::new(2.0, color::ACCENT),
                egui::StrokeKind::Inside,
            );
        } else {
            painter.rect_filled(mark, CornerRadius::same(2), color::ACCENT);
        }
    }
    if let Some(p) = pos {
        let label = match moving.as_slice() {
            [one] => app
                .doc
                .index_of(*one)
                .and_then(|i| app.doc.layers.get(i))
                .map_or_else(String::new, |l| l.name.clone()),
            many => format!("{} camadas", many.len()),
        };
        let galley = painter.layout_no_wrap(label, egui::FontId::proportional(13.0), color::TEXT);
        // Acima e à esquerda do dedo, para não ficar escondida embaixo dele.
        let r = egui::Rect::from_min_size(
            p + Vec2::new(-galley.size().x - 24.0, -galley.size().y - 30.0),
            galley.size() + Vec2::new(16.0, 8.0),
        );
        painter.rect_filled(r, CornerRadius::same(6), color::PANEL);
        painter.rect_stroke(
            r,
            CornerRadius::same(6),
            Stroke::new(1.0, color::ACCENT),
            egui::StrokeKind::Inside,
        );
        painter.galley(r.min + Vec2::new(8.0, 4.0), galley, color::TEXT);
    }
    ui.ctx().request_repaint();
    changed
}

/// Toque numa camada da lista: escolhe (e desmarca as outras), ou marca/desmarca no modo de
/// várias (botão, Ctrl ou segurar); Shift marca tudo entre a ativa e ela.
fn pick_layer(app: &mut ArtApp, i: usize, long: bool, mods: egui::Modifiers) {
    let Some(id) = app.doc.layers.get(i).map(|l| l.id) else {
        return;
    };
    let active_id = app.doc.active_layer().map(|l| l.id);
    if mods.shift && !long {
        let (a, b) = (app.doc.active.min(i), app.doc.active.max(i));
        for l in &app.doc.layers[a..=b] {
            if Some(l.id) != active_id && !app.layer_sel.contains(&l.id) {
                app.layer_sel.push(l.id);
            }
        }
        return;
    }
    if !(long || app.layer_multi || mods.command) {
        app.doc.active = i;
        app.selected = None;
        app.layer_sel.clear();
        return;
    }
    if long {
        app.layer_multi = true;
    }
    if Some(id) == active_id {
        // Desmarcar a ativa: outra marcada passa a ser a ativa.
        if long || app.layer_sel.is_empty() {
            return;
        }
        let next = app.layer_sel.remove(0);
        if let Some(k) = app.doc.index_of(next) {
            app.doc.active = k;
            app.selected = None;
        }
    } else if let Some(k) = app.layer_sel.iter().position(|x| *x == id) {
        if !long {
            app.layer_sel.remove(k);
        }
    } else {
        app.layer_sel.push(id);
    }
}

#[derive(Clone, Copy)]
struct RowState {
    active: bool,
    /// Marcada na seleção múltipla.
    picked: bool,
    /// Modo de selecionar várias ligado (mostra a bolinha de marcar).
    multi: bool,
    /// Está sendo arrastada.
    moving: bool,
}

struct RowOut {
    rect: egui::Rect,
    /// Miniatura (numa pasta, abre e fecha) e alça de arrastar.
    thumb: egui::Rect,
    grip: egui::Rect,
    eye: bool,
}

/// Uma linha da lista de camadas.
fn layer_row(
    ui: &mut egui::Ui,
    layer: &artapp_core::Layer,
    thumb: Option<&TextureHandle>,
    st: RowState,
) -> RowOut {
    let w = ui.available_width();
    // Toques e arrastes da linha são tratados pela lista (`layer_list`), não pelo egui: assim
    // segurar e arrastar, rolar com o dedo e tocar não brigam entre si.
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, ROW_H), Sense::hover());
    let p = ui.painter();
    let bg = if st.active {
        color::SELECTION.gamma_multiply(0.25)
    } else if st.picked {
        color::SELECTION.gamma_multiply(0.14)
    } else if resp.hovered() {
        color::SURFACE_HOVER
    } else {
        Color32::TRANSPARENT
    };
    p.rect_filled(rect, CornerRadius::same(8), bg);
    if st.active || st.picked {
        p.rect_stroke(
            rect,
            CornerRadius::same(8),
            Stroke::new(
                1.0,
                color::SELECTION.gamma_multiply(if st.active { 1.0 } else { 0.6 }),
            ),
            egui::StrokeKind::Inside,
        );
    }
    // Bolinha de marcar, no modo de selecionar várias.
    let check_w = if st.multi { 24.0 } else { 0.0 };
    if st.multi {
        let c = Pos2::new(rect.left() + 13.0, rect.center().y);
        let on = st.active || st.picked;
        p.circle_stroke(
            c,
            8.0,
            Stroke::new(
                1.5,
                if on {
                    color::SELECTION
                } else {
                    color::TEXT_MUTED
                },
            ),
        );
        if on {
            p.circle_filled(c, 5.0, color::SELECTION);
        }
    }
    // Camada recortada: recuada, com uma barra ligando-a à base embaixo (como no Clip Studio).
    // Máscara: recuada também, com a barra azul da seleção.
    // Dentro de pasta: mais recuada ainda.
    let nest = check_w + if layer.parent.is_some() { 16.0 } else { 0.0 };
    let indent = nest + if layer.clip || layer.mask { 14.0 } else { 0.0 };
    if layer.clip || layer.mask {
        p.rect_filled(
            egui::Rect::from_min_size(
                rect.left_top() + Vec2::new(8.0 + nest, 4.0),
                Vec2::new(3.0, ROW_H - 8.0),
            ),
            CornerRadius::same(1),
            if layer.mask {
                color::SELECTION
            } else {
                color::ACCENT
            },
        );
    }
    let thumb_rect = egui::Rect::from_min_size(
        rect.left_top() + Vec2::new(6.0 + indent, (ROW_H - THUMB.y) / 2.0),
        THUMB,
    );
    if layer.folder {
        // Pasta: o ícone no lugar da miniatura, e uma seta para abrir e fechar.
        p.text(
            thumb_rect.center() - Vec2::new(4.0, 0.0),
            Align2::CENTER_CENTER,
            icon::FOLDER_OPEN,
            theme::icon_font(24.0),
            color::ACCENT,
        );
        p.text(
            Pos2::new(thumb_rect.max.x - 2.0, thumb_rect.center().y),
            Align2::CENTER_CENTER,
            if layer.collapsed {
                icon::CHEVRON_UP
            } else {
                icon::CHEVRON_DOWN
            },
            theme::icon_font(14.0),
            color::TEXT_MUTED,
        );
    } else {
        p.rect_filled(thumb_rect, CornerRadius::same(4), Color32::WHITE);
    }
    if let Some(t) = thumb.filter(|_| !layer.folder) {
        p.image(
            t.id(),
            thumb_rect,
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    if !layer.folder {
        p.rect_stroke(
            thumb_rect,
            CornerRadius::same(4),
            Stroke::new(1.0, color::BORDER),
            egui::StrokeKind::Outside,
        );
    }
    let text_x = thumb_rect.max.x + 10.0;
    let fg = if layer.visible {
        color::TEXT
    } else {
        color::TEXT_MUTED
    };
    // O nome não passa por baixo da alça e do olho.
    let text_clip = egui::Rect::from_min_max(
        Pos2::new(text_x, rect.top()),
        Pos2::new(rect.max.x - 70.0, rect.bottom()),
    );
    let tp = p.with_clip_rect(text_clip.intersect(p.clip_rect()));
    tp.text(
        Pos2::new(text_x, rect.center().y - 8.0),
        Align2::LEFT_CENTER,
        &layer.name,
        egui::FontId::proportional(14.0),
        fg,
    );
    let mut sub = format!("{:.0}%  ·  {}", layer.opacity * 100.0, layer.blend.label());
    if layer.is_vector() {
        sub = format!("vetor  ·  {sub}");
    }
    for (on, glyph) in [
        (layer.lock_alpha, icon::LOCK),
        (layer.reference, icon::REFERENCE),
        (layer.draft, icon::DRAFT),
    ] {
        if on {
            sub.push_str(&format!("  {glyph}"));
        }
    }
    tp.text(
        Pos2::new(text_x, rect.center().y + 10.0),
        Align2::LEFT_CENTER,
        sub,
        egui::FontId::proportional(11.0),
        color::TEXT_MUTED,
    );
    // Olho (mostrar/ocultar) e, à esquerda dele, a alça de arrastar.
    let eye_rect = egui::Rect::from_center_size(
        Pos2::new(rect.max.x - 20.0, rect.center().y),
        Vec2::new(36.0, ROW_H),
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
        theme::icon_font(17.0),
        eye_col,
    );
    let grip_rect = egui::Rect::from_center_size(
        Pos2::new(rect.max.x - 54.0, rect.center().y),
        Vec2::new(32.0, ROW_H),
    );
    let grip_hover = resp.hover_pos().is_some_and(|p| grip_rect.contains(p));
    let grip_col = if st.moving {
        color::ACCENT
    } else if grip_hover {
        color::TEXT
    } else {
        color::TEXT_MUTED
    };
    ui.painter().text(
        grip_rect.center(),
        Align2::CENTER_CENTER,
        icon::MENU,
        theme::icon_font(16.0),
        grip_col,
    );
    if st.moving {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(8),
            color::PANEL.gamma_multiply(0.55),
        );
    }
    RowOut {
        rect,
        thumb: thumb_rect.expand(6.0),
        grip: grip_rect,
        eye: eye.clicked(),
    }
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
    let mut drop_at: Option<Pos2> = None;
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
                        // A cor atual: puxe a bolinha até a tela para preencher ali (ColorDrop,
                        // como no Procreate). A barra em volta serve para mover a roda.
                        let (r, chip) = ui.allocate_exact_size(Vec2::splat(HEADER), Sense::drag());
                        let chip = chip.on_hover_text("Arraste a cor até a tela para preencher");
                        let ring = if chip.dragged() {
                            color::ACCENT
                        } else {
                            color::BORDER
                        };
                        ui.painter().circle_filled(r.center(), 11.0, app.color);
                        ui.painter()
                            .circle_stroke(r.center(), 11.0, Stroke::new(1.5, ring));
                        if chip.dragged()
                            && let Some(at) = chip.interact_pointer_pos()
                        {
                            let p = ctx.layer_painter(egui::LayerId::new(
                                Order::Tooltip,
                                Id::unique("float-color-drop"),
                            ));
                            // A bolinha fica um pouco acima do dedo, para ver onde cai.
                            let c = at - Vec2::new(0.0, 28.0);
                            p.circle_filled(c, 17.0, Color32::WHITE);
                            p.circle_filled(c, 15.0, app.color);
                            p.circle_stroke(at, 4.0, Stroke::new(1.5, Color32::WHITE));
                        }
                        if chip.drag_stopped() {
                            drop_at = chip
                                .interact_pointer_pos()
                                .or_else(|| ctx.input(|i| i.pointer.latest_pos()));
                        }
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
    // Soltou a cor: preenche na tela, se caiu nela (não em cima de um painel nem da roda).
    if let Some(at) = drop_at.filter(|p| !resp.response.rect.contains(*p)) {
        let over_panel = ctx
            .layer_id_at(at)
            .is_some_and(|l| l.order != Order::Background);
        if !over_panel {
            crate::canvas::color_drop(app, at);
        }
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
/// Quadros guardados para a média do diagnóstico de desempenho.
const PERF_FRAMES: usize = 120;

/// Canto de diagnóstico: tempo médio e pior tempo de quadro dos últimos quadros, quadros por
/// segundo, camadas e tamanho da tela. Acima de ~16 ms o traço começa a parecer atrasado.
pub fn perf_overlay(app: &mut ArtApp, ctx: &egui::Context, canvas: egui::Rect) {
    let dt = ctx.input(|i| i.unstable_dt);
    // Pausas sem nada acontecendo (o egui só redesenha quando precisa) não contam.
    if dt.is_finite() && dt > 0.0 && dt < 0.25 {
        app.frame_times.push_back(dt);
        while app.frame_times.len() > PERF_FRAMES {
            app.frame_times.pop_front();
        }
    }
    // Amostras da caneta/dedo por segundo enquanto desenha: mostra se o aparelho entrega a
    // caneta a 60, 120 ou 240 Hz, e se o app está perdendo pontos (traço quebrado ou atrasado).
    let (now, down, moves) = ctx.input(|i| {
        let moves = i
            .events
            .iter()
            .filter(|e| match e {
                egui::Event::PointerMoved(_) => true,
                egui::Event::Touch { phase, .. } => *phase == egui::TouchPhase::Move,
                _ => false,
            })
            .count();
        (i.time, i.pointer.any_down(), moves)
    });
    if down {
        app.input_samples.extend(std::iter::repeat_n(now, moves));
        while app.input_samples.front().is_some_and(|t| now - t > 1.0) {
            app.input_samples.pop_front();
        }
        if let (Some(a), Some(b)) = (app.input_samples.front(), app.input_samples.back())
            && b - a > 0.2
        {
            app.input_rate = (app.input_samples.len() - 1) as f32 / (b - a) as f32;
        }
    } else {
        app.input_samples.clear();
    }
    let n = app.frame_times.len().max(1) as f32;
    let avg = app.frame_times.iter().sum::<f32>() / n * 1000.0;
    let worst = app.frame_times.iter().copied().fold(0.0f32, f32::max) * 1000.0;
    let rate = if app.input_rate > 0.0 {
        format!("\ncaneta: {:.0} amostras/s", app.input_rate)
    } else {
        "\ncaneta: desenhe para medir".to_string()
    };
    let text = format!(
        "{avg:.1} ms (pior {worst:.0} ms) · {:.0} qps\n{} camadas · {} × {} px · {} MB{rate}",
        if avg > 0.0 { 1000.0 / avg } else { 0.0 },
        app.doc.layers.len(),
        app.doc.width,
        app.doc.height,
        app.mem_bytes >> 20
    );
    let warn = avg > 20.0 || app.mem_bytes > crate::app::MEMORY_WARN_BYTES;
    egui::Area::new(Id::unique("perf"))
        .order(Order::Tooltip)
        .interactable(false)
        .fixed_pos(canvas.left_top() + Vec2::new(8.0, 8.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_black_alpha(160))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(6.0)
                .show(ui, |ui| {
                    ui.label(RichText::new(text).size(11.5).monospace().color(if warn {
                        color::ACCENT
                    } else {
                        Color32::WHITE
                    }));
                });
        });
}

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
            for (name, w, h, dpi) in CANVAS_PRESETS {
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
                    create = Some((w, h, dpi));
                }
            }
            ui.add_space(6.0);
            section(ui, "Tamanho personalizado");
            let s = &mut app.new_size;
            crate::units::size_editor(ui, &mut s.0, &mut s.1, &mut s.2, &mut s.3, max);
            if ui
                .add(
                    egui::Button::new(RichText::new("Criar").strong())
                        .min_size(Vec2::new(120.0, 36.0)),
                )
                .clicked()
            {
                create = Some((s.0, s.1, s.2));
            }
            ui.label(
                RichText::new(format!("Tamanho máximo neste aparelho: {max} px"))
                    .size(12.0)
                    .color(color::TEXT_MUTED),
            );
        });
    if let Some((w, h, dpi)) = create {
        app.new_canvas(w, h);
        app.doc.dpi = dpi;
        app.show_new_dialog = false;
    } else if !open {
        app.show_new_dialog = false;
    }
}
