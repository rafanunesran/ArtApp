//! Simetria do traço: os espelhos do PhotoCraft (vertical, horizontal, quatro lados e diagonal)
//! e a radial do Procreate, que gira o traço em `count` fatias em volta do centro, com ou sem
//! espelhar dentro de cada fatia.

use photocraft_paint::StrokePoint;
use photocraft_paint::symmetry::PresetSymmetry;

/// Fatias da simetria radial.
pub const RADIAL_MAX: u32 = 32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Symmetry {
    Mirror(PresetSymmetry),
    Radial {
        center: [f64; 2],
        count: u32,
        mirror: bool,
        /// Graus no sentido horário.
        rotation: f64,
    },
}

impl Symmetry {
    pub fn radial(center: [f64; 2], count: u32, mirror: bool, rotation: f64) -> Option<Self> {
        (center.iter().all(|c| c.is_finite()) && rotation.is_finite()).then_some(Self::Radial {
            center,
            count: count.clamp(2, RADIAL_MAX),
            mirror,
            rotation,
        })
    }

    /// Quantas cópias além do traço original.
    pub fn mirror_count(&self) -> usize {
        match *self {
            Self::Mirror(s) => s.mirror_count(),
            Self::Radial { count, mirror, .. } => {
                let n = count.clamp(2, RADIAL_MAX) as usize;
                if mirror { 2 * n - 1 } else { n - 1 }
            }
        }
    }

    /// As cópias de `points`, uma lista por espelho.
    pub fn passes(&self, points: &[StrokePoint]) -> Vec<Vec<StrokePoint>> {
        match *self {
            Self::Mirror(s) => s.reflected_passes(points),
            Self::Radial {
                center,
                count,
                mirror,
                rotation,
            } => {
                let n = count.clamp(2, RADIAL_MAX);
                let step = std::f64::consts::TAU / f64::from(n);
                let rotate = |a: f64| {
                    let (s, c) = a.sin_cos();
                    points
                        .iter()
                        .map(|p| {
                            let (dx, dy) = (p.x - center[0], p.y - center[1]);
                            StrokePoint {
                                x: center[0] + dx * c - dy * s,
                                y: center[1] + dx * s + dy * c,
                                ..*p
                            }
                        })
                        .collect::<Vec<_>>()
                };
                let mut out: Vec<_> = (1..n).map(|k| rotate(step * f64::from(k))).collect();
                if mirror {
                    // Reflexões pelos n eixos que passam nas divisas e no meio das fatias.
                    let base = rotation.to_radians() - std::f64::consts::FRAC_PI_2;
                    for k in 0..n {
                        let axis = base + step * f64::from(k) / 2.0;
                        let (s2, c2) = (2.0 * axis).sin_cos();
                        out.push(
                            points
                                .iter()
                                .map(|p| {
                                    let (dx, dy) = (p.x - center[0], p.y - center[1]);
                                    StrokePoint {
                                        x: center[0] + dx * c2 + dy * s2,
                                        y: center[1] + dx * s2 - dy * c2,
                                        ..*p
                                    }
                                })
                                .collect(),
                        );
                    }
                }
                out
            }
        }
    }

    /// As guias para desenhar na tela, recortadas ao documento `size`.
    pub fn segments(&self, size: [f64; 2]) -> Vec<[[f64; 2]; 2]> {
        match *self {
            Self::Mirror(s) => s.segments(size),
            Self::Radial {
                center,
                count,
                rotation,
                ..
            } => {
                let n = count.clamp(2, RADIAL_MAX);
                let step = std::f64::consts::TAU / f64::from(n);
                (0..n)
                    .filter_map(|k| {
                        let a = rotation.to_radians() - std::f64::consts::FRAC_PI_2
                            + step * f64::from(k);
                        let d = [a.cos(), a.sin()];
                        // Até onde o raio sai do documento.
                        let mut t = f64::INFINITY;
                        for i in 0..2 {
                            if d[i] > 1e-12 {
                                t = t.min((size[i] - center[i]) / d[i]);
                            } else if d[i] < -1e-12 {
                                t = t.min(-center[i] / d[i]);
                            }
                        }
                        (t.is_finite() && t > 0.0)
                            .then(|| [center, [center[0] + d[0] * t, center[1] + d[1] * t]])
                    })
                    .collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn p(x: f64, y: f64) -> StrokePoint {
        StrokePoint::new(x, y, 1.0)
    }

    #[test]
    fn radial_rotates_around_the_center() {
        let s = Symmetry::radial([50.0, 50.0], 4, false, 0.0).unwrap();
        assert_eq!(s.mirror_count(), 3);
        let out = s.passes(&[p(60.0, 50.0)]);
        let got: Vec<_> = out
            .iter()
            .map(|v| (v[0].x.round(), v[0].y.round()))
            .collect();
        assert_eq!(got, [(50.0, 60.0), (40.0, 50.0), (50.0, 40.0)]);
        assert_eq!(s.segments([100.0, 100.0]).len(), 4);
    }

    #[test]
    fn mirrored_radial_has_twice_the_copies() {
        let s = Symmetry::radial([0.0, 0.0], 3, true, 0.0).unwrap();
        assert_eq!(s.mirror_count(), 5);
        let out = s.passes(&[p(10.0, 3.0)]);
        assert_eq!(out.len(), 5);
        // Todas as cópias ficam à mesma distância do centro.
        for v in &out {
            assert!((v[0].x.hypot(v[0].y) - 10.0f64.hypot(3.0)).abs() < 1e-9);
        }
    }
}
