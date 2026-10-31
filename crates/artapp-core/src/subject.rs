//! Selecionar assunto e intervalo de cores (do PhotoCraft), sem aprendizado de máquina.
//!
//! Assunto: o fundo é tudo que, a partir da borda da imagem, dá para alcançar passando só por
//! cores parecidas com as da borda (ou transparentes). O resto vira a seleção, sem os pedacinhos
//! soltos. Funciona bem com desenho sobre fundo liso ou pouco variado.

use std::collections::VecDeque;

use photocraft_geom::Rect;

use crate::composite::{Pass, composite_pass};
use crate::document::Document;
use crate::selection::Selection;

/// Quantas cores de fundo a borda pode ter.
const CLUSTERS: usize = 4;

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Algumas cores que resumem `samples` (k-médias simples).
fn clusters(samples: &[[f32; 3]]) -> Vec<[f32; 3]> {
    if samples.is_empty() {
        return Vec::new();
    }
    let k = CLUSTERS.min(samples.len());
    let mut centers: Vec<[f32; 3]> = (0..k).map(|i| samples[i * samples.len() / k]).collect();
    for _ in 0..8 {
        let mut sum = vec![[0.0f32; 4]; k];
        for s in samples {
            let (j, _) = centers
                .iter()
                .enumerate()
                .map(|(j, c)| (j, dist2(*s, *c)))
                .fold((0, f32::MAX), |a, b| if b.1 < a.1 { b } else { a });
            for c in 0..3 {
                sum[j][c] += s[c];
            }
            sum[j][3] += 1.0;
        }
        for (c, s) in centers.iter_mut().zip(&sum) {
            if s[3] > 0.0 {
                *c = [s[0] / s[3], s[1] / s[3], s[2] / s[3]];
            }
        }
    }
    centers
}

/// Máscara do assunto (1 dentro) para a imagem `px` de `w` × `h`.
pub fn subject_mask(px: &[[f32; 4]], w: usize, h: usize, tolerance: f32) -> Vec<f32> {
    let n = w * h;
    let mut out = vec![0.0f32; n];
    if w == 0 || h == 0 || px.len() < n {
        return out;
    }
    let rgb = |i: usize| [px[i][0], px[i][1], px[i][2]];
    let clear = |i: usize| px[i][3] < 0.05;
    let border: Vec<usize> = (0..w)
        .flat_map(|x| [x, (h - 1) * w + x])
        .chain((0..h).flat_map(|y| [y * w, y * w + w - 1]))
        .collect();
    let samples: Vec<[f32; 3]> = border
        .iter()
        .filter(|i| !clear(**i))
        .map(|i| rgb(*i))
        .collect();
    let centers = clusters(&samples);
    let tol2 = (tolerance.clamp(0.01, 1.0) * 3f32.sqrt()).powi(2);
    let is_bg = |i: usize| clear(i) || centers.iter().any(|c| dist2(rgb(i), *c) <= tol2);
    // Fundo: alcançável da borda passando só por cores de fundo.
    let mut bg = vec![false; n];
    let mut queue = VecDeque::new();
    for i in border {
        if !bg[i] && is_bg(i) {
            bg[i] = true;
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        let (x, y) = (i % w, i / w);
        let mut visit = |j: usize| {
            if !bg[j] && is_bg(j) {
                bg[j] = true;
                queue.push_back(j);
            }
        };
        if x > 0 {
            visit(i - 1);
        }
        if x + 1 < w {
            visit(i + 1);
        }
        if y > 0 {
            visit(i - w);
        }
        if y + 1 < h {
            visit(i + w);
        }
    }
    // Pedaços do assunto: fica só o que tiver pelo menos 1/50 do maior.
    let mut label = vec![0u32; n];
    let mut sizes = vec![0usize];
    for start in 0..n {
        if bg[start] || label[start] != 0 {
            continue;
        }
        let id = sizes.len() as u32;
        let mut size = 0;
        label[start] = id;
        queue.push_back(start);
        while let Some(i) = queue.pop_front() {
            size += 1;
            let (x, y) = (i % w, i / w);
            let mut visit = |j: usize| {
                if !bg[j] && label[j] == 0 {
                    label[j] = id;
                    queue.push_back(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
        sizes.push(size);
    }
    let biggest = sizes.iter().copied().max().unwrap_or(0);
    let keep: Vec<bool> = sizes.iter().map(|s| *s > 0 && s * 50 >= biggest).collect();
    for (o, l) in out.iter_mut().zip(&label) {
        if *l != 0 && keep[*l as usize] {
            *o = 1.0;
        }
    }
    out
}

/// Máscara suave dos pixels perto de `color` (1 igual, 0 a mais de `fuzziness` de distância).
pub fn color_range_mask(px: &[[f32; 4]], color: [f32; 3], fuzziness: f32) -> Vec<f32> {
    let f = fuzziness.clamp(0.01, 1.0) * 3f32.sqrt();
    px.iter()
        .map(|p| {
            if p[3] < 0.05 {
                return 0.0;
            }
            let d = dist2([p[0], p[1], p[2]], color).sqrt() / f;
            // Cheio até a metade da distância, depois cai até zero.
            ((1.0 - d) * 2.0).clamp(0.0, 1.0)
        })
        .collect()
}

impl Document {
    /// A imagem composta (sem papel nem rascunhos) em 0..1.
    fn composite_f32(&self) -> (Rect, Vec<[f32; 4]>) {
        let (r, img) = composite_pass(self, self.bounds(), false, Pass::Final);
        let px = img
            .iter()
            .map(|p| p.map(|v| f32::from(v) / 255.0))
            .collect();
        (r, px)
    }

    /// Seleciona o assunto da imagem (tudo que não é o fundo liso em volta).
    pub fn select_subject(&self, tolerance: f32) -> Option<Selection> {
        let (r, px) = self.composite_f32();
        let m = subject_mask(&px, r.width() as usize, r.height() as usize, tolerance);
        Selection::from_mask(&m, r)
    }

    /// Seleciona na imagem inteira as cores perto de `color`.
    pub fn select_color_range(&self, color: [f32; 3], fuzziness: f32) -> Option<Selection> {
        let (r, px) = self.composite_f32();
        let m = color_range_mask(&px, color, fuzziness);
        Selection::from_mask(&m, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_is_the_shape_on_a_plain_background() {
        let (w, h) = (40, 30);
        let mut px = vec![[0.9f32, 0.9, 0.85, 1.0]; w * h];
        // Um quadrado escuro com um buraco da cor do fundo, e um pingo solto.
        for y in 8..22 {
            for x in 10..30 {
                px[y * w + x] = [0.1, 0.2, 0.6, 1.0];
            }
        }
        px[15 * w + 20] = [0.9, 0.9, 0.85, 1.0];
        px[2 * w + 2] = [0.0, 0.0, 0.0, 1.0];
        let m = subject_mask(&px, w, h, 0.1);
        assert_eq!(m[10 * w + 12], 1.0);
        assert_eq!(m[15 * w + 20], 1.0, "o buraco fechado fica no assunto");
        assert_eq!(m[2 * w + 2], 0.0, "o pingo solto sai");
        assert_eq!(m[0], 0.0);
        assert_eq!(m.iter().filter(|v| **v > 0.0).count(), 20 * 14);
    }

    #[test]
    fn color_range_is_soft() {
        let px = [
            [1.0f32, 0.0, 0.0, 1.0],
            [0.9, 0.1, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
        ];
        let m = color_range_mask(&px, [1.0, 0.0, 0.0], 0.3);
        assert_eq!(m[0], 1.0);
        assert!(m[1] > 0.0);
        assert_eq!(m[2], 0.0);
    }
}
