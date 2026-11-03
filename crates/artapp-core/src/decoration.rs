//! Pincéis de decoração (como os de "Decoração" do Clip Studio): carimbam uma forma ao longo do
//! traço — estrelas, corações, flores, folhas, costura, corrente, renda, corda, flocos. As
//! pontas são desenhadas aqui mesmo, por fórmula, e as que têm sentido seguem a direção do traço.

use photocraft_paint::{
    BrushSettings, Control, Dynamic, GrayTile, Scattering, ShapeDynamics, TipShape,
};

use crate::brushes::BrushEntry;

const GROUP: &str = "Decoração";

/// Ponta `w × h` a partir de um teste "dentro" em coordenadas `u, v` de −1 a 1 (o lado maior;
/// `v` cresce para baixo), com 4×4 amostras por pixel para a borda sair lisa.
fn tile(w: u32, h: u32, inside: impl Fn(f32, f32) -> bool) -> GrayTile {
    let side = w.max(h) as f32;
    GrayTile::from_fn(w, h, |x, y| {
        let mut n = 0;
        for sy in 0..4 {
            for sx in 0..4 {
                let px = x as f32 + (sx as f32 + 0.5) / 4.0;
                let py = y as f32 + (sy as f32 + 0.5) / 4.0;
                let u = (px - w as f32 / 2.0) / (side / 2.0);
                let v = (py - h as f32 / 2.0) / (side / 2.0);
                if inside(u, v) {
                    n += 1;
                }
            }
        }
        n as f32 / 16.0
    })
}

/// Dentro do polígono (par-ímpar).
fn in_poly(pts: &[(f32, f32)], u: f32, v: f32) -> bool {
    let mut odd = false;
    let n = pts.len();
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + n - 1) % n]);
        if (a.1 > v) != (b.1 > v) && u < (b.0 - a.0) * (v - a.1) / (b.1 - a.1) + a.0 {
            odd = !odd;
        }
    }
    odd
}

/// Distância do ponto ao segmento de `a` a `b`.
fn seg_dist(u: f32, v: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((u - a.0) * dx + (v - a.1) * dy) / (dx * dx + dy * dy).max(1e-6)).clamp(0.0, 1.0);
    (u - a.0 - t * dx).hypot(v - a.1 - t * dy)
}

fn star() -> GrayTile {
    use std::f32::consts::{FRAC_PI_2, PI};
    let pts: Vec<(f32, f32)> = (0..10)
        .map(|i| {
            let a = -FRAC_PI_2 + i as f32 * PI / 5.0;
            let r = if i % 2 == 0 { 0.95 } else { 0.42 };
            (r * a.cos(), r * a.sin())
        })
        .collect();
    tile(96, 96, |u, v| in_poly(&pts, u, v))
}

fn heart() -> GrayTile {
    tile(96, 96, |u, v| {
        let (x, y) = (u * 1.25, -v * 1.25 + 0.15);
        let a = x * x + y * y - 1.0;
        a * a * a - x * x * y * y * y <= 0.0
    })
}

fn flower() -> GrayTile {
    tile(96, 96, |u, v| {
        let r = u.hypot(v);
        let a = v.atan2(u);
        let petal = 0.5 + 0.45 * (2.5 * a).cos().abs();
        r <= petal && !(0.17..=0.25).contains(&r)
    })
}

/// Folha deitada (o traço anda para a direita), com a nervura em branco.
fn leaf() -> GrayTile {
    tile(96, 48, |u, v| {
        let half = 0.42 * (1.0 - u * u).max(0.0);
        v.abs() <= half && !(v.abs() < 0.035 && u > -0.75 && u < 0.85)
    })
}

/// Um ponto de costura: traço curto deitado, de pontas redondas.
fn stitch() -> GrayTile {
    tile(96, 32, |u, v| {
        seg_dist(u, v, (-0.6, 0.0), (0.6, 0.0)) <= 0.16
    })
}

/// Um elo de corrente: anel oval deitado.
fn chain_link() -> GrayTile {
    tile(96, 64, |u, v| {
        let d = (u / 0.92).powi(2) + (v / 0.6).powi(2);
        (0.5..=1.0).contains(&d)
    })
}

/// Renda: um arco de meia-lua para baixo, com um furinho no meio.
fn lace() -> GrayTile {
    tile(96, 96, |u, v| {
        let r = u.hypot(v + 0.35);
        let arc = v + 0.35 >= 0.0 && (0.62..=0.92).contains(&r);
        let dot = u.hypot(v - 0.15) <= 0.12;
        arc || dot
    })
}

/// Corda: um fio oval inclinado; vários seguidos parecem trançados.
fn rope() -> GrayTile {
    let (s, c) = 0.65f32.sin_cos();
    tile(96, 96, |u, v| {
        let (x, y) = (u * c + v * s, -u * s + v * c);
        (x / 0.95).powi(2) + (y / 0.38).powi(2) <= 1.0
    })
}

fn snowflake() -> GrayTile {
    use std::f32::consts::PI;
    tile(96, 96, |u, v| {
        (0..6).any(|k| {
            let a = k as f32 * PI / 3.0;
            let tip = (0.92 * a.cos(), 0.92 * a.sin());
            let mid = (0.55 * a.cos(), 0.55 * a.sin());
            let arm = seg_dist(u, v, (0.0, 0.0), tip) <= 0.07;
            let twig = [a + 0.6, a - 0.6].iter().any(|b| {
                let end = (mid.0 + 0.3 * b.cos(), mid.1 + 0.3 * b.sin());
                seg_dist(u, v, mid, end) <= 0.055
            });
            arm || twig
        })
    })
}

/// Ângulo seguindo a direção do traço (e um tanto de variação).
fn follow(jitter: f32) -> ShapeDynamics {
    ShapeDynamics {
        enabled: true,
        angle: Dynamic {
            jitter,
            ..Dynamic::controlled(Control::Direction)
        },
        ..Default::default()
    }
}

/// Tamanho e ângulo variando ao acaso (carimbos soltos).
pub(crate) fn loose(size_jitter: f32, angle_jitter: f32) -> ShapeDynamics {
    ShapeDynamics {
        enabled: true,
        size: Dynamic {
            jitter: size_jitter,
            minimum: 0.4,
            ..Dynamic::default()
        },
        angle: Dynamic {
            jitter: angle_jitter,
            ..Dynamic::default()
        },
        ..Default::default()
    }
}

pub(crate) fn scatter(amount: f32) -> Scattering {
    Scattering {
        enabled: true,
        scatter: Dynamic {
            jitter: amount,
            ..Dynamic::default()
        },
        both_axes: true,
        ..Default::default()
    }
}

fn entry(name: &str, settings: BrushSettings) -> BrushEntry {
    BrushEntry {
        name: name.into(),
        group: GROUP.into(),
        settings,
        mixes: false,
        folder: String::new(),
    }
}

/// Os pincéis de decoração do ArtApp.
pub fn brushes() -> Vec<BrushEntry> {
    let stamp = |t: GrayTile, size: f32, spacing: f32| BrushSettings {
        tip: TipShape::Sampled(t),
        size,
        spacing,
        hardness: 1.0,
        ..BrushSettings::default()
    };
    vec![
        entry(
            "Estrelas",
            BrushSettings {
                shape_dynamics: loose(0.5, 0.15),
                scattering: scatter(0.8),
                ..stamp(star(), 34.0, 1.6)
            },
        ),
        entry(
            "Corações",
            BrushSettings {
                shape_dynamics: loose(0.35, 0.06),
                ..stamp(heart(), 30.0, 1.5)
            },
        ),
        entry(
            "Flores",
            BrushSettings {
                shape_dynamics: loose(0.4, 0.5),
                scattering: scatter(0.6),
                ..stamp(flower(), 34.0, 1.4)
            },
        ),
        entry(
            "Ramo de folhas",
            BrushSettings {
                shape_dynamics: follow(0.08),
                ..stamp(leaf(), 32.0, 0.9)
            },
        ),
        entry(
            "Costura",
            BrushSettings {
                shape_dynamics: follow(0.0),
                ..stamp(stitch(), 18.0, 1.9)
            },
        ),
        entry(
            "Corrente",
            BrushSettings {
                shape_dynamics: follow(0.0),
                ..stamp(chain_link(), 26.0, 0.82)
            },
        ),
        entry(
            "Renda",
            BrushSettings {
                shape_dynamics: follow(0.0),
                ..stamp(lace(), 28.0, 0.95)
            },
        ),
        entry(
            "Corda",
            BrushSettings {
                shape_dynamics: follow(0.0),
                ..stamp(rope(), 24.0, 0.55)
            },
        ),
        entry(
            "Flocos de neve",
            BrushSettings {
                shape_dynamics: loose(0.6, 1.0),
                scattering: scatter(1.6),
                ..stamp(snowflake(), 30.0, 1.8)
            },
        ),
        entry(
            "Bolinhas",
            BrushSettings {
                hardness: 1.0,
                ..stamp(tile(64, 64, |u, v| u.hypot(v) <= 0.9), 16.0, 1.9)
            },
        ),
    ]
}

/// Uma frase sobre cada pincel de decoração.
pub fn describe(name: &str) -> Option<&'static str> {
    Some(match name {
        "Estrelas" => "Estrelas espalhadas, de tamanhos variados.",
        "Corações" => "Uma fila de corações ao longo do traço.",
        "Flores" => "Florzinhas soltas, cada uma girada de um jeito.",
        "Ramo de folhas" => "Folhas que acompanham a curva do traço, como um ramo.",
        "Costura" => "Pesponto tracejado de roupa e bichos de pano.",
        "Corrente" => "Elos de corrente seguindo o traço.",
        "Renda" => "Barrado de renda em meias-luas.",
        "Corda" => "Fios inclinados que formam uma corda trançada.",
        "Flocos de neve" => "Flocos espalhados e girados para neve caindo.",
        "Bolinhas" => "Linha de bolinhas espaçadas.",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tip_has_ink_and_space() {
        for b in brushes() {
            let TipShape::Sampled(t) = &b.settings.tip else {
                unreachable!("{} sem ponta desenhada", b.name);
            };
            let cells: Vec<f32> = (0..t.height)
                .flat_map(|y| (0..t.width).map(move |x| (x, y)))
                .map(|(x, y)| t.get(x, y))
                .collect();
            let ink = cells.iter().filter(|c| **c > 0.5).count();
            assert!(ink > cells.len() / 20, "{} quase sem tinta", b.name);
            assert!(ink < cells.len(), "{} sem fundo", b.name);
            assert!(describe(&b.name).is_some(), "{} sem descrição", b.name);
            let px = crate::brushes::preview(&b.settings, 160, 40);
            assert!(
                px.iter().any(|p| p[3] > 128),
                "{} sem marca na prévia",
                b.name
            );
        }
    }
}
