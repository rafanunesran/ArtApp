//! Forma rápida (a mecânica "QuickShape" do Procreate): ao segurar a caneta parada no fim de um
//! traço, o rabisco vira uma reta, um arco, uma elipse ou um polígono limpo, desenhado com o
//! mesmo pincel.

/// A forma reconhecida, em pixels do documento.
#[derive(Clone, Debug, PartialEq)]
pub enum QuickShape {
    Line([f64; 2], [f64; 2]),
    /// Arco de círculo de `from` a `to` passando por `via`.
    Arc {
        from: [f64; 2],
        via: [f64; 2],
        to: [f64; 2],
    },
    /// Elipse com centro, raios e giro (radianos).
    Ellipse {
        center: [f64; 2],
        radii: [f64; 2],
        angle: f64,
    },
    /// Polígono de cantos vivos; `closed` fecha o último lado.
    Polygon {
        points: Vec<[f64; 2]>,
        closed: bool,
    },
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Distância do ponto `p` ao segmento `a`–`b`.
fn seg_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    if l2 <= 1e-12 {
        return dist(p, a);
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0);
    dist(p, [a[0] + dx * t, a[1] + dy * t])
}

/// Simplificação de Ramer–Douglas–Peucker.
fn simplify(points: &[[f64; 2]], eps: f64) -> Vec<[f64; 2]> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let (a, b) = (points[0], points[points.len() - 1]);
    let (mut far, mut idx) = (0.0, 0);
    for (i, p) in points.iter().enumerate().take(points.len() - 1).skip(1) {
        let d = seg_dist(*p, a, b);
        if d > far {
            far = d;
            idx = i;
        }
    }
    if far <= eps {
        return vec![a, b];
    }
    let mut left = simplify(&points[..=idx], eps);
    let right = simplify(&points[idx..], eps);
    left.pop();
    left.extend(right);
    left
}

/// Ângulo interno no vértice `b` (graus, 180 = reto).
fn corner_angle(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    let (u, v) = ([a[0] - b[0], a[1] - b[1]], [c[0] - b[0], c[1] - b[1]]);
    let den = u[0].hypot(u[1]) * v[0].hypot(v[1]);
    if den <= 1e-12 {
        return 180.0;
    }
    ((u[0] * v[0] + u[1] * v[1]) / den)
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees()
}

/// Reconhece a forma de um traço (pontos em ordem). `None` quando é curto demais.
pub fn detect(points: &[[f64; 2]]) -> Option<QuickShape> {
    if points.len() < 2 {
        return None;
    }
    let len: f64 = points.windows(2).map(|w| dist(w[0], w[1])).sum();
    if len < 4.0 {
        return None;
    }
    let (first, last) = (points[0], points[points.len() - 1]);
    let chord = dist(first, last);
    let off = points
        .iter()
        .map(|p| seg_dist(*p, first, last))
        .fold(0.0, f64::max);
    if chord > len * 0.8 && off < (len * 0.06).max(3.0) {
        return Some(QuickShape::Line(first, last));
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in points {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    let diag = (x1 - x0).hypot(y1 - y0).max(1.0);
    // Tolerância pequena: curvas viram muitos cantos e não passam por polígono.
    let eps = (diag * 0.03).max(2.5);
    let closed = chord < len * 0.15;
    if closed {
        let mut ring = points.to_vec();
        ring.push(first);
        let mut v = simplify(&ring, eps);
        v.pop();
        // Um polígono de verdade tem poucos cantos, todos bem fechados.
        let n = v.len();
        let sharp = (0..n).all(|i| corner_angle(v[(i + n - 1) % n], v[i], v[(i + 1) % n]) < 140.0);
        if (3..=6).contains(&n) && sharp {
            return Some(QuickShape::Polygon {
                points: v,
                closed: true,
            });
        }
        return Some(fit_ellipse(points));
    }
    let v = simplify(points, eps);
    let inner_sharp = v.windows(3).all(|w| corner_angle(w[0], w[1], w[2]) < 150.0);
    if (3..=6).contains(&v.len()) && inner_sharp {
        return Some(QuickShape::Polygon {
            points: v,
            closed: false,
        });
    }
    // Curva aberta: arco pelo começo, pelo ponto mais longe da corda e pelo fim.
    let via = points
        .iter()
        .copied()
        .max_by(|a, b| seg_dist(*a, first, last).total_cmp(&seg_dist(*b, first, last)))
        .unwrap_or(first);
    Some(QuickShape::Arc {
        from: first,
        via,
        to: last,
    })
}

/// Elipse pelos momentos dos pontos (centro, eixos principais e giro).
fn fit_ellipse(points: &[[f64; 2]]) -> QuickShape {
    let n = points.len() as f64;
    let cx = points.iter().map(|p| p[0]).sum::<f64>() / n;
    let cy = points.iter().map(|p| p[1]).sum::<f64>() / n;
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for p in points {
        let (dx, dy) = (p[0] - cx, p[1] - cy);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    let (sxx, syy, sxy) = (sxx / n, syy / n, sxy / n);
    let tr = sxx + syy;
    let det = sxx * syy - sxy * sxy;
    let disc = (tr * tr / 4.0 - det).max(0.0).sqrt();
    let (l1, l2) = (tr / 2.0 + disc, (tr / 2.0 - disc).max(0.0));
    let angle = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    // Num contorno de elipse, a variância ao longo de um eixo é o raio²/2.
    let mut rx = (2.0 * l1).sqrt();
    let mut ry = (2.0 * l2).sqrt();
    // Quase redonda vira círculo.
    if (rx - ry).abs() < rx * 0.12 {
        let r = (rx + ry) / 2.0;
        rx = r;
        ry = r;
    }
    QuickShape::Ellipse {
        center: [cx, cy],
        radii: [rx, ry],
        angle,
    }
}

impl QuickShape {
    /// Pontos ao longo da forma, a cada `step` pixels, para desenhar com o pincel.
    pub fn sample(&self, step: f64) -> Vec<[f64; 2]> {
        let step = step.max(0.5);
        let poly = |pts: &[[f64; 2]]| {
            let mut out = Vec::new();
            for w in pts.windows(2) {
                let n = (dist(w[0], w[1]) / step).ceil().max(1.0) as usize;
                for i in 0..n {
                    let t = i as f64 / n as f64;
                    out.push([
                        w[0][0] + (w[1][0] - w[0][0]) * t,
                        w[0][1] + (w[1][1] - w[0][1]) * t,
                    ]);
                }
            }
            if let Some(l) = pts.last() {
                out.push(*l);
            }
            out
        };
        match self {
            QuickShape::Line(a, b) => poly(&[*a, *b]),
            QuickShape::Polygon { points, closed } => {
                let mut p = points.clone();
                if *closed && let Some(f) = points.first() {
                    p.push(*f);
                }
                poly(&p)
            }
            QuickShape::Ellipse {
                center,
                radii,
                angle,
            } => {
                let perim =
                    std::f64::consts::TAU * ((radii[0].powi(2) + radii[1].powi(2)) / 2.0).sqrt();
                let n = (perim / step).ceil().max(8.0) as usize;
                let (s, c) = angle.sin_cos();
                (0..=n)
                    .map(|i| {
                        let t = i as f64 / n as f64 * std::f64::consts::TAU;
                        let (x, y) = (radii[0] * t.cos(), radii[1] * t.sin());
                        [center[0] + x * c - y * s, center[1] + x * s + y * c]
                    })
                    .collect()
            }
            QuickShape::Arc { from, via, to } => match circle(*from, *via, *to) {
                Some((c, r)) => {
                    let ang = |p: [f64; 2]| (p[1] - c[1]).atan2(p[0] - c[0]);
                    let (a0, am, a1) = (ang(*from), ang(*via), ang(*to));
                    // Vai de a0 a a1 pelo lado que passa por am.
                    let tau = std::f64::consts::TAU;
                    let ccw = |a: f64, b: f64| (b - a).rem_euclid(tau);
                    let sweep = if ccw(a0, am) <= ccw(a0, a1) {
                        ccw(a0, a1)
                    } else {
                        ccw(a0, a1) - tau
                    };
                    let n = ((sweep.abs() * r) / step).ceil().max(2.0) as usize;
                    (0..=n)
                        .map(|i| {
                            let a = a0 + sweep * i as f64 / n as f64;
                            [c[0] + r * a.cos(), c[1] + r * a.sin()]
                        })
                        .collect()
                }
                None => poly(&[*from, *to]),
            },
        }
    }

    /// Versão "perfeita" (com um dedo na tela no Procreate): a elipse vira círculo.
    pub fn perfect(&self) -> Self {
        match self {
            QuickShape::Ellipse { center, radii, .. } => {
                let r = (radii[0] + radii[1]) / 2.0;
                QuickShape::Ellipse {
                    center: *center,
                    radii: [r, r],
                    angle: 0.0,
                }
            }
            s => s.clone(),
        }
    }
}

/// Círculo pelos três pontos (`None` se estão em linha).
fn circle(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> Option<([f64; 2], f64)> {
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    if d.abs() < 1e-9 {
        return None;
    }
    let sq = |p: [f64; 2]| p[0] * p[0] + p[1] * p[1];
    let ux = (sq(a) * (b[1] - c[1]) + sq(b) * (c[1] - a[1]) + sq(c) * (a[1] - b[1])) / d;
    let uy = (sq(a) * (c[0] - b[0]) + sq(b) * (a[0] - c[0]) + sq(c) * (b[0] - a[0])) / d;
    let center = [ux, uy];
    Some((center, dist(center, a)))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic)]
    use super::*;

    fn wobble(i: usize) -> f64 {
        [0.0, 1.2, -0.8, 0.5, -1.1][i % 5]
    }

    #[test]
    fn recognises_line_ellipse_rectangle_and_arc() {
        let line: Vec<[f64; 2]> = (0..50)
            .map(|i| [i as f64 * 4.0, 10.0 + wobble(i)])
            .collect();
        assert!(matches!(detect(&line), Some(QuickShape::Line(..))));

        let circ: Vec<[f64; 2]> = (0..=80)
            .map(|i| {
                let t = i as f64 / 80.0 * std::f64::consts::TAU;
                [100.0 + 60.0 * t.cos() + wobble(i), 100.0 + 58.0 * t.sin()]
            })
            .collect();
        match detect(&circ) {
            Some(QuickShape::Ellipse { center, radii, .. }) => {
                assert!((center[0] - 100.0).abs() < 3.0 && (radii[0] - 59.0).abs() < 4.0);
            }
            s => panic!("{s:?}"),
        }

        let mut rect = Vec::new();
        for (a, b) in [
            ([0.0, 0.0], [100.0, 0.0]),
            ([100.0, 0.0], [100.0, 60.0]),
            ([100.0, 60.0], [0.0, 60.0]),
            ([0.0, 60.0], [0.0, 2.0]),
        ] {
            for i in 0..20 {
                let t = i as f64 / 20.0;
                rect.push([
                    a[0] + (b[0] - a[0]) * t + wobble(i),
                    a[1] + (b[1] - a[1]) * t,
                ]);
            }
        }
        match detect(&rect) {
            Some(QuickShape::Polygon { points, closed }) => assert!(closed && points.len() == 4),
            s => panic!("{s:?}"),
        }

        let arc: Vec<[f64; 2]> = (0..=40)
            .map(|i| {
                let t = i as f64 / 40.0 * std::f64::consts::PI;
                [100.0 + 50.0 * t.cos(), 100.0 - 50.0 * t.sin()]
            })
            .collect();
        let s = detect(&arc).unwrap_or(QuickShape::Line([0.0; 2], [0.0; 2]));
        assert!(matches!(s, QuickShape::Arc { .. }), "{s:?}");
        let pts = s.sample(2.0);
        // O meio do arco passa perto do topo do semicírculo.
        assert!(
            pts.iter()
                .any(|p| (p[0] - 100.0).abs() < 3.0 && (p[1] - 50.0).abs() < 3.0)
        );
    }
}
