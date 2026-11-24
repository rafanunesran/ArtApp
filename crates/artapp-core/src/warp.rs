//! Deformar com malha (o "Warp" do Procreate e a "Malha" do Clip Studio): uma grade 4×4 de
//! pontos de controle faz uma superfície de Bézier bicúbica. Os cantos ficam presos aos pontos
//! dos cantos; os do meio puxam a imagem como ímãs.

use photocraft_color::PixelFormat;
use photocraft_geom::Rect;
use photocraft_raster::Surface;

use crate::transform::{Homography, Interp, Quad};

/// Pontos por lado da grade.
pub const SIDE: usize = 4;

/// Pontos de controle, linha por linha, de cima à esquerda, em pixels do documento.
pub type Grid = [[f64; 2]; SIDE * SIDE];

/// Grade inicial que reproduz o quadrilátero (exata para retângulos e paralelogramos).
pub fn grid_from_quad(src: Rect, quad: &Quad) -> Option<Grid> {
    let s = [
        f64::from(src.x0),
        f64::from(src.y0),
        f64::from(src.x1),
        f64::from(src.y1),
    ];
    let h = Homography::rect_to_quad(s, *quad)?;
    let mut g = [[0.0; 2]; SIDE * SIDE];
    for j in 0..SIDE {
        for i in 0..SIDE {
            let (u, v) = (i as f64 / 3.0, j as f64 / 3.0);
            let (x, y) = h.apply(s[0] + u * (s[2] - s[0]), s[1] + v * (s[3] - s[1]));
            g[j * SIDE + i] = [x, y];
        }
    }
    Some(g)
}

fn bernstein(t: f64) -> [f64; 4] {
    let s = 1.0 - t;
    [s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t]
}

/// Ponto da superfície em (u, v), ambos em 0..1.
pub fn point(g: &Grid, u: f64, v: f64) -> [f64; 2] {
    let (bu, bv) = (bernstein(u), bernstein(v));
    let mut p = [0.0, 0.0];
    for j in 0..SIDE {
        for i in 0..SIDE {
            let w = bu[i] * bv[j];
            p[0] += w * g[j * SIDE + i][0];
            p[1] += w * g[j * SIDE + i][1];
        }
    }
    p
}

/// Quantos triângulos por lado ao desenhar a imagem deformada.
const STEPS: usize = 32;

/// Desenha `src` (a área `rect` dela) deformada pela grade, cortada em `clip`.
pub fn warp_surface(src: &Surface, rect: Rect, g: &Grid, interp: Interp, clip: Rect) -> Surface {
    let mut out = Surface::new(PixelFormat::RGBA8);
    let (sw, sh) = (rect.width() as usize, rect.height() as usize);
    if sw == 0 || sh == 0 {
        return out;
    }
    let mut px = vec![[0.0f32; 4]; sw * sh];
    src.read_rgba_into(rect, &mut px);
    // Interpolar com a cor pré-multiplicada evita a franja escura nas bordas.
    for p in &mut px {
        for c in 0..3 {
            p[c] *= p[3];
        }
    }
    let n = STEPS;
    let mut dst = Vec::with_capacity((n + 1) * (n + 1));
    for j in 0..=n {
        for i in 0..=n {
            dst.push(point(g, i as f64 / n as f64, j as f64 / n as f64));
        }
    }
    let (minx, miny, maxx, maxy) = dst.iter().fold(
        (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
        |(a, b, c, d), p| (a.min(p[0]), b.min(p[1]), c.max(p[0]), d.max(p[1])),
    );
    let area = Rect::new(
        minx.floor() as i32,
        miny.floor() as i32,
        maxx.ceil() as i32 + 1,
        maxy.ceil() as i32 + 1,
    )
    .intersect(&clip);
    if area.is_empty() {
        return out;
    }
    let (aw, ah) = (area.width() as usize, area.height() as usize);
    let mut buf = vec![0.0f32; aw * ah * 4];
    let sample = |x: f64, y: f64| -> [f32; 4] {
        // (x, y) em pixels da origem, relativo a `rect`, centro do pixel em +0.5.
        let get = |ix: i64, iy: i64| -> [f32; 4] {
            if ix < 0 || iy < 0 || ix >= sw as i64 || iy >= sh as i64 {
                [0.0; 4]
            } else {
                px[iy as usize * sw + ix as usize]
            }
        };
        if interp == Interp::Nearest {
            return get(x.floor() as i64, y.floor() as i64);
        }
        let (fx, fy) = (x - 0.5, y - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = ((fx - x0) as f32, (fy - y0) as f32);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let (a, b, c, d) = (
            get(x0, y0),
            get(x0 + 1, y0),
            get(x0, y0 + 1),
            get(x0 + 1, y0 + 1),
        );
        std::array::from_fn(|k| {
            (a[k] * (1.0 - tx) + b[k] * tx) * (1.0 - ty) + (c[k] * (1.0 - tx) + d[k] * tx) * ty
        })
    };
    let at = |i: usize, j: usize| j * (n + 1) + i;
    let srcp = |i: usize, j: usize| {
        [
            i as f64 / n as f64 * sw as f64,
            j as f64 / n as f64 * sh as f64,
        ]
    };
    for j in 0..n {
        for i in 0..n {
            for tri in [
                [(i, j), (i + 1, j), (i + 1, j + 1)],
                [(i, j), (i + 1, j + 1), (i, j + 1)],
            ] {
                let d = tri.map(|(a, b)| dst[at(a, b)]);
                let s = tri.map(|(a, b)| srcp(a, b));
                fill_triangle(&d, &s, area, &mut buf, aw, &sample);
            }
        }
    }
    // De volta para a cor sem pré-multiplicar.
    for p in buf.as_chunks_mut::<4>().0 {
        if p[3] > 1e-6 {
            for c in 0..3 {
                p[c] = (p[c] / p[3]).clamp(0.0, 1.0);
            }
        }
    }
    out.write_region(area, &buf);
    out.prune();
    out
}

/// Pinta os pixels cujo centro cai no triângulo `d`, lendo a origem no triângulo `s`.
fn fill_triangle(
    d: &[[f64; 2]; 3],
    s: &[[f64; 2]; 3],
    area: Rect,
    buf: &mut [f32],
    aw: usize,
    sample: &impl Fn(f64, f64) -> [f32; 4],
) {
    let det = (d[1][0] - d[0][0]) * (d[2][1] - d[0][1]) - (d[2][0] - d[0][0]) * (d[1][1] - d[0][1]);
    if det.abs() < 1e-9 {
        return;
    }
    let x0 = (d.iter().map(|p| p[0]).fold(f64::MAX, f64::min).floor() as i32).max(area.x0);
    let x1 = (d.iter().map(|p| p[0]).fold(f64::MIN, f64::max).ceil() as i32).min(area.x1);
    let y0 = (d.iter().map(|p| p[1]).fold(f64::MAX, f64::min).floor() as i32).max(area.y0);
    let y1 = (d.iter().map(|p| p[1]).fold(f64::MIN, f64::max).ceil() as i32).min(area.y1);
    // Uma folguinha nas bordas para os triângulos vizinhos não deixarem fresta.
    const EPS: f64 = -1e-6;
    for y in y0..y1 {
        for x in x0..x1 {
            let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let w1 =
                ((px - d[0][0]) * (d[2][1] - d[0][1]) - (d[2][0] - d[0][0]) * (py - d[0][1])) / det;
            let w2 =
                ((d[1][0] - d[0][0]) * (py - d[0][1]) - (px - d[0][0]) * (d[1][1] - d[0][1])) / det;
            let w0 = 1.0 - w1 - w2;
            if w0 < EPS || w1 < EPS || w2 < EPS {
                continue;
            }
            let sx = w0 * s[0][0] + w1 * s[1][0] + w2 * s[2][0];
            let sy = w0 * s[0][1] + w1 * s[1][1] + w2 * s[2][1];
            let c = sample(sx, sy);
            let k = ((y - area.y0) as usize * aw + (x - area.x0) as usize) * 4;
            buf[k..k + 4].copy_from_slice(&c);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn identity_grid_keeps_the_image_and_a_pull_moves_it() {
        let mut s = Surface::new(PixelFormat::RGBA8);
        s.fill_rect(Rect::new(0, 0, 20, 20), &[0.0, 1.0, 0.0, 1.0]);
        let r = Rect::new(0, 0, 20, 20);
        let q = [[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [0.0, 20.0]];
        let g = grid_from_quad(r, &q).unwrap();
        let c = point(&g, 0.5, 0.5);
        assert!((c[0] - 10.0).abs() < 1e-9 && (c[1] - 10.0).abs() < 1e-9);
        let clip = Rect::new(0, 0, 100, 100);
        let out = warp_surface(&s, r, &g, Interp::Bilinear, clip);
        assert!(out.rgba(10, 10)[1] > 0.99);
        assert!(out.rgba(25, 10)[3] < 0.01);
        // Puxa o canto de baixo à direita para longe: a imagem chega lá.
        let mut g2 = g;
        g2[15] = [60.0, 60.0];
        let out = warp_surface(&s, r, &g2, Interp::Bilinear, clip);
        let far = point(&g2, 0.9, 0.9);
        assert!(far[0] > 30.0);
        assert!(out.rgba(far[0] as i32, far[1] as i32)[3] > 0.5);
        assert!(out.rgba(5, 5)[1] > 0.99);
    }
}
