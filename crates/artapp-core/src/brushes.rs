//! Pincéis prontos. Os de pintura vêm dos presets embutidos do PhotoCraft; os de desenho
//! (lápis, caneta-tinteiro, marcador) são ajustes nossos para trabalho de linha, no estilo Clip
//! Studio, feitos sobre o mesmo motor.

/// Maior tamanho de pincel aceito pelo motor do PhotoCraft.
pub const MAX_SIZE: f32 = photocraft_paint::MAX_BRUSH_SIZE;

use photocraft_paint::{
    BrushPreset, BrushSettings, Control, Dynamic, ShapeDynamics, Smoothing, Transfer, presets,
};

/// Um pincel da paleta: nome mostrado e configurações.
#[derive(Clone, Debug)]
pub struct BrushEntry {
    pub name: String,
    pub group: String,
    pub settings: BrushSettings,
}

fn pressure_size(minimum: f32) -> ShapeDynamics {
    ShapeDynamics {
        enabled: true,
        size: Dynamic {
            minimum,
            ..Dynamic::controlled(Control::PenPressure)
        },
        ..Default::default()
    }
}

fn ours(name: &str, settings: BrushSettings) -> BrushEntry {
    BrushEntry {
        name: name.into(),
        group: "Desenho".into(),
        settings,
    }
}

/// Pincéis de linha do ArtApp.
pub fn drawing() -> Vec<BrushEntry> {
    let base = BrushSettings::default();
    vec![
        ours(
            "Caneta G",
            BrushSettings {
                size: 8.0,
                hardness: 1.0,
                spacing: 0.05,
                shape_dynamics: pressure_size(0.05),
                smoothing: Smoothing {
                    amount: 0.3,
                    ..Default::default()
                },
                ..base.clone()
            },
        ),
        ours(
            "Lápis",
            BrushSettings {
                size: 4.0,
                hardness: 0.6,
                spacing: 0.08,
                opacity: 0.85,
                shape_dynamics: pressure_size(0.4),
                transfer: Transfer {
                    enabled: true,
                    opacity: Dynamic::controlled(Control::PenPressure),
                    ..Default::default()
                },
                noise: true,
                ..base.clone()
            },
        ),
        ours(
            "Marcador",
            BrushSettings {
                size: 18.0,
                hardness: 0.9,
                spacing: 0.05,
                opacity: 0.6,
                shape_dynamics: ShapeDynamics::default(),
                ..base.clone()
            },
        ),
        ours(
            "Aquarela",
            BrushSettings {
                size: 40.0,
                hardness: 0.0,
                flow: 0.25,
                opacity: 0.8,
                wet_edges: true,
                shape_dynamics: pressure_size(0.3),
                ..base.clone()
            },
        ),
        ours(
            "Aerógrafo",
            BrushSettings {
                size: 80.0,
                hardness: 0.0,
                flow: 0.08,
                spacing: 0.1,
                build_up: true,
                ..base
            },
        ),
    ]
}

/// Todos os pincéis: os de desenho e depois os presets embutidos do PhotoCraft.
pub fn all() -> Vec<BrushEntry> {
    let mut v = drawing();
    v.extend(
        presets::builtin()
            .into_iter()
            .map(|p: BrushPreset| BrushEntry {
                name: p.name,
                group: p.group,
                settings: p.brush,
            }),
    );
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_drawing_and_photocraft_brushes() {
        let all = all();
        assert!(all.iter().any(|b| b.name == "Caneta G"));
        assert!(all.iter().any(|b| b.group == "General"));
    }
}
