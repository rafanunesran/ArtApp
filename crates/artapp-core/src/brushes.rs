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

/// Maior lado de uma ponta feita de imagem (as maiores são reduzidas).
const MAX_TIP_SIDE: u32 = 512;

/// Importa pincéis de um arquivo: `.abr` (Photoshop) ou uma imagem, que vira a ponta de um
/// pincel (escuro ou opaco = tinta). Os pincéis entram em "Meus pincéis".
pub fn import(file_name: &str, bytes: &[u8]) -> Result<Vec<BrushEntry>, String> {
    let lower = file_name.to_ascii_lowercase();
    let stem = std::path::Path::new(file_name)
        .file_stem()
        .map_or_else(|| "Pincel".into(), |s| s.to_string_lossy().into_owned());
    if lower.ends_with(".abr") {
        let imp = crate::abr::read_abr(bytes, &stem)?;
        if imp.presets.is_empty() {
            return Err("o arquivo não tem pincéis".into());
        }
        return Ok(imp
            .presets
            .into_iter()
            .map(|p| BrushEntry {
                name: p.name,
                group: CUSTOM_GROUP.into(),
                settings: p.brush,
            })
            .collect());
    }
    let (w, h, px) = crate::edit::decode_image(bytes)?;
    let tip = tip_from_rgba(w, h, &px).ok_or("a imagem está vazia")?;
    let size = tip.width.max(tip.height).min(120) as f32;
    Ok(vec![BrushEntry {
        name: stem,
        group: CUSTOM_GROUP.into(),
        settings: BrushSettings {
            tip: photocraft_paint::TipShape::Sampled(tip),
            size,
            spacing: 0.25,
            ..BrushSettings::default()
        },
    }])
}

/// A ponta em tons de cinza de uma imagem RGBA: tinta = opaco e escuro. Reduzida até
/// [`MAX_TIP_SIDE`]. `None` se não sobra tinta nenhuma.
fn tip_from_rgba(w: u32, h: u32, px: &[u8]) -> Option<photocraft_paint::GrayTile> {
    if w == 0 || h == 0 || px.len() < (w * h * 4) as usize {
        return None;
    }
    let ink = |x: u32, y: u32| -> f32 {
        let i = ((y * w + x) * 4) as usize;
        let p = |k: usize| f32::from(px.get(i + k).copied().unwrap_or(0)) / 255.0;
        let luma = 0.299 * p(0) + 0.587 * p(1) + 0.114 * p(2);
        p(3) * (1.0 - luma)
    };
    let step = w.max(h).div_ceil(MAX_TIP_SIDE).max(1);
    let (tw, th) = (w.div_ceil(step), h.div_ceil(step));
    let tile = photocraft_paint::GrayTile::from_fn(tw, th, |x, y| {
        let mut acc = 0.0;
        let mut n = 0.0;
        for dy in 0..step {
            for dx in 0..step {
                let (sx, sy) = (x * step + dx, y * step + dy);
                if sx < w && sy < h {
                    acc += ink(sx, sy);
                    n += 1.0;
                }
            }
        }
        if n > 0.0 { acc / n } else { 0.0 }
    });
    let any = (0..th).any(|y| (0..tw).any(|x| tile.get(x, y) > 0.02));
    any.then_some(tile)
}

#[cfg(test)]
mod import_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn image_becomes_a_sampled_tip() {
        // Círculo preto em fundo branco, 64×64.
        let mut img = image::RgbaImage::from_pixel(64, 64, image::Rgba([255, 255, 255, 255]));
        for y in 0..64 {
            for x in 0..64 {
                let (dx, dy) = (x as f32 - 32.0, y as f32 - 32.0);
                if dx * dx + dy * dy < 400.0 {
                    img.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
                }
            }
        }
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let v = import("bolinha.png", &png).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name, "bolinha");
        assert_eq!(v[0].group, CUSTOM_GROUP);
        let t = match &v[0].settings.tip {
            photocraft_paint::TipShape::Sampled(t) => t.clone(),
            _ => photocraft_paint::GrayTile::from_fn(1, 1, |_, _| 0.0),
        };
        assert_eq!((t.width, t.height), (64, 64));
        assert!(t.get(32, 32) > 0.9);
        assert!(t.get(2, 2) < 0.05);
        assert!(
            import("vazio.png", &{
                let blank = image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 255, 255, 255]));
                let mut b = Vec::new();
                blank
                    .write_to(&mut std::io::Cursor::new(&mut b), image::ImageFormat::Png)
                    .unwrap();
                b
            })
            .is_err()
        );
        assert!(import("x.abr", b"lixo").is_err());
    }
}
