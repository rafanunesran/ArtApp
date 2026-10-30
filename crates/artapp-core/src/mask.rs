//! Operações em máscaras do tamanho da tela: crescer e encolher (por distância euclidiana exata) e
//! suavizar a borda. Usadas por expandir/contrair/suavizar a seleção e pelo "fechar lacunas" e
//! "expandir área" do balde.

/// Distância ao quadrado de cada pixel até o pixel ligado mais próximo (`f32::INFINITY` se não há
/// nenhum). Transformada de Felzenszwalb–Huttenlocher, separável: linhas e depois colunas.
pub fn distance_sq(on: &[bool], w: usize, h: usize) -> Vec<f32> {
    const INF: f32 = 1e20;
    let mut d: Vec<f32> = on.iter().map(|&b| if b { 0.0 } else { INF }).collect();
    let n = w.max(h);
    let mut f = vec![0.0f32; n];
    let mut out = vec![0.0f32; n];
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f32; n + 1];
    for y in 0..h {
        f[..w].copy_from_slice(&d[y * w..y * w + w]);
        edt_1d(&f[..w], &mut out[..w], &mut v, &mut z);
        d[y * w..y * w + w].copy_from_slice(&out[..w]);
    }
    for x in 0..w {
        for y in 0..h {
            f[y] = d[y * w + x];
        }
        edt_1d(&f[..h], &mut out[..h], &mut v, &mut z);
        for y in 0..h {
            d[y * w + x] = out[y];
        }
    }
    for v in &mut d {
        if *v >= INF * 0.5 {
            *v = f32::INFINITY;
        }
    }
    d
}

/// Transformada de distância 1D (envelope inferior de parábolas).
fn edt_1d(f: &[f32], d: &mut [f32], v: &mut [usize], z: &mut [f32]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        let qf = q as f32;
        let mut s;
        loop {
            let pf = v[k] as f32;
            s = ((f[q] + qf * qf) - (f[v[k]] + pf * pf)) / (2.0 * qf - 2.0 * pf);
            // z[0] é -∞, então k nunca passa de 0 para baixo.
            if s <= z[k] {
                k -= 1;
            } else {
                break;
            }
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f32::INFINITY;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate() {
        let qf = q as f32;
        while z[k + 1] < qf {
            k += 1;
        }
        let p = v[k] as f32;
        *out = (qf - p) * (qf - p) + f[v[k]];
    }
}

/// Cresce (`r > 0`) ou encolhe (`r < 0`) a máscara `r` pixels, com borda redonda.
pub fn grow(on: &[bool], w: usize, h: usize, r: f32) -> Vec<bool> {
    if r == 0.0 {
        return on.to_vec();
    }
    let r2 = r * r;
    if r > 0.0 {
        distance_sq(on, w, h).into_iter().map(|d| d <= r2).collect()
    } else {
        let off: Vec<bool> = on.iter().map(|b| !b).collect();
        distance_sq(&off, w, h)
            .into_iter()
            .map(|d| d > r2)
            .collect()
    }
}

/// Desfoque de caixa separável, três passadas (quase gaussiano), para suavizar a borda.
pub fn feather(m: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    if radius == 0 {
        return m.to_vec();
    }
    let mut a = m.to_vec();
    let mut b = vec![0.0f32; a.len()];
    let r = (radius as f32 / 1.7).ceil().max(1.0) as usize;
    for _ in 0..3 {
        box_rows(&a, &mut b, w, h, r);
        box_cols(&b, &mut a, w, h, r);
    }
    a
}

fn box_rows(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize) {
    let norm = 1.0 / (2 * r + 1) as f32;
    for y in 0..h {
        let row = &src[y * w..y * w + w];
        let at = |i: isize| row[i.clamp(0, w as isize - 1) as usize];
        let mut acc: f32 = (-(r as isize)..=r as isize).map(at).sum();
        for x in 0..w {
            dst[y * w + x] = acc * norm;
            acc += at(x as isize + r as isize + 1) - at(x as isize - r as isize);
        }
    }
}

fn box_cols(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize) {
    let norm = 1.0 / (2 * r + 1) as f32;
    for x in 0..w {
        let at = |i: isize| src[i.clamp(0, h as isize - 1) as usize * w + x];
        let mut acc: f32 = (-(r as isize)..=r as isize).map(at).sum();
        for y in 0..h {
            dst[y * w + x] = acc * norm;
            acc += at(y as isize + r as isize + 1) - at(y as isize - r as isize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_and_grow_are_round() {
        let (w, h) = (21, 21);
        let mut on = vec![false; w * h];
        on[10 * w + 10] = true;
        let d = distance_sq(&on, w, h);
        assert_eq!(d[10 * w + 13], 9.0);
        assert_eq!(d[13 * w + 14], 25.0);
        let g = grow(&on, w, h, 5.0);
        assert!(g[10 * w + 15] && g[13 * w + 14] && !g[14 * w + 14]);
        let s = grow(&g, w, h, -3.0);
        assert!(s[10 * w + 10] && s[10 * w + 12] && !s[10 * w + 14]);
    }

    #[test]
    fn feather_softens_an_edge() {
        let (w, h) = (40, 4);
        let m: Vec<f32> = (0..w * h)
            .map(|i| if i % w < 20 { 1.0 } else { 0.0 })
            .collect();
        let f = feather(&m, w, h, 6);
        assert!(f[19] > 0.3 && f[19] < 0.8);
        assert!(f[2] > 0.99 && f[38] < 0.01);
    }
}
