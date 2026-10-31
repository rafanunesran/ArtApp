//! Janela do boneco 3D de pose: toca numa parte para escolher, arrasta a parte para girar em
//! volta da junta, arrasta o fundo para girar a câmera, e põe o boneco no desenho como camada
//! (que dá para editar de novo depois, como a camada 3D do Clip Studio).

use artapp_core::pose::{self, BONES, Built, Figure, Style};
use eframe::egui::{self, Align2, Color32, Pos2, RichText, Sense, Stroke, TextureOptions, Vec2};
use serde::{Deserialize, Serialize};

use crate::app::ArtApp;
use crate::platform;
use crate::theme::{self, color, icon, panel_frame};

/// Lado maior da vista desenhada, em pixels.
const VIEW_PX: f32 = 900.0;
const UNDO_STEPS: usize = 40;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SavedPose {
    name: String,
    pose: Vec<[f32; 3]>,
}

enum Drag {
    Bone(usize),
    Orbit,
}

#[derive(Default)]
pub struct PoseState {
    pub open: bool,
    fig: Figure,
    sel: Option<usize>,
    /// A camada de boneco que está sendo editada (senão, o boneco vai para uma camada nova).
    layer: Option<u64>,
    undo: Vec<Figure>,
    drag: Option<Drag>,
    built: Option<(Figure, Built)>,
    view: Option<artapp_core::model3d::View>,
    shown: Option<(Figure, Option<usize>, [u32; 2])>,
    handle: Option<egui::TextureHandle>,
    saved: Vec<SavedPose>,
    loaded: bool,
    name: String,
}

fn saved_path() -> std::path::PathBuf {
    platform::data_dir().join("poses.json")
}

impl PoseState {
    fn push_undo(&mut self) {
        if self.undo.last() != Some(&self.fig) {
            self.undo.push(self.fig.clone());
            if self.undo.len() > UNDO_STEPS {
                self.undo.remove(0);
            }
        }
    }

    fn load_saved(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.saved = std::fs::read(saved_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
    }

    fn store_saved(&self) {
        if let Ok(b) = serde_json::to_vec_pretty(&self.saved) {
            let _ = std::fs::write(saved_path(), b);
        }
    }

    fn built(&mut self) -> &Built {
        if self.built.as_ref().is_none_or(|(f, _)| *f != self.fig) {
            self.built = Some((self.fig.clone(), self.fig.build()));
        }
        match &self.built {
            Some((_, b)) => b,
            None => unreachable!(),
        }
    }
}

impl ArtApp {
    /// Abre o boneco: o da camada ativa, se ela for de boneco, ou um novo.
    pub fn open_pose(&mut self) {
        let st = &mut self.pose;
        match self
            .doc
            .active_layer()
            .and_then(|l| l.pose.clone().map(|f| (l.id, f)))
        {
            Some((id, f)) => {
                st.fig = f;
                st.layer = Some(id);
            }
            None => st.layer = None,
        }
        st.undo.clear();
        st.sel = None;
        st.drag = None;
        st.open = true;
        self.close_panels();
    }

    fn apply_pose(&mut self) {
        let fig = self.pose.fig.clone();
        let r = self.doc.place_figure(&fig, self.pose.layer);
        self.invalidate(r);
        self.kit.reset();
        self.status = if self.pose.layer.is_some() {
            "Boneco 3D atualizado.".into()
        } else {
            "Boneco 3D posto numa camada nova. Desenhe por cima numa camada acima.".into()
        };
        self.pose.open = false;
    }
}

pub fn show(app: &mut ArtApp, ctx: &egui::Context) {
    if !app.pose.open {
        return;
    }
    app.pose.load_saved();
    let screen = ctx.content_rect();
    // A vista tem o formato da tela de desenho (o boneco sai no desenho como aparece aqui).
    let aspect = app.doc.width.max(1) as f32 / app.doc.height.max(1) as f32;
    let (max_w, max_h) = if app.portrait {
        (screen.width() - 56.0, screen.height() * 0.42)
    } else {
        (
            (screen.width() - 360.0).clamp(220.0, 460.0),
            (screen.height() - 120.0).clamp(220.0, 560.0),
        )
    };
    let vh = max_h.min(max_w / aspect);
    let view_size = Vec2::new(vh * aspect, vh);
    let mut open = true;
    let mut apply = false;
    egui::Window::new(format!("{}  Boneco 3D", icon::PERSON))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(panel_frame())
        .open(&mut open)
        .show(ctx, |ui| {
            if app.portrait {
                viewport(app, ui, view_size);
                egui::ScrollArea::vertical()
                    .max_height(screen.height() * 0.32)
                    .show(ui, |ui| apply |= controls(app, ui));
            } else {
                ui.horizontal_top(|ui| {
                    viewport(app, ui, view_size);
                    ui.vertical(|ui| {
                        ui.set_width(250.0);
                        egui::ScrollArea::vertical()
                            .max_height(view_size.y)
                            .show(ui, |ui| apply |= controls(app, ui));
                    });
                });
            }
        });
    if apply {
        app.apply_pose();
    }
    if !open {
        app.pose.open = false;
    }
}

/// Ponto do mundo na vista (pontos da tela).
fn to_screen(fig: &Figure, rect: egui::Rect, p: [f32; 3]) -> Option<Pos2> {
    let s = fig.camera.project(p, rect.width(), rect.height())?;
    Some(rect.min + Vec2::new(s[0], s[1]))
}

fn viewport(app: &mut ArtApp, ui: &mut egui::Ui, size: Vec2) {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let ctx = ui.ctx().clone();
    let st = &mut app.pose;
    // A parte (ou o fundo) sob o ponto da tela.
    let pick_at = |st: &PoseState, p: Pos2| {
        let v = st.view.as_ref()?;
        let x = (p.x - rect.min.x) / rect.width() * v.width as f32;
        let y = (p.y - rect.min.y) / rect.height() * v.height as f32;
        pose::pick(v, x, y)
    };
    let (scroll, zoom, multi, origin) = ctx.input(|i| {
        (
            i.smooth_scroll_delta.y,
            i.zoom_delta(),
            i.multi_touch(),
            i.pointer.press_origin(),
        )
    });
    if resp.hovered() {
        if scroll != 0.0 {
            st.fig.camera.zoom((scroll / 200.0).exp());
        }
        if zoom != 1.0 {
            st.fig.camera.zoom(zoom);
        }
    }
    if let Some(m) = multi {
        st.fig
            .camera
            .orbit(-m.translation_delta.x * 0.01, m.translation_delta.y * 0.01);
        st.drag = None;
    } else {
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            st.sel = pick_at(st, p);
        }
        if resp.drag_started() {
            let at = origin.or(resp.interact_pointer_pos());
            st.drag = Some(match at.and_then(|p| pick_at(st, p)) {
                Some(b) => {
                    st.sel = Some(b);
                    st.push_undo();
                    Drag::Bone(b)
                }
                None => Drag::Orbit,
            });
        }
        if resp.dragged()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let d = resp.drag_delta();
            match st.drag {
                Some(Drag::Bone(b)) => {
                    let joint = st.built().joints[b];
                    if let Some(j) = to_screen(&st.fig, rect, joint) {
                        let prev = p - d;
                        let ang = |q: Pos2| (-(q.y - j.y)).atan2(q.x - j.x);
                        let (a0, a1) = (ang(prev), ang(p));
                        let mut delta = a1 - a0;
                        if delta > std::f32::consts::PI {
                            delta -= std::f32::consts::TAU;
                        } else if delta < -std::f32::consts::PI {
                            delta += std::f32::consts::TAU;
                        }
                        // Perto da junta o ângulo pula; ignora movimentos ali.
                        if (p - j).length() > 6.0 && delta != 0.0 {
                            let axis = st.fig.camera.toward_viewer();
                            st.fig.rotate_world(b, axis, delta);
                        }
                    }
                }
                Some(Drag::Orbit) => st.fig.camera.orbit(-d.x * 0.008, d.y * 0.008),
                None => {}
            }
        }
        if resp.drag_stopped() {
            st.drag = None;
        }
    }

    // A vista desenhada (refeita só quando algo muda).
    let ppp = ctx.pixels_per_point();
    let k = (VIEW_PX / (size.x.max(size.y) * ppp)).min(1.0) * ppp;
    let px = [
        (size.x * k).round().max(1.0) as u32,
        (size.y * k).round().max(1.0) as u32,
    ];
    let want = (st.fig.clone(), st.sel, px);
    if st.shown.as_ref() != Some(&want) {
        st.built();
        let view = match &st.built {
            Some((_, b)) => st.fig.render_view(b, st.sel, px[0], px[1]),
            None => return,
        };
        let img =
            egui::ColorImage::from_rgba_unmultiplied([px[0] as usize, px[1] as usize], &view.rgba);
        match &mut st.handle {
            Some(h) => h.set(img, TextureOptions::LINEAR),
            None => st.handle = Some(ctx.load_texture("pose_view", img, TextureOptions::LINEAR)),
        }
        st.view = Some(view);
        st.shown = Some(want);
    }
    let painter = ui.painter_at(rect);
    if let Some(h) = &st.handle {
        painter.image(
            h.id(),
            rect,
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    // A junta do osso escolhido.
    if let Some(b) = st.sel {
        let joint = st.built().joints[b];
        if let Some(j) = to_screen(&st.fig, rect, joint) {
            painter.circle(j, 6.0, color::ACCENT, Stroke::new(2.0, Color32::WHITE));
        }
    }
    if st.sel.is_none() && st.undo.is_empty() {
        painter.text(
            rect.center_bottom() - Vec2::new(0.0, 10.0),
            Align2::CENTER_BOTTOM,
            "Arraste uma parte para girar · o fundo gira a câmera",
            egui::FontId::proportional(12.0),
            Color32::from_gray(190),
        );
    }
}

fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).size(12.0).color(color::TEXT_MUTED));
}

fn degrees(ui: &mut egui::Ui, v: &mut f32, label: &str) -> egui::Response {
    let mut d = v.to_degrees();
    let r = ui.add(
        egui::Slider::new(&mut d, -180.0..=180.0)
            .text(label)
            .suffix("°")
            .step_by(1.0),
    );
    if r.changed() {
        *v = d.to_radians();
    }
    r
}

/// Os ajustes ao lado da vista. Devolve `true` para pôr o boneco no desenho.
fn controls(app: &mut ArtApp, ui: &mut egui::Ui) -> bool {
    let editing = app.pose.layer.is_some();
    let st = &mut app.pose;
    let mut apply = false;

    section(ui, "Poses");
    let mut pick: Option<Vec<[f32; 3]>> = None;
    let mut remove = None;
    ui.horizontal_wrapped(|ui| {
        for (name, p) in pose::presets() {
            if ui.selectable_label(st.fig.pose == p, name).clicked() {
                pick = Some(p);
            }
        }
        for (i, s) in st.saved.iter().enumerate() {
            let r = ui
                .selectable_label(st.fig.pose == s.pose, &s.name)
                .on_hover_text("Toque para usar · toque e segure para apagar");
            if r.clicked() {
                pick = Some(s.pose.clone());
            }
            if r.long_touched() || r.secondary_clicked() {
                remove = Some(i);
            }
        }
    });
    if let Some(p) = pick {
        st.push_undo();
        st.fig.pose = p;
    }
    if let Some(i) = remove {
        let name = st.saved.remove(i).name;
        st.store_saved();
        app.status = format!("Pose “{name}” apagada.");
    }
    let st = &mut app.pose;
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                !st.undo.is_empty(),
                egui::Button::new(format!("{}", icon::UNDO)),
            )
            .on_hover_text("Desfazer no boneco")
            .clicked()
            && let Some(f) = st.undo.pop()
        {
            st.fig = f;
        }
        if ui
            .button("Espelhar")
            .on_hover_text("Troca esquerda e direita")
            .clicked()
        {
            st.push_undo();
            st.fig = st.fig.mirrored();
        }
        if ui
            .button("Esq. para dir.")
            .on_hover_text("Copia a pose do lado esquerdo para o direito")
            .clicked()
        {
            st.push_undo();
            st.fig.copy_side(true);
        }
        if ui
            .button("Dir. para esq.")
            .on_hover_text("Copia a pose do lado direito para o esquerdo")
            .clicked()
        {
            st.push_undo();
            st.fig.copy_side(false);
        }
    });

    section(ui, "Parte escolhida");
    match st.sel {
        Some(b) if b < BONES => {
            ui.horizontal(|ui| {
                ui.label(RichText::new(pose::NAMES[b]).strong());
                if ui.small_button("Endireitar").clicked() {
                    st.push_undo();
                    st.fig.set_angles(b, [0.0; 3]);
                }
            });
            let names = if b == pose::PELVIS {
                ["Inclinar", "Virar", "Tombar"]
            } else {
                ["Dobrar", "Girar", "Abrir"]
            };
            let mut a = st.fig.angles(b);
            let mut changed = false;
            for (k, name) in names.iter().enumerate() {
                let r = degrees(ui, &mut a[k], name);
                if r.drag_started() || (r.changed() && !r.dragged()) {
                    st.push_undo();
                }
                changed |= r.changed();
            }
            if changed {
                st.fig.set_angles(b, a);
            }
        }
        _ => {
            ui.label(
                RichText::new("Toque numa parte do boneco para escolher e ajustar os ângulos.")
                    .size(12.0)
                    .color(color::TEXT_MUTED),
            );
        }
    }

    section(ui, "Corpo");
    let r = ui.add(
        egui::Slider::new(&mut st.fig.heads, pose::HEADS)
            .text("Cabeças")
            .step_by(0.5),
    );
    if r.drag_started() {
        st.push_undo();
    }
    let r = ui.add(egui::Slider::new(&mut st.fig.build, pose::BUILD).text("Grossura"));
    if r.drag_started() {
        st.push_undo();
    }
    ui.add_sized([230.0, 32.0], |ui: &mut egui::Ui| {
        theme::switch(ui, &mut st.fig.grounded, "Pés no chão")
    });

    section(ui, "Câmera");
    ui.horizontal_wrapped(|ui| {
        for (name, yaw) in [
            ("Frente", 0.0),
            ("3/4", 0.6),
            ("Lado", std::f32::consts::FRAC_PI_2),
            ("Costas", std::f32::consts::PI),
        ] {
            if ui.button(name).clicked() {
                st.fig.camera.yaw = yaw;
                st.fig.camera.pitch = 0.1;
            }
        }
    });
    ui.add(
        egui::Slider::new(&mut st.fig.camera.pitch, -1.4..=1.4)
            .text("De cima / de baixo")
            .show_value(false),
    );
    let mut near = 13.3 - st.fig.camera.dist;
    if ui
        .add(
            egui::Slider::new(&mut near, 1.3..=12.0)
                .text("Aproximar")
                .show_value(false),
        )
        .changed()
    {
        st.fig.camera.dist = 13.3 - near;
    }

    section(ui, "No desenho");
    ui.horizontal(|ui| {
        for (s, name) in [
            (Style::Shaded, "Sombreado"),
            (Style::Outline, "Contorno"),
            (Style::Both, "Os dois"),
        ] {
            if ui.selectable_label(st.fig.style == s, name).clicked() {
                st.fig.style = s;
            }
        }
    });

    section(ui, "Guardar a pose");
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut st.name)
                .hint_text("Nome")
                .desired_width(140.0),
        );
        if ui.button("Guardar").clicked() {
            let name = st.name.trim().to_string();
            let name = if name.is_empty() {
                format!("Pose {}", st.saved.len() + 1)
            } else {
                name
            };
            let pose = st.fig.pose.clone();
            match st.saved.iter_mut().find(|s| s.name == name) {
                Some(s) => s.pose = pose,
                None => st.saved.push(SavedPose {
                    name: name.clone(),
                    pose,
                }),
            }
            st.store_saved();
            st.name.clear();
        }
    });

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let text = if editing {
            "Atualizar o boneco"
        } else {
            "Pôr no desenho"
        };
        let b = egui::Button::new(RichText::new(text).color(color::ON_ACCENT)).fill(color::ACCENT);
        if ui.add(b).clicked() {
            apply = true;
        }
        if ui.button("Cancelar").clicked() {
            st.open = false;
        }
    });
    apply
}
