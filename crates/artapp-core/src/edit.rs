//! Edições da camada ativa: degradê, seleção automática, transformar, espelhar, filtros e ajustes
//! de cor. Os filtros e a seleção automática vêm do `photocraft-algo`; tudo respeita a seleção e o
//! bloqueio de transparência, e cada edição é um passo do desfazer.

use photocraft_algo::FilterParams;
use photocraft_algo::transform::{Homography, Interp, warp_surface};
use photocraft_color::{BlendMode, PixelFormat, blend};
use photocraft_geom::Rect;
use photocraft_raster::Surface;

use crate::composite::{Pass, composite_pass};
use crate::document::Document;
use crate::selection::Selection;

/// Forma do degradê.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientShape {
    Linear,
    Radial,
}

/// Ajustes de cor (o menu "Correção de tons" do Clip Studio).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Adjust {
    /// Matiz em graus (-180..180), saturação e luminosidade em -1..1.
    HueSatLight {
        hue: f32,
        sat: f32,
        light: f32,
    },
    /// Brilho e contraste em -1..1.
    BrightnessContrast {
        brightness: f32,
        contrast: f32,
    },
    /// Níveis: preto e branco de entrada (0..1) e gama.
    Levels {
        black: f32,
        white: f32,
        gamma: f32,
    },
    /// Número de níveis por canal (2..32).
    Posterize {
        levels: u32,
    },
    /// Binarização: acima do limiar fica branco, abaixo preto.
    Threshold {
        level: f32,
    },
    Invert,
    Desaturate,
    /// Curvas: a saída em cinco entradas fixas (0, ¼, ½, ¾, 1), para o RGB junto e cada canal.
    Curves {
        /// `[rgb, r, g, b]`, cada um com as cinco alturas 0..1.
        ys: [[f32; 5]; 4],
    },
    /// Mapa de gradiente: o brilho vira uma cor entre `dark` e `light`.
    GradientMap {
        dark: [f32; 3],
        light: [f32; 3],
    },
    /// Equilíbrio de cores: desvio RGB (-1..1) nas sombras, meios-tons e realces.
    ColorBalance {
        tones: [[f32; 3]; 3],
    },
}

/// Curvas sem mudança nenhuma.
pub const CURVE_IDENTITY: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

/// A curva por cinco pontos igualmente espaçados (Catmull-Rom, presa a 0..1).
pub fn curve_eval(ys: &[f32; 5], x: f32) -> f32 {
    let t = x.clamp(0.0, 1.0) * 4.0;
    let i = (t.floor() as usize).min(3);
    let f = t - i as f32;
    // Fora das pontas, continua em reta (assim a curva identidade fica reta de verdade).
    let y = |k: isize| match k {
        -1 => 2.0 * ys[0] - ys[1],
        5 => 2.0 * ys[4] - ys[3],
        k => ys[k as usize],
    };
    let (p0, p1, p2, p3) = (
        y(i as isize - 1),
        y(i as isize),
        y(i as isize + 1),
        y(i as isize + 2),
    );
    let v = 0.5
        * (2.0 * p1
            + (p2 - p0) * f
            + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * f * f
            + (3.0 * p1 - p0 - 3.0 * p2 + p3) * f * f * f);
    v.clamp(0.0, 1.0)
}

impl Document {
    /// Pixels da camada ativa raster que podem ser editados (`None` se oculta, vetorial ou vazia),
    /// recortados à seleção.
    fn edit_area(&self, whole: bool) -> Option<Rect> {
        let layer = self.active_layer()?;
        if !layer.visible || layer.is_vector() {
            return None;
        }
        let mut r = if whole {
            self.bounds()
        } else {
            layer.surface.content_bounds().intersect(&self.bounds())
        };
        if let Some(sel) = &self.selection {
            r = r.intersect(&sel.bounds);
        }
        (!r.is_empty()).then_some(r)
    }

    /// Troca cada pixel de `r` por `f(x, y, pixel)`, misturado pela seleção e respeitando o
    /// bloqueio de transparência.
    fn map_pixels(&mut self, r: Rect, f: impl Fn(i32, i32, [f32; 4]) -> [f32; 4]) -> Rect {
        let sel = self.selection.as_ref().map(|s| s.mask.read_region(r));
        self.checkpoint();
        let Some(layer) = self.active_layer_mut() else {
            return Rect::EMPTY;
        };
        let lock = layer.lock_alpha;
        let mut px = vec![[0.0f32; 4]; r.width() as usize * r.height() as usize];
        layer.surface.read_rgba_into(r, &mut px);
        let w = r.width() as usize;
        let mut out = Vec::with_capacity(px.len() * 4);
        for (i, p) in px.iter().enumerate() {
            let k = sel
                .as_ref()
                .map_or(1.0, |m| m.get(i).copied().unwrap_or(0.0));
            let (x, y) = (r.x0 + (i % w) as i32, r.y0 + (i / w) as i32);
            let q = if k > 0.0 { f(x, y, *p) } else { *p };
            let mut o = [0.0f32; 4];
            for c in 0..4 {
                o[c] = p[c] + (q[c] - p[c]) * k;
            }
            if lock {
                o[3] = p[3];
            }
            out.extend_from_slice(&o);
        }
        layer.surface.write_region(r, &out);
        r
    }

    /// Degradê de `c0` (em `a`) para `c1` (em `b`) pintado por cima da camada ativa.
    pub fn gradient(
        &mut self,
        a: [f64; 2],
        b: [f64; 2],
        shape: GradientShape,
        c0: [f32; 4],
        c1: [f32; 4],
    ) -> Rect {
        let Some(r) = self.edit_area(true) else {
            return Rect::EMPTY;
        };
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = (dx * dx + dy * dy).max(1e-9);
        self.map_pixels(r, |x, y, p| {
            let (px, py) = (x as f64 + 0.5 - a[0], y as f64 + 0.5 - a[1]);
            let t = match shape {
                GradientShape::Linear => (px * dx + py * dy) / len2,
                GradientShape::Radial => ((px * px + py * py) / len2).sqrt(),
            }
            .clamp(0.0, 1.0) as f32;
            let mut c = [0.0f32; 4];
            for i in 0..4 {
                c[i] = c0[i] + (c1[i] - c0[i]) * t;
            }
            blend::composite(BlendMode::Normal, p, c, 1.0)
        })
    }

    /// Seleção automática (varinha mágica) a partir de `(x, y)`. `all_layers` olha a imagem
    /// composta (sem rascunhos); senão, só a camada ativa.
    pub fn magic_wand(
        &self,
        x: i32,
        y: i32,
        tolerance: f32,
        all_layers: bool,
        contiguous: bool,
    ) -> Option<Selection> {
        let area = self.bounds();
        if !area.contains(x, y) {
            return None;
        }
        let px: Vec<[f32; 4]> = if all_layers {
            let (_, img) = composite_pass(self, area, false, Pass::Final);
            img.iter()
                .map(|p| p.map(|v| f32::from(v) / 255.0))
                .collect()
        } else {
            let layer = self.active_layer()?;
            let mut v = vec![[0.0f32; 4]; area.width() as usize * area.height() as usize];
            layer.surface.read_rgba_into(area, &mut v);
            v
        };
        let m = photocraft_algo::selection::magic_wand(
            &px,
            area,
            (x, y),
            tolerance * 255.0,
            contiguous,
            true,
        );
        Selection::from_mask(&m, area)
    }

    /// Borracha mágica (do PhotoCraft): apaga da camada ativa a área de cor parecida com a do
    /// ponto tocado, respeitando a seleção.
    pub fn magic_erase(&mut self, x: i32, y: i32, tolerance: f32, contiguous: bool) -> Rect {
        let Some(wand) = self.magic_wand(x, y, tolerance, false, contiguous) else {
            return Rect::EMPTY;
        };
        let Some(r) = self.edit_area(false) else {
            return Rect::EMPTY;
        };
        self.map_pixels(
            r,
            move |px, py, p| {
                if wand.contains(px, py) { [0.0; 4] } else { p }
            },
        )
    }

    /// Aplica um filtro do PhotoCraft na camada ativa (dentro da seleção, se houver).
    pub fn apply_filter(&mut self, params: &FilterParams) -> Rect {
        let Some(r) = self.edit_area(true) else {
            return Rect::EMPTY;
        };
        let bounds = self.bounds();
        let sel = self.selection.as_ref().map(|s| s.mask.clone());
        self.checkpoint();
        let Some(layer) = self.active_layer_mut() else {
            return Rect::EMPTY;
        };
        let before = layer.surface.clone();
        let mut out =
            photocraft_algo::apply_in(&layer.surface, params, r, bounds, sel.as_ref(), bounds);
        if layer.lock_alpha {
            // Devolve a transparência original.
            let mut a = vec![[0.0f32; 4]; r.width() as usize * r.height() as usize];
            let mut b = a.clone();
            before.read_rgba_into(r, &mut a);
            out.read_rgba_into(r, &mut b);
            let data: Vec<f32> = a
                .iter()
                .zip(&b)
                .flat_map(|(o, n)| [n[0], n[1], n[2], o[3]])
                .collect();
            out.write_region(r, &data);
        }
        layer.surface = out;
        r
    }

    /// Ajuste de cor na camada ativa.
    pub fn adjust(&mut self, adj: Adjust) -> Rect {
        let Some(r) = self.edit_area(false) else {
            return Rect::EMPTY;
        };
        self.map_pixels(r, move |_, _, p| adjust_pixel(adj, p))
    }

    /// Escala, gira e desloca o conteúdo da camada ativa (ou só o que está selecionado) em volta
    /// do centro dele. `angle` em graus, no sentido horário.
    pub fn transform_layer(&mut self, sx: f64, sy: f64, angle: f64, dx: f64, dy: f64) -> Rect {
        let Some(r) = self.edit_area(false) else {
            return Rect::EMPTY;
        };
        if !(sx.is_finite() && sy.is_finite() && sx.abs() > 1e-3 && sy.abs() > 1e-3) {
            return Rect::EMPTY;
        }
        let (cx, cy) = ((r.x0 + r.x1) as f64 / 2.0, (r.y0 + r.y1) as f64 / 2.0);
        let (sin, cos) = angle.to_radians().sin_cos();
        // destino = T(c + d) · R · S · T(-c)
        let (a, b, c, d) = (cos * sx, -sin * sy, sin * sx, cos * sy);
        let h = Homography([
            a,
            b,
            cx + dx - a * cx - b * cy,
            c,
            d,
            cy + dy - c * cx - d * cy,
            0.0,
            0.0,
            1.0,
        ]);
        let sel = self.selection.clone();
        self.checkpoint();
        let canvas = self.bounds();
        let Some(layer) = self.active_layer_mut() else {
            return Rect::EMPTY;
        };
        // Separa o que vai ser transformado (a seleção, ou a camada toda) do que fica.
        let (moving, mut rest) = match &sel {
            Some(s) => split_by_mask(&layer.surface, &s.mask, r),
            None => (layer.surface.clone(), Surface::new(PixelFormat::RGBA8)),
        };
        let Ok(warped) = warp_surface(&moving, r, &h, Interp::Bicubic) else {
            return Rect::EMPTY;
        };
        let out_r = warped.content_bounds().intersect(&canvas);
        if !out_r.is_empty() {
            let n = out_r.width() as usize * out_r.height() as usize;
            let (mut a, mut b) = (vec![[0.0f32; 4]; n], vec![[0.0f32; 4]; n]);
            rest.read_rgba_into(out_r, &mut a);
            warped.read_rgba_into(out_r, &mut b);
            let data: Vec<f32> = a
                .iter()
                .zip(&b)
                .flat_map(|(d, s)| blend::composite(BlendMode::Normal, *d, *s, 1.0))
                .collect();
            rest.write_region(out_r, &data);
        }
        layer.surface = rest;
        // A seleção acompanha o conteúdo.
        if let Some(s) = sel {
            let pts: Vec<[f64; 2]> = s
                .outline
                .iter()
                .map(|p| {
                    let (x, y) = h.apply(p[0], p[1]);
                    [x, y]
                })
                .collect();
            self.selection = if pts.len() >= 3 && !s.inverted {
                Selection::polygon(&pts, self.width, self.height)
            } else {
                None
            };
        }
        r.union(&out_r)
    }

    /// Espelha o conteúdo da camada ativa (ou da seleção) na horizontal ou na vertical.
    pub fn flip_layer(&mut self, horizontal: bool) -> Rect {
        if horizontal {
            self.transform_layer(-1.0, 1.0, 0.0, 0.0, 0.0)
        } else {
            self.transform_layer(1.0, -1.0, 0.0, 0.0, 0.0)
        }
    }
}

/// Separa `s` em (pixels selecionados, o resto), pela máscara.
fn split_by_mask(s: &Surface, mask: &Surface, r: Rect) -> (Surface, Surface) {
    let n = r.width() as usize * r.height() as usize;
    let mut px = vec![[0.0f32; 4]; n];
    s.read_rgba_into(r, &mut px);
    let m = mask.read_region(r);
    let (mut inside, mut outside) = (Vec::with_capacity(n * 4), Vec::with_capacity(n * 4));
    for (p, k) in px.iter().zip(&m) {
        inside.extend_from_slice(&[p[0], p[1], p[2], p[3] * k]);
        outside.extend_from_slice(&[p[0], p[1], p[2], p[3] * (1.0 - k)]);
    }
    let mut a = Surface::new(PixelFormat::RGBA8);
    a.write_region(r, &inside);
    let mut b = s.clone();
    b.write_region(r, &outside);
    (a, b)
}

fn luma(p: [f32; 4]) -> f32 {
    0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]
}

fn rgb_to_hsl([r, g, b]: [f32; 3]) -> [f32; 3] {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 {
        return [0.0, 0.0, l];
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h / 6.0, s, l]
}

fn hsl_to_rgb([h, s, l]: [f32; 3]) -> [f32; 3] {
    if s <= 0.0 {
        return [l, l, l];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let f = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0)]
}

pub fn adjust_pixel(adj: Adjust, p: [f32; 4]) -> [f32; 4] {
    let rgb = |f: &dyn Fn(f32) -> f32| [f(p[0]), f(p[1]), f(p[2]), p[3]];
    let out = match adj {
        Adjust::HueSatLight { hue, sat, light } => {
            let [h, s, l] = rgb_to_hsl([p[0], p[1], p[2]]);
            let s = if sat >= 0.0 {
                s + (1.0 - s) * sat
            } else {
                s * (1.0 + sat)
            };
            let l = if light >= 0.0 {
                l + (1.0 - l) * light
            } else {
                l * (1.0 + light)
            };
            let [r, g, b] = hsl_to_rgb([h + hue / 360.0, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0)]);
            [r, g, b, p[3]]
        }
        Adjust::BrightnessContrast {
            brightness,
            contrast,
        } => {
            let k = if contrast >= 0.0 {
                1.0 / (1.0 - contrast * 0.99)
            } else {
                1.0 + contrast
            };
            rgb(&|v| (v - 0.5) * k + 0.5 + brightness * 0.5)
        }
        Adjust::Levels {
            black,
            white,
            gamma,
        } => {
            let span = (white - black).max(1e-3);
            let g = 1.0 / gamma.max(0.05);
            rgb(&|v| ((v - black) / span).clamp(0.0, 1.0).powf(g))
        }
        Adjust::Posterize { levels } => {
            let n = levels.clamp(2, 255) as f32 - 1.0;
            rgb(&|v| (v * n).round() / n)
        }
        Adjust::Threshold { level } => {
            let v = if luma(p) >= level { 1.0 } else { 0.0 };
            [v, v, v, p[3]]
        }
        Adjust::Curves { ys } => {
            let c = |ch: usize, v: f32| curve_eval(&ys[0], curve_eval(&ys[ch + 1], v));
            [c(0, p[0]), c(1, p[1]), c(2, p[2]), p[3]]
        }
        Adjust::GradientMap { dark, light } => {
            let l = luma(p).clamp(0.0, 1.0);
            let m = |i: usize| dark[i] + (light[i] - dark[i]) * l;
            [m(0), m(1), m(2), p[3]]
        }
        Adjust::ColorBalance { tones } => {
            let l = luma(p).clamp(0.0, 1.0);
            // Pesos suaves de cada faixa de tom.
            let w = [
                (1.0 - l * 2.0).clamp(0.0, 1.0),
                1.0 - (l * 2.0 - 1.0).abs(),
                (l * 2.0 - 1.0).clamp(0.0, 1.0),
            ];
            let shift = |c: usize| (0..3).map(|t| tones[t][c] * w[t]).sum::<f32>() * 0.5;
            [p[0] + shift(0), p[1] + shift(1), p[2] + shift(2), p[3]]
        }
        Adjust::Invert => rgb(&|v| 1.0 - v),
        Adjust::Desaturate => {
            let l = luma(p);
            [l, l, l, p[3]]
        }
    };
    out.map(|v| v.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn import_image_centres_and_fits() {
        let mut doc = Document::new(100, 50);
        let (w, h) = (200u32, 50u32);
        let px: Vec<u8> = (0..w * h).flat_map(|_| [10u8, 200, 30, 255]).collect();
        let r = doc.import_image("Foto", w, h, &px);
        assert_eq!((r.width(), r.height()), (100, 25));
        assert_eq!(doc.layers.len(), 2);
        let p = doc.active_layer().unwrap().surface.rgba(50, 25);
        assert!(p[3] > 0.99 && p[1] > 0.7);
        assert_eq!(doc.active_layer().unwrap().surface.rgba(50, 5)[3], 0.0);
        assert!(decode_image(b"nope").is_err());
    }

    #[test]
    fn magic_erase_removes_the_touched_colour_only() {
        let mut doc = Document::new(40, 20);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 20, 20), &[1.0, 0.0, 0.0, 1.0]);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(20, 0, 40, 20), &[0.0, 0.0, 1.0, 1.0]);
        doc.magic_erase(5, 5, 0.1, true);
        assert_eq!(doc.layers[0].surface.rgba(5, 5)[3], 0.0);
        assert_eq!(doc.layers[0].surface.rgba(30, 5)[3], 1.0);
        let c = |x| doc.layers[0].surface.rgba(x, 5);
        assert!(c(30)[2] > 0.9);
    }

    #[test]
    fn curves_identity_and_gradient_map() {
        let p = [0.3, 0.6, 0.9, 1.0];
        let id = adjust_pixel(
            Adjust::Curves {
                ys: [CURVE_IDENTITY; 4],
            },
            p,
        );
        for c in 0..3 {
            assert!((id[c] - p[c]).abs() < 1e-4);
        }
        let g = adjust_pixel(
            Adjust::GradientMap {
                dark: [0.0, 0.0, 0.0],
                light: [1.0, 0.0, 0.0],
            },
            [1.0, 1.0, 1.0, 1.0],
        );
        assert!(g[0] > 0.99 && g[1] < 0.01);
    }

    #[test]
    fn linear_gradient_goes_from_one_color_to_the_other() {
        let mut doc = Document::new(100, 10);
        doc.gradient(
            [0.0, 5.0],
            [100.0, 5.0],
            GradientShape::Linear,
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
        );
        let s = &doc.layers[0].surface;
        assert!(s.rgba(1, 5)[0] < 0.05);
        assert!(s.rgba(98, 5)[0] > 0.95);
        assert!((s.rgba(50, 5)[0] - 0.5).abs() < 0.05);
    }

    #[test]
    fn magic_wand_selects_the_enclosed_area() {
        let mut doc = Document::new(20, 20);
        let black = [0.0, 0.0, 0.0, 1.0];
        for i in 0..20 {
            doc.layers[0].surface.write_pixel(10, i, &black);
        }
        let s = doc.magic_wand(2, 2, 0.1, true, true).unwrap();
        assert!(s.contains(5, 5));
        assert!(!s.contains(15, 5));
    }

    #[test]
    fn invert_and_filter_change_pixels_and_undo() {
        let mut doc = Document::new(20, 20);
        let b = doc.bounds();
        doc.layers[0].surface.fill_rect(b, &[1.0, 0.0, 0.0, 1.0]);
        doc.adjust(Adjust::Invert);
        assert_eq!(doc.layers[0].surface.rgba(5, 5), [0.0, 1.0, 1.0, 1.0]);
        doc.apply_filter(&FilterParams::GaussianBlur { radius: 2.0 });
        assert!(doc.undo());
        assert!(doc.undo());
        assert_eq!(doc.layers[0].surface.rgba(5, 5), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn flip_and_scale_move_content() {
        let mut doc = Document::new(40, 40);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 10, 20), &[1.0, 1.0, 1.0, 1.0]);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(10, 0, 20, 20), &[0.0, 0.0, 0.0, 1.0]);
        doc.flip_layer(true);
        assert!(doc.layers[0].surface.rgba(15, 10)[0] > 0.9);
        assert!(doc.layers[0].surface.rgba(5, 10)[0] < 0.1);
        doc.transform_layer(2.0, 2.0, 0.0, 0.0, 0.0);
        assert!(doc.layers[0].surface.rgba(25, 10)[3] > 0.9);
    }
}

/// Lê PNG, JPEG, WebP, GIF ou BMP: largura, altura e RGBA 8 bits.
pub fn decode_image(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("imagem ilegível: {e}"))?;
    let img = img.to_rgba8();
    Ok((img.width(), img.height(), img.into_raw()))
}

/// Reduz uma imagem RGBA 8 bits pela média da área (com alfa pré-multiplicado, sem halos).
fn downscale(src: &[u8], w: u32, h: u32, nw: u32, nh: u32) -> Vec<u8> {
    let (w, h, nw, nh) = (w as usize, h as usize, nw as usize, nh as usize);
    let mut out = vec![0u8; nw * nh * 4];
    for oy in 0..nh {
        let (y0, y1) = (oy * h / nh, ((oy + 1) * h / nh).max(oy * h / nh + 1).min(h));
        for ox in 0..nw {
            let (x0, x1) = (ox * w / nw, ((ox + 1) * w / nw).max(ox * w / nw + 1).min(w));
            let mut acc = [0.0f64; 4];
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = &src[(y * w + x) * 4..(y * w + x) * 4 + 4];
                    let a = f64::from(p[3]) / 255.0;
                    for c in 0..3 {
                        acc[c] += f64::from(p[c]) * a;
                    }
                    acc[3] += a;
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as f64;
            let o = &mut out[(oy * nw + ox) * 4..(oy * nw + ox) * 4 + 4];
            if acc[3] > 0.0 {
                for c in 0..3 {
                    o[c] = (acc[c] / acc[3]).round().clamp(0.0, 255.0) as u8;
                }
            }
            o[3] = (acc[3] / n * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// O que foi copiado: os pixels e onde estavam.
#[derive(Clone)]
pub struct Clip {
    pub surface: Surface,
    pub rect: Rect,
}

impl Document {
    /// Copia a seleção (ou a camada inteira) da camada ativa raster.
    pub fn copy_pixels(&self) -> Option<Clip> {
        let layer = self.active_layer()?;
        if layer.is_vector() {
            return None;
        }
        let mut r = layer.surface.content_bounds().intersect(&self.bounds());
        if let Some(sel) = &self.selection {
            r = r.intersect(&sel.bounds);
        }
        if r.is_empty() {
            return None;
        }
        let mut px = vec![[0.0f32; 4]; r.width() as usize * r.height() as usize];
        layer.surface.read_rgba_into(r, &mut px);
        if let Some(sel) = &self.selection {
            for (p, k) in px.iter_mut().zip(sel.mask.read_region(r)) {
                p[3] *= k;
            }
        }
        let flat: Vec<f32> = px.iter().flatten().copied().collect();
        let mut surface = Surface::new(PixelFormat::RGBA8);
        surface.write_region(r, &flat);
        surface.prune();
        Some(Clip { surface, rect: r })
    }

    /// Importa uma imagem (RGBA 8 bits, sem pré-multiplicar) como camada nova, no centro da
    /// tela. Se for maior que a tela, é reduzida para caber.
    pub fn import_image(&mut self, name: &str, w: u32, h: u32, rgba: &[u8]) -> Rect {
        if w == 0 || h == 0 || rgba.len() != w as usize * h as usize * 4 {
            return Rect::EMPTY;
        }
        let k = (f64::from(self.width) / f64::from(w))
            .min(f64::from(self.height) / f64::from(h))
            .min(1.0);
        let nw = ((f64::from(w) * k).round() as u32).max(1);
        let nh = ((f64::from(h) * k).round() as u32).max(1);
        let px = if (nw, nh) == (w, h) {
            rgba.to_vec()
        } else {
            downscale(rgba, w, h, nw, nh)
        };
        let x = (self.width as i32 - nw as i32) / 2;
        let y = (self.height as i32 - nh as i32) / 2;
        let r = Rect::new(x, y, x + nw as i32, y + nh as i32);
        self.add_layer();
        if let Some(l) = self.active_layer_mut() {
            l.name = name.to_string();
            l.surface = Surface::from_interleaved(PixelFormat::RGBA8, r, &px);
            l.surface.prune();
        }
        r.intersect(&self.bounds())
    }

    /// Recorta: copia e apaga o que foi copiado.
    pub fn cut_pixels(&mut self) -> Option<(Clip, Rect)> {
        let clip = self.copy_pixels()?;
        let dirty = if self.selection.is_some() {
            self.clear_selection()
        } else {
            self.clear_layer();
            self.bounds()
        };
        Some((clip, dirty))
    }

    /// Cola numa camada nova acima da ativa, no mesmo lugar de onde veio.
    pub fn paste_layer(&mut self, clip: &Clip) -> Rect {
        self.add_layer();
        if let Some(l) = self.active_layer_mut() {
            l.name = l.name.replace("Camada", "Colado");
            l.surface = clip.surface.clone();
        }
        clip.rect.intersect(&self.bounds())
    }

    /// "Mudar cor da linha para a cor de desenho": pinta tudo da camada (ou da seleção) com
    /// `color`, mantendo a transparência.
    pub fn line_to_color(&mut self, color: [f32; 3]) -> Rect {
        let Some(r) = self.edit_area(false) else {
            return Rect::EMPTY;
        };
        self.map_pixels(r, |_, _, p| [color[0], color[1], color[2], p[3]])
    }

    /// "Converter brilho em opacidade": o branco some e o escuro vira `color` opaca, como ao
    /// digitalizar um desenho no papel.
    pub fn brightness_to_opacity(&mut self, color: [f32; 3]) -> Rect {
        let Some(r) = self.edit_area(false) else {
            return Rect::EMPTY;
        };
        self.map_pixels(r, |_, _, p| {
            let l = 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
            [
                color[0],
                color[1],
                color[2],
                p[3] * (1.0 - l).clamp(0.0, 1.0),
            ]
        })
    }
}

#[cfg(test)]
mod clip_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn copy_paste_and_line_colour() {
        let mut doc = Document::new(30, 30);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(5, 5, 15, 15), &[0.0, 0.0, 0.0, 1.0]);
        doc.set_selection(Selection::rect([0.0, 0.0], [10.0, 10.0], 30, 30));
        let (clip, _) = doc.cut_pixels().unwrap();
        assert_eq!(doc.layers[0].surface.rgba(7, 7)[3], 0.0);
        assert_eq!(doc.layers[0].surface.rgba(12, 12)[3], 1.0);
        doc.set_selection(None);
        doc.paste_layer(&clip);
        assert_eq!(doc.layers.len(), 2);
        assert_eq!(doc.layers[1].surface.rgba(7, 7)[3], 1.0);
        doc.line_to_color([1.0, 0.0, 0.0]);
        assert_eq!(doc.layers[1].surface.rgba(7, 7), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn white_becomes_transparent() {
        let mut doc = Document::new(10, 10);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 10, 10), &[1.0, 1.0, 1.0, 1.0]);
        doc.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 5, 10), &[0.0, 0.0, 0.0, 1.0]);
        doc.brightness_to_opacity([0.0, 0.0, 0.0]);
        assert!(doc.layers[0].surface.rgba(8, 5)[3] < 0.01);
        assert!(doc.layers[0].surface.rgba(2, 5)[3] > 0.99);
    }
}
