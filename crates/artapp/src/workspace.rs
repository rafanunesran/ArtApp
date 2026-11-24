//! Área de trabalho: onde ficam os painéis (encaixados na lateral, empurrando a tela, ou
//! flutuando por cima), de que lado, a largura e quais ficam abertos. Dá para guardar várias
//! e trocar pelo menu, como as áreas de trabalho do Clip Studio.

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::app::ArtApp;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub name: String,
    /// Painel da ferramenta (pincel e ajustes) encaixado na lateral.
    pub tools_docked: bool,
    pub tools_right: bool,
    pub layers_docked: bool,
    pub layers_right: bool,
    /// Largura dos painéis, em pontos.
    pub width: f32,
    pub tools_open: bool,
    pub layers_open: bool,
    pub color_float: bool,
    pub navigator: bool,
}

pub const MIN_WIDTH: f32 = 240.0;
pub const MAX_WIDTH: f32 = 480.0;

impl Default for Workspace {
    fn default() -> Self {
        Self {
            name: "Padrão".into(),
            tools_docked: false,
            tools_right: false,
            layers_docked: false,
            layers_right: true,
            width: 290.0,
            tools_open: false,
            layers_open: false,
            color_float: false,
            navigator: false,
        }
    }
}

/// As áreas de trabalho que vêm com o app.
pub fn presets() -> Vec<Workspace> {
    vec![
        Workspace::default(),
        Workspace {
            name: "Pintura".into(),
            tools_docked: true,
            layers_docked: true,
            tools_open: true,
            layers_open: true,
            color_float: true,
            ..Workspace::default()
        },
        Workspace {
            name: "Quadrinho".into(),
            layers_docked: true,
            layers_open: true,
            navigator: true,
            ..Workspace::default()
        },
        Workspace {
            name: "Tela limpa".into(),
            ..Workspace::default()
        },
    ]
}

impl ArtApp {
    /// Os painéis encaixam só com a tela deitada (em pé eles abrem embaixo).
    pub fn tools_docked(&self) -> bool {
        self.workspace.tools_docked && !self.portrait
    }

    pub fn layers_docked(&self) -> bool {
        self.workspace.layers_docked && !self.portrait
    }

    /// A área de trabalho como está agora (painéis abertos incluídos).
    pub fn current_workspace(&self, name: &str) -> Workspace {
        Workspace {
            name: name.to_string(),
            tools_open: self.show_tools_panel,
            layers_open: self.show_layers_panel,
            color_float: self.color_float,
            navigator: self.show_navigator,
            ..self.workspace.clone()
        }
    }

    /// Passa a usar a área de trabalho `w`.
    pub fn apply_workspace(&mut self, w: &Workspace) {
        self.workspace = w.clone();
        self.workspace.width = w.width.clamp(MIN_WIDTH, MAX_WIDTH);
        self.show_tools_panel = w.tools_open;
        self.show_layers_panel = w.layers_open;
        self.color_float = w.color_float;
        self.show_navigator = w.navigator;
        self.status = format!("Área de trabalho: {}", w.name);
        self.save_prefs();
    }

    /// Guarda a área de trabalho atual com o nome `name` (substitui uma de mesmo nome).
    pub fn save_workspace(&mut self, name: &str) {
        let name = name.trim();
        let name = if name.is_empty() { "Minha área" } else { name };
        let w = self.current_workspace(name);
        self.workspace.name = name.to_string();
        match self.workspaces.iter_mut().find(|x| x.name == name) {
            Some(x) => *x = w,
            None => self.workspaces.push(w),
        }
        self.status = format!("Área de trabalho “{name}” guardada.");
        self.save_prefs();
    }
}

/// O que dá para fazer com a área de trabalho no menu Visualização.
pub fn menu(app: &mut ArtApp, ui: &mut egui::Ui) {
    use crate::theme;
    ui.label(
        egui::RichText::new("Área de trabalho")
            .size(12.0)
            .color(theme::color::TEXT_MUTED),
    );
    let mut pick = None;
    ui.horizontal_wrapped(|ui| {
        ui.set_max_width(210.0);
        for w in presets().into_iter().chain(app.workspaces.clone()) {
            let on = app.workspace.name == w.name;
            if ui.selectable_label(on, &w.name).clicked() {
                pick = Some(w);
            }
        }
    });
    if let Some(w) = pick {
        app.apply_workspace(&w);
    }
    let mut changed = false;
    ui.add_sized([210.0, 36.0], |ui: &mut egui::Ui| {
        let r = theme::switch(ui, &mut app.workspace.tools_docked, "Encaixar ferramenta");
        changed |= r.changed();
        r
    });
    ui.add_sized([210.0, 36.0], |ui: &mut egui::Ui| {
        let r = theme::switch(ui, &mut app.workspace.layers_docked, "Encaixar camadas");
        changed |= r.changed();
        r
    });
    changed |= ui
        .add(
            egui::Slider::new(&mut app.workspace.width, MIN_WIDTH..=MAX_WIDTH)
                .text("Largura")
                .suffix(" pt"),
        )
        .changed();
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.workspace_name)
                .hint_text("Nome")
                .desired_width(110.0),
        );
        if ui.button("Guardar").clicked() {
            let name = std::mem::take(&mut app.workspace_name);
            app.save_workspace(&name);
        }
    });
    let mine = app.workspaces.iter().any(|w| w.name == app.workspace.name);
    if mine && ui.button("Apagar esta área").clicked() {
        let name = app.workspace.name.clone();
        app.workspaces.retain(|w| w.name != name);
        app.workspace.name = "Padrão".into();
        changed = true;
    }
    if changed {
        app.save_prefs();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_have_distinct_names_and_round_trip() {
        let p = presets();
        let mut names: Vec<_> = p.iter().map(|w| w.name.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), p.len());
        for w in &p {
            let json = serde_json::to_value(w).unwrap_or_default();
            let back: Workspace = serde_json::from_value(json).unwrap_or_default();
            assert_eq!(&back, w);
        }
        // Campos que faltam (prefs antigas) ficam no padrão.
        let old: Workspace =
            serde_json::from_value(serde_json::json!({"name": "X"})).unwrap_or_default();
        assert_eq!(old.width, Workspace::default().width);
    }
}
