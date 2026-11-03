//! Testes das mudanças do ArtApp no motor de pincel (third_party/photocraft-paint): bordas
//! queimadas, vidrado e pontas múltiplas.

use photocraft_color::PixelFormat;
use photocraft_geom::Rect;
use photocraft_paint::{BrushSettings, Glaze, GrayTile, StrokePoint, TipShape, render_stroke};
use photocraft_raster::Surface;

/// Um traço que vai e volta na mesma linha (cruza a si mesmo), lido em RGBA 8 bits.
fn back_and_forth(b: &BrushSettings, base: Option<[f32; 4]>) -> Vec<[u8; 4]> {
    let mut s = match base {
        Some(c) => Surface::with_default(PixelFormat::RGBA8, &c),
        None => Surface::new(PixelFormat::RGBA8),
    };
    let pts: Vec<StrokePoint> = (0..=40)
        .chain((0..=40).rev())
        .map(|i| StrokePoint::new(10.0 + i as f64 * 2.0, 20.0, 1.0))
        .collect();
    render_stroke(&mut s, &b.bounded_for_render(), &pts, None, false, 1.0);
    let mut out = vec![[0u8; 4]; 100 * 40];
    s.read_rgba8_into(Rect::new(0, 0, 100, 40), &mut out);
    out
}

fn soft() -> BrushSettings {
    BrushSettings {
        size: 12.0,
        hardness: 0.6,
        flow: 0.3,
        opacity: 1.0,
        spacing: 0.15,
        pressure_size: false,
        color: [0.1, 0.3, 0.8, 1.0],
        ..Default::default()
    }
}

#[test]
fn uniform_glaze_does_not_darken_where_the_stroke_crosses_itself() {
    let normal = back_and_forth(&soft(), None);
    let uniform = back_and_forth(
        &BrushSettings {
            glaze: Glaze::Uniform,
            ..soft()
        },
        None,
    );
    let mid = 20 * 100 + 50;
    assert!(
        uniform[mid][3] < normal[mid][3],
        "uniforme {} >= normal {}",
        uniform[mid][3],
        normal[mid][3]
    );
    let intense = back_and_forth(
        &BrushSettings {
            glaze: Glaze::Intense,
            transfer: photocraft_paint::Transfer::default(),
            ..soft()
        },
        None,
    );
    assert!(intense[mid][3] >= normal[mid][3]);
}

#[test]
fn burnt_edges_darken_the_rim_over_paint() {
    let paper = Some([0.95, 0.85, 0.3, 1.0]);
    let b = BrushSettings {
        flow: 1.0,
        hardness: 0.9,
        ..soft()
    };
    let plain = back_and_forth(&b, paper);
    let burnt = back_and_forth(
        &BrushSettings {
            burnt_edges: 1.0,
            ..b.clone()
        },
        paper,
    );
    let lum = |p: [u8; 4]| p[0] as u32 + p[1] as u32 + p[2] as u32;
    // Perto da borda de cima do traço (centro em y = 20, raio 6).
    let rim = 15 * 100 + 50;
    let center = 20 * 100 + 50;
    assert!(
        lum(burnt[rim]) < lum(plain[rim]),
        "borda {:?} não escureceu ({:?})",
        burnt[rim],
        plain[rim]
    );
    assert!(
        lum(burnt[center]).abs_diff(lum(plain[center])) < 12,
        "o meio fica igual"
    );
}

#[test]
fn extra_tips_take_turns() {
    // Ponta 1: só a metade de cima; ponta 2: só a de baixo. Com as duas, os dois lados pintam.
    let half = |top: bool| GrayTile::from_fn(16, 16, |_, y| if (y < 8) == top { 1.0 } else { 0.0 });
    let b = BrushSettings {
        size: 12.0,
        spacing: 1.5,
        pressure_size: false,
        tip: TipShape::Sampled(half(true)),
        ..Default::default()
    };
    let one = back_and_forth(&b, None);
    let two = back_and_forth(
        &BrushSettings {
            extra_tips: vec![half(false)],
            ..b.clone()
        },
        None,
    );
    // Tinta abaixo do centro do traço (y > 21), em qualquer x.
    let below = |px: &[[u8; 4]]| px[22 * 100..].iter().map(|p| p[3] as u32).sum::<u32>();
    let ink = |px: &[[u8; 4]]| px.iter().filter(|p| p[3] > 0).count();
    assert!(ink(&one) > 0 && ink(&two) > 0);
    assert!(
        below(&two) > below(&one),
        "a segunda ponta pinta embaixo ({} <= {})",
        below(&two),
        below(&one)
    );
}
