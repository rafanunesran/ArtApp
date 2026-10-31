//! Curva de pressão da caneta: pontos de controle entre (0, 0) e (1, 1), ligados por uma curva
//! suave que nunca desce (Hermite monotônica), como o ajuste de pressão do Procreate.

/// Mais pontos que isso deixam a curva difícil de mexer com o dedo.
pub const MAX_POINTS: usize = 4;

/// Arruma os pontos do meio: dentro de [0, 1], em ordem de x, sem x repetido e nunca descendo.
pub fn sanitize(points: &mut Vec<[f32; 2]>) {
    points.retain(|p| p[0].is_finite() && p[1].is_finite());
    for p in points.iter_mut() {
        p[0] = p[0].clamp(0.01, 0.99);
        p[1] = p[1].clamp(0.0, 1.0);
    }
    points.sort_by(|a, b| a[0].total_cmp(&b[0]));
    points.dedup_by(|b, a| (b[0] - a[0]).abs() < 0.02);
    points.truncate(MAX_POINTS);
    let mut low = 0.0f32;
    for p in points.iter_mut() {
        p[1] = p[1].max(low);
        low = p[1];
    }
}

/// Aplica a curva com os pontos do meio `points` (já arrumados) à pressão `x`.
pub fn apply(points: &[[f32; 2]], x: f32) -> f32 {
    if points.is_empty() {
        return x.clamp(0.0, 1.0);
    }
    let x = x.clamp(0.0, 1.0);
    let mut k = Vec::with_capacity(points.len() + 2);
    k.push([0.0, 0.0]);
    k.extend_from_slice(points);
    k.push([1.0, 1.0]);
    let n = k.len();
    let slope = |i: usize| (k[i + 1][1] - k[i][1]) / (k[i + 1][0] - k[i][0]).max(1e-6);
    // Tangentes de Fritsch–Carlson: a curva não passa dos pontos nem volta para trás.
    let mut m = vec![0.0f32; n];
    m[0] = slope(0);
    m[n - 1] = slope(n - 2);
    for (i, mi) in m.iter_mut().enumerate().take(n - 1).skip(1) {
        let (a, b) = (slope(i - 1), slope(i));
        let (ha, hb) = (k[i][0] - k[i - 1][0], k[i + 1][0] - k[i][0]);
        *mi = if a * b <= 0.0 {
            0.0
        } else {
            3.0 * (ha + hb) / ((2.0 * hb + ha) / a + (hb + 2.0 * ha) / b)
        };
    }
    let i = (0..n - 1).find(|&i| x <= k[i + 1][0]).unwrap_or(n - 2);
    let h = (k[i + 1][0] - k[i][0]).max(1e-6);
    let t = (x - k[i][0]) / h;
    let (t2, t3) = (t * t, t * t * t);
    let y = (2.0 * t3 - 3.0 * t2 + 1.0) * k[i][1]
        + (t3 - 2.0 * t2 + t) * h * m[i]
        + (-2.0 * t3 + 3.0 * t2) * k[i + 1][1]
        + (t3 - t2) * h * m[i + 1];
    y.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_curve_is_identity_and_points_are_hit() {
        assert_eq!(apply(&[], 0.3), 0.3);
        let mut p = vec![[0.5, 0.2], [0.25, 0.9]];
        sanitize(&mut p);
        // O segundo ponto não pode ficar abaixo do primeiro.
        assert_eq!(p, vec![[0.25, 0.9], [0.5, 0.9]]);
        assert!((apply(&p, 0.25) - 0.9).abs() < 1e-4);
        let mut last = 0.0;
        for i in 0..=100 {
            let y = apply(&p, i as f32 / 100.0);
            assert!(y + 1e-5 >= last, "a curva desceu em {i}");
            last = y;
        }
        assert!((apply(&p, 1.0) - 1.0).abs() < 1e-5);
    }
}
