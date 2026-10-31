//! Retícula (a "Retícula" das propriedades de camada do Clip Studio, para quadrinho): a tinta da
//! camada vira pontos pretos, maiores onde ela é mais escura. Como a borda, não mexe nos pixels:
//! entra na composição e só vira tinta ao mesclar ou exportar em PSD.

use photocraft_geom::Rect;

/// Espaço entre os pontos e o ângulo da grade.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tone {
    /// Distância entre os centros dos pontos, em pixels.
    pub period: f32,
    /// Graus.
    pub angle: f32,
}

impl Default for Tone {
    fn default() -> Self {
        Self {
            period: 8.0,
            angle: 45.0,
        }
    }
}

pub const MIN_PERIOD: f32 = 3.0;
pub const MAX_PERIOD: f32 = 60.0;

/// Troca os pixels de `row` (a área `r` da camada) pelos pontos da retícula.
pub fn apply(tone: &Tone, r: Rect, row: &mut [[f32; 4]]) {
    let period = f64::from(tone.period.clamp(MIN_PERIOD, MAX_PERIOD));
    let (sin, cos) = f64::from(tone.angle).to_radians().sin_cos();
    let w = r.width() as usize;
    for (k, p) in row.iter_mut().enumerate() {
        if p[3] <= 0.0 {
            continue;
        }
        let (x, y) = (
            f64::from(r.x0) + (k % w) as f64 + 0.5,
            f64::from(r.y0) + (k / w) as f64 + 0.5,
        );
        // Posição dentro da célula da grade girada, de -0.5 a 0.5.
        let u = (x * cos + y * sin) / period;
        let v = (-x * sin + y * cos) / period;
        let (du, dv) = (u - u.round(), v - v.round());
        let dist = (du * du + dv * dv).sqrt() * period;
        // Quanto escuro: a tinta preta e opaca enche a célula; a branca some.
        let luma = 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
        let dark = f64::from(p[3] * (1.0 - luma));
        // Raio do ponto com a área proporcional ao escuro (passa um pouco do meio da célula
        // para os pontos se tocarem no preto).
        let radius = (dark / std::f64::consts::PI).sqrt() * period * 1.13;
        // Pontos menores que um pixel ficam mais claros em vez de sumir de vez.
        let a = (radius - dist + 0.5).clamp(0.0, 1.0).min(radius * 2.0) as f32;
        *p = [0.0, 0.0, 0.0, a];
    }
}

/// `surface` com a retícula aplicada (para mesclar ou exportar).
pub fn bake(
    surface: &photocraft_raster::Surface,
    tone: &Tone,
    bounds: Rect,
) -> photocraft_raster::Surface {
    let r = surface.content_bounds().intersect(&bounds);
    let mut out = surface.clone();
    if r.is_empty() {
        return out;
    }
    let mut row = vec![[0.0f32; 4]; r.width() as usize * r.height() as usize];
    surface.read_rgba_into(r, &mut row);
    apply(tone, r, &mut row);
    out.write_region(r, row.as_flattened());
    out.prune();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gray_becomes_dots_and_white_disappears() {
        let r = Rect::new(0, 0, 40, 40);
        let mut gray = vec![[0.5f32, 0.5, 0.5, 1.0]; 1600];
        apply(&Tone::default(), r, &mut gray);
        let cover: f32 = gray.iter().map(|p| p[3]).sum::<f32>() / 1600.0;
        assert!((cover - 0.5).abs() < 0.15, "cobertura {cover}");
        assert!(gray.iter().any(|p| p[3] == 0.0) && gray.iter().any(|p| p[3] == 1.0));
        let mut white = vec![[1.0f32, 1.0, 1.0, 1.0]; 1600];
        apply(&Tone::default(), r, &mut white);
        assert!(white.iter().all(|p| p[3] < 0.01));
    }
}
