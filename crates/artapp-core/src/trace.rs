//! Vetorizar camada: traça a camada raster ativa em formas vetoriais (o Image Trace do
//! VectorCraft) e põe o resultado numa camada vetorial nova acima dela.

use photocraft_geom::Rect;
use vectorcraft_doc::Node;
use vectorcraft_trace::Raster;

use crate::document::Document;
use crate::vector::VectorStyle;

/// As predefinições oferecidas, com o nome em português e o nome do VectorCraft.
pub const PRESETS: &[(&str, &str)] = &[
    ("Arte de linha", "Line Art"),
    ("Contorno (traços)", "Outline"),
    ("Logo preto e branco", "Black and White Logo"),
    ("Esboço", "Sketched Art"),
    ("Silhuetas", "Silhouettes"),
    ("3 cores", "3 Colors"),
    ("6 cores", "6 Colors"),
    ("16 cores", "16 Colors"),
    ("Tons de cinza", "Shades of Gray"),
    ("Foto (baixa fidelidade)", "Low Fidelity Photo"),
    ("Foto (alta fidelidade)", "High Fidelity Photo"),
];

/// Lado máximo traçado: acima disso a camada é reduzida antes (e as formas, ampliadas de volta).
const MAX_SIDE: u32 = 2048;

impl Document {
    /// Vetoriza a camada raster ativa com a predefinição `preset` (nome do VectorCraft). Cria uma
    /// camada vetorial acima e devolve quantas formas saíram (0: nada a vetorizar).
    pub fn trace_layer(&mut self, preset: &str) -> (usize, Rect) {
        let Some(params) = vectorcraft_trace::preset(preset) else {
            return (0, Rect::EMPTY);
        };
        if self.active_is_vector() {
            return (0, Rect::EMPTY);
        }
        let Some(layer) = self.active_layer() else {
            return (0, Rect::EMPTY);
        };
        let (w, h) = (self.width, self.height);
        let mut px = vec![[0.0f32; 4]; (w * h) as usize];
        layer.surface.read_rgba_into(self.bounds(), &mut px);
        let Some((raster, scale)) = to_raster(&px, w, h) else {
            return (0, Rect::EMPTY);
        };
        let result = vectorcraft_trace::trace(&raster, &params);
        if result.paths.is_empty() {
            return (0, Rect::EMPTY);
        }
        let name = format!("{} (vetor)", layer.name);
        self.add_vector_layer();
        if let Some(l) = self.active_layer_mut() {
            l.name = name;
        }
        let n = result.paths.len();
        let r = self.add_traced(&result.paths, scale);
        (n, r)
    }

    fn add_traced(&mut self, paths: &[vectorcraft_trace::TracedPath], scale: f64) -> Rect {
        let back = vectorcraft_geom::Affine::scale(scale);
        self.edit_vector(false, |art| {
            let Some(parent) = art.target() else {
                return false;
            };
            for t in paths {
                let c = t.color.map(|v| f32::from(v) / 255.0);
                let style = match t.stroke {
                    Some(width) => VectorStyle {
                        stroke: Some(c),
                        width: width * scale,
                        fill: None,
                        ..VectorStyle::default()
                    },
                    None => VectorStyle {
                        stroke: None,
                        width: 0.0,
                        fill: Some(c),
                        ..VectorStyle::default()
                    },
                };
                let mut path = t.path.clone();
                if scale != 1.0 {
                    path.transform(back);
                }
                let id = art.doc.alloc_id();
                if art
                    .doc
                    .insert(
                        Some(parent),
                        usize::MAX,
                        Node::path(id, path, style.appearance()),
                    )
                    .is_err()
                {
                    return false;
                }
            }
            true
        })
    }
}

/// A camada em RGBA de 8 bits, reduzida se passar de [`MAX_SIDE`]; o transparente vira branco
/// (o traçado ignora o branco nas predefinições de preto e branco). Devolve também a escala para
/// voltar ao tamanho da tela. `None` se a camada está vazia.
fn to_raster(px: &[[f32; 4]], w: u32, h: u32) -> Option<(Raster, f64)> {
    if !px.iter().any(|p| p[3] > 0.01) {
        return None;
    }
    let step = w.max(h).div_ceil(MAX_SIDE).max(1);
    let (tw, th) = (w.div_ceil(step), h.div_ceil(step));
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let mut rgba = Vec::with_capacity((tw * th * 4) as usize);
    for y in 0..th {
        for x in 0..tw {
            // Média do bloco step×step, sobre fundo branco.
            let mut acc = [0.0f32; 3];
            let mut n = 0.0f32;
            for dy in 0..step {
                for dx in 0..step {
                    let (sx, sy) = (x * step + dx, y * step + dy);
                    if sx >= w || sy >= h {
                        continue;
                    }
                    let p = px[(sy * w + sx) as usize];
                    let a = p[3].clamp(0.0, 1.0);
                    for c in 0..3 {
                        acc[c] += p[c] * a + (1.0 - a);
                    }
                    n += 1.0;
                }
            }
            let n = n.max(1.0);
            rgba.extend([q(acc[0] / n), q(acc[1] / n), q(acc[2] / n), 255]);
        }
    }
    let raster = Raster::new(tw, th, rgba).ok()?;
    Some((raster, f64::from(step)))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn traces_a_black_square() {
        let mut d = Document::new(120, 120);
        d.layers[0]
            .surface
            .fill_rect(Rect::new(30, 30, 90, 90), &[0.0, 0.0, 0.0, 1.0]);
        let (n, r) = d.trace_layer("Line Art");
        assert!(n >= 1, "{n}");
        assert!(!r.is_empty());
        assert!(d.active_is_vector());
        assert_eq!(d.layers.len(), 2);
        // A forma cobre o quadrado: o meio dele está pintado na camada nova.
        assert!(d.active_layer().unwrap().surface.rgba(60, 60)[3] > 0.9);
        assert!(d.active_layer().unwrap().surface.rgba(10, 10)[3] < 0.1);
    }
}
