//! Painel de cor, como os do Clip Studio: círculo de cores (anel de matiz e quadrado ou triângulo
//! de saturação e valor), barras RGB/HSV com hexadecimal, conjuntos de cores salvos e cor intermediária
//! (mistura entre quatro cantos).

use eframe::egui::{self, Color32, CornerRadius, Pos2, Sense, Stroke, Vec2, ecolor};
use serde::{Deserialize, Serialize};

use crate::app::ArtApp;
use crate::theme::{color, icon};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorTab {
    #[default]
    Wheel,
    Sliders,
    Sets,
    Mix,
    Harmony,
}

/// As harmonias de cor do círculo (giros de matiz a partir da cor base).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Harmony {
    #[default]
    Complementary,
    Split,
    Analogous,
    Triadic,
    Tetradic,
}

impl Harmony {
    const ALL: [(Harmony, &'static str); 5] = [
        (Harmony::Complementary, "Complementar"),
        (Harmony::Split, "Compl. dividida"),
        (Harmony::Analogous, "Análoga"),
        (Harmony::Triadic, "Triádica"),
        (Harmony::Tetradic, "Tetrádica"),
    ];

    /// Giros de matiz (em voltas) das cores da harmonia, a base incluída.
    fn turns(self) -> &'static [f32] {
        match self {
            Harmony::Complementary => &[0.0, 0.5],
            Harmony::Split => &[0.0, 5.0 / 12.0, 7.0 / 12.0],
            Harmony::Analogous => &[-1.0 / 12.0, 0.0, 1.0 / 12.0],
            Harmony::Triadic => &[0.0, 1.0 / 3.0, 2.0 / 3.0],
            Harmony::Tetradic => &[0.0, 0.25, 0.5, 0.75],
        }
    }
}

/// Um conjunto de cores salvo.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ColorSet {
    pub name: String,
    /// RGBA 8 bits.
    pub colors: Vec<[u8; 4]>,
}

/// O estado do painel que sobrevive entre quadros.
pub struct ColorState {
    pub tab: ColorTab,
    /// Matiz, saturação e valor da cor atual (guardados para o matiz não sumir no preto/branco).
    pub hsv: [f32; 3],
    /// A cor para a qual `hsv` foi calculado.
    pub synced: Color32,
    pub hex: String,
    pub sets: Vec<ColorSet>,
    pub set_index: usize,
    /// Cantos da cor intermediária: cima-esquerda, cima-direita, baixo-esquerda, baixo-direita.
    pub corners: [Color32; 4],
    /// O meio do círculo é um triângulo (matiz pura, branco e preto) em vez do quadrado.
    pub triangle: bool,
    pub harmony: Harmony,
    /// A cor base da harmonia (escolher uma das outras não a muda).
    pub harmony_base: Option<[f32; 3]>,
}

impl Default for ColorState {
    fn default() -> Self {
        Self {
            tab: ColorTab::Wheel,
            hsv: [0.0, 0.0, 0.0],
            synced: Color32::TRANSPARENT,
            hex: String::new(),
            sets: load_sets(),
            set_index: 0,
            corners: [
                Color32::from_rgb(230, 70, 60),
                Color32::from_rgb(250, 210, 80),
                Color32::from_rgb(60, 110, 220),
                Color32::from_rgb(40, 40, 50),
            ],
            triangle: false,
            harmony: Harmony::default(),
            harmony_base: None,
        }
    }
}

fn sets_path() -> std::path::PathBuf {
    crate::platform::data_dir().join("colorsets.json")
}

fn load_sets() -> Vec<ColorSet> {
    let saved = std::fs::read(sets_path())
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<ColorSet>>(&b).ok())
        .filter(|v| !v.is_empty());
    saved.unwrap_or_else(default_sets)
}

pub(crate) fn save_sets(sets: &[ColorSet]) {
    let path = sets_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(sets) {
        let _ = std::fs::write(path, json);
    }
}

fn default_sets() -> Vec<ColorSet> {
    let hex = |s: &[u32]| {
        s.iter()
            .map(|c| [(c >> 16) as u8, (c >> 8) as u8, *c as u8, 255])
            .collect()
    };
    vec![
        ColorSet {
            name: "Básicas".into(),
            colors: hex(&[
                0x000000, 0x404040, 0x808080, 0xc0c0c0, 0xffffff, 0xe53935, 0xfb8c00, 0xfdd835,
                0x43a047, 0x00acc1, 0x1e88e5, 0x5e35b1, 0xd81b60, 0x6d4c41,
            ]),
        },
        ColorSet {
            name: "Pele".into(),
            colors: hex(&[
                0xffe0c8, 0xf8cfae, 0xeab38f, 0xd99a74, 0xc58058, 0xa86743, 0x8a5233, 0x6b3d25,
                0x4e2c1b, 0xf3b5a0, 0xe39a8a,
            ]),
        },
        ColorSet {
            name: "Tons de cinza".into(),
            colors: (0..=10)
                .map(|i| {
                    let v = (i * 255 / 10) as u8;
                    [v, v, v, 255]
                })
                .collect(),
        },
    ]
}

// HSV calculado direto sobre o RGB da tela (sRGB), como nos programas de desenho. Fazer em RGB
// linear deixa os degradês do círculo lavados, com pouca cor escura.
fn hsv_of(c: Color32) -> [f32; 3] {
    let f = |v: u8| v as f32 / 255.0;
    let (h, s, v) = ecolor::hsv_from_rgb([f(c.r()), f(c.g()), f(c.b())]);
    [h, s, v]
}

fn color_of(hsv: [f32; 3]) -> Color32 {
    let [r, g, b] = ecolor::rgb_from_hsv((hsv[0], hsv[1], hsv[2]));
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(q(r), q(g), q(b))
}

fn rgba(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

/// Atualiza o HSV guardado quando a cor mudou por fora (conta-gotas, recentes, troca...).
fn sync(app: &mut ArtApp) {
    let st = &mut app.color_state;
    if st.synced != app.color {
        let hsv = hsv_of(app.color);
        // Preto, branco e cinzas não têm matiz: mantém o que já estava.
        st.hsv = if hsv[1] <= 0.0 || hsv[2] <= 0.0 {
            [st.hsv[0], hsv[1], hsv[2]]
        } else {
            hsv
        };
        st.synced = app.color;
        st.hex = format!(
            "{:02X}{:02X}{:02X}",
            app.color.r(),
            app.color.g(),
            app.color.b()
        );
    }
}

fn apply(app: &mut ArtApp, picked: Option<Color32>) {
    if let Some(c) = picked
        && c != app.color
    {
        app.color = c;
        app.transparent = false;
        app.color_state.synced = c;
        app.color_state.hex = format!("{:02X}{:02X}{:02X}", c.r(), c.g(), c.b());
    }
}

/// Mostra o painel (o conteúdo do popup de cor).
pub fn show(app: &mut ArtApp, ui: &mut egui::Ui) {
    sync(app);
    ui.horizontal(|ui| {
        for (tab, name) in [
            (ColorTab::Wheel, "Círculo"),
            (ColorTab::Sliders, "Barras"),
            (ColorTab::Sets, "Conjuntos"),
            (ColorTab::Mix, "Intermediária"),
            (ColorTab::Harmony, "Harmonia"),
        ] {
            ui.selectable_value(&mut app.color_state.tab, tab, name);
        }
    });
    ui.add_space(4.0);
    let picked = match app.color_state.tab {
        ColorTab::Wheel => {
            let c = wheel(ui, &mut app.color_state, 240.0);
            wheel_options(app, ui);
            c
        }
        ColorTab::Sliders => sliders(ui, &mut app.color_state),
        ColorTab::Sets => sets(ui, &mut app.color_state, app.color),
        ColorTab::Mix => mix(ui, &mut app.color_state, app.color),
        ColorTab::Harmony => harmony(ui, &mut app.color_state, app.color),
    };
    apply(app, picked);
}

/// Só o círculo, do tamanho pedido (a roda flutuante).
pub fn show_wheel(app: &mut ArtApp, ui: &mut egui::Ui, size: f32) {
    sync(app);
    let picked = wheel(ui, &mut app.color_state, size);
    apply(app, picked);
}

/// Quadrado ou triângulo, e deixar a roda flutuante.
fn wheel_options(app: &mut ArtApp, ui: &mut egui::Ui) {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Meio");
        changed |= ui
            .selectable_value(
                &mut app.color_state.triangle,
                false,
                format!("{}  Quadrado", icon::SQUARE),
            )
            .changed();
        changed |= ui
            .selectable_value(
                &mut app.color_state.triangle,
                true,
                format!("{}  Triângulo", icon::TRIANGLE),
            )
            .changed();
    });
    let text = if app.color_float {
        "Tirar a roda flutuante"
    } else {
        "Deixar a roda flutuante"
    };
    if ui
        .button(format!("{}  {text}", icon::MAXIMIZE))
        .on_hover_text("A roda fica sempre na tela, mesmo enquanto você desenha")
        .clicked()
    {
        app.color_float = !app.color_float;
        app.color_float_collapsed = false;
        if app.color_float {
            app.show_color = false;
        }
        changed = true;
    }
    if changed {
        app.save_prefs();
    }
}

/// Anel de matiz em volta de um quadrado de saturação (x) e valor (y), ou de um triângulo com o
/// branco em cima à esquerda, o preto embaixo à esquerda e a matiz pura na ponta da direita.
fn wheel(ui: &mut egui::Ui, st: &mut ColorState, max: f32) -> Option<Color32> {
    let size = ui.available_width().min(max);
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let c = rect.center();
    let outer = size / 2.0 - 2.0;
    let ring_w = (size * 0.09).clamp(14.0, 22.0);
    let inner = outer - ring_w;
    // Anel.
    let n = 96;
    let mut mesh = egui::Mesh::default();
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let a = hue_angle(t);
        let dir = Vec2::new(a.cos(), a.sin());
        let col = color_of([t, 1.0, 1.0]);
        mesh.colored_vertex(c + dir * inner, col);
        mesh.colored_vertex(c + dir * outer, col);
        if i > 0 {
            let k = (i * 2) as u32;
            mesh.add_triangle(k - 2, k - 1, k);
            mesh.add_triangle(k - 1, k, k + 1);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    let shape = if st.triangle {
        Inner::triangle(c, inner - 4.0)
    } else {
        let half = inner * std::f32::consts::FRAC_1_SQRT_2 - 4.0;
        Inner::Square(egui::Rect::from_center_size(c, Vec2::splat(half * 2.0)))
    };
    painter.add(egui::Shape::mesh(shape.mesh(st.hsv[0])));
    // Marcadores.
    let a = hue_angle(st.hsv[0]);
    let hm = c + Vec2::new(a.cos(), a.sin()) * (inner + outer) / 2.0;
    painter.circle_stroke(hm, ring_w * 0.4, Stroke::new(2.0, Color32::WHITE));
    let sm = shape.pos(st.hsv[1], st.hsv[2]);
    painter.circle_stroke(sm, 7.0, Stroke::new(2.0, Color32::WHITE));
    painter.circle_stroke(sm, 8.5, Stroke::new(1.0, Color32::BLACK));
    // Arrastar: o que foi tocado primeiro decide (anel ou o meio).
    let id = resp.id.with("ring");
    if (resp.drag_started() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let on_ring = (p - c).length() > inner - 2.0;
        ui.data_mut(|d| d.insert_temp(id, on_ring));
    }
    let p = resp.interact_pointer_pos()?;
    if !(resp.dragged() || resp.clicked()) {
        return None;
    }
    let on_ring = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    if on_ring {
        let d = p - c;
        st.hsv[0] = ((d.y.atan2(d.x) - HUE_OFFSET) / std::f32::consts::TAU).rem_euclid(1.0);
    } else {
        let (s, v) = shape.sv(p, st.hsv[1]);
        st.hsv[1] = s;
        st.hsv[2] = v;
    }
    Some(color_of(st.hsv))
}

/// Onde fica o vermelho no anel (ângulo de tela, y para baixo): em cima à esquerda, com o
/// amarelo no topo e o azul embaixo, como no Clip Studio.
const HUE_OFFSET: f32 = -150.0 * std::f32::consts::PI / 180.0;

fn hue_angle(hue: f32) -> f32 {
    hue * std::f32::consts::TAU + HUE_OFFSET
}

/// O meio do círculo de cores.
enum Inner {
    Square(egui::Rect),
    /// Vértices: matiz pura, branco, preto.
    Triangle([Pos2; 3]),
}

impl Inner {
    fn triangle(c: Pos2, r: f32) -> Self {
        let at = |deg: f32| {
            let a = deg.to_radians();
            c + Vec2::new(a.cos(), a.sin()) * r
        };
        // Lado vertical à esquerda (branco em cima, preto embaixo), cor pura na ponta direita.
        Self::Triangle([at(0.0), at(-120.0), at(120.0)])
    }

    /// Onde fica a saturação `s` e o valor `v`.
    fn pos(&self, s: f32, v: f32) -> Pos2 {
        match self {
            Self::Square(sq) => Pos2::new(
                sq.min.x + sq.width() * s,
                sq.min.y + sq.height() * (1.0 - v),
            ),
            // Do preto em direção à mistura branco→matiz: v escolhe a distância, s o lado.
            Self::Triangle([h, w, b]) => *b + (*w - *b) * v * (1.0 - s) + (*h - *b) * v * s,
        }
    }

    /// Saturação e valor no ponto `p` (preso dentro da forma). No preto a saturação não muda.
    fn sv(&self, p: Pos2, s_before: f32) -> (f32, f32) {
        match self {
            Self::Square(sq) => (
                ((p.x - sq.min.x) / sq.width()).clamp(0.0, 1.0),
                (1.0 - (p.y - sq.min.y) / sq.height()).clamp(0.0, 1.0),
            ),
            Self::Triangle([h, w, b]) => {
                // p = b + x·(w − b) + y·(h − b), com x, y ≥ 0 e x + y ≤ 1.
                let (e1, e2, d) = (*w - *b, *h - *b, p - *b);
                let det = e1.x * e2.y - e1.y * e2.x;
                let mut x = ((d.x * e2.y - d.y * e2.x) / det).max(0.0);
                let mut y = ((e1.x * d.y - e1.y * d.x) / det).max(0.0);
                if x + y > 1.0 {
                    let k = x + y;
                    x /= k;
                    y /= k;
                }
                let v = x + y;
                let s = if v > 1e-4 { y / v } else { s_before };
                (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0))
            }
        }
    }

    fn mesh(&self, hue: f32) -> egui::Mesh {
        let g = 12;
        let mut mesh = egui::Mesh::default();
        match self {
            Self::Square(_) => {
                for yi in 0..=g {
                    for xi in 0..=g {
                        let (s, v) = (xi as f32 / g as f32, 1.0 - yi as f32 / g as f32);
                        mesh.colored_vertex(self.pos(s, v), color_of([hue, s, v]));
                        if xi > 0 && yi > 0 {
                            let k = (yi * (g + 1) + xi) as u32;
                            let w = (g + 1) as u32;
                            mesh.add_triangle(k - w - 1, k - w, k);
                            mesh.add_triangle(k - w - 1, k - 1, k);
                        }
                    }
                }
            }
            Self::Triangle(_) => {
                // Fileiras de valor (do preto para fora); a fileira i tem i + 1 pontos.
                let start = |i: u32| i * (i + 1) / 2;
                for i in 0..=g {
                    let v = i as f32 / g as f32;
                    for j in 0..=i {
                        let s = if i == 0 { 0.0 } else { j as f32 / i as f32 };
                        mesh.colored_vertex(self.pos(s, v), color_of([hue, s, v]));
                    }
                    if i > 0 {
                        let (prev, cur) = (start(i as u32 - 1), start(i as u32));
                        for j in 0..i as u32 {
                            mesh.add_triangle(prev + j, cur + j, cur + j + 1);
                            if j + 1 < i as u32 {
                                mesh.add_triangle(prev + j, prev + j + 1, cur + j + 1);
                            }
                        }
                    }
                }
            }
        }
        mesh
    }
}

fn sliders(ui: &mut egui::Ui, st: &mut ColorState) -> Option<Color32> {
    let mut out = None;
    let c = color_of(st.hsv);
    let mut rgb = [c.r(), c.g(), c.b()];
    let mut changed_rgb = false;
    for (i, name) in ["R", "G", "B"].iter().enumerate() {
        changed_rgb |= ui
            .add(egui::Slider::new(&mut rgb[i], 0..=255).text(*name))
            .changed();
    }
    if changed_rgb {
        let c = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        let hsv = hsv_of(c);
        st.hsv = [
            if hsv[1] > 0.0 { hsv[0] } else { st.hsv[0] },
            hsv[1],
            hsv[2],
        ];
        out = Some(c);
    }
    ui.separator();
    let mut h = st.hsv[0] * 360.0;
    let mut s = st.hsv[1] * 100.0;
    let mut v = st.hsv[2] * 100.0;
    let mut changed = false;
    changed |= ui
        .add(
            egui::Slider::new(&mut h, 0.0..=360.0)
                .suffix("°")
                .text("Matiz"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut s, 0.0..=100.0)
                .suffix("%")
                .text("Saturação"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut v, 0.0..=100.0)
                .suffix("%")
                .text("Valor"),
        )
        .changed();
    if changed {
        st.hsv = [h / 360.0, s / 100.0, v / 100.0];
        out = Some(color_of(st.hsv));
    }
    ui.horizontal(|ui| {
        ui.label("#");
        let r = ui.add(egui::TextEdit::singleline(&mut st.hex).desired_width(80.0));
        if r.lost_focus() || r.changed() {
            let t = st.hex.trim().trim_start_matches('#');
            if t.len() == 6
                && let Ok(n) = u32::from_str_radix(t, 16)
            {
                let c = Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8);
                st.hsv = hsv_of(c);
                out = Some(c);
            }
        }
    });
    out
}

/// Harmonia: o círculo escolhe a base e as amostras mostram as cores que combinam com ela.
fn harmony(ui: &mut egui::Ui, st: &mut ColorState, current: Color32) -> Option<Color32> {
    let mut out = wheel(ui, st, 200.0);
    if let Some(c) = out {
        st.harmony_base = Some(hsv_of(c));
    }
    let base = *st.harmony_base.get_or_insert(st.hsv);
    ui.horizontal_wrapped(|ui| {
        for (h, name) in Harmony::ALL {
            ui.selectable_value(&mut st.harmony, h, name);
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for t in st.harmony.turns() {
            let c = color_of([(base[0] + t).rem_euclid(1.0), base[1], base[2]]);
            if swatch(ui, c, c == current, 44.0)
                .on_hover_text("Usar esta cor")
                .clicked()
            {
                out = Some(c);
            }
        }
    });
    out
}

fn swatch(ui: &mut egui::Ui, c: Color32, selected: bool, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    ui.painter().rect_filled(rect, CornerRadius::same(5), c);
    let edge = if selected {
        Stroke::new(2.0, color::ACCENT)
    } else {
        Stroke::new(1.0, color::BORDER)
    };
    ui.painter()
        .rect_stroke(rect, CornerRadius::same(5), edge, egui::StrokeKind::Outside);
    resp
}

fn sets(ui: &mut egui::Ui, st: &mut ColorState, current: Color32) -> Option<Color32> {
    let mut out = None;
    if st.sets.is_empty() {
        st.sets = default_sets();
    }
    st.set_index = st.set_index.min(st.sets.len() - 1);
    egui::ComboBox::from_id_salt("color_set")
        .selected_text(st.sets[st.set_index].name.clone())
        .show_ui(ui, |ui| {
            for (i, s) in st.sets.iter().enumerate() {
                ui.selectable_value(&mut st.set_index, i, s.name.clone());
            }
        });
    let mut remove = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(5.0);
        for (i, c) in st.sets[st.set_index].colors.iter().enumerate() {
            let c = rgba(*c);
            let r = swatch(ui, c, c == current, 26.0);
            if r.clicked() {
                out = Some(c);
            }
            r.context_menu(|ui| {
                if ui
                    .button(format!("{}  Tirar do conjunto", icon::TRASH))
                    .clicked()
                {
                    remove = Some(i);
                    ui.close();
                }
            });
        }
    });
    let mut dirty = false;
    if let Some(i) = remove {
        st.sets[st.set_index].colors.remove(i);
        dirty = true;
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(format!("{}  Pôr a cor atual", icon::PLUS))
            .clicked()
        {
            let c = [current.r(), current.g(), current.b(), 255];
            let set = &mut st.sets[st.set_index];
            if !set.colors.contains(&c) {
                set.colors.push(c);
                dirty = true;
            }
        }
        if ui
            .button(format!("{}  Novo conjunto", icon::FILE_PLUS))
            .clicked()
        {
            st.sets.push(ColorSet {
                name: format!("Conjunto {}", st.sets.len() + 1),
                colors: Vec::new(),
            });
            st.set_index = st.sets.len() - 1;
            dirty = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Nome");
        if ui
            .text_edit_singleline(&mut st.sets[st.set_index].name)
            .lost_focus()
        {
            dirty = true;
        }
    });
    ui.label(
        egui::RichText::new("Toque e segure (ou botão direito) numa cor para tirá-la.")
            .size(12.0)
            .color(color::TEXT_MUTED),
    );
    if dirty {
        save_sets(&st.sets);
    }
    out
}

/// Cor intermediária: uma grade que mistura os quatro cantos.
fn mix(ui: &mut egui::Ui, st: &mut ColorState, current: Color32) -> Option<Color32> {
    let mut out = None;
    let n = 7;
    let lerp = |a: Color32, b: Color32, t: f32| {
        let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
        Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
    };
    let cell = ((ui.available_width() - 6.0 * (n - 1) as f32) / n as f32).min(30.0);
    for yi in 0..n {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(4.0);
            for xi in 0..n {
                let (tx, ty) = (xi as f32 / (n - 1) as f32, yi as f32 / (n - 1) as f32);
                let top = lerp(st.corners[0], st.corners[1], tx);
                let bottom = lerp(st.corners[2], st.corners[3], tx);
                let c = lerp(top, bottom, ty);
                let corner = match (xi, yi) {
                    (0, 0) => Some(0),
                    (x, 0) if x == n - 1 => Some(1),
                    (0, y) if y == n - 1 => Some(2),
                    (x, y) if x == n - 1 && y == n - 1 => Some(3),
                    _ => None,
                };
                let r = swatch(ui, c, c == current, cell);
                if r.clicked() {
                    out = Some(c);
                }
                if let Some(k) = corner {
                    r.context_menu(|ui| {
                        if ui.button("Usar a cor atual neste canto").clicked() {
                            st.corners[k] = current;
                            ui.close();
                        }
                    });
                }
            }
        });
    }
    ui.label(
        egui::RichText::new(
            "Os quatro cantos são as cores de base: toque e segure (ou botão direito) num canto para trocar pela cor atual.",
        )
        .size(12.0)
        .color(color::TEXT_MUTED),
    );
    out
}
