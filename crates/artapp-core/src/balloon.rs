//! Balões de quadrinho: fala (oval), retangular, pensamento (nuvem) e grito (espinhos), com
//! rabicho apontando para quem fala. Saem como caminhos vetoriais editáveis.

use vectorcraft_geom::Point;
use vectorcraft_geom::path::{Anchor, SubPath};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BalloonShape {
    #[default]
    Oval,
    Rounded,
    Cloud,
    Shout,
}

impl BalloonShape {
    pub const ALL: [BalloonShape; 4] = [Self::Oval, Self::Rounded, Self::Cloud, Self::Shout];

    pub fn label(self) -> &'static str {
        match self {
            Self::Oval => "Fala",
            Self::Rounded => "Retangular",
            Self::Cloud => "Pensamento",
            Self::Shout => "Grito",
        }
    }
}

/// Um ponto do contorno: posição, ângulo em volta do centro e se a curva passa suave por ele.
struct Pt {
    p: Point,
    angle: f64,
    smooth: bool,
}

/// O contorno do balão em `[x0, y0, x1, y1]`, do ângulo 0 dando a volta.
fn outline(shape: BalloonShape, r: [f64; 4]) -> Vec<Pt> {
    let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    let (rx, ry) = ((r[2] - r[0]).abs() / 2.0, (r[3] - r[1]).abs() / 2.0);
    let at = |a: f64, k: f64, smooth: bool| Pt {
        p: Point::new(cx + rx * k * a.cos(), cy + ry * k * a.sin()),
        angle: a,
        smooth,
    };
    let tau = std::f64::consts::TAU;
    match shape {
        BalloonShape::Oval => (0..48)
            .map(|i| at(tau * i as f64 / 48.0, 1.0, true))
            .collect(),
        BalloonShape::Cloud => {
            // Calombos: cantos para dentro entre cada um, o meio puxado para fora.
            let k = 11;
            (0..k * 2)
                .map(|i| {
                    let a = tau * i as f64 / (k * 2) as f64;
                    if i % 2 == 0 {
                        at(a, 0.88, false)
                    } else {
                        at(a, 1.08, true)
                    }
                })
                .collect()
        }
        BalloonShape::Shout => {
            let k = 14;
            (0..k * 2)
                .map(|i| {
                    let a = tau * i as f64 / (k * 2) as f64;
                    // Espinhos um pouco irregulares, sempre iguais para o mesmo balão.
                    let jitter = 0.06 * ((i as f64 * 2.3).sin());
                    at(a, if i % 2 == 0 { 1.12 + jitter } else { 0.8 }, false)
                })
                .collect()
        }
        BalloonShape::Rounded => {
            // Retângulo de cantos redondos, amostrado em volta.
            let rad = rx.min(ry) * 0.45;
            let n = 96;
            (0..n)
                .map(|i| {
                    let a = tau * i as f64 / n as f64;
                    // Raio do "superelipse" que vira retângulo arredondado.
                    let (c, s) = (a.cos(), a.sin());
                    let e = 2.0 + 6.0 * (1.0 - rad / rx.min(ry).max(1e-9));
                    let k = (c.abs().powf(e) + s.abs().powf(e)).powf(-1.0 / e);
                    Pt {
                        p: Point::new(cx + rx * k * c, cy + ry * k * s),
                        angle: a,
                        smooth: true,
                    }
                })
                .collect()
        }
    }
}

/// Distância entre ângulos (0..π).
fn angle_gap(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(std::f64::consts::TAU);
    d.min(std::f64::consts::TAU - d)
}

/// Fecha os pontos numa curva: os suaves ganham alças (Catmull-Rom), os outros ficam cantos.
fn to_subpath(pts: &[Pt]) -> SubPath {
    let n = pts.len();
    let anchors = (0..n)
        .map(|i| {
            let cur = &pts[i];
            if !cur.smooth {
                return Anchor::corner(cur.p);
            }
            let prev = pts[(i + n - 1) % n].p;
            let next = pts[(i + 1) % n].p;
            let t = Point::new((next.x - prev.x) / 6.0, (next.y - prev.y) / 6.0);
            Anchor::smooth(cur.p, Point::new(cur.p.x + t.x, cur.p.y + t.y))
        })
        .collect();
    SubPath::new(anchors, true)
}

/// Onde o rabicho aponta quando não se diz: embaixo, um pouco à esquerda.
pub fn default_tail(r: [f64; 4]) -> [f64; 2] {
    let (w, h) = ((r[2] - r[0]).abs(), (r[3] - r[1]).abs());
    [r[0].min(r[2]) + w * 0.3, r[1].max(r[3]) + h * 0.5]
}

/// Os caminhos do balão no retângulo `r`, com o rabicho apontando para `tail` (se houver). O
/// primeiro é o balão; no pensamento, o rabicho são bolinhas, que vêm depois.
pub fn balloon(shape: BalloonShape, r: [f64; 4], tail: Option<[f64; 2]>) -> Vec<SubPath> {
    let r = [
        r[0].min(r[2]),
        r[1].min(r[3]),
        r[0].max(r[2]),
        r[1].max(r[3]),
    ];
    if r[2] - r[0] < 2.0 || r[3] - r[1] < 2.0 {
        return vec![];
    }
    let mut pts = outline(shape, r);
    let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    let (rx, ry) = ((r[2] - r[0]) / 2.0, (r[3] - r[1]) / 2.0);
    let mut extra = vec![];
    if let Some(tip) = tail {
        // Ângulo do rabicho medido no balão "redondo" (normalizado pelos raios).
        let phi = ((tip[1] - cy) / ry).atan2((tip[0] - cx) / rx);
        let inside = ((tip[0] - cx) / rx).powi(2) + ((tip[1] - cy) / ry).powi(2) <= 1.0;
        if shape == BalloonShape::Cloud {
            if !inside {
                // Três bolinhas diminuindo do balão até a ponta.
                let edge = Point::new(cx + rx * phi.cos(), cy + ry * phi.sin());
                let base = rx.min(ry) * 0.16;
                for (k, f) in [0.3f64, 0.6, 0.9].iter().enumerate() {
                    let c = Point::new(
                        edge.x + (tip[0] - edge.x) * f,
                        edge.y + (tip[1] - edge.y) * f,
                    );
                    let rad = base * (1.0 - 0.28 * k as f64);
                    let circle: Vec<Pt> = (0..12)
                        .map(|i| {
                            let a = std::f64::consts::TAU * i as f64 / 12.0;
                            Pt {
                                p: Point::new(c.x + rad * a.cos(), c.y + rad * 0.8 * a.sin()),
                                angle: a,
                                smooth: true,
                            }
                        })
                        .collect();
                    extra.push(to_subpath(&circle));
                }
            }
        } else if !inside {
            let half = 0.2;
            let first = pts.iter().position(|p| angle_gap(p.angle, phi) < half);
            pts.retain(|p| angle_gap(p.angle, phi) >= half);
            let at = |a: f64| -> Point {
                // Base do rabicho no contorno (no raio do balão).
                let k = if shape == BalloonShape::Shout {
                    0.85
                } else {
                    1.0
                };
                let c = a.cos();
                let s = a.sin();
                if shape == BalloonShape::Rounded {
                    let e = 8.0f64;
                    let kk = (c.abs().powf(e) + s.abs().powf(e)).powf(-1.0 / e).min(1.3);
                    return Point::new(cx + rx * kk * c * 0.97, cy + ry * kk * s * 0.97);
                }
                Point::new(cx + rx * k * c, cy + ry * k * s)
            };
            let insert = first.unwrap_or(pts.len()).min(pts.len());
            let tail_pts = [
                Pt {
                    p: at(phi - half),
                    angle: phi - half,
                    smooth: false,
                },
                Pt {
                    p: Point::new(tip[0], tip[1]),
                    angle: phi,
                    smooth: false,
                },
                Pt {
                    p: at(phi + half),
                    angle: phi + half,
                    smooth: false,
                },
            ];
            // Os pontos seguem a ordem dos ângulos: o rabicho entra onde os removidos estavam.
            let mut out: Vec<Pt> = Vec::with_capacity(pts.len() + 3);
            let mut tail_pts = Some(tail_pts);
            for (i, p) in pts.into_iter().enumerate() {
                if i == insert
                    && let Some(t) = tail_pts.take()
                {
                    out.extend(t);
                }
                out.push(p);
            }
            if let Some(t) = tail_pts {
                out.extend(t);
            }
            pts = out;
        }
    }
    let mut v = vec![to_subpath(&pts)];
    v.extend(extra);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bbox(s: &SubPath) -> [f64; 4] {
        s.anchors
            .iter()
            .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, a| {
                [
                    b[0].min(a.p.x),
                    b[1].min(a.p.y),
                    b[2].max(a.p.x),
                    b[3].max(a.p.y),
                ]
            })
    }

    #[test]
    fn tail_reaches_the_tip() {
        let r = [100.0, 100.0, 300.0, 200.0];
        for shape in [
            BalloonShape::Oval,
            BalloonShape::Rounded,
            BalloonShape::Shout,
        ] {
            let tip = default_tail(r);
            let v = balloon(shape, r, Some(tip));
            assert_eq!(v.len(), 1, "{shape:?}");
            let b = bbox(&v[0]);
            assert!((b[3] - tip[1]).abs() < 1e-6, "{shape:?} {b:?}");
            assert!(v[0].closed);
            // A ponta é um canto exatamente no alvo.
            assert!(
                v[0].anchors
                    .iter()
                    .any(|a| (a.p.x - tip[0]).abs() < 1e-6 && (a.p.y - tip[1]).abs() < 1e-6)
            );
        }
        let cloud = balloon(BalloonShape::Cloud, r, Some(default_tail(r)));
        assert_eq!(cloud.len(), 4, "nuvem e três bolinhas");
        // Sem rabicho, só o balão, dentro do retângulo (com folga dos calombos).
        let plain = balloon(BalloonShape::Oval, r, None);
        let b = bbox(&plain[0]);
        assert!(
            b[0] >= 99.0 && b[2] <= 301.0 && b[1] >= 99.0 && b[3] <= 201.0,
            "{b:?}"
        );
        assert!(balloon(BalloonShape::Oval, [0.0, 0.0, 1.0, 1.0], None).is_empty());
    }
}
