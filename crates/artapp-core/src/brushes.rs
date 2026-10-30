//! Pincéis prontos. Os de pintura vêm dos presets embutidos do PhotoCraft; os de desenho
//! (lápis, caneta-tinteiro, marcador) são ajustes nossos para trabalho de linha, no estilo Clip
//! Studio, feitos sobre o mesmo motor.

/// Maior tamanho de pincel aceito pelo motor do PhotoCraft.
pub const MAX_SIZE: f32 = photocraft_paint::MAX_BRUSH_SIZE;

use photocraft_paint::{
    BrushPreset, BrushSettings, Control, Dynamic, ShapeDynamics, Smoothing, Transfer, presets,
};

/// Um pincel da paleta: nome mostrado e configurações.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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

/// Grupo dos pincéis criados pelo usuário.
pub const CUSTOM_GROUP: &str = "Meus pincéis";

/// Lê os pincéis salvos pelo usuário (lista vazia se o arquivo não existe ou não abre).
pub fn load_custom(path: &std::path::Path) -> Vec<BrushEntry> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<BrushEntry>>(&b).ok())
        .unwrap_or_default()
}

/// Grava os pincéis do usuário.
pub fn save_custom(path: &std::path::Path, brushes: &[BrushEntry]) -> Result<(), String> {
    let custom: Vec<&BrushEntry> = brushes.iter().filter(|b| b.group == CUSTOM_GROUP).collect();
    let json = serde_json::to_vec_pretty(&custom).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_brushes_round_trip() {
        let dir = std::env::temp_dir().join(format!("artapp-test-{}", std::process::id()));
        let path = dir.join("brushes.json");
        let mut b = drawing();
        b[0].group = CUSTOM_GROUP.into();
        b[0].name = "Meu".into();
        b[0].settings.size = 33.0;
        save_custom(&path, &b).unwrap_or_default();
        let back = load_custom(&path);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "Meu");
        assert_eq!(back[0].settings.size, 33.0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn has_drawing_and_photocraft_brushes() {
        let all = all();
        assert!(all.iter().any(|b| b.name == "Caneta G"));
        assert!(all.iter().any(|b| b.group == "General"));
    }
}

/// Amostra de um pincel para a lista: um traço em S de `width × height`, em RGBA 8 bits
/// (branco sobre transparente, para o tema escuro). O tamanho do pincel é limitado à altura da amostra.
pub fn preview(settings: &BrushSettings, width: u32, height: u32) -> Vec<[u8; 4]> {
    use photocraft_color::PixelFormat;
    use photocraft_geom::Rect;
    use photocraft_raster::Surface;

    let (w, h) = (width.max(8), height.max(8));
    let mut brush = settings.clone();
    brush.size = brush.size.clamp(2.0, h as f32 * 0.45);
    brush.color = [1.0, 1.0, 1.0, 1.0];
    brush.erase = false;
    let n = 48;
    let pts: Vec<photocraft_paint::StrokePoint> = (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let x = 8.0 + t * (w as f64 - 16.0);
            let y = h as f64 * 0.5 + (t * std::f64::consts::TAU).sin() * h as f64 * 0.22;
            // Pressão sobe e desce, para mostrar a dinâmica do pincel.
            let p = (t * std::f64::consts::PI).sin() as f32;
            photocraft_paint::StrokePoint {
                time: t * 400.0,
                ..photocraft_paint::StrokePoint::new(x, y, 0.15 + 0.85 * p)
            }
        })
        .collect();
    let mut s = Surface::new(PixelFormat::RGBA8);
    photocraft_paint::render_stroke(&mut s, &brush.bounded_for_render(), &pts, None, false, 1.0);
    let r = Rect::new(0, 0, w as i32, h as i32);
    let mut out = vec![[0u8; 4]; (w * h) as usize];
    s.read_rgba8_into(r, &mut out);
    out
}

#[cfg(test)]
mod preview_tests {
    use super::*;

    #[test]
    fn preview_has_ink() {
        let b = drawing()
            .into_iter()
            .next()
            .map(|b| b.settings)
            .unwrap_or_default();
        let px = preview(&b, 120, 32);
        assert_eq!(px.len(), 120 * 32);
        assert!(px.iter().any(|p| p[3] > 200));
    }
}
