//! Linhas de concentração e de velocidade (as "Linhas de efeito" do Clip Studio para quadrinho):
//! uma camada vetorial nova com cunhas finas, cada uma um objeto que dá para mexer depois.

use photocraft_geom::Rect;

use crate::document::Document;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinesKind {
    /// Para o centro (concentração / foco).
    Focus,
    /// Paralelas (velocidade).
    Speed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MangaLines {
    pub kind: LinesKind,
    pub count: u32,
    /// Foco: o vazio no meio, em fração da meia diagonal. Velocidade: quanto da tela as
    /// linhas cobrem no comprimento (0..1).
    pub inner: f32,
    /// Espessura da ponta grossa, em pixels.
    pub width: f32,
    /// Quanto o comprimento varia de linha para linha (0..1).
    pub jitter: f32,
    /// Velocidade: direção das linhas, em graus.
    pub angle: f32,
    pub seed: u64,
}

impl Default for MangaLines {
    fn default() -> Self {
        Self {
            kind: LinesKind::Focus,
            count: 120,
            inner: 0.35,
            width: 8.0,
            jitter: 0.4,
            angle: 0.0,
            seed: 1,
        }
    }
}

/// Gerador pequeno e repetível (o mesmo `seed` dá as mesmas linhas).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// As cunhas (triângulos) das linhas numa tela `w`×`h`, com o foco em `center`.
pub fn wedges(p: &MangaLines, w: f64, h: f64, center: [f64; 2]) -> Vec<Vec<[f64; 2]>> {
    let mut rng = Rng(p.seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let n = p.count.clamp(1, 2000) as usize;
    let half_w = f64::from(p.width.max(0.5)) / 2.0;
    let jitter = f64::from(p.jitter.clamp(0.0, 1.0));
    let mut out = Vec::with_capacity(n);
    match p.kind {
        LinesKind::Focus => {
            // Longe o bastante para sair da tela de qualquer ponto de foco.
            let far = (w * w + h * h).sqrt() * 1.5;
            let half_diag = (w * w + h * h).sqrt() / 2.0;
            let inner = f64::from(p.inner.clamp(0.0, 1.0)) * half_diag;
            for i in 0..n {
                let t = (i as f64 + rng.next() * 0.8) / n as f64 * std::f64::consts::TAU;
                let (s, c) = t.sin_cos();
                let r0 = inner * (1.0 + jitter * (rng.next() - 0.3));
                let width = half_w * (0.5 + rng.next());
                let tip = [center[0] + c * r0, center[1] + s * r0];
                let base = [center[0] + c * far, center[1] + s * far];
                out.push(vec![
                    tip,
                    [base[0] - s * width, base[1] + c * width],
                    [base[0] + s * width, base[1] - c * width],
                ]);
            }
        }
        LinesKind::Speed => {
            let (s, c) = f64::from(p.angle).to_radians().sin_cos();
            // Eixo das linhas (c, s) e o perpendicular (-s, c), cobrindo a tela inteira.
            let span = (w * c.abs() + h * s.abs()).max(1.0);
            let across = w * s.abs() + h * c.abs();
            let cover = f64::from(p.inner.clamp(0.05, 1.0));
            let mid = [w / 2.0, h / 2.0];
            for _ in 0..n {
                let off = (rng.next() - 0.5) * across;
                let len = span * cover * (1.0 - jitter * rng.next() * 0.8);
                // As linhas começam na borda de trás e afinam para a frente.
                let start = -span / 2.0 - span * 0.05;
                let end = start + len;
                let width = half_w * (0.5 + rng.next());
                let at = |a: f64, b: f64| [mid[0] + c * a - s * b, mid[1] + s * a + c * b];
                out.push(vec![
                    at(start, off - width),
                    at(end, off),
                    at(start, off + width),
                ]);
            }
        }
    }
    out
}

impl Document {
    /// Camada vetorial nova com as linhas, na cor `color`. Foco em `center` (pixels do documento).
    pub fn add_manga_lines(&mut self, p: &MangaLines, center: [f64; 2], color: [f32; 3]) -> Rect {
        let polys = wedges(p, f64::from(self.width), f64::from(self.height), center);
        self.add_vector_layer();
        if let Some(l) = self.active_layer_mut() {
            l.name = match p.kind {
                LinesKind::Focus => "Linhas de concentração".into(),
                LinesKind::Speed => "Linhas de velocidade".into(),
            };
        }
        self.add_filled_polygons(&polys, color)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn focus_lines_leave_the_middle_empty() {
        let mut doc = Document::new(200, 200);
        let p = MangaLines {
            count: 60,
            inner: 0.5,
            jitter: 0.0,
            ..Default::default()
        };
        assert!(
            !doc.add_manga_lines(&p, [100.0, 100.0], [0.0, 0.0, 0.0])
                .is_empty()
        );
        let l = doc.active_layer().unwrap();
        assert!(l.is_vector());
        assert_eq!(l.surface.rgba(100, 100)[3], 0.0);
        let s = &l.surface;
        let painted = (0..200)
            .flat_map(|x| [s.rgba(x, 2)[3], s.rgba(x, 197)[3]])
            .filter(|a| *a > 0.5)
            .count();
        assert!(painted > 20, "bordas quase vazias: {painted}");

        let speed = MangaLines {
            kind: LinesKind::Speed,
            ..Default::default()
        };
        assert_eq!(wedges(&speed, 200.0, 100.0, [0.0, 0.0]).len(), 120);
    }
}
