//! Editor completo do pincel: todos os ajustes do motor do PhotoCraft, separados em abas como o
//! "Estúdio de pincel" do Procreate e a paleta "Detalhes da sub-ferramenta" do Clip Studio.
//!
//! Onde entra um ajuste novo (ver [`BrushTab`]):
//! - **Geral**: tamanho, opacidade, fluxo, dureza, mesclagem, pressão/curva — o que se mexe toda hora.
//! - **Forma**: a ponta (imagem, ângulo, redondeza, espaçamento, espelhar), início e fim, pincel duplo.
//! - **Granulado**: textura/papel, ruído nas bordas, tudo que é grão aplicado ao traço.
//! - **Dinâmica**: o que varia ao longo do traço (pressão, inclinação, giro, aleatório): forma,
//!   dispersão, transferência e cor.
//! - **Suavização**: estabilizador e correção de traço.
//! - **Avançado**: pintura úmida, aerógrafo, simetria e opções raras.

use artapp_core::BlendMode;
use artapp_core::Taper;
use artapp_core::paint::{
    BrushSettings, Control, Dynamic, MaskMode, Pattern, PatternStyle, TipShape,
};
use eframe::egui::{self, RichText};

use crate::theme::color;

fn slider(ui: &mut egui::Ui, v: &mut f32, range: std::ops::RangeInclusive<f32>, text: &str) {
    ui.add(egui::Slider::new(v, range).text(text));
}

fn pct(ui: &mut egui::Ui, v: &mut f32, text: &str) {
    let mut p = *v * 100.0;
    if ui
        .add(
            egui::Slider::new(&mut p, 0.0..=100.0)
                .suffix("%")
                .text(text),
        )
        .changed()
    {
        *v = p / 100.0;
    }
}

fn hint(ui: &mut egui::Ui, t: &str) {
    ui.label(RichText::new(t).size(12.0).color(color::TEXT_MUTED));
}

fn control_label(c: Control) -> &'static str {
    match c {
        Control::Off => "Nenhum",
        Control::Fade => "Esmaecer",
        Control::PenPressure => "Pressão",
        Control::PenTilt => "Inclinação",
        Control::StylusWheel => "Roda da caneta",
        Control::Rotation => "Giro da caneta",
        Control::InitialDirection => "Direção inicial",
        Control::Direction => "Direção do traço",
    }
}

const CONTROLS: [Control; 6] = [
    Control::Off,
    Control::PenPressure,
    Control::Fade,
    Control::PenTilt,
    Control::Rotation,
    Control::StylusWheel,
];

/// Um parâmetro dinâmico: variação aleatória, o que o controla e o mínimo.
fn dynamic(ui: &mut egui::Ui, id: &str, label: &str, d: &mut Dynamic, angle: bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(id)
            .selected_text(control_label(d.control))
            .width(130.0)
            .show_ui(ui, |ui| {
                let extra: &[Control] = if angle {
                    &[Control::InitialDirection, Control::Direction]
                } else {
                    &[]
                };
                for c in CONTROLS.iter().chain(extra) {
                    ui.selectable_value(&mut d.control, *c, control_label(*c));
                }
            });
    });
    pct(ui, &mut d.jitter, "Variação");
    if d.control == Control::Fade {
        let mut steps = d.fade_steps as f32;
        if ui
            .add(egui::Slider::new(&mut steps, 1.0..=500.0).text("Passos"))
            .changed()
        {
            d.fade_steps = steps as u32;
        }
    }
    if !angle {
        pct(ui, &mut d.minimum, "Mínimo");
    }
}

fn mask_mode(ui: &mut egui::Ui, id: &str, m: &mut MaskMode) {
    let label = |m: MaskMode| match m {
        MaskMode::Multiply => "Multiplicar",
        MaskMode::Subtract => "Subtrair",
        MaskMode::Darken => "Escurecer",
        MaskMode::Overlay => "Sobrepor",
        MaskMode::ColorDodge => "Subexposição de cor",
        MaskMode::ColorBurn => "Superexposição de cor",
        MaskMode::LinearBurn => "Superexposição linear",
        MaskMode::HardMix => "Mistura sólida",
        MaskMode::LinearHeight => "Altura linear",
        MaskMode::Height => "Altura",
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(label(*m))
        .width(170.0)
        .show_ui(ui, |ui| {
            for v in [
                MaskMode::Multiply,
                MaskMode::Subtract,
                MaskMode::Darken,
                MaskMode::Overlay,
                MaskMode::ColorDodge,
                MaskMode::ColorBurn,
                MaskMode::LinearBurn,
                MaskMode::HardMix,
                MaskMode::LinearHeight,
                MaskMode::Height,
            ] {
                ui.selectable_value(m, v, label(v));
            }
        });
}

/// Seção recolhível com um interruptor no título (como as caixas de seção do Photoshop/Clip).
fn section(
    ui: &mut egui::Ui,
    title: &str,
    enabled: Option<&mut bool>,
    add: impl FnOnce(&mut egui::Ui),
) {
    let id = ui.make_persistent_id(("brush-section", title));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
        .show_header(ui, |ui| {
            ui.label(RichText::new(title).strong());
            if let Some(on) = enabled {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(on, "");
                });
            }
        })
        .body(add);
}

/// Abas das configurações do pincel. Ajuste novo: ver o comentário do topo do arquivo.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BrushTab {
    /// Biblioteca de pincéis (só na ferramenta Pincel).
    Library,
    #[default]
    General,
    Shape,
    Grain,
    Dynamics,
    Smoothing,
    Advanced,
}

impl BrushTab {
    pub const SETTINGS: [BrushTab; 6] = [
        BrushTab::General,
        BrushTab::Shape,
        BrushTab::Grain,
        BrushTab::Dynamics,
        BrushTab::Smoothing,
        BrushTab::Advanced,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BrushTab::Library => "Pincéis",
            BrushTab::General => "Geral",
            BrushTab::Shape => "Forma",
            BrushTab::Grain => "Granulado",
            BrushTab::Dynamics => "Dinâmica",
            BrushTab::Smoothing => "Suavização",
            BrushTab::Advanced => "Avançado",
        }
    }
}

/// Barra de abas (botões grandes, quebrando linha em tela estreita). A aba escolhida fica na
/// memória do egui, por `id`, para cada ferramenta lembrar a sua.
pub fn tab_bar(ui: &mut egui::Ui, id: &str, tabs: &[BrushTab]) -> BrushTab {
    let key = egui::Id::unique(("brush-tab", id));
    let mut cur = ui
        .ctx()
        .data_mut(|d| d.get_persisted::<u8>(key))
        .and_then(|i| tabs.get(i as usize).copied())
        .unwrap_or(tabs[0]);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for &t in tabs {
            let on = cur == t;
            let text = RichText::new(t.label()).size(13.5);
            let text = if on {
                text.strong().color(color::ACCENT)
            } else {
                text
            };
            let btn = egui::Button::new(text)
                .selected(on)
                .min_size(egui::vec2(0.0, 34.0));
            if ui.add(btn).clicked() {
                cur = t;
            }
        }
    });
    if let Some(i) = tabs.iter().position(|t| *t == cur) {
        ui.ctx().data_mut(|d| d.insert_persisted(key, i as u8));
    }
    ui.separator();
    cur
}

/// Tamanho, opacidade, fluxo e dureza (o topo da aba Geral).
pub fn basics(ui: &mut egui::Ui, b: &mut BrushSettings, max_size: f32) {
    ui.add(
        egui::Slider::new(&mut b.size, 1.0..=max_size)
            .logarithmic(true)
            .text("Tamanho"),
    );
    // Tamanhos prontos, como a paleta "Tamanho do pincel" do Clip Studio.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::splat(3.0);
        for s in [
            1.0, 2.0, 3.0, 5.0, 8.0, 12.0, 20.0, 30.0, 50.0, 80.0, 120.0, 200.0,
        ] {
            if s <= max_size
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
}

/// Modo de mesclagem do pincel.
pub fn blend_mode(ui: &mut egui::Ui, b: &mut BrushSettings) {
    ui.horizontal(|ui| {
        ui.label("Mesclagem");
        egui::ComboBox::from_id_salt("brush-mode")
            .selected_text(b.mode.label())
            .width(150.0)
            .show_ui(ui, |ui| {
                for m in BlendMode::layer_modes() {
                    ui.selectable_value(&mut b.mode, m, m.label());
                }
            });
    });
}

/// O conteúdo de uma aba (menos Biblioteca e o topo da Geral, que o painel monta). Devolve
/// `true` se algo mudou.
pub fn show_tab(
    ui: &mut egui::Ui,
    tab: BrushTab,
    b: &mut BrushSettings,
    taper: &mut Taper,
) -> bool {
    let before = b.clone();
    let taper_before = *taper;
    match tab {
        BrushTab::Library | BrushTab::General => {}
        BrushTab::Shape => shape_tab(ui, b, taper),
        BrushTab::Grain => grain_tab(ui, b),
        BrushTab::Dynamics => dynamics_tab(ui, b),
        BrushTab::Smoothing => smoothing_tab(ui, b),
        BrushTab::Advanced => advanced_tab(ui, b),
    }
    *b != before || *taper != taper_before
}

fn shape_tab(ui: &mut egui::Ui, b: &mut BrushSettings, taper: &mut Taper) {
    section(ui, "Ponta", None, |ui| {
        let sampled = !matches!(b.tip, TipShape::Round);
        if sampled {
            hint(ui, "Ponta com imagem (do preset).");
            if ui.button("Trocar por ponta redonda").clicked() {
                b.tip = TipShape::Round;
            }
        }
        slider(ui, &mut b.angle, -180.0..=180.0, "Ângulo (°)");
        pct(ui, &mut b.roundness, "Redondeza");
        pct(ui, &mut b.spacing, "Espaçamento");
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut b.flip_x, "Espelhar X");
            ui.checkbox(&mut b.flip_y, "Espelhar Y");
            ui.checkbox(&mut b.aliased, "Sem antisserrilhado");
        });
    });

    section(ui, "Início e fim", None, |ui| {
        hint(
            ui,
            "Afina o traço no começo e no fim, como uma caneta de nanquim.",
        );
        slider(ui, &mut taper.start, 0.0..=400.0, "Início (px)");
        slider(ui, &mut taper.end, 0.0..=400.0, "Fim (px)");
    });
    let db = &mut b.dual_brush;
    section(ui, "Pincel duplo", Some(&mut db.enabled), |ui| {
        hint(
            ui,
            "Uma segunda ponta recorta o traço (cerdas, giz, esponja).",
        );
        mask_mode(ui, "db-mode", &mut db.mode);
        slider(ui, &mut db.size, 1.0..=500.0, "Tamanho");
        pct(ui, &mut db.hardness, "Dureza");
        pct(ui, &mut db.roundness, "Redondeza");
        slider(ui, &mut db.angle, -180.0..=180.0, "Ângulo (°)");
        pct(ui, &mut db.spacing, "Espaçamento");
        slider(ui, &mut db.scatter, 0.0..=10.0, "Dispersar");
        let mut count = db.count as f32;
        if ui
            .add(egui::Slider::new(&mut count, 1.0..=16.0).text("Quantidade"))
            .changed()
        {
            db.count = count as u32;
        }
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut db.both_axes, "Nos dois eixos");
            ui.checkbox(&mut db.flip, "Espelhar");
        });
    });
}

fn grain_tab(ui: &mut egui::Ui, b: &mut BrushSettings) {
    let tx = &mut b.texture;
    section(ui, "Textura", Some(&mut tx.enabled), |ui| {
        let style = match &tx.pattern {
            Pattern::Procedural { style, .. } => Some(*style),
            _ => None,
        };
        let name = |s: PatternStyle| match s {
            PatternStyle::Noise => "Ruído",
            PatternStyle::Canvas => "Tela de pintura",
            PatternStyle::Paper => "Papel",
            PatternStyle::Dots => "Pontos",
        };
        egui::ComboBox::from_id_salt("tx-pattern")
            .selected_text(style.map_or("Imagem do preset", name))
            .width(170.0)
            .show_ui(ui, |ui| {
                for s in [
                    PatternStyle::Paper,
                    PatternStyle::Canvas,
                    PatternStyle::Noise,
                    PatternStyle::Dots,
                ] {
                    if ui.selectable_label(style == Some(s), name(s)).clicked() {
                        tx.pattern = Pattern::Procedural {
                            style: s,
                            size: 128,
                            seed: 1,
                        };
                    }
                }
            });
        slider(ui, &mut tx.scale, 0.1..=4.0, "Escala");
        slider(ui, &mut tx.brightness, -1.0..=1.0, "Brilho");
        slider(ui, &mut tx.contrast, -1.0..=1.0, "Contraste");
        pct(ui, &mut tx.depth, "Profundidade");
        mask_mode(ui, "tx-mode", &mut tx.mode);
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut tx.invert, "Inverter");
            ui.checkbox(&mut tx.each_tip, "Em cada marca");
        });
    });

    section(ui, "Bordas", None, |ui| {
        ui.checkbox(&mut b.noise, "Ruído nas bordas");
    });
}

fn dynamics_tab(ui: &mut egui::Ui, b: &mut BrushSettings) {
    let sd = &mut b.shape_dynamics;
    section(ui, "Dinâmica de forma", Some(&mut sd.enabled), |ui| {
        dynamic(ui, "sd-size", "Tamanho", &mut sd.size, false);
        ui.separator();
        dynamic(ui, "sd-angle", "Ângulo", &mut sd.angle, true);
        ui.separator();
        dynamic(ui, "sd-round", "Redondeza", &mut sd.roundness, false);
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut sd.flip_x_jitter, "Espelhar X ao acaso");
            ui.checkbox(&mut sd.flip_y_jitter, "Espelhar Y ao acaso");
        });
        ui.checkbox(
            &mut sd.brush_projection,
            "Projeção da caneta (inclinação e giro)",
        );
    });

    let sc = &mut b.scattering;
    section(ui, "Dispersão", Some(&mut sc.enabled), |ui| {
        dynamic(ui, "sc-scatter", "Dispersar", &mut sc.scatter, false);
        ui.checkbox(&mut sc.both_axes, "Nos dois eixos");
        let mut count = sc.count as f32;
        if ui
            .add(egui::Slider::new(&mut count, 1.0..=16.0).text("Quantidade"))
            .changed()
        {
            sc.count = count as u32;
        }
        dynamic(
            ui,
            "sc-count",
            "Variação da quantidade",
            &mut sc.count_jitter,
            false,
        );
    });

    let tr = &mut b.transfer;
    section(ui, "Transferência", Some(&mut tr.enabled), |ui| {
        dynamic(ui, "tr-opacity", "Opacidade", &mut tr.opacity, false);
        ui.separator();
        dynamic(ui, "tr-flow", "Fluxo", &mut tr.flow, false);
    });

    let cd = &mut b.color_dynamics;
    section(ui, "Dinâmica de cor", Some(&mut cd.enabled), |ui| {
        hint(
            ui,
            "Varia a cor entre a principal e a secundária, e o matiz, a saturação e o brilho.",
        );
        dynamic(
            ui,
            "cd-fgbg",
            "Principal ↔ secundária",
            &mut cd.fg_bg,
            false,
        );
        pct(ui, &mut cd.hue_jitter, "Matiz");
        pct(ui, &mut cd.saturation_jitter, "Saturação");
        pct(ui, &mut cd.brightness_jitter, "Brilho");
        slider(ui, &mut cd.purity, -1.0..=1.0, "Pureza");
        ui.checkbox(&mut cd.per_tip, "Em cada marca");
    });
}

fn smoothing_tab(ui: &mut egui::Ui, b: &mut BrushSettings) {
    let sm = &mut b.smoothing;
    let mut p = sm.amount * 100.0;
    if ui
        .add(
            egui::Slider::new(&mut p, 0.0..=100.0)
                .suffix("%")
                .text("Estabilizador"),
        )
        .changed()
    {
        sm.amount = p / 100.0;
        sm.enabled = sm.amount > 0.0;
    }
    section(ui, "Estabilização", Some(&mut sm.enabled), |ui| {
        ui.checkbox(
            &mut sm.pulled_string,
            "Corda puxada (só anda depois da folga)",
        );
        ui.checkbox(&mut sm.catch_up, "Alcançar o cursor");
        ui.checkbox(&mut sm.catch_up_on_end, "Alcançar ao soltar");
        ui.checkbox(&mut sm.adjust_for_zoom, "Ajustar ao zoom");
    });
}

fn advanced_tab(ui: &mut egui::Ui, b: &mut BrushSettings) {
    section(ui, "Pintura úmida e aerógrafo", None, |ui| {
        ui.checkbox(&mut b.wet_edges, "Bordas molhadas (aquarela)");
        ui.checkbox(&mut b.build_up, "Acumular parado (aerógrafo)");
        if b.build_up {
            slider(ui, &mut b.build_up_rate, 1.0..=100.0, "Marcas por segundo");
        }
    });
}
