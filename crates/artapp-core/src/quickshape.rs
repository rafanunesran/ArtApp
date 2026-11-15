//! Forma rápida: ao segurar a caneta parada no fim de um traço, o rabisco vira uma reta, um arco,
//! um círculo, uma elipse, um quadrado, um retângulo, um triângulo ou outro polígono limpo,
//! desenhado com o mesmo pincel (ou como caminho vetorial, na caneta vetorial).
//!
//! Como reconhece um traço fechado:
//! 1. corta a sobra do fim (quando a caneta passa do ponto de partida) e reamostra o contorno
//!    em passos iguais;
//! 2. ajusta cada candidato: círculo e elipse por mínimos quadrados (cônica geral), retângulo pelo
//!    menor retângulo que cobre a casca convexa, triângulo pelo maior triângulo dentro dela, e
//!    polígonos pelos cantos achados na curvatura; os lados dos polígonos são reajustados como
//!    retas pelos pontos do traço e os cantos viram as interseções;
//! 3. mede o erro de cada candidato nos dois sentidos (traço→forma e forma→traço) e fica com o
//!    menor, com um pequeno peso a mais para formas com mais parâmetros;
//! 4. arruma a escolhida: retângulo quase 1:1 vira quadrado, elipse quase redonda vira círculo,
//!    triângulo quase equilátero fica equilátero, e giros perto da horizontal/vertical endireitam.

use std::f64::consts::{PI, TAU};

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
    /// Elipse com centro, raios e giro (radianos). Raios iguais = círculo.
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

type P = [f64; 2];

fn dist(a: P, b: P) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: P, b: P) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

/// Distância do ponto `p` ao segmento `a`–`b`.
fn seg_dist(p: P, a: P, b: P) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    if l2 <= 1e-12 {
        return dist(p, a);
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0);
    dist(p, [a[0] + dx * t, a[1] + dy * t])
}

/// Simplificação de Ramer–Douglas–Peucker.
fn simplify(points: &[P], eps: f64) -> Vec<P> {
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
fn corner_angle(a: P, b: P, c: P) -> f64 {
    let (u, v) = (sub(a, b), sub(c, b));
    let den = u[0].hypot(u[1]) * v[0].hypot(v[1]);
    if den <= 1e-12 {
        return 180.0;
    }
    (dot(u, v) / den).clamp(-1.0, 1.0).acos().to_degrees()
}

fn path_len(points: &[P]) -> f64 {
    points.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// Reamostra a polilinha em `n` pontos igualmente espaçados pelo comprimento.
fn resample(points: &[P], n: usize) -> Vec<P> {
    let total = path_len(points);
    if points.len() < 2 || total <= 1e-9 || n < 2 {
        return points.to_vec();
    }
    let step = total / (n - 1) as f64;
    let mut out = Vec::with_capacity(n);
    out.push(points[0]);
    let (mut seg, mut acc) = (0usize, 0.0);
    for k in 1..n - 1 {
        let target = step * k as f64;
        while seg + 1 < points.len() - 1 && acc + dist(points[seg], points[seg + 1]) < target {
            acc += dist(points[seg], points[seg + 1]);
            seg += 1;
        }
        let (a, b) = (points[seg], points[seg + 1]);
        let l = dist(a, b).max(1e-12);
        let t = ((target - acc) / l).clamp(0.0, 1.0);
        out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
    }
    out.push(points[points.len() - 1]);
    out
}

fn bbox(points: &[P]) -> (P, P) {
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in points {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    (lo, hi)
}

fn centroid(points: &[P]) -> P {
    let n = points.len().max(1) as f64;
    let s = points
        .iter()
        .fold([0.0, 0.0], |s, p| [s[0] + p[0], s[1] + p[1]]);
    [s[0] / n, s[1] / n]
}

/// Reconhece a forma de um traço (pontos em ordem). `None` quando é curto demais ou não parece
/// nenhuma forma.
pub fn detect(points: &[P]) -> Option<QuickShape> {
    let points: Vec<P> = points
        .iter()
        .copied()
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .collect();
    if points.len() < 2 {
        return None;
    }
    let len = path_len(&points);
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
    let (lo, hi) = bbox(&points);
    let diag = dist(lo, hi).max(1.0);
    if let Some(ring) = close_loop(&points, len, diag) {
        return detect_closed(&ring, diag);
    }
    detect_open(&points, diag)
}

/// Se o traço volta perto do começo, devolve o laço (sem a sobra) reamostrado.
fn close_loop(points: &[P], len: f64, diag: f64) -> Option<Vec<P>> {
    if len < diag * 1.6 {
        return None;
    }
    let pts = resample(points, 256);
    let n = pts.len();
    let last = pts[n - 1];
    // O fim procura o ponto mais perto no primeiro quarto do traço (passou do começo = sobra).
    let early = (n / 4).max(1);
    let (j, gap) = (0..early)
        .map(|i| (i, dist(pts[i], last)))
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    // E o começo procura no último quarto (parou antes = sobra no fim).
    let late = n - early;
    let first = pts[j];
    let (k, gap2) = (late..n)
        .map(|i| (i, dist(pts[i], first)))
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    let tol = (diag * 0.22).max(6.0);
    if gap.min(gap2) > tol {
        return None;
    }
    let ring = &pts[j..=k.max(j + 2)];
    if path_len(ring) < diag * 1.5 {
        return None;
    }
    Some(resample(ring, 128))
}

/// Candidato com o erro medido (menor é melhor).
struct Cand {
    shape: QuickShape,
    score: f64,
}

fn detect_closed(ring: &[P], diag: f64) -> Option<QuickShape> {
    let mut cands: Vec<Cand> = Vec::new();
    let mut push = |shape: Option<QuickShape>, weight: f64| {
        if let Some(shape) = shape {
            let score = fit_error(ring, &shape, true) * weight;
            if score.is_finite() {
                cands.push(Cand { shape, score });
            }
        }
    };
    let circle = fit_circle(ring);
    push(circle.clone(), 1.0);
    let ellipse = fit_conic(ring).or_else(|| Some(fit_moments(ring)));
    // Elipse quase redonda: o círculo já cobre (e é mais firme no centro).
    let ellipse = ellipse.filter(|e| match e {
        QuickShape::Ellipse { radii, .. } => radii[1] / radii[0].max(1e-9) < 0.92,
        _ => true,
    });
    push(ellipse, 1.15);
    let hull = convex_hull(ring);
    push(
        min_area_rect(&hull)
            .and_then(|q| refine_polygon(ring, &q).map(tidy_quad))
            .map(closed_poly),
        1.0,
    );
    push(
        max_triangle(&hull)
            .and_then(|t| refine_polygon(ring, &t).map(tidy_triangle))
            .map(closed_poly),
        1.0,
    );
    for corners in corner_polygons(ring) {
        let w = if corners.len() <= 4 { 1.05 } else { 1.25 };
        let tidy = |q: Vec<P>| match q.len() {
            3 => tidy_triangle(q),
            4 => tidy_quad(q),
            _ => q,
        };
        push(refine_polygon(ring, &corners).map(tidy).map(closed_poly), w);
    }
    cands.sort_by(|a, b| a.score.total_cmp(&b.score));
    let best = cands.into_iter().next()?;
    // Nada parecido: melhor deixar o traço como está.
    if best.score > diag * 0.09 {
        return None;
    }
    Some(match best.shape {
        QuickShape::Polygon { points, .. } => QuickShape::Polygon {
            points,
            closed: true,
        },
        s => s,
    })
}

fn closed_poly(points: Vec<P>) -> QuickShape {
    QuickShape::Polygon {
        points,
        closed: true,
    }
}

fn detect_open(points: &[P], diag: f64) -> Option<QuickShape> {
    let (first, last) = (points[0], points[points.len() - 1]);
    let ring = resample(points, 96);
    let mut best: Option<(f64, QuickShape)> = None;
    let mut consider = |shape: QuickShape, weight: f64| {
        let e = fit_error(&ring, &shape, false) * weight;
        if e.is_finite() && best.as_ref().is_none_or(|(b, _)| e < *b) {
            best = Some((e, shape));
        }
    };
    // Linha com cantos: só quando os cantos são bem marcados.
    let eps = (diag * 0.035).max(2.5);
    let v = drop_flat(simplify(points, eps), false);
    let sharp = v.windows(3).all(|w| corner_angle(w[0], w[1], w[2]) < 135.0);
    if (3..=6).contains(&v.len()) && sharp {
        consider(
            QuickShape::Polygon {
                points: v,
                closed: false,
            },
            1.0,
        );
    }
    // Curva aberta: arco pelo começo, pelo ponto mais longe da corda e pelo fim.
    let via = points
        .iter()
        .copied()
        .max_by(|a, b| seg_dist(*a, first, last).total_cmp(&seg_dist(*b, first, last)))
        .unwrap_or(first);
    consider(
        QuickShape::Arc {
            from: first,
            via,
            to: last,
        },
        1.0,
    );
    best.filter(|(e, _)| *e < diag * 0.08).map(|(_, s)| s)
}

/// Tira cantos quase retos (lados que seguem em linha), que só atrapalham o ajuste.
fn drop_flat(mut v: Vec<P>, closed: bool) -> Vec<P> {
    // Lados curtinhos (dois cantos quase no mesmo lugar) viram um canto só.
    if closed {
        loop {
            let n = v.len();
            if n <= 3 {
                break;
            }
            let perim: f64 = (0..n).map(|i| dist(v[i], v[(i + 1) % n])).sum();
            let short = (0..n)
                .map(|i| (i, dist(v[i], v[(i + 1) % n])))
                .filter(|(_, l)| *l < perim * 0.06)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let Some((i, _)) = short else {
                break;
            };
            let j = (i + 1) % n;
            v[i] = [(v[i][0] + v[j][0]) / 2.0, (v[i][1] + v[j][1]) / 2.0];
            v.remove(j);
        }
    }
    loop {
        let n = v.len();
        let min = if closed { 3 } else { 2 };
        if n <= min {
            return v;
        }
        let range = if closed { 0..n } else { 1..n - 1 };
        let flat = range
            .map(|i| (i, corner_angle(v[(i + n - 1) % n], v[i], v[(i + 1) % n])))
            .filter(|(_, a)| *a > 150.0)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        match flat {
            Some((i, _)) => {
                v.remove(i);
            }
            None => return v,
        }
    }
}

/// Erro médio entre o traço e a forma, nos dois sentidos (assim uma forma que sobra para fora
/// do traço também perde pontos).
fn fit_error(ring: &[P], shape: &QuickShape, closed: bool) -> f64 {
    let outline = shape.sample(2.0);
    if outline.len() < 2 || ring.is_empty() {
        return f64::INFINITY;
    }
    let outline = resample(&outline, 160);
    let to_poly = |p: P, poly: &[P]| {
        poly.windows(2)
            .map(|w| seg_dist(p, w[0], w[1]))
            .fold(f64::MAX, f64::min)
    };
    let rms = |a: &[P], b: &[P]| {
        (a.iter().map(|p| to_poly(*p, b).powi(2)).sum::<f64>() / a.len() as f64).sqrt()
    };
    let mut closed_ring = ring.to_vec();
    if closed {
        closed_ring.push(ring[0]);
    }
    (rms(ring, &outline) + rms(&outline, &closed_ring)) / 2.0
}

/// Resolve `m·x = b` (eliminação de Gauss com pivô parcial).
fn solve<const N: usize>(mut m: [[f64; N]; N], mut b: [f64; N]) -> Option<[f64; N]> {
    for c in 0..N {
        let piv = (c..N).max_by(|&i, &j| m[i][c].abs().total_cmp(&m[j][c].abs()))?;
        if m[piv][c].abs() < 1e-12 {
            return None;
        }
        m.swap(c, piv);
        b.swap(c, piv);
        for r in c + 1..N {
            let f = m[r][c] / m[c][c];
            let pivot = m[c];
            for (mk, pk) in m[r][c..].iter_mut().zip(&pivot[c..]) {
                *mk -= f * pk;
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = [0.0; N];
    for c in (0..N).rev() {
        let s: f64 = (c + 1..N).map(|k| m[c][k] * x[k]).sum();
        x[c] = (b[c] - s) / m[c][c];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// Mínimos quadrados genérico: linhas `rows` (vetor de atributos) contra `rhs`.
fn least_squares<const N: usize>(rows: impl Iterator<Item = ([f64; N], f64)>) -> Option<[f64; N]> {
    let mut m = [[0.0; N]; N];
    let mut b = [0.0; N];
    for (r, y) in rows {
        for i in 0..N {
            for j in 0..N {
                m[i][j] += r[i] * r[j];
            }
            b[i] += r[i] * y;
        }
    }
    solve(m, b)
}

/// Círculo por mínimos quadrados algébricos (Kåsa), em coordenadas centradas e escaladas.
fn fit_circle(ring: &[P]) -> Option<QuickShape> {
    let c = centroid(ring);
    let s = ring.iter().map(|p| dist(*p, c)).sum::<f64>() / ring.len() as f64;
    if s <= 1e-9 {
        return None;
    }
    let q: Vec<P> = ring
        .iter()
        .map(|p| [(p[0] - c[0]) / s, (p[1] - c[1]) / s])
        .collect();
    // x² + y² = 2ax + 2by + k
    let x = least_squares(
        q.iter()
            .map(|p| ([2.0 * p[0], 2.0 * p[1], 1.0], dot(*p, *p))),
    )?;
    let r2 = x[2] + x[0] * x[0] + x[1] * x[1];
    if r2 <= 0.0 {
        return None;
    }
    let r = r2.sqrt() * s;
    Some(QuickShape::Ellipse {
        center: [c[0] + x[0] * s, c[1] + x[1] * s],
        radii: [r, r],
        angle: 0.0,
    })
}

/// Elipse por mínimos quadrados da cônica geral `Ax²+Bxy+Cy²+Dx+Ey = 1` (coordenadas centradas
/// no centroide, onde a curva não passa pela origem).
fn fit_conic(ring: &[P]) -> Option<QuickShape> {
    let c = centroid(ring);
    let s = ring.iter().map(|p| dist(*p, c)).sum::<f64>() / ring.len() as f64;
    if s <= 1e-9 {
        return None;
    }
    let q: Vec<P> = ring
        .iter()
        .map(|p| [(p[0] - c[0]) / s, (p[1] - c[1]) / s])
        .collect();
    let [a, b, cc, d, e] = least_squares(
        q.iter()
            .map(|p| ([p[0] * p[0], p[0] * p[1], p[1] * p[1], p[0], p[1]], 1.0)),
    )?;
    if b * b - 4.0 * a * cc >= 0.0 {
        return None; // não é elipse
    }
    // Centro: gradiente zero.
    let det = 4.0 * a * cc - b * b;
    let x0 = (b * e - 2.0 * cc * d) / det;
    let y0 = (b * d - 2.0 * a * e) / det;
    let g = a * x0 * x0 + b * x0 * y0 + cc * y0 * y0 + d * x0 + e * y0 - 1.0;
    // Autovalores de [[a, b/2], [b/2, c]].
    let tr = a + cc;
    let disc = (((a - cc) / 2.0).powi(2) + (b / 2.0).powi(2)).sqrt();
    let (l1, l2) = (tr / 2.0 - disc, tr / 2.0 + disc);
    if l1 * (-g) <= 0.0 || l2 * (-g) <= 0.0 {
        return None;
    }
    // l1 (menor autovalor) dá o eixo maior.
    let rx = (-g / l1).sqrt() * s;
    let ry = (-g / l2).sqrt() * s;
    let angle = if b.abs() < 1e-12 && a <= cc {
        0.0
    } else if b.abs() < 1e-12 {
        PI / 2.0
    } else {
        (l1 - a).atan2(b / 2.0)
    };
    if !(rx.is_finite() && ry.is_finite()) || rx > s * 50.0 {
        return None;
    }
    Some(tidy_ellipse(
        [c[0] + x0 * s, c[1] + y0 * s],
        [rx, ry],
        angle,
    ))
}

/// Elipse pelos momentos dos pontos (reserva, quando a cônica falha).
fn fit_moments(points: &[P]) -> QuickShape {
    let [cx, cy] = centroid(points);
    let n = points.len().max(1) as f64;
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
    tidy_ellipse([cx, cy], [(2.0 * l1).sqrt(), (2.0 * l2).sqrt()], angle)
}

/// Raio maior primeiro, giro em (-π/2, π/2], e giro quase reto endireita.
fn tidy_ellipse(center: P, radii: P, angle: f64) -> QuickShape {
    let (mut rx, mut ry, mut angle) = (radii[0], radii[1], angle);
    if ry > rx {
        std::mem::swap(&mut rx, &mut ry);
        angle += PI / 2.0;
    }
    angle = (angle + PI / 2.0).rem_euclid(PI) - PI / 2.0;
    angle = snap_angle(angle, PI / 2.0);
    if ry / rx.max(1e-9) > 0.92 {
        let r = (rx + ry) / 2.0;
        return QuickShape::Ellipse {
            center,
            radii: [r, r],
            angle: 0.0,
        };
    }
    QuickShape::Ellipse {
        center,
        radii: [rx, ry],
        angle,
    }
}

/// Endireita um ângulo que está a menos de 6° de um múltiplo de `period`.
fn snap_angle(a: f64, period: f64) -> f64 {
    let k = (a / period).round();
    if (a - k * period).abs() < 6f64.to_radians() {
        k * period
    } else {
        a
    }
}

/// Casca convexa (cadeia monótona de Andrew), no sentido anti-horário.
fn convex_hull(points: &[P]) -> Vec<P> {
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let mut hull: Vec<P> = Vec::with_capacity(pts.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &P>> = if pass == 0 {
            Box::new(pts.iter())
        } else {
            Box::new(pts.iter().rev())
        };
        for &p in iter {
            while hull.len() >= start + 2
                && cross(
                    sub(hull[hull.len() - 1], hull[hull.len() - 2]),
                    sub(p, hull[hull.len() - 2]),
                ) <= 0.0
            {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// Menor retângulo (girado) que cobre a casca: um dos lados sempre encosta num lado da casca.
fn min_area_rect(hull: &[P]) -> Option<Vec<P>> {
    let n = hull.len();
    if n < 3 {
        return None;
    }
    let mut best: Option<(f64, Vec<P>)> = None;
    for i in 0..n {
        let e = sub(hull[(i + 1) % n], hull[i]);
        let l = e[0].hypot(e[1]);
        if l < 1e-9 {
            continue;
        }
        let u = [e[0] / l, e[1] / l];
        let v = [-u[1], u[0]];
        let (mut u0, mut u1, mut v0, mut v1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for p in hull {
            let (a, b) = (dot(*p, u), dot(*p, v));
            u0 = u0.min(a);
            u1 = u1.max(a);
            v0 = v0.min(b);
            v1 = v1.max(b);
        }
        let area = (u1 - u0) * (v1 - v0);
        if best.as_ref().is_none_or(|(a, _)| area < *a) {
            let at = |a: f64, b: f64| [u[0] * a + v[0] * b, u[1] * a + v[1] * b];
            best = Some((area, vec![at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1)]));
        }
    }
    best.map(|(_, r)| r)
}

/// Maior triângulo com cantos na casca.
fn max_triangle(hull: &[P]) -> Option<Vec<P>> {
    let h: Vec<P> = if hull.len() > 60 {
        let step = hull.len() as f64 / 60.0;
        (0..60).map(|i| hull[(i as f64 * step) as usize]).collect()
    } else {
        hull.to_vec()
    };
    let n = h.len();
    if n < 3 {
        return None;
    }
    let mut best = (0.0, [0usize; 3]);
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let a = cross(sub(h[j], h[i]), sub(h[k], h[i])).abs();
                if a > best.0 {
                    best = (a, [i, j, k]);
                }
            }
        }
    }
    (best.0 > 0.0).then(|| best.1.iter().map(|&i| h[i]).collect())
}

/// Polígonos a partir dos cantos achados pela virada do traço (3 a 6 cantos).
fn corner_polygons(ring: &[P]) -> Vec<Vec<P>> {
    let n = ring.len();
    if n < 16 {
        return Vec::new();
    }
    let at = |i: isize| ring[i.rem_euclid(n as isize) as usize];
    let mut out = Vec::new();
    for w in [3isize, 5, 8] {
        let turn: Vec<f64> = (0..n as isize)
            .map(|i| {
                let (a, b) = (sub(at(i), at(i - w)), sub(at(i + w), at(i)));
                cross(a, b).atan2(dot(a, b)).abs()
            })
            .collect();
        let sup = (w as usize * 2).max(4);
        let mut idx: Vec<usize> = (0..n)
            .filter(|&i| {
                turn[i] > 35f64.to_radians()
                    && (1..=sup).all(|d| {
                        let (l, r) = (turn[(i + n - d) % n], turn[(i + d) % n]);
                        turn[i] >= l && turn[i] > r
                    })
            })
            .collect();
        idx.sort_unstable();
        if (3..=6).contains(&idx.len()) {
            let poly: Vec<P> = idx.iter().map(|&i| ring[i]).collect();
            if !out.contains(&poly) {
                out.push(poly);
            }
        }
    }
    out
}

/// Reajusta cada lado do polígono como reta (mínimos quadrados) pelos pontos do traço que caem
/// perto do meio dele, e põe os cantos nas interseções das retas vizinhas.
fn refine_polygon(ring: &[P], poly: &[P]) -> Option<Vec<P>> {
    let k = poly.len();
    if k < 3 {
        return None;
    }
    let mut v = drop_flat(poly.to_vec(), true);
    let k = v.len();
    if k < 3 {
        return None;
    }
    for _ in 0..3 {
        let mut lines: Vec<(P, P)> = Vec::with_capacity(k);
        for e in 0..k {
            let (a, b) = (v[e], v[(e + 1) % k]);
            let d = sub(b, a);
            let l2 = dot(d, d);
            if l2 < 1e-9 {
                return None;
            }
            let near: Vec<P> = ring
                .iter()
                .copied()
                .filter(|p| {
                    let t = dot(sub(*p, a), d) / l2;
                    if !(0.15..=0.85).contains(&t) {
                        return false;
                    }
                    let mine = seg_dist(*p, a, b);
                    (0..k).all(|o| o == e || seg_dist(*p, v[o], v[(o + 1) % k]) >= mine)
                })
                .collect();
            if near.len() < 3 {
                lines.push((a, [d[0] / l2.sqrt(), d[1] / l2.sqrt()]));
                continue;
            }
            // Reta de mínimos quadrados total: centroide + direção principal.
            let c = centroid(&near);
            let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
            for p in &near {
                let q = sub(*p, c);
                sxx += q[0] * q[0];
                syy += q[1] * q[1];
                sxy += q[0] * q[1];
            }
            let ang = 0.5 * (2.0 * sxy).atan2(sxx - syy);
            let mut dir = [ang.cos(), ang.sin()];
            if dot(dir, d) < 0.0 {
                dir = [-dir[0], -dir[1]];
            }
            lines.push((c, dir));
        }
        let mut next = Vec::with_capacity(k);
        for i in 0..k {
            let (p1, d1) = lines[(i + k - 1) % k];
            let (p2, d2) = lines[i];
            let den = cross(d1, d2);
            if den.abs() < 0.05 {
                next.push(v[i]);
                continue;
            }
            let t = cross(sub(p2, p1), d2) / den;
            next.push([p1[0] + d1[0] * t, p1[1] + d1[1] * t]);
        }
        v = next;
    }
    // Um canto que fugiu para longe é sinal de reta mal ajustada: desiste.
    let (lo, hi) = bbox(ring);
    let diag = dist(lo, hi);
    let c = centroid(ring);
    if v.iter()
        .any(|p| !p[0].is_finite() || dist(*p, c) > diag * 1.2)
    {
        return None;
    }
    let v = drop_flat(v, true);
    (v.len() >= 3).then_some(v)
}

/// Quadrilátero quase reto vira retângulo (e quase 1:1, quadrado); giro quase reto endireita.
fn tidy_quad(q: Vec<P>) -> Vec<P> {
    if q.len() != 4 {
        return q;
    }
    let right =
        (0..4).all(|i| (corner_angle(q[(i + 3) % 4], q[i], q[(i + 1) % 4]) - 90.0).abs() < 16.0);
    if !right {
        return q;
    }
    // Direção média dos lados, módulo 90° (média de 4θ).
    let (mut sx, mut sy) = (0.0, 0.0);
    for i in 0..4 {
        let e = sub(q[(i + 1) % 4], q[i]);
        let l = e[0].hypot(e[1]);
        let a = e[1].atan2(e[0]) * 4.0;
        sx += l * a.cos();
        sy += l * a.sin();
    }
    let theta = snap_angle(sy.atan2(sx) / 4.0, PI / 2.0);
    let u = [theta.cos(), theta.sin()];
    let v = [-u[1], u[0]];
    let c = centroid(&q);
    // Meia largura/altura: média das projeções dos cantos.
    let hw = q.iter().map(|p| dot(sub(*p, c), u).abs()).sum::<f64>() / 4.0;
    let hh = q.iter().map(|p| dot(sub(*p, c), v).abs()).sum::<f64>() / 4.0;
    let (hw, hh) = if (hw - hh).abs() < hw.max(hh) * 0.12 {
        let m = (hw + hh) / 2.0;
        (m, m)
    } else {
        (hw, hh)
    };
    let at = |a: f64, b: f64| [c[0] + u[0] * a + v[0] * b, c[1] + u[1] * a + v[1] * b];
    vec![at(-hw, -hh), at(hw, -hh), at(hw, hh), at(-hw, hh)]
}

/// Triângulo quase equilátero fica equilátero; um lado quase horizontal/vertical endireita.
fn tidy_triangle(t: Vec<P>) -> Vec<P> {
    if t.len() != 3 {
        return t;
    }
    let c = centroid(&t);
    let sides: Vec<f64> = (0..3).map(|i| dist(t[i], t[(i + 1) % 3])).collect();
    let mean = sides.iter().sum::<f64>() / 3.0;
    let mut t = if sides.iter().all(|s| (s - mean).abs() < mean * 0.1) {
        // Mantém a direção do primeiro canto e o sentido de giro.
        let r = mean / 3f64.sqrt();
        let a0 = (t[0][1] - c[1]).atan2(t[0][0] - c[0]);
        let dir = if cross(sub(t[1], t[0]), sub(t[2], t[0])) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        (0..3)
            .map(|i| {
                let a = a0 + dir * TAU / 3.0 * i as f64;
                [c[0] + r * a.cos(), c[1] + r * a.sin()]
            })
            .collect()
    } else {
        t
    };
    // Gira em torno do centro para endireitar o lado mais perto da horizontal/vertical.
    let mut rot = 0.0;
    let mut best = f64::MAX;
    for i in 0..3 {
        let e = sub(t[(i + 1) % 3], t[i]);
        let a = e[1].atan2(e[0]);
        let k = (a / (PI / 2.0)).round() * (PI / 2.0);
        if (a - k).abs() < best {
            best = (a - k).abs();
            rot = k - a;
        }
    }
    if best < 6f64.to_radians() {
        let (s, co) = rot.sin_cos();
        t = t
            .iter()
            .map(|p| {
                let q = sub(*p, c);
                [c[0] + q[0] * co - q[1] * s, c[1] + q[0] * s + q[1] * co]
            })
            .collect();
    }
    t
}

impl QuickShape {
    /// Nome para a barra de status ("Forma rápida: …").
    pub fn name(&self) -> &'static str {
        match self {
            QuickShape::Line(..) => "reta",
            QuickShape::Arc { .. } => "arco",
            QuickShape::Ellipse { radii, .. } => {
                if (radii[0] - radii[1]).abs() < 1e-6 {
                    "círculo"
                } else {
                    "elipse"
                }
            }
            QuickShape::Polygon {
                points,
                closed: false,
            } if points.len() >= 3 => "linha com cantos",
            QuickShape::Polygon { points, .. } => match points.len() {
                3 => "triângulo",
                4 => {
                    let right = (0..4).all(|i| {
                        (corner_angle(points[(i + 3) % 4], points[i], points[(i + 1) % 4]) - 90.0)
                            .abs()
                            < 0.5
                    });
                    let (a, b) = (dist(points[0], points[1]), dist(points[1], points[2]));
                    if right && (a - b).abs() < a.max(b) * 1e-6 {
                        "quadrado"
                    } else if right {
                        "retângulo"
                    } else {
                        "quadrilátero"
                    }
                }
                5 => "pentágono",
                6 => "hexágono",
                _ => "polígono",
            },
        }
    }

    /// Pontos ao longo da forma, a cada `step` pixels, para desenhar com o pincel.
    pub fn sample(&self, step: f64) -> Vec<P> {
        let step = step.max(0.5);
        let poly = |pts: &[P]| {
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
                let perim = TAU * ((radii[0].powi(2) + radii[1].powi(2)) / 2.0).sqrt();
                let n = (perim / step).ceil().clamp(8.0, 20000.0) as usize;
                let (s, c) = angle.sin_cos();
                (0..=n)
                    .map(|i| {
                        let t = i as f64 / n as f64 * TAU;
                        let (x, y) = (radii[0] * t.cos(), radii[1] * t.sin());
                        [center[0] + x * c - y * s, center[1] + x * s + y * c]
                    })
                    .collect()
            }
            QuickShape::Arc { from, via, to } => match arc_params(*from, *via, *to) {
                Some((c, r, a0, sweep)) => {
                    let n = ((sweep.abs() * r) / step).ceil().clamp(2.0, 20000.0) as usize;
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

    /// Versão "perfeita": elipse vira círculo e retângulo vira quadrado.
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
            QuickShape::Polygon { points, closed } if *closed && points.len() == 4 => {
                let c = centroid(points);
                let e = sub(points[1], points[0]);
                let l = e[0].hypot(e[1]).max(1e-9);
                let u = [e[0] / l, e[1] / l];
                let v = [-u[1], u[0]];
                let h = points
                    .iter()
                    .map(|p| dot(sub(*p, c), u).abs() + dot(sub(*p, c), v).abs())
                    .sum::<f64>()
                    / 8.0;
                let at = |a: f64, b: f64| [c[0] + u[0] * a + v[0] * b, c[1] + u[1] * a + v[1] * b];
                QuickShape::Polygon {
                    points: vec![at(-h, -h), at(h, -h), at(h, h), at(-h, h)],
                    closed: true,
                }
            }
            s => s.clone(),
        }
    }

    /// A forma como caminho vetorial exato (cantos vivos, elipse em quatro Béziers).
    pub fn to_subpath(&self) -> vectorcraft_geom::path::SubPath {
        use vectorcraft_geom::Point;
        use vectorcraft_geom::path::{Anchor, SubPath};
        let corner = |p: P| Anchor::corner(Point::new(p[0], p[1]));
        match self {
            QuickShape::Line(a, b) => SubPath::new(vec![corner(*a), corner(*b)], false),
            QuickShape::Polygon { points, closed } => {
                SubPath::new(points.iter().map(|p| corner(*p)).collect(), *closed)
            }
            QuickShape::Ellipse {
                center,
                radii,
                angle,
            } => {
                // Quatro quartos de elipse; alça = 4/3·tan(π/8) da derivada.
                let k = 4.0 / 3.0 * (PI / 8.0).tan();
                let (s, c) = angle.sin_cos();
                let rot = |x: f64, y: f64| [x * c - y * s, x * s + y * c];
                let anchors = (0..4)
                    .map(|i| {
                        let t = i as f64 * PI / 2.0;
                        let p = rot(radii[0] * t.cos(), radii[1] * t.sin());
                        let d = rot(-radii[0] * t.sin(), radii[1] * t.cos());
                        let p = [center[0] + p[0], center[1] + p[1]];
                        Anchor::smooth(
                            Point::new(p[0], p[1]),
                            Point::new(p[0] + d[0] * k, p[1] + d[1] * k),
                        )
                    })
                    .collect();
                SubPath::new(anchors, true)
            }
            QuickShape::Arc { .. } => {
                let pts: Vec<Point> = self
                    .sample(2.0)
                    .into_iter()
                    .map(|p| Point::new(p[0], p[1]))
                    .collect();
                crate::vector::fit_freehand(&pts, 0.5, false)
            }
        }
    }
}

/// Centro, raio, ângulo inicial e varredura do arco que vai de `from` a `to` passando por `via`.
fn arc_params(from: P, via: P, to: P) -> Option<(P, f64, f64, f64)> {
    let (c, r) = circle(from, via, to)?;
    let ang = |p: P| (p[1] - c[1]).atan2(p[0] - c[0]);
    let (a0, am, a1) = (ang(from), ang(via), ang(to));
    let ccw = |a: f64, b: f64| (b - a).rem_euclid(TAU);
    let sweep = if ccw(a0, am) <= ccw(a0, a1) {
        ccw(a0, a1)
    } else {
        ccw(a0, a1) - TAU
    };
    Some((c, r, a0, sweep))
}

/// Círculo pelos três pontos (`None` se estão em linha).
fn circle(a: P, b: P, c: P) -> Option<(P, f64)> {
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    if d.abs() < 1e-9 {
        return None;
    }
    let sq = |p: P| p[0] * p[0] + p[1] * p[1];
    let ux = (sq(a) * (b[1] - c[1]) + sq(b) * (c[1] - a[1]) + sq(c) * (a[1] - b[1])) / d;
    let uy = (sq(a) * (c[0] - b[0]) + sq(b) * (a[0] - c[0]) + sq(c) * (b[0] - a[0])) / d;
    let center = [ux, uy];
    Some((center, dist(center, a)))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::unwrap_used)]
    use super::*;

    /// Gerador pseudo-aleatório determinístico (xorshift) para o tremor da mão.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
        }
    }

    /// Simula a mão: tremor suave (ruído filtrado) e pontos irregulares.
    fn hand(points: &[P], amp: f64, seed: u64) -> Vec<P> {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let (mut ox, mut oy) = (0.0, 0.0);
        points
            .iter()
            .map(|p| {
                ox = ox * 0.8 + rng.next() * amp * 0.6;
                oy = oy * 0.8 + rng.next() * amp * 0.6;
                [p[0] + ox, p[1] + oy]
            })
            .collect()
    }

    fn polyline(corners: &[P], per_side: usize) -> Vec<P> {
        let mut out = Vec::new();
        for i in 0..corners.len() - 1 {
            let (a, b) = (corners[i], corners[i + 1]);
            for k in 0..per_side {
                let t = k as f64 / per_side as f64;
                out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
            }
        }
        out.push(corners[corners.len() - 1]);
        out
    }

    /// Contorno fechado começando no meio do primeiro lado, com sobra no fim.
    fn closed_poly(corners: &[P], overshoot: f64) -> Vec<P> {
        let n = corners.len();
        let mid = |a: P, b: P, t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        let mut c = vec![mid(corners[0], corners[1], 0.4)];
        c.extend((1..=n).map(|i| corners[i % n]));
        c.push(mid(corners[0], corners[1], 0.4 + overshoot));
        polyline(&c, 25)
    }

    fn ellipse_pts(c: P, r: P, angle: f64, start: f64, turns: f64) -> Vec<P> {
        let (s, co) = angle.sin_cos();
        (0..=150)
            .map(|i| {
                let t = start + i as f64 / 150.0 * TAU * turns;
                let (x, y) = (r[0] * t.cos(), r[1] * t.sin());
                [c[0] + x * co - y * s, c[1] + x * s + y * co]
            })
            .collect()
    }

    fn rotate(pts: &[P], c: P, a: f64) -> Vec<P> {
        let (s, co) = a.sin_cos();
        pts.iter()
            .map(|p| {
                let q = sub(*p, c);
                [c[0] + q[0] * co - q[1] * s, c[1] + q[0] * s + q[1] * co]
            })
            .collect()
    }

    fn name(pts: &[P]) -> &'static str {
        detect(pts).map_or("nada", |s| s.name())
    }

    #[test]
    fn line() {
        for seed in 0..10 {
            let pts = hand(&polyline(&[[0.0, 0.0], [200.0, 30.0]], 60), 1.5, seed);
            assert!(matches!(detect(&pts), Some(QuickShape::Line(..))), "{seed}");
        }
    }

    #[test]
    fn circles() {
        for seed in 0..20 {
            let r = 40.0 + seed as f64 * 6.0;
            let start = seed as f64 * 0.7;
            let pts = hand(
                &ellipse_pts([300.0, 300.0], [r, r * 0.96], 0.3, start, 1.06),
                2.5,
                seed,
            );
            let s = detect(&pts);
            assert_eq!(
                s.as_ref().map(|s| s.name()),
                Some("círculo"),
                "{seed} {s:?}"
            );
            if let Some(QuickShape::Ellipse { center, radii, .. }) = s {
                assert!(dist(center, [300.0, 300.0]) < 5.0, "{seed} {center:?}");
                assert!((radii[0] - r * 0.98).abs() < r * 0.08, "{seed} {radii:?}");
            }
        }
    }

    #[test]
    fn ellipses_rotated() {
        for seed in 0..20 {
            let angle = seed as f64 * 0.37 - 1.2;
            let pts = hand(
                &ellipse_pts([250.0, 200.0], [140.0, 70.0], angle, seed as f64, 1.05),
                2.5,
                seed,
            );
            match detect(&pts) {
                Some(QuickShape::Ellipse {
                    center,
                    radii,
                    angle: a,
                }) => {
                    assert!(dist(center, [250.0, 200.0]) < 6.0, "{seed}");
                    assert!((radii[0] - 140.0).abs() < 10.0, "{seed} {radii:?}");
                    assert!((radii[1] - 70.0).abs() < 8.0, "{seed} {radii:?}");
                    let d = (a - angle).rem_euclid(PI);
                    assert!(d.min(PI - d) < 0.12, "{seed} {a} {angle}");
                }
                s => panic!("{seed} {s:?}"),
            }
        }
    }

    #[test]
    fn rectangles_and_squares() {
        for seed in 0..20 {
            let (w, h) = (180.0, 90.0 + (seed % 3) as f64 * 15.0);
            let rect = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]];
            let angle = if seed % 2 == 0 {
                0.0
            } else {
                0.25 + seed as f64 * 0.05
            };
            let pts = rotate(
                &hand(&closed_poly(&rect, 0.15), 2.0, seed),
                [w / 2.0, h / 2.0],
                angle,
            );
            let s = detect(&pts);
            assert_eq!(
                s.as_ref().map(|s| s.name()),
                Some("retângulo"),
                "{seed} {s:?}"
            );
            if let Some(QuickShape::Polygon { points, .. }) = s {
                let (a, b) = (dist(points[0], points[1]), dist(points[1], points[2]));
                let (long, short) = (a.max(b), a.min(b));
                assert!(
                    (long - w).abs() < 10.0 && (short - h).abs() < 10.0,
                    "{seed} {a} {b}"
                );
            }

            let sq = [[0.0, 0.0], [120.0, 0.0], [120.0, 124.0], [0.0, 124.0]];
            let pts = rotate(
                &hand(&closed_poly(&sq, -0.05), 2.0, seed + 100),
                [60.0, 60.0],
                angle,
            );
            assert_eq!(name(&pts), "quadrado", "{seed} {:?}", detect(&pts));
        }
    }

    #[test]
    fn straight_rectangle_is_axis_aligned() {
        let rect = [[0.0, 0.0], [160.0, 3.0], [158.0, 100.0], [-2.0, 98.0]];
        let pts = hand(&closed_poly(&rect, 0.1), 1.5, 7);
        match detect(&pts) {
            Some(QuickShape::Polygon {
                points,
                closed: true,
            }) => {
                assert!((points[0][1] - points[1][1]).abs() < 1e-6, "{points:?}");
            }
            s => panic!("{s:?}"),
        }
    }

    #[test]
    fn triangles() {
        for seed in 0..20 {
            let tri = match seed % 3 {
                0 => [[0.0, 150.0], [90.0, 0.0], [180.0, 150.0]],
                1 => [[0.0, 0.0], [200.0, 20.0], [60.0, 120.0]],
                _ => [[0.0, 0.0], [0.0, 160.0], [140.0, 160.0]],
            };
            let angle = seed as f64 * 0.3;
            let pts = rotate(
                &hand(&closed_poly(&tri, 0.1), 2.0, seed),
                [90.0, 90.0],
                angle,
            );
            let s = detect(&pts);
            assert_eq!(
                s.as_ref().map(|s| s.name()),
                Some("triângulo"),
                "{seed} {s:?}"
            );
            if let Some(QuickShape::Polygon { points, .. }) = s {
                let want = rotate(&tri, [90.0, 90.0], angle);
                for w in &want {
                    let d = points.iter().map(|p| dist(*p, *w)).fold(f64::MAX, f64::min);
                    assert!(d < 14.0, "{seed} {d} {points:?}");
                }
            }
        }
    }

    #[test]
    fn rounded_hand_square_is_not_ellipse() {
        // Quadrado com cantos arredondados, como sai à mão rápida.
        let mut pts = Vec::new();
        let (r, h) = (12.0, 60.0);
        for q in 0..4 {
            let base = q as f64 * PI / 2.0;
            let c = rotate(&[[h - r, h - r]], [0.0, 0.0], base)[0];
            for i in 0..=10 {
                let a = base + i as f64 / 10.0 * PI / 2.0;
                pts.push([c[0] + r * a.cos(), c[1] + r * a.sin()]);
            }
            let n = rotate(&[[-(h - r), h]], [0.0, 0.0], base)[0];
            let last = *pts.last().unwrap();
            for i in 1..20 {
                let t = i as f64 / 20.0;
                pts.push([
                    last[0] + (n[0] - last[0]) * t,
                    last[1] + (n[1] - last[1]) * t,
                ]);
            }
        }
        pts.extend(pts.clone().into_iter().take(8));
        let pts = hand(&pts, 1.5, 3);
        assert_eq!(name(&pts), "quadrado", "{:?}", detect(&pts));
    }

    #[test]
    fn open_shapes() {
        let arc: Vec<P> = (0..=40)
            .map(|i| {
                let t = i as f64 / 40.0 * PI;
                [100.0 + 50.0 * t.cos(), 100.0 - 50.0 * t.sin()]
            })
            .collect();
        let s = detect(&arc).unwrap();
        assert!(matches!(s, QuickShape::Arc { .. }), "{s:?}");
        let pts = s.sample(2.0);
        assert!(
            pts.iter()
                .any(|p| (p[0] - 100.0).abs() < 3.0 && (p[1] - 50.0).abs() < 3.0)
        );

        let v = hand(
            &polyline(&[[0.0, 0.0], [80.0, 150.0], [160.0, 0.0]], 40),
            1.5,
            2,
        );
        assert!(
            matches!(detect(&v), Some(QuickShape::Polygon { closed: false, ref points }) if points.len() == 3),
            "{:?}",
            detect(&v)
        );
    }

    #[test]
    fn scribble_is_not_a_shape() {
        let mut rng = Rng(42);
        let mut p = [100.0, 100.0];
        let pts: Vec<P> = (0..200)
            .map(|_| {
                p = [p[0] + rng.next() * 25.0, p[1] + rng.next() * 25.0];
                p
            })
            .collect();
        let s = detect(&pts);
        assert!(!matches!(s, Some(QuickShape::Line(..))), "{s:?}");
    }

    /// Muitos traços com mais tremor, começos em lugares diferentes, sobras e lacunas no fim:
    /// cada tipo precisa acertar quase sempre, e rápido.
    #[test]
    fn stress_accuracy() {
        let t0 = std::time::Instant::now();
        let mut total = 0;
        let mut fails: Vec<String> = Vec::new();
        for seed in 0..60u64 {
            let amp = 2.0 + (seed % 4) as f64;
            let ang = (seed as f64 * 0.61).rem_euclid(PI);
            let tail = [-0.12, 0.0, 0.1, 0.25][(seed % 4) as usize];
            let c = [200.0, 200.0];
            let cases: Vec<(&str, Vec<P>)> = vec![
                (
                    "círculo",
                    ellipse_pts(c, [90.0, 87.0], 0.0, seed as f64, 1.0 + tail * 0.4),
                ),
                (
                    "elipse",
                    ellipse_pts(c, [130.0, 60.0], ang, seed as f64, 1.0 + tail * 0.4),
                ),
                (
                    "quadrado",
                    rotate(
                        &closed_poly(
                            &[[0.0, 0.0], [150.0, 0.0], [150.0, 150.0], [0.0, 150.0]],
                            tail,
                        ),
                        [75.0, 75.0],
                        ang,
                    ),
                ),
                (
                    "retângulo",
                    rotate(
                        &closed_poly(
                            &[[0.0, 0.0], [220.0, 0.0], [220.0, 100.0], [0.0, 100.0]],
                            tail,
                        ),
                        [110.0, 50.0],
                        ang,
                    ),
                ),
                (
                    "triângulo",
                    rotate(
                        &closed_poly(&[[0.0, 160.0], [100.0, 0.0], [190.0, 150.0]], tail),
                        [95.0, 100.0],
                        ang,
                    ),
                ),
            ];
            for (want, pts) in cases {
                total += 1;
                let got = name(&hand(&pts, amp, seed * 7 + total as u64));
                if got != want {
                    fails.push(format!("{seed}: {want} -> {got}"));
                }
            }
        }
        assert!(
            fails.len() * 20 <= total,
            "{} de {total}: {fails:?}",
            fails.len()
        );
        assert!(t0.elapsed().as_secs_f64() < 5.0, "{:?}", t0.elapsed());
    }

    #[test]
    fn vector_paths() {
        let e = QuickShape::Ellipse {
            center: [10.0, 10.0],
            radii: [5.0, 3.0],
            angle: 0.0,
        };
        let sp = e.to_subpath();
        assert!(sp.closed && sp.anchors.len() == 4);
        let r = QuickShape::Polygon {
            points: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
            closed: true,
        };
        assert_eq!(r.to_subpath().anchors.len(), 3);
    }
}
