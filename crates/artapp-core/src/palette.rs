//! Paleta a partir de uma foto (as "paletas de câmera e de arquivo" do Procreate): as cores
//! principais da imagem por k-médias, da mais escura para a mais clara.

/// Até `n` cores que resumem `rgba` (RGBA 8 bits). Pixels quase transparentes ficam de fora.
pub fn from_pixels(rgba: &[u8], n: usize) -> Vec<[u8; 3]> {
    let all: Vec<[f32; 3]> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] >= 128)
        .map(|p| [f32::from(p[0]), f32::from(p[1]), f32::from(p[2])])
        .collect();
    if all.is_empty() || n == 0 {
        return Vec::new();
    }
    // Uma amostra espalhada basta (e deixa rápido em fotos grandes).
    let step = (all.len() / 20_000).max(1);
    let samples: Vec<[f32; 3]> = all.iter().step_by(step).copied().collect();
    let d2 = |a: [f32; 3], b: [f32; 3]| {
        (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
    };
    // Começo: a primeira cor e depois sempre a mais longe das já escolhidas.
    let mut centers = vec![samples[0]];
    while centers.len() < n {
        let (far, dist) = samples
            .iter()
            .map(|s| centers.iter().map(|c| d2(*s, *c)).fold(f32::MAX, f32::min))
            .enumerate()
            .fold((0, 0.0f32), |a, (i, d)| if d > a.1 { (i, d) } else { a });
        if dist < 1.0 {
            break; // Menos cores distintas que `n`.
        }
        centers.push(samples[far]);
    }
    for _ in 0..12 {
        let mut sum = vec![[0.0f64; 4]; centers.len()];
        for s in &samples {
            let j = centers
                .iter()
                .enumerate()
                .map(|(j, c)| (j, d2(*s, *c)))
                .fold((0, f32::MAX), |a, b| if b.1 < a.1 { b } else { a })
                .0;
            for c in 0..3 {
                sum[j][c] += f64::from(s[c]);
            }
            sum[j][3] += 1.0;
        }
        for (c, s) in centers.iter_mut().zip(&sum) {
            if s[3] > 0.0 {
                *c = [0, 1, 2].map(|k| (s[k] / s[3]) as f32);
            }
        }
    }
    let mut out: Vec<[u8; 3]> = centers
        .iter()
        .map(|c| c.map(|v| v.round().clamp(0.0, 255.0) as u8))
        .collect();
    out.sort_by_key(|c| 299 * u32::from(c[0]) + 587 * u32::from(c[1]) + 114 * u32::from(c[2]));
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_main_colors() {
        let mut px = Vec::new();
        for i in 0..300 {
            let c = match i % 3 {
                0 => [250, 10, 10, 255],
                1 => [10, 240, 20, 255],
                _ => [5, 5, 200, 255],
            };
            px.extend_from_slice(&c);
        }
        px.extend_from_slice(&[0, 0, 0, 0]);
        let p = from_pixels(&px, 3);
        assert_eq!(p.len(), 3);
        assert!(
            p.contains(&[250, 10, 10]) && p.contains(&[10, 240, 20]) && p.contains(&[5, 5, 200])
        );
        assert_eq!(from_pixels(&px, 8).len(), 3);
    }
}
