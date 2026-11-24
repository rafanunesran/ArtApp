//! Efeitos que o PhotoCraft não tem (os ajustes do Procreate): brilho difuso (bloom), falha
//! digital (glitch) e aberração cromática. Agem na camada ativa, dentro da seleção.

use photocraft_geom::Rect;

use crate::document::Document;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// Os claros (acima de `threshold`, 0..1) se espalham por `radius` px, com `intensity`.
    Bloom {
        radius: f32,
        intensity: f32,
        threshold: f32,
    },
    /// Faixas da imagem deslocadas e com as cores separadas; `amount` 0..1.
    Glitch { amount: f32, block: f32, seed: u32 },
    /// Vermelho e azul afastados a partir do centro, até `amount` px na borda.
    Aberration { amount: f32 },
}

fn premul(p: [f32; 4]) -> [f32; 4] {
    [p[0] * p[3], p[1] * p[3], p[2] * p[3], p[3]]
}

fn unpremul(p: [f32; 4]) -> [f32; 4] {
    if p[3] <= 1e-6 {
        return [0.0; 4];
    }
    let a = p[3].min(1.0);
    [
        (p[0] / a).clamp(0.0, 1.0),
        (p[1] / a).clamp(0.0, 1.0),
        (p[2] / a).clamp(0.0, 1.0),
        a,
    ]
}

/// Uma passada de média móvel de raio `r` nas linhas (ou colunas, com `stride` = largura).
fn box_pass(
    src: &[[f32; 4]],
    dst: &mut [[f32; 4]],
    len: usize,
    count: usize,
    step: usize,
    line: usize,
    r: usize,
) {
    let norm = 1.0 / (2 * r + 1) as f32;
    for l in 0..count {
        let base = l * line;
        let at = |i: usize| src[base + i * step];
        let mut acc = [0.0f32; 4];
        for k in 0..=r.min(len - 1) {
            let p = at(k);
            for c in 0..4 {
                acc[c] += p[c];
            }
        }
        for i in 0..len {
            dst[base + i * step] = acc.map(|v| v * norm);
            if i + r + 1 < len {
                let p = at(i + r + 1);
                for c in 0..4 {
                    acc[c] += p[c];
                }
            }
            if i >= r {
                let p = at(i - r);
                for c in 0..4 {
                    acc[c] -= p[c];
                }
            }
        }
    }
}

/// Desfoque parecido com o gaussiano (três médias móveis), em pixels pré-multiplicados.
pub fn blur(px: &mut [[f32; 4]], w: usize, h: usize, radius: f32) {
    let r = (radius / 3f32.sqrt()).round().max(1.0) as usize;
    let mut tmp = vec![[0.0f32; 4]; px.len()];
    for _ in 0..3 {
        box_pass(px, &mut tmp, w, h, 1, w, r);
        box_pass(&tmp, px, h, w, w, 1, r);
    }
}

fn sample(px: &[[f32; 4]], w: usize, h: usize, x: f32, y: f32) -> [f32; 4] {
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let mut o = [0.0f32; 4];
    for c in 0..4 {
        let top = px[y0 * w + x0][c] * (1.0 - fx) + px[y0 * w + x1][c] * fx;
        let bot = px[y1 * w + x0][c] * (1.0 - fx) + px[y1 * w + x1][c] * fx;
        o[c] = top * (1.0 - fy) + bot * fy;
    }
    o
}

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// O efeito aplicado a `px` (`w` × `h`, não pré-multiplicado).
pub fn render(e: &Effect, px: &[[f32; 4]], w: usize, h: usize) -> Vec<[f32; 4]> {
    if w == 0 || h == 0 {
        return px.to_vec();
    }
    let pm: Vec<[f32; 4]> = px.iter().map(|p| premul(*p)).collect();
    match *e {
        Effect::Bloom {
            radius,
            intensity,
            threshold,
        } => {
            let t = threshold.clamp(0.0, 0.99);
            let mut glow: Vec<[f32; 4]> = pm
                .iter()
                .map(|p| {
                    let a = p[3].max(1e-6);
                    let luma = (0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]) / a;
                    let k = ((luma - t) / (1.0 - t)).clamp(0.0, 1.0);
                    p.map(|v| v * k)
                })
                .collect();
            blur(&mut glow, w, h, radius.max(1.0));
            pm.iter()
                .zip(&glow)
                .map(|(p, g)| {
                    let g = g.map(|v| (v * intensity).clamp(0.0, 1.0));
                    unpremul([0, 1, 2, 3].map(|c| p[c] + g[c] - p[c] * g[c]))
                })
                .collect()
        }
        Effect::Aberration { amount } => {
            let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
            let reach = (cx * cx + cy * cy).sqrt().max(1.0);
            (0..w * h)
                .map(|i| {
                    let (x, y) = ((i % w) as f32 + 0.5, (i / w) as f32 + 0.5);
                    let (dx, dy) = ((x - cx) / reach * amount, (y - cy) / reach * amount);
                    let r = sample(&pm, w, h, x - 0.5 - dx, y - 0.5 - dy);
                    let g = pm[i];
                    let b = sample(&pm, w, h, x - 0.5 + dx, y - 0.5 + dy);
                    let a = r[3].max(g[3]).max(b[3]);
                    unpremul([r[0], g[1], b[2], a])
                })
                .collect()
        }
        Effect::Glitch {
            amount,
            block,
            seed,
        } => {
            let mut rng = Rng(seed.max(1).wrapping_mul(2_654_435_761) | 1);
            let amount = amount.clamp(0.0, 1.0);
            let mut out = px.to_vec();
            let mut y = 0;
            while y < h {
                let band = ((block.max(2.0) * (0.3 + rng.next() * 1.4)) as usize).max(1);
                let hit = rng.next() < amount;
                let shift = if hit {
                    ((rng.next() - 0.5) * 2.0 * amount * w as f32 * 0.15) as isize
                } else {
                    0
                };
                let split = if hit {
                    (rng.next() * 12.0 * amount) as isize + 1
                } else {
                    0
                };
                for yy in y..(y + band).min(h) {
                    let row = yy * w;
                    let at = |dx: isize, x: usize| {
                        let sx = (x as isize - dx).rem_euclid(w as isize) as usize;
                        pm[row + sx]
                    };
                    for x in 0..w {
                        if !hit {
                            continue;
                        }
                        let r = at(shift + split, x);
                        let g = at(shift, x);
                        let b = at(shift - split, x);
                        out[row + x] = unpremul([r[0], g[1], b[2], r[3].max(g[3]).max(b[3])]);
                    }
                }
                y += band;
            }
            out
        }
    }
}

impl Document {
    /// Aplica `e` na camada ativa raster (dentro da seleção, se houver).
    pub fn apply_effect(&mut self, e: &Effect) -> Rect {
        let Some(r) = self.edit_area(true) else {
            return Rect::EMPTY;
        };
        let Some(layer) = self.active_layer() else {
            return Rect::EMPTY;
        };
        let (w, h) = (r.width() as usize, r.height() as usize);
        let mut px = vec![[0.0f32; 4]; w * h];
        layer.surface.read_rgba_into(r, &mut px);
        let out = render(e, &px, w, h);
        let out = self.map_pixels(r, move |x, y, _| {
            out[(y - r.y0) as usize * w + (x - r.x0) as usize]
        });
        if let Some(l) = self.active_layer_mut() {
            l.surface.prune();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bloom_spreads_light_into_the_dark() {
        let (w, h) = (21, 21);
        let mut px = vec![[0.0f32, 0.0, 0.0, 1.0]; w * h];
        px[10 * w + 10] = [1.0, 1.0, 1.0, 1.0];
        let out = render(
            &Effect::Bloom {
                radius: 4.0,
                intensity: 4.0,
                threshold: 0.5,
            },
            &px,
            w,
            h,
        );
        assert!(out[10 * w + 12][0] > 0.01);
        assert_eq!(out[0][0], 0.0);
        assert!((out[10 * w + 10][0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn aberration_splits_red_and_blue_at_the_edge() {
        let (w, h) = (40, 10);
        let mut px = vec![[1.0f32, 1.0, 1.0, 1.0]; w * h];
        for y in 0..h {
            px[y * w + 35] = [0.0, 0.0, 0.0, 1.0];
        }
        let out = render(&Effect::Aberration { amount: 3.0 }, &px, w, h);
        let p = out[5 * w + 35];
        assert!(p[0] > 0.5 || p[2] > 0.5, "{p:?}");
        assert!(p[1] < 0.01);
    }

    #[test]
    fn glitch_keeps_the_size_and_zero_does_nothing() {
        let px: Vec<[f32; 4]> = (0..64).map(|i| [i as f32 / 64.0, 0.5, 0.2, 1.0]).collect();
        let out = render(
            &Effect::Glitch {
                amount: 0.0,
                block: 4.0,
                seed: 3,
            },
            &px,
            8,
            8,
        );
        assert_eq!(out, px);
    }
}
