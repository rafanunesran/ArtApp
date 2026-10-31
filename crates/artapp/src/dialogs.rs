//! Diálogos do menu Editar: transformar, filtros do PhotoCraft e ajustes de cor.
//!
//! Os filtros vêm do catálogo do `photocraft-algo`. Cada um é descrito aqui por um modelo JSON
//! (o mesmo formato do `FilterParams`) e pelos campos que ganham slider, então acrescentar um
//! filtro é acrescentar uma linha em [`FILTERS`].

use artapp_core::FilterParams;
use artapp_core::edit::Adjust;
use eframe::egui::{self, Align2, RichText, Vec2};

use crate::app::{ArtApp, color_to_f32};
use crate::theme::{color, panel_frame};

/// Um campo numérico de filtro: chave no JSON, rótulo, mínimo e máximo.
type Field = (&'static str, &'static str, f32, f32);

struct FilterDef {
    group: &'static str,
    name: &'static str,
    template: &'static str,
    fields: &'static [Field],
}

const FILTERS: &[FilterDef] = &[
    FilterDef {
        group: "Desfoque",
        name: "Desfoque gaussiano",
        template: r#"{"filter":"gaussianBlur","radius":4}"#,
        fields: &[("radius", "Raio (px)", 0.1, 250.0)],
    },
    FilterDef {
        group: "Desfoque",
        name: "Desfoque de caixa",
        template: r#"{"filter":"boxBlur","radius":4}"#,
        fields: &[("radius", "Raio (px)", 1.0, 250.0)],
    },
    FilterDef {
        group: "Desfoque",
        name: "Desfoque de movimento",
        template: r#"{"filter":"motionBlur","angle":0,"distance":20}"#,
        fields: &[
            ("angle", "Ângulo (°)", -180.0, 180.0),
            ("distance", "Distância (px)", 1.0, 500.0),
        ],
    },
    FilterDef {
        group: "Desfoque",
        name: "Desfoque de superfície",
        template: r#"{"filter":"surfaceBlur","radius":5,"threshold":15}"#,
        fields: &[
            ("radius", "Raio (px)", 1.0, 100.0),
            ("threshold", "Limiar", 2.0, 255.0),
        ],
    },
    FilterDef {
        group: "Desfoque",
        name: "Mediana",
        template: r#"{"filter":"median","radius":2}"#,
        fields: &[("radius", "Raio (px)", 1.0, 100.0)],
    },
    FilterDef {
        group: "Nitidez",
        name: "Máscara de nitidez",
        template: r#"{"filter":"unsharpMask","amount":100,"radius":2,"threshold":0}"#,
        fields: &[
            ("amount", "Intensidade (%)", 1.0, 500.0),
            ("radius", "Raio (px)", 0.1, 250.0),
            ("threshold", "Limiar", 0.0, 255.0),
        ],
    },
    FilterDef {
        group: "Nitidez",
        name: "Passa-alta",
        template: r#"{"filter":"highPass","radius":10}"#,
        fields: &[("radius", "Raio (px)", 0.1, 250.0)],
    },
    FilterDef {
        group: "Ruído",
        name: "Adicionar ruído",
        template: r#"{"filter":"addNoise","amount":12,"distribution":"gaussian","monochromatic":true,"seed":1}"#,
        fields: &[("amount", "Quantidade (%)", 0.1, 400.0)],
    },
    FilterDef {
        group: "Ruído",
        name: "Poeira e arranhões",
        template: r#"{"filter":"dustAndScratches","radius":2,"threshold":10}"#,
        fields: &[
            ("radius", "Raio (px)", 1.0, 100.0),
            ("threshold", "Limiar", 0.0, 255.0),
        ],
    },
    FilterDef {
        group: "Ruído",
        name: "Mínimo (afinar linhas claras)",
        template: r#"{"filter":"minimum","radius":1,"preserve":"roundness"}"#,
        fields: &[("radius", "Raio (px)", 1.0, 50.0)],
    },
    FilterDef {
        group: "Ruído",
        name: "Máximo (engrossar linhas claras)",
        template: r#"{"filter":"maximum","radius":1,"preserve":"roundness"}"#,
        fields: &[("radius", "Raio (px)", 1.0, 50.0)],
    },
    FilterDef {
        group: "Pixelizar",
        name: "Mosaico",
        template: r#"{"filter":"mosaic","cellSize":12}"#,
        fields: &[("cellSize", "Célula (px)", 2.0, 200.0)],
    },
    FilterDef {
        group: "Pixelizar",
        name: "Cristalizar",
        template: r#"{"filter":"crystallize","cellSize":12,"seed":1}"#,
        fields: &[("cellSize", "Célula (px)", 3.0, 300.0)],
    },
    FilterDef {
        group: "Pixelizar",
        name: "Pontilhar",
        template: r#"{"filter":"pointillize","cellSize":6,"seed":1,"background":[1,1,1,1]}"#,
        fields: &[("cellSize", "Célula (px)", 3.0, 300.0)],
    },
    FilterDef {
        group: "Pixelizar",
        name: "Meio-tom colorido",
        template: r#"{"filter":"colorHalftone","maxRadius":8,"angles":[108,162,90,45]}"#,
        fields: &[("maxRadius", "Raio máximo (px)", 4.0, 127.0)],
    },
    FilterDef {
        group: "Estilizar",
        name: "Relevo",
        template: r#"{"filter":"emboss","angle":135,"height":3,"amount":100}"#,
        fields: &[
            ("angle", "Ângulo (°)", -180.0, 180.0),
            ("height", "Altura (px)", 1.0, 100.0),
            ("amount", "Intensidade (%)", 1.0, 500.0),
        ],
    },
    FilterDef {
        group: "Estilizar",
        name: "Encontrar bordas",
        template: r#"{"filter":"findEdges"}"#,
        fields: &[],
    },
    FilterDef {
        group: "Estilizar",
        name: "Solarizar",
        template: r#"{"filter":"solarize"}"#,
        fields: &[],
    },
    FilterDef {
        group: "Estilizar",
        name: "Traçar contorno",
        template: r#"{"filter":"traceContour","level":128,"upper":true}"#,
        fields: &[("level", "Nível", 0.0, 255.0)],
    },
    FilterDef {
        group: "Estilizar",
        name: "Pintura a óleo",
        template: r#"{"filter":"oilPaint","stylization":4,"cleanliness":5,"scale":1,"bristleDetail":3,"lighting":true,"angle":-60,"shine":1}"#,
        fields: &[
            ("stylization", "Estilização", 0.1, 10.0),
            ("cleanliness", "Limpeza", 0.0, 10.0),
            ("scale", "Escala", 0.1, 10.0),
            ("bristleDetail", "Detalhe das cerdas", 0.0, 10.0),
            ("shine", "Brilho", 0.0, 10.0),
        ],
    },
    FilterDef {
        group: "Distorcer",
        name: "Torcer",
        template: r#"{"filter":"twirl","angle":50}"#,
        fields: &[("angle", "Ângulo (°)", -999.0, 999.0)],
    },
    FilterDef {
        group: "Distorcer",
        name: "Comprimir",
        template: r#"{"filter":"pinch","amount":50}"#,
        fields: &[("amount", "Quantidade (%)", -100.0, 100.0)],
    },
    FilterDef {
        group: "Distorcer",
        name: "Esferizar",
        template: r#"{"filter":"spherize","amount":50,"mode":"normal"}"#,
        fields: &[("amount", "Quantidade (%)", -100.0, 100.0)],
    },
    FilterDef {
        group: "Distorcer",
        name: "Ondulação",
        template: r#"{"filter":"ripple","amount":100,"size":"medium"}"#,
        fields: &[("amount", "Quantidade (%)", -999.0, 999.0)],
    },
    FilterDef {
        group: "Renderizar",
        name: "Nuvens (cor principal e secundária)",
        template: r#"{"filter":"clouds","seed":1,"difference":false,"foreground":[0,0,0,1],"background":[1,1,1,1]}"#,
        fields: &[],
    },
    FilterDef {
        group: "Renderizar",
        name: "Fibras",
        template: r#"{"filter":"fibers","variance":16,"strength":4,"seed":1,"foreground":[0,0,0,1],"background":[1,1,1,1]}"#,
        fields: &[
            ("variance", "Variação", 1.0, 64.0),
            ("strength", "Força", 1.0, 64.0),
        ],
    },
    FilterDef {
        group: "Cor",
        name: "Inverter",
        template: r#"{"filter":"invert"}"#,
        fields: &[],
    },
    FilterDef {
        group: "Cor",
        name: "Tirar a saturação",
        template: r#"{"filter":"desaturate"}"#,
        fields: &[],
    },
];

/// Diálogo aberto.
#[derive(Clone, Debug)]
pub enum Dialog {
    Transform {
        scale_x: f64,
        scale_y: f64,
        uniform: bool,
        angle: f64,
        dx: f64,
        dy: f64,
    },
    Filter {
        index: usize,
        params: serde_json::Value,
    },
    Adjust(Adjust),
    /// Texto numa camada nova (raster, ou vetorial para editar depois).
    Text {
        text: String,
        size: f32,
        editable: bool,
    },
    /// Engrossar (positivo) ou afinar (negativo) a tinta da camada, em pixels.
    LineWidth {
        delta: f32,
    },
    /// Tamanho da tela: `scale` escala o desenho; senão só a área muda, presa em `anchor`
    /// (0..9, da esquerda para a direita e de cima para baixo).
    CanvasSize {
        width: u32,
        height: u32,
        keep_ratio: bool,
        scale: bool,
        anchor: usize,
    },
}

impl Dialog {
    pub fn canvas_size(width: u32, height: u32) -> Self {
        Dialog::CanvasSize {
            width,
            height,
            keep_ratio: true,
            scale: false,
            anchor: 4,
        }
    }

    pub fn transform() -> Self {
        Dialog::Transform {
            scale_x: 100.0,
            scale_y: 100.0,
            uniform: true,
            angle: 0.0,
            dx: 0.0,
            dy: 0.0,
        }
    }

    pub fn filter(index: usize) -> Self {
        let params = FILTERS
            .get(index)
            .and_then(|f| serde_json::from_str(f.template).ok())
            .unwrap_or(serde_json::Value::Null);
        Dialog::Filter { index, params }
    }

    pub fn adjust() -> Self {
        Dialog::Adjust(Adjust::HueSatLight {
            hue: 0.0,
            sat: 0.0,
            light: 0.0,
        })
    }
}

/// O que o botão do diálogo pediu.
enum Outcome {
    Keep,
    Close,
    Apply,
}

pub fn show(app: &mut ArtApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.dialog.take() else {
        return;
    };
    let title = match &dialog {
        Dialog::Transform { .. } => "Transformar",
        Dialog::Filter { .. } => "Filtro",
        Dialog::Adjust(_) => "Correção de cor",
        Dialog::CanvasSize { .. } => "Tamanho da tela",
        Dialog::Text { .. } => "Texto",
        Dialog::LineWidth { .. } => "Ajustar largura da linha",
    };
    let mut outcome = Outcome::Keep;
    let mut open = true;
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(panel_frame())
        // Celular deitado: a janela não cabe na altura, então rola.
        .vscroll(true)
        .max_height(ctx.content_rect().height() - 60.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(320.0);
            match &mut dialog {
                Dialog::Transform { .. } => transform_ui(app, ui, &mut dialog, &mut outcome),
                Dialog::Filter { index, params } => filter_ui(ui, index, params),
                Dialog::Adjust(adj) => adjust_ui(ui, adj),
                Dialog::CanvasSize { .. } => canvas_size_ui(app, ui, &mut dialog),
                Dialog::LineWidth { delta } => {
                    ui.add(
                        egui::Slider::new(delta, -artapp_core::line_width::MAX_DELTA..=artapp_core::line_width::MAX_DELTA)
                            .step_by(0.5)
                            .suffix(" px")
                            .text("Largura"),
                    );
                    ui.label(
                        RichText::new(
                            "Positivo engrossa, negativo afina a tinta da camada ativa. A cor nova vem da linha mais perto.",
                        )
                        .size(12.0)
                        .color(color::TEXT_MUTED),
                    );
                }
                Dialog::Text {
                    text,
                    size,
                    editable,
                } => {
                    ui.add(
                        egui::TextEdit::multiline(text)
                            .hint_text("Escreva o texto")
                            .desired_width(f32::INFINITY)
                            .desired_rows(3),
                    );
                    ui.add(
                        egui::Slider::new(size, 6.0..=600.0)
                            .logarithmic(true)
                            .suffix(" px")
                            .text("Tamanho"),
                    );
                    ui.checkbox(editable, "Manter editável (camada vetorial)");
                    ui.label(
                        RichText::new(
                            "Usa a cor principal. Em camada raster, a caixa de transformar abre para posicionar.",
                        )
                        .size(12.0)
                        .color(color::TEXT_MUTED),
                    );
                }
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Aplicar").strong()).clicked() {
                    outcome = Outcome::Apply;
                }
                if ui.button("Cancelar").clicked() {
                    outcome = Outcome::Close;
                }
            });
            if app.doc.selection.is_some()
                && !matches!(dialog, Dialog::CanvasSize { .. } | Dialog::Text { .. })
            {
                ui.label(
                    RichText::new("Age só dentro da seleção.")
                        .size(12.0)
                        .color(color::TEXT_MUTED),
                );
            }
        });
    if !open {
        outcome = Outcome::Close;
    }
    match outcome {
        Outcome::Keep => app.dialog = Some(dialog),
        Outcome::Close => {}
        Outcome::Apply => apply(app, &dialog),
    }
}

fn apply(app: &mut ArtApp, dialog: &Dialog) {
    if let Dialog::LineWidth { delta } = *dialog {
        app.commit_transform();
        let r = app.doc.adjust_line_width(delta);
        if r.is_empty() {
            app.status = "Nada para ajustar: a camada está vazia, oculta ou é vetorial.".into();
        }
        app.invalidate(r);
        return;
    }
    if let Dialog::Text {
        text,
        size,
        editable,
    } = dialog
    {
        app.commit_transform();
        let [r, g, b, _] = color_to_f32(app.color);
        let (w, h) = (f64::from(app.doc.width), f64::from(app.doc.height));
        let at = [w * 0.1, h * 0.5];
        let dirty = app
            .doc
            .add_text(text, at, f64::from(*size), [r, g, b], !*editable);
        if dirty.is_empty() {
            app.status = "Escreva algum texto.".into();
            return;
        }
        app.remember_color();
        app.invalidate_all();
        if !*editable {
            app.start_transform();
        }
        return;
    }
    if let Dialog::CanvasSize {
        width,
        height,
        scale,
        anchor,
        ..
    } = *dialog
    {
        app.commit_transform();
        let (ow, oh) = (app.doc.width as i64, app.doc.height as i64);
        let changed = if scale {
            app.doc
                .resize_image(width, height, artapp_core::transform::Interp::Bicubic)
        } else {
            let at = |extra: i64, k: usize| (extra * k as i64 / 2) as i32;
            app.doc.resize_canvas(
                width,
                height,
                at(i64::from(width) - ow, anchor % 3),
                at(i64::from(height) - oh, anchor / 3),
            )
        };
        if changed {
            app.canvas_resized();
        }
        return;
    }
    let r = match dialog {
        Dialog::Transform {
            scale_x,
            scale_y,
            angle,
            dx,
            dy,
            ..
        } => app
            .doc
            .transform_layer(scale_x / 100.0, scale_y / 100.0, *angle, *dx, *dy),
        Dialog::Filter { params, .. } => {
            let mut params = params.clone();
            // Filtros que desenham com duas cores usam a principal e a secundária.
            if let Some(obj) = params.as_object_mut() {
                for (key, c) in [("foreground", app.color), ("background", app.sub_color)] {
                    if obj.contains_key(key) {
                        obj.insert(key.into(), serde_json::json!(color_to_f32(c)));
                    }
                }
            }
            match serde_json::from_value::<FilterParams>(params) {
                Ok(p) => app.doc.apply_filter(&p),
                Err(e) => {
                    app.status = format!("Filtro inválido: {e}");
                    return;
                }
            }
        }
        Dialog::Adjust(adj) => app.doc.adjust(*adj),
        Dialog::CanvasSize { .. } | Dialog::Text { .. } | Dialog::LineWidth { .. } => return,
    };
    if r.is_empty() {
        app.status = "Escolha uma camada raster visível com desenho.".into();
    }
    app.invalidate_all();
}

fn transform_ui(app: &mut ArtApp, ui: &mut egui::Ui, dialog: &mut Dialog, outcome: &mut Outcome) {
    let Dialog::Transform {
        scale_x,
        scale_y,
        uniform,
        angle,
        dx,
        dy,
    } = dialog
    else {
        return;
    };
    ui.label(
        RichText::new("Transforma a camada ativa (ou a seleção) em volta do centro do desenho.")
            .size(12.5)
            .color(color::TEXT_MUTED),
    );
    ui.checkbox(uniform, "Manter a proporção");
    if ui
        .add(
            egui::Slider::new(scale_x, 1.0..=800.0)
                .suffix("%")
                .text("Largura"),
        )
        .changed()
        && *uniform
    {
        *scale_y = *scale_x;
    }
    if ui
        .add(
            egui::Slider::new(scale_y, 1.0..=800.0)
                .suffix("%")
                .text("Altura"),
        )
        .changed()
        && *uniform
    {
        *scale_x = *scale_y;
    }
    ui.add(
        egui::Slider::new(angle, -180.0..=180.0)
            .suffix("°")
            .text("Giro"),
    );
    let (w, h) = (app.doc.width as f64, app.doc.height as f64);
    ui.add(egui::Slider::new(dx, -w..=w).text("Mover X (px)"));
    ui.add(egui::Slider::new(dy, -h..=h).text("Mover Y (px)"));
    ui.horizontal_wrapped(|ui| {
        let mut quick =
            |ui: &mut egui::Ui, label: &str, f: &dyn Fn(&mut ArtApp) -> artapp_core::Rect| {
                if ui.button(label).clicked() {
                    let r = f(app);
                    app.invalidate(r);
                    *outcome = Outcome::Close;
                }
            };
        quick(ui, "Espelhar horizontal", &|a| a.doc.flip_layer(true));
        quick(ui, "Espelhar vertical", &|a| a.doc.flip_layer(false));
        quick(ui, "Girar 90°", &|a| {
            a.doc.transform_layer(1.0, 1.0, 90.0, 0.0, 0.0)
        });
        quick(ui, "Girar -90°", &|a| {
            a.doc.transform_layer(1.0, 1.0, -90.0, 0.0, 0.0)
        });
    });
}

fn canvas_size_ui(app: &ArtApp, ui: &mut egui::Ui, dialog: &mut Dialog) {
    let Dialog::CanvasSize {
        width,
        height,
        keep_ratio,
        scale,
        anchor,
    } = dialog
    else {
        return;
    };
    let (ow, oh) = (app.doc.width, app.doc.height);
    ui.horizontal(|ui| {
        ui.selectable_value(scale, false, "Mudar a área");
        ui.selectable_value(scale, true, "Escalar o desenho");
    });
    ui.label(
        RichText::new(if *scale {
            "Todas as camadas crescem ou encolhem junto com a tela."
        } else {
            "O desenho fica do mesmo tamanho; a tela ganha ou perde borda em volta do ponto fixo."
        })
        .size(12.5)
        .color(color::TEXT_MUTED),
    );
    ui.checkbox(keep_ratio, "Manter a proporção");
    let max = artapp_core::document::MAX_CANVAS_SIDE;
    ui.horizontal(|ui| {
        ui.label("Largura");
        if ui
            .add(egui::DragValue::new(width).range(1..=max).suffix(" px"))
            .changed()
            && *keep_ratio
        {
            *height =
                ((f64::from(*width) * f64::from(oh) / f64::from(ow)).round() as u32).clamp(1, max);
        }
        ui.label("Altura");
        if ui
            .add(egui::DragValue::new(height).range(1..=max).suffix(" px"))
            .changed()
            && *keep_ratio
        {
            *width =
                ((f64::from(*height) * f64::from(ow) / f64::from(oh)).round() as u32).clamp(1, max);
        }
    });
    ui.horizontal_wrapped(|ui| {
        for pct in [50u32, 75, 150, 200] {
            if ui.button(format!("{pct}%")).clicked() {
                *width = (ow * pct / 100).clamp(1, max);
                *height = (oh * pct / 100).clamp(1, max);
            }
        }
        if ui.button("Original").clicked() {
            *width = ow;
            *height = oh;
        }
    });
    if !*scale {
        ui.label(
            RichText::new("Ponto fixo")
                .size(12.5)
                .color(color::TEXT_MUTED),
        );
        egui::Grid::new("anchor")
            .spacing([4.0, 4.0])
            .show(ui, |ui| {
                for i in 0..9 {
                    let on = *anchor == i;
                    let resp = ui.add_sized([34.0, 28.0], egui::Button::new("").selected(on));
                    let c = resp.rect.center();
                    if on {
                        ui.painter().circle_filled(c, 5.0, color::TEXT);
                    } else {
                        let st = egui::Stroke::new(1.2, color::TEXT_MUTED);
                        ui.painter().circle_stroke(c, 4.0, st);
                    }
                    if resp.clicked() {
                        *anchor = i;
                    }
                    if i % 3 == 2 {
                        ui.end_row();
                    }
                }
            });
    }
    ui.label(
        RichText::new(format!("Agora: {ow} × {oh} px"))
            .size(12.0)
            .color(color::TEXT_MUTED),
    );
}

fn filter_ui(ui: &mut egui::Ui, index: &mut usize, params: &mut serde_json::Value) {
    let current = FILTERS.get(*index).map_or("", |f| f.name);
    let mut picked = None;
    egui::ComboBox::from_id_salt("filter-pick")
        .selected_text(current)
        .width(300.0)
        .height(420.0)
        .show_ui(ui, |ui| {
            let mut group = "";
            for (i, f) in FILTERS.iter().enumerate() {
                if f.group != group {
                    group = f.group;
                    ui.label(
                        RichText::new(group.to_uppercase())
                            .size(11.0)
                            .color(color::TEXT_MUTED),
                    );
                }
                if ui.selectable_label(i == *index, f.name).clicked() {
                    picked = Some(i);
                }
            }
        });
    if let Some(i) = picked.filter(|i| i != index)
        && let Dialog::Filter {
            index: ni,
            params: np,
        } = Dialog::filter(i)
    {
        *index = ni;
        *params = np;
    }
    let Some(def) = FILTERS.get(*index) else {
        return;
    };
    if def.fields.is_empty() {
        ui.label(RichText::new("Sem ajustes.").color(color::TEXT_MUTED));
    }
    for (key, label, lo, hi) in def.fields {
        let Some(v) = params.get_mut(*key) else {
            continue;
        };
        let mut x = v.as_f64().unwrap_or(0.0) as f32;
        let log = *lo > 0.0 && hi / lo > 100.0;
        if ui
            .add(
                egui::Slider::new(&mut x, *lo..=*hi)
                    .logarithmic(log)
                    .text(*label),
            )
            .changed()
        {
            *v = serde_json::json!(x);
        }
    }
}

fn adjust_ui(ui: &mut egui::Ui, adj: &mut Adjust) {
    use artapp_core::edit::CURVE_IDENTITY;
    let kinds: [(&str, Adjust); 10] = [
        (
            "Matiz/Saturação/Luminosidade",
            Adjust::HueSatLight {
                hue: 0.0,
                sat: 0.0,
                light: 0.0,
            },
        ),
        (
            "Brilho/Contraste",
            Adjust::BrightnessContrast {
                brightness: 0.0,
                contrast: 0.0,
            },
        ),
        (
            "Níveis",
            Adjust::Levels {
                black: 0.0,
                white: 1.0,
                gamma: 1.0,
            },
        ),
        ("Posterizar", Adjust::Posterize { levels: 4 }),
        ("Binarizar (limiar)", Adjust::Threshold { level: 0.5 }),
        ("Inverter cores", Adjust::Invert),
        ("Tirar a saturação", Adjust::Desaturate),
        (
            "Curvas",
            Adjust::Curves {
                ys: [CURVE_IDENTITY; 4],
            },
        ),
        (
            "Mapa de gradiente",
            Adjust::GradientMap {
                dark: [0.1, 0.05, 0.25],
                light: [1.0, 0.85, 0.55],
            },
        ),
        (
            "Equilíbrio de cores",
            Adjust::ColorBalance {
                tones: [[0.0; 3]; 3],
            },
        ),
    ];
    let same = |a: &Adjust, b: &Adjust| std::mem::discriminant(a) == std::mem::discriminant(b);
    let name = kinds
        .iter()
        .find(|(_, k)| same(k, adj))
        .map_or("", |(n, _)| n);
    egui::ComboBox::from_id_salt("adjust-pick")
        .selected_text(name)
        .width(300.0)
        .show_ui(ui, |ui| {
            for (n, k) in kinds {
                if ui.selectable_label(same(&k, adj), n).clicked() && !same(&k, adj) {
                    *adj = k;
                }
            }
        });
    match adj {
        Adjust::HueSatLight { hue, sat, light } => {
            ui.add(
                egui::Slider::new(hue, -180.0..=180.0)
                    .suffix("°")
                    .text("Matiz"),
            );
            ui.add(egui::Slider::new(sat, -1.0..=1.0).text("Saturação"));
            ui.add(egui::Slider::new(light, -1.0..=1.0).text("Luminosidade"));
        }
        Adjust::BrightnessContrast {
            brightness,
            contrast,
        } => {
            ui.add(egui::Slider::new(brightness, -1.0..=1.0).text("Brilho"));
            ui.add(egui::Slider::new(contrast, -1.0..=1.0).text("Contraste"));
        }
        Adjust::Levels {
            black,
            white,
            gamma,
        } => {
            ui.add(egui::Slider::new(black, 0.0..=1.0).text("Preto"));
            ui.add(egui::Slider::new(white, 0.0..=1.0).text("Branco"));
            ui.add(egui::Slider::new(gamma, 0.1..=4.0).text("Gama"));
        }
        Adjust::Posterize { levels } => {
            ui.add(egui::Slider::new(levels, 2..=32).text("Níveis"));
        }
        Adjust::Threshold { level } => {
            ui.add(egui::Slider::new(level, 0.0..=1.0).text("Limiar"));
        }
        Adjust::Invert | Adjust::Desaturate => {
            ui.label(RichText::new("Sem ajustes.").color(color::TEXT_MUTED));
        }
        Adjust::Curves { ys } => curves_ui(ui, ys),
        Adjust::GradientMap { dark, light } => {
            ui.horizontal(|ui| {
                ui.color_edit_button_rgb(dark);
                ui.label("Sombras");
                ui.add_space(16.0);
                ui.color_edit_button_rgb(light);
                ui.label("Luzes");
            });
            if ui.button("Inverter").clicked() {
                std::mem::swap(dark, light);
            }
        }
        Adjust::ColorBalance { tones } => {
            let id = egui::Id::unique("balance-range");
            let mut range: usize = ui.data(|d| d.get_temp(id)).unwrap_or(1);
            ui.horizontal(|ui| {
                for (i, n) in ["Sombras", "Meios-tons", "Realces"].into_iter().enumerate() {
                    ui.selectable_value(&mut range, i, n);
                }
            });
            ui.data_mut(|d| d.insert_temp(id, range));
            let t = &mut tones[range];
            ui.add(egui::Slider::new(&mut t[0], -1.0..=1.0).text("Ciano / Vermelho"));
            ui.add(egui::Slider::new(&mut t[1], -1.0..=1.0).text("Magenta / Verde"));
            ui.add(egui::Slider::new(&mut t[2], -1.0..=1.0).text("Amarelo / Azul"));
        }
    }
}

/// Editor de curvas: um canal por vez, cinco pontos arrastáveis na vertical.
fn curves_ui(ui: &mut egui::Ui, ys: &mut [[f32; 5]; 4]) {
    use artapp_core::edit::{CURVE_IDENTITY, curve_eval};
    let id = egui::Id::unique("curves-channel");
    let mut ch: usize = ui.data(|d| d.get_temp(id)).unwrap_or(0);
    ui.horizontal(|ui| {
        for (i, n) in ["RGB", "Vermelho", "Verde", "Azul"].into_iter().enumerate() {
            ui.selectable_value(&mut ch, i, n);
        }
    });
    ui.data_mut(|d| d.insert_temp(id, ch));
    let size = egui::vec2(260.0, 200.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_gray(28));
    let grid = egui::Stroke::new(1.0, egui::Color32::from_gray(55));
    for i in 1..4 {
        let f = i as f32 / 4.0;
        painter.vline(rect.left() + rect.width() * f, rect.y_range(), grid);
        painter.hline(rect.x_range(), rect.top() + rect.height() * f, grid);
    }
    let at = |x: f32, y: f32| {
        egui::pos2(
            rect.left() + x * rect.width(),
            rect.bottom() - y * rect.height(),
        )
    };
    let line_color = [
        egui::Color32::from_gray(230),
        egui::Color32::from_rgb(240, 80, 80),
        egui::Color32::from_rgb(90, 210, 90),
        egui::Color32::from_rgb(90, 140, 250),
    ][ch];
    let curve = &mut ys[ch];
    // Arrasta o ponto mais perto na horizontal.
    if let Some(p) = resp.interact_pointer_pos()
        && (resp.dragged() || resp.clicked())
    {
        let x = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let i = (x * 4.0).round() as usize;
        curve[i.min(4)] = ((rect.bottom() - p.y) / rect.height()).clamp(0.0, 1.0);
    }
    let pts: Vec<egui::Pos2> = (0..=64)
        .map(|i| {
            let x = i as f32 / 64.0;
            at(x, curve_eval(curve, x))
        })
        .collect();
    painter.add(egui::Shape::line(pts, egui::Stroke::new(2.0, line_color)));
    for (i, y) in curve.iter().enumerate() {
        painter.circle_filled(at(i as f32 / 4.0, *y), 5.0, line_color);
    }
    if ui.button("Restaurar canal").clicked() {
        *curve = CURVE_IDENTITY;
    }
}
