//! Borda da camada (a "Borda" das propriedades de camada do Clip Studio): um contorno de cor
//! sólida em volta da tinta, desenhado embaixo dela. Não mexe nos pixels: entra na composição,
//! e só vira tinta ao mesclar ou exportar em PSD.

use photocraft_geom::Rect;
use photocraft_raster::Surface;

use crate::document::Document;

/// Espessura e cor da borda.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    /// Em pixels.
    pub width: f32,
    pub color: [f32; 3],
}

/// Mais que isso deixa a composição lenta demais a cada traço.
pub const MAX_WIDTH: f32 = 64.0;

impl Border {
    /// Quanto a borda passa da tinta, em pixels inteiros.
    pub fn pad(&self) -> i32 {
        self.width.clamp(0.0, MAX_WIDTH).ceil() as i32 + 1
    }
}

const FAR: f32 = 1e20;

/// Transformada de distância 1D (Felzenszwalb–Huttenlocher), distâncias ao quadrado.
fn edt_1d(f: &[f32], out: &mut [f32], v: &mut [usize], z: &mut [f32]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0;
    v[0] = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    let cross = |q: usize, p: usize| {
        ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * (q - p) as f32)
    };
    for q in 1..n {
        let mut s = cross(q, v[k]);
        // z[0] é -∞, então k nunca passa de 0 para baixo.
        while s <= z[k] {
            k -= 1;
            s = cross(q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f32::INFINITY;
    }
    k = 0;
    for (q, o) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let p = v[k];
        let d = q as f32 - p as f32;
        *o = d * d + f[p];
    }
}

/// Alfa da borda para cada pixel de uma imagem `w`×`h`, a partir do alfa da tinta: cheio até
/// `radius` pixels da tinta, com meio pixel de suavização na beirada.
pub fn border_alpha(alpha: &[f32], w: usize, h: usize, radius: f32) -> Vec<f32> {
    let n = w * h;
    let mut d: Vec<f32> = alpha
        .iter()
        .map(|a| if *a >= 0.5 { 0.0 } else { FAR })
        .collect();
    let side = w.max(h);
    let (mut f, mut out) = (vec![0.0f32; side], vec![0.0f32; side]);
    let (mut v, mut z) = (vec![0usize; side], vec![0.0f32; side + 1]);
    for x in 0..w {
        for y in 0..h {
            f[y] = d[y * w + x];
        }
        edt_1d(&f[..h], &mut out[..h], &mut v, &mut z);
        for y in 0..h {
            d[y * w + x] = out[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&d[y * w..(y + 1) * w]);
        edt_1d(&f[..w], &mut out[..w], &mut v, &mut z);
        d[y * w..(y + 1) * w].copy_from_slice(&out[..w]);
    }
    let mut res = vec![0.0f32; n];
    for (r, dd) in res.iter_mut().zip(&d) {
        *r = (radius + 0.5 - dd.sqrt()).clamp(0.0, 1.0);
    }
    res
}

/// Lê `r` de `surface` (dentro de `bounds`) já com a borda por baixo da tinta.
pub fn read_with_border(
    surface: &Surface,
    b: &Border,
    r: Rect,
    bounds: Rect,
    row: &mut [[f32; 4]],
) {
    let e = r.inflate(b.pad()).intersect(&bounds);
    let (ew, eh) = (e.width() as usize, e.height() as usize);
    let mut px = vec![[0.0f32; 4]; ew * eh];
    surface.read_rgba_into(e, &mut px);
    let alpha: Vec<f32> = px.iter().map(|p| p[3]).collect();
    let ba = border_alpha(&alpha, ew, eh, b.width.clamp(0.0, MAX_WIDTH));
    let rw = r.width() as usize;
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            let k = (y - e.y0) as usize * ew + (x - e.x0) as usize;
            let (s, a) = (px[k], ba[k]);
            let out_a = s[3] + a * (1.0 - s[3]);
            let o = &mut row[(y - r.y0) as usize * rw + (x - r.x0) as usize];
            *o = if out_a <= 0.0 {
                [0.0; 4]
            } else {
                let mix = |c: usize| (s[c] * s[3] + b.color[c] * a * (1.0 - s[3])) / out_a;
                [mix(0), mix(1), mix(2), out_a]
            };
        }
    }
}

impl Document {
    /// Quanto as bordas das camadas passam da tinta (para redesenhar a área certa).
    pub fn border_pad(&self) -> i32 {
        self.layers
            .iter()
            .filter_map(|l| l.border.map(|b| b.pad()))
            .max()
            .unwrap_or(0)
    }
}

/// `surface` com a borda pintada nela (para mesclar ou exportar).
pub fn bake(surface: &Surface, b: &Border, bounds: Rect) -> Surface {
    let r = surface.content_bounds().inflate(b.pad()).intersect(&bounds);
    let mut out = surface.clone();
    if r.is_empty() {
        return out;
    }
    let mut row = vec![[0.0f32; 4]; r.width() as usize * r.height() as usize];
    read_with_border(surface, b, r, bounds, &mut row);
    out.write_region(r, row.as_flattened());
    out.prune();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_color::PixelFormat;

    #[test]
    fn border_surrounds_the_paint_at_the_right_distance() {
        // Um ponto no meio de uma imagem 21×21.
        let mut a = vec![0.0; 21 * 21];
        a[10 * 21 + 10] = 1.0;
        let b = border_alpha(&a, 21, 21, 4.0);
        assert_eq!(b[10 * 21 + 13], 1.0);
        assert_eq!(b[10 * 21 + 14], 0.5);
        assert_eq!(b[10 * 21 + 16], 0.0);
        // Na diagonal, 3·√2 ≈ 4.24: meio coberto.
        assert!(b[13 * 21 + 13] > 0.0 && b[13 * 21 + 13] < 1.0);

        let mut s = Surface::new(PixelFormat::RGBA8);
        s.fill_rect(Rect::new(10, 10, 20, 20), &[1.0, 1.0, 1.0, 1.0]);
        let border = Border {
            width: 3.0,
            color: [1.0, 0.0, 0.0],
        };
        let out = bake(&s, &border, Rect::new(0, 0, 40, 40));
        assert_eq!(out.rgba(15, 15)[1], 1.0);
        let p = out.rgba(8, 15);
        assert!(p[0] > 0.99 && p[1] < 0.01 && p[3] > 0.99);
        assert_eq!(out.rgba(5, 15)[3], 0.0);
    }
}
