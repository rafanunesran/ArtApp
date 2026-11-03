//! Balde de tinta: preenche a região contígua de cor parecida na camada ativa. Como no Clip Studio,
//! amostra as camadas de referência se houver alguma; senão, a imagem composta sem os rascunhos
//! (ou só a camada atual, se pedido). Com uma seleção ativa, só pinta dentro dela. Cor com alfa 0
//! (a cor transparente) apaga.
//!
//! Para não transbordar por falhas pequenas nas linhas ("fechar lacunas"): as linhas são
//! engrossadas pela metade da abertura, o balde enche a área que sobra e depois devolve a faixa
//! colada nas linhas, mas só os pixels que ficam no caminho reto entre a área e a linha. Assim a
//! tinta volta até encostar no traço e não escorre pela abertura.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use photocraft_geom::Rect;
use rayon::prelude::*;

use crate::composite::{Pass, composite_pass};
use crate::document::Document;

/// Que imagem o balde olha para achar as bordas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRef {
    /// As camadas de referência, se houver; senão todas as camadas visíveis.
    #[default]
    Auto,
    /// Todas as camadas visíveis (sem rascunhos).
    All,
    /// Só a camada atual.
    Layer,
}

impl FillRef {
    pub const ALL: [FillRef; 3] = [FillRef::Auto, FillRef::All, FillRef::Layer];

    pub fn label(self) -> &'static str {
        match self {
            FillRef::Auto => "Referência (ou todas)",
            FillRef::All => "Todas as camadas",
            FillRef::Layer => "Só a camada atual",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            FillRef::Auto => "auto",
            FillRef::All => "all",
            FillRef::Layer => "layer",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.name() == s)
    }
}

/// Opções do balde, como na paleta de propriedades do Clip Studio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillOptions {
    /// 0..1: diferença máxima por canal.
    pub tolerance: f32,
    /// Fecha aberturas de até tantos pixels nas linhas (0 desliga).
    pub close_gap: f32,
    /// Cresce (> 0) ou encolhe (< 0) a área pintada, em pixels; 0 = só a franja de 1 px.
    pub expand: f32,
    /// Borda suave: a franja sob o antisserrilhado da linha recebe tinta parcial.
    pub antialias: bool,
    /// Onde procurar as bordas.
    pub reference: FillRef,
}

impl Default for FillOptions {
    fn default() -> Self {
        Self {
            tolerance: 0.1,
            close_gap: 0.0,
            expand: 0.0,
            antialias: true,
            reference: FillRef::Auto,
        }
    }
}

/// Preenche a partir de `(x, y)` com `color`. `tolerance` 0..1 (diferença máxima por canal).
/// Devolve o retângulo alterado, vazio se nada mudou.
pub fn flood_fill(doc: &mut Document, x: i32, y: i32, color: [f32; 4], tolerance: f32) -> Rect {
    let opts = FillOptions {
        tolerance,
        ..FillOptions::default()
    };
    flood_fill_with(doc, x, y, color, &opts)
}

/// A imagem em que o balde procura as bordas, inteira, em RGBA de 8 bits.
fn sample(doc: &Document, mode: FillRef) -> Vec<[u8; 4]> {
    let bounds = doc.bounds();
    let reference = doc.layers.iter().any(|l| l.reference && l.visible);
    match mode {
        FillRef::Layer => {
            let (w, h) = (doc.width as usize, doc.height as usize);
            let mut img = vec![[0u8; 4]; w * h];
            let Some(layer) = doc.active_layer() else {
                return img;
            };
            // Em faixas: a camada inteira em f32 seria 16 bytes por pixel.
            img.par_chunks_mut(w * 128)
                .enumerate()
                .for_each(|(b, band)| {
                    let y0 = (b * 128) as i32;
                    let r = Rect::new(0, y0, w as i32, y0 + (band.len() / w) as i32);
                    let mut px = vec![[0.0f32; 4]; band.len()];
                    layer.surface.read_rgba_into(r, &mut px);
                    for (o, p) in band.iter_mut().zip(&px) {
                        *o = p.map(|c| (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
                    }
                });
            img
        }
        FillRef::Auto if reference => composite_pass(doc, bounds, false, Pass::Reference).1,
        _ => composite_pass(doc, bounds, true, Pass::Final).1,
    }
}

/// Retângulo envolvente (inclusive) que vai crescendo.
#[derive(Clone, Copy)]
struct BBox {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

impl BBox {
    const EMPTY: BBox = BBox {
        x0: usize::MAX,
        y0: usize::MAX,
        x1: 0,
        y1: 0,
    };

    fn add(&mut self, x: usize, y: usize) {
        self.add_span(x, x, y);
    }

    fn add_span(&mut self, l: usize, r: usize, y: usize) {
        self.x0 = self.x0.min(l);
        self.x1 = self.x1.max(r);
        self.y0 = self.y0.min(y);
        self.y1 = self.y1.max(y);
    }

    fn is_empty(&self) -> bool {
        self.x0 > self.x1
    }
}

/// Preenchimento por linhas de varredura em `allowed` a partir de `(x, y)`.
fn scanline(allowed: &[bool], w: usize, h: usize, x: usize, y: usize) -> (Vec<bool>, BBox) {
    let idx = |x: usize, y: usize| y * w + x;
    let mut mask = vec![false; w * h];
    let mut bb = BBox::EMPTY;
    let mut stack = vec![(x, y)];
    while let Some((sx, sy)) = stack.pop() {
        let row = sy * w;
        let ok = |xx: usize, mask: &[bool]| !mask[row + xx] && allowed[row + xx];
        if !ok(sx, &mask) {
            continue;
        }
        let mut l = sx;
        while l > 0 && ok(l - 1, &mask) {
            l -= 1;
        }
        let mut r = sx;
        while r + 1 < w && ok(r + 1, &mask) {
            r += 1;
        }
        mask[row + l..=row + r].fill(true);
        bb.add_span(l, r, sy);
        // Nas linhas vizinhas, só o começo de cada trecho preenchível entra na pilha.
        for ny in [sy.wrapping_sub(1), sy + 1] {
            if ny >= h {
                continue;
            }
            let mut inside = false;
            for xx in l..=r {
                let ok = !mask[idx(xx, ny)] && allowed[idx(xx, ny)];
                if ok && !inside {
                    stack.push((xx, ny));
                }
                inside = ok;
            }
        }
    }
    (mask, bb)
}

/// Devolve à área a faixa que as linhas engrossadas tiraram: um pixel parecido com a semente e
/// perto da linha volta se a distância andada desde a área (sem atravessar linha) mais a
/// distância até a linha não passa de `half + 1`, isto é, se está no caminho direto entre a
/// área e a linha. Pela abertura, a tinta precisa dar a volta e por isso não escorre.
#[allow(clippy::too_many_arguments)]
fn restore(
    mask: &mut [bool],
    bb: &mut BBox,
    similar: &[bool],
    allowed: &[bool],
    dist2: &[u16],
    w: usize,
    h: usize,
    half: f32,
) {
    // Dijkstra com passos de 1 e √2: em 8 direções, erra no máximo uns 8% para cima.
    let limit = half + 1.0 + 0.06 * half;
    let candidate = |i: usize, mask: &[bool]| similar[i] && !allowed[i] && !mask[i];
    let neighbors = |i: usize| {
        let (x, y) = ((i % w) as i64, (i / w) as i64);
        const D: [(i64, i64, f32); 8] = [
            (-1, 0, 1.0),
            (1, 0, 1.0),
            (0, -1, 1.0),
            (0, 1, 1.0),
            (-1, -1, std::f32::consts::SQRT_2),
            (1, -1, std::f32::consts::SQRT_2),
            (-1, 1, std::f32::consts::SQRT_2),
            (1, 1, std::f32::consts::SQRT_2),
        ];
        D.into_iter().filter_map(move |(dx, dy, c)| {
            let (nx, ny) = (x + dx, y + dy);
            (nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h)
                .then(|| (ny as usize * w + nx as usize, c))
        })
    };
    let key = |c: f32| (c * 1024.0) as u32;
    // Ponto de partida: os pixels da faixa colada nas linhas que encostam na área. Procura
    // pelos candidatos (poucos, perto das linhas), linha a linha em paralelo.
    let (y0, y1) = (bb.y0.saturating_sub(1), (bb.y1 + 1).min(h - 1));
    let (x0, x1) = (bb.x0.saturating_sub(1), (bb.x1 + 1).min(w - 1));
    let start: Vec<(usize, f32)> = (y0..=y1)
        .into_par_iter()
        .flat_map_iter(|y| {
            let mask = &*mask;
            (x0..=x1).filter_map(move |x| {
                let i = y * w + x;
                if !candidate(i, mask) {
                    return None;
                }
                neighbors(i)
                    .filter(|(n, _)| mask[*n])
                    .map(|(_, c)| c)
                    .min_by(f32::total_cmp)
                    .map(|c| (i, c))
            })
        })
        .collect();
    let mut best: HashMap<usize, f32> = HashMap::with_capacity(start.len() * 2);
    let mut heap = BinaryHeap::with_capacity(start.len());
    for (i, c) in start {
        best.insert(i, c);
        heap.push(Reverse((key(c), i)));
    }
    let mut added = Vec::new();
    while let Some(Reverse((k, i))) = heap.pop() {
        let g = k as f32 / 1024.0;
        if mask[i] || best.get(&i).is_some_and(|b| key(*b) < k) {
            continue;
        }
        if g + f32::from(dist2[i]).sqrt() > limit {
            continue;
        }
        mask[i] = true;
        added.push(i);
        for (n, c) in neighbors(i) {
            let ng = g + c;
            if candidate(n, mask) && best.get(&n).is_none_or(|b| ng < *b) {
                best.insert(n, ng);
                heap.push(Reverse((key(ng), n)));
            }
        }
    }
    for i in added {
        bb.add(i % w, i / w);
    }
}

/// Distância ao quadrado até o pixel *desligado* mais próximo (a linha, para o balde), exata
/// até `r`; além disso `u16::MAX`. Para o raio pequeno do "fechar lacunas" sai bem mais barato
/// que a transformada inteira: distância na linha (limitada a `r + 1`) e depois o mínimo nas
/// `2r + 1` linhas vizinhas, sempre em linhas inteiras (amigável ao cache) e em paralelo.
fn near_dist_sq(similar: &[bool], w: usize, h: usize, r: usize) -> Vec<u16> {
    let cap = (r + 1) as u16;
    let mut g = vec![cap; w * h];
    g.par_chunks_mut(w)
        .zip(similar.par_chunks(w))
        .for_each(|(row, src)| {
            let mut d = cap;
            for (o, s) in row.iter_mut().zip(src) {
                d = if *s { (d + 1).min(cap) } else { 0 };
                *o = d;
            }
            let mut d = cap;
            for (o, s) in row.iter_mut().zip(src).rev() {
                d = if *s { (d + 1).min(cap) } else { 0 };
                *o = (*o).min(d);
            }
        });
    let lim = (r * r) as u16;
    let mut out = vec![u16::MAX; w * h];
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for dy in -(r as isize)..=(r as isize) {
            let ny = y as isize + dy;
            if ny < 0 || ny >= h as isize {
                continue;
            }
            let dy2 = (dy * dy) as u16;
            let src = &g[ny as usize * w..ny as usize * w + w];
            for (o, gx) in row.iter_mut().zip(src) {
                if *gx < cap {
                    *o = (*o).min(gx * gx + dy2);
                }
            }
        }
        for o in row.iter_mut() {
            if *o > lim {
                *o = u16::MAX;
            }
        }
    });
    out
}

/// O mesmo, com fechar lacunas, expandir área, borda suave e escolha da referência.
pub fn flood_fill_with(
    doc: &mut Document,
    x: i32,
    y: i32,
    color: [f32; 4],
    opts: &FillOptions,
) -> Rect {
    let bounds = doc.bounds();
    if !bounds.contains(x, y)
        || doc
            .active_layer()
            .is_none_or(|l| !l.visible || l.is_vector())
    {
        return Rect::EMPTY;
    }
    let (w, h) = (doc.width as usize, doc.height as usize);
    let (sx, sy) = (x as usize, y as usize);
    let img = sample(doc, opts.reference);
    let Some(&seed) = img.get(sy * w + sx) else {
        return Rect::EMPTY;
    };
    let tol = (opts.tolerance.clamp(0.0, 1.0) * 255.0) as u8;
    // Diferença de cada pixel para a semente (maior entre os canais).
    let (dif, similar): (Vec<u8>, Vec<bool>) = img
        .par_iter()
        .map(|p| {
            let d = (0..4).map(|c| p[c].abs_diff(seed[c])).max().unwrap_or(0);
            (d, d <= tol)
        })
        .unzip();
    drop(img);
    // Fechar lacunas: engrossa as linhas pela metade da abertura antes de preencher.
    let half = (opts.close_gap.max(0.0) / 2.0).ceil();
    let gap = (half > 0.0)
        .then(|| {
            let dist2 = near_dist_sq(&similar, w, h, half as usize);
            let allowed: Vec<bool> = dist2.par_iter().map(|d| *d == u16::MAX).collect();
            (allowed, dist2)
        })
        // Tocou num vão mais estreito que a abertura: preenche sem fechar lacunas.
        .filter(|(a, _)| a[sy * w + sx]);
    let (mut mask, mut bb) = match &gap {
        Some((allowed, _)) => scanline(allowed, w, h, sx, sy),
        None => scanline(&similar, w, h, sx, sy),
    };
    if let Some((allowed, dist2)) = &gap {
        restore(&mut mask, &mut bb, &similar, allowed, dist2, w, h, half);
    }
    drop(gap);
    if bb.is_empty() {
        return Rect::EMPTY;
    }
    // Daqui em diante, só o recorte em volta da área (mais a margem do expandir).
    let pad = opts.expand.max(0.0).ceil() as i32 + 2;
    let c = Rect::new(
        bb.x0 as i32 - pad,
        bb.y0 as i32 - pad,
        bb.x1 as i32 + 1 + pad,
        bb.y1 as i32 + 1 + pad,
    )
    .intersect(&bounds);
    let (cw, ch) = (c.width() as usize, c.height() as usize);
    let (cx, cy) = (c.x0 as usize, c.y0 as usize);
    let mut cm: Vec<bool> = (0..ch)
        .into_par_iter()
        .flat_map_iter(|ry| {
            let row = (cy + ry) * w + cx;
            mask[row..row + cw].to_vec()
        })
        .collect();
    drop(mask);
    if opts.expand != 0.0 {
        cm = crate::mask::grow(&cm, cw, ch, opts.expand);
    }
    let range = 255u8.saturating_sub(tol).max(1) as f32;
    let soft = |d: u8| (1.0 - d.saturating_sub(tol) as f32 / range).clamp(0.0, 1.0);
    // Cobertura 0..1 da tinta. A franja de 1 px em volta cobre o antisserrilhado da linha:
    // inteira (borda dura) ou na medida em que o pixel ainda parece com a semente (suave).
    let mut cov = vec![0.0f32; cw * ch];
    cov.par_chunks_mut(cw).enumerate().for_each(|(ry, row)| {
        for (rx, o) in row.iter_mut().enumerate() {
            let i = ry * cw + rx;
            *o = if cm[i] {
                1.0
            } else if (rx > 0 && cm[i - 1])
                || (rx + 1 < cw && cm[i + 1])
                || (ry > 0 && cm[i - cw])
                || (ry + 1 < ch && cm[i + cw])
            {
                if !opts.antialias {
                    1.0
                } else if opts.expand != 0.0 {
                    0.5
                } else {
                    soft(dif[(cy + ry) * w + cx + rx])
                }
            } else {
                0.0
            };
        }
    });
    drop(dif);
    let mut bb = BBox::EMPTY;
    for (ry, row) in cov.chunks_exact(cw).enumerate() {
        let (Some(a), Some(b)) = (
            row.iter().position(|v| *v > 0.0),
            row.iter().rposition(|v| *v > 0.0),
        ) else {
            continue;
        };
        bb.add_span(a, b, ry);
    }
    if bb.is_empty() {
        return Rect::EMPTY;
    }
    let r = Rect::new(
        (cx + bb.x0) as i32,
        (cy + bb.y0) as i32,
        (cx + bb.x1) as i32 + 1,
        (cy + bb.y1) as i32 + 1,
    );
    let sel_mask = doc.selection.as_ref().map(|s| s.mask.clone());
    let erase = color[3] <= 0.0;
    doc.checkpoint();
    let Some(layer) = doc.active_layer_mut() else {
        return Rect::EMPTY;
    };
    let rw = r.width() as usize;
    let lock = layer.lock_alpha;
    // Em faixas de 256 linhas, para não pedir centenas de MB de uma vez numa tela grande.
    let (mut px, mut out) = (Vec::new(), Vec::new());
    let mut y0 = r.y0;
    while y0 < r.y1 {
        let r = Rect::new(r.x0, y0, r.x1, (y0 + 256).min(r.y1));
        y0 = r.y1;
        let n = rw * r.height() as usize;
        px.resize(n, [0.0f32; 4]);
        layer.surface.read_rgba_into(r, &mut px);
        out.resize(n * 4, 0.0f32);
        let sel = sel_mask.as_ref().map(|m| m.read_region(r));
        out.par_chunks_mut(rw * 4)
            .zip(px.par_chunks(rw))
            .enumerate()
            .for_each(|(ry, (dst, src))| {
                let crow = (r.y0 as usize + ry - cy) * cw + (r.x0 as usize - cx);
                for (j, (o, p)) in dst.as_chunks_mut::<4>().0.iter_mut().zip(src).enumerate() {
                    let s = sel
                        .as_ref()
                        .map_or(1.0, |m| m.get(ry * rw + j).copied().unwrap_or(0.0));
                    let k = (cov[crow + j] * s).clamp(0.0, 1.0);
                    *o = if k <= 0.0 || (lock && p[3] <= 0.0) {
                        *p
                    } else if k >= 1.0 && (erase || color[3] >= 1.0) {
                        // Caminho rápido (o grosso da área): cobre por inteiro.
                        if erase {
                            [p[0], p[1], p[2], 0.0]
                        } else {
                            [color[0], color[1], color[2], if lock { p[3] } else { 1.0 }]
                        }
                    } else if erase {
                        [p[0], p[1], p[2], p[3] * (1.0 - k)]
                    } else {
                        let c = photocraft_color::blend::composite(
                            photocraft_color::BlendMode::Normal,
                            *p,
                            color,
                            k,
                        );
                        [c[0], c[1], c[2], if lock { p[3] } else { c[3] }]
                    };
                }
            });
        layer.surface.write_region(r, &out);
    }
    r
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn fills_inside_a_closed_shape_only() {
        let mut doc = Document::new(20, 20);
        let black = [0.0, 0.0, 0.0, 1.0];
        let s = &mut doc.layers[0].surface;
        for i in 5..15 {
            s.write_pixel(i, 5, &black);
            s.write_pixel(i, 14, &black);
            s.write_pixel(5, i, &black);
            s.write_pixel(14, i, &black);
        }
        let r = flood_fill(&mut doc, 10, 10, [1.0, 0.0, 0.0, 1.0], 0.1);
        assert!(!r.is_empty());
        assert_eq!(doc.layers[0].surface.rgba(10, 10), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(doc.layers[0].surface.rgba(1, 1)[3], 0.0);
        assert!(doc.undo());
        assert_eq!(doc.layers[0].surface.rgba(10, 10)[3], 0.0);
    }

    #[test]
    fn reference_layer_bounds_the_fill_on_another_layer() {
        let mut doc = Document::new(20, 20);
        let black = [0.0, 0.0, 0.0, 1.0];
        // Lineart na camada de baixo, marcada como referência; o balde pinta na de cima.
        for i in 0..20 {
            doc.layers[0].surface.write_pixel(10, i, &black);
        }
        doc.layers[0].reference = true;
        doc.add_layer();
        let r = flood_fill(&mut doc, 2, 2, [1.0, 0.0, 0.0, 1.0], 0.1);
        assert!(!r.is_empty());
        assert_eq!(doc.layers[1].surface.rgba(2, 2), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(doc.layers[1].surface.rgba(15, 2)[3], 0.0);
    }

    #[test]
    fn fill_stays_inside_the_selection() {
        let mut doc = Document::new(20, 20);
        doc.set_selection(crate::Selection::rect([0.0, 0.0], [5.0, 5.0], 20, 20));
        flood_fill(&mut doc, 2, 2, [1.0, 0.0, 0.0, 1.0], 0.1);
        assert_eq!(doc.layers[0].surface.rgba(2, 2)[3], 1.0);
        assert_eq!(doc.layers[0].surface.rgba(12, 12)[3], 0.0);
    }

    #[test]
    fn close_gap_stops_the_fill_at_a_small_opening() {
        let black = [0.0, 0.0, 0.0, 1.0];
        let make = || {
            let mut doc = Document::new(40, 40);
            let s = &mut doc.layers[0].surface;
            // Quadrado de 10..30 com uma abertura de 3 px no lado de cima.
            for i in 10..30 {
                if !(18..21).contains(&i) {
                    s.write_pixel(i, 10, &black);
                }
                s.write_pixel(i, 29, &black);
                s.write_pixel(10, i, &black);
                s.write_pixel(29, i, &black);
            }
            doc
        };
        let red = [1.0, 0.0, 0.0, 1.0];
        let mut leaky = make();
        flood_fill(&mut leaky, 20, 20, red, 0.1);
        assert_eq!(leaky.layers[0].surface.rgba(2, 2), red);
        let mut closed = make();
        let opts = FillOptions {
            close_gap: 5.0,
            ..FillOptions::default()
        };
        flood_fill_with(&mut closed, 20, 20, red, &opts);
        assert_eq!(closed.layers[0].surface.rgba(20, 20), red);
        assert_eq!(closed.layers[0].surface.rgba(2, 2)[3], 0.0);
        // Expandir área passa por baixo da linha.
        let mut grown = make();
        let opts = FillOptions {
            close_gap: 5.0,
            expand: 2.0,
            ..FillOptions::default()
        };
        flood_fill_with(&mut grown, 20, 20, [0.0, 1.0, 0.0, 1.0], &opts);
        assert_eq!(grown.layers[0].surface.rgba(10, 20), [0.0, 1.0, 0.0, 1.0]);
    }

    /// Quadrado 10..30 com uma abertura de `gap` px no lado de cima, linha de `t` px.
    fn open_square(gap: i32, t: i32) -> Document {
        let black = [0.0, 0.0, 0.0, 1.0];
        let mut doc = Document::new(60, 60);
        let s = &mut doc.layers[0].surface;
        for k in 0..t {
            for i in 10..40 {
                let c = 25 - gap / 2;
                if !(c..c + gap).contains(&i) {
                    s.write_pixel(i, 10 + k, &black);
                }
                s.write_pixel(i, 39 - k, &black);
                s.write_pixel(10 + k, i, &black);
                s.write_pixel(39 - k, i, &black);
            }
        }
        doc
    }

    #[test]
    fn closed_gap_does_not_spill_outside_the_opening() {
        let red = [1.0, 0.0, 0.0, 1.0];
        for (gap, t) in [(2, 1), (4, 1), (6, 2), (9, 3), (14, 2)] {
            let mut doc = open_square(gap, t);
            let opts = FillOptions {
                close_gap: gap as f32 + 1.0,
                antialias: false,
                ..FillOptions::default()
            };
            flood_fill_with(&mut doc, 25, 25, red, &opts);
            let s = &doc.layers[0].surface;
            // Dentro: até encostar na linha, também nos cantos retos de perto.
            assert_eq!(s.rgba(25, 25), red, "gap {gap}");
            assert_eq!(s.rgba(10 + t, 25), red, "gap {gap}: encosta na parede");
            assert_eq!(
                s.rgba(25, 10 + t),
                red,
                "gap {gap}: encosta embaixo da abertura"
            );
            // Fora: nada, nem colado na abertura (a franja de 1 px pode tocar a linha de fora).
            for y in 0..(10 - 1) {
                for x in 0..60 {
                    assert_eq!(s.rgba(x, y)[3], 0.0, "gap {gap}: vazou em ({x},{y})");
                }
            }
        }
    }

    #[test]
    fn soft_edge_paints_partially_under_antialiased_line() {
        let mut doc = Document::new(20, 20);
        let s = &mut doc.layers[0].surface;
        for i in 0..20 {
            s.write_pixel(10, i, &[0.0, 0.0, 0.0, 1.0]);
            s.write_pixel(9, i, &[0.0, 0.0, 0.0, 0.5]);
        }
        let red = [1.0, 0.0, 0.0, 1.0];
        let opts = FillOptions::default();
        flood_fill_with(&mut doc, 2, 2, red, &opts);
        let a = doc.layers[0].surface.rgba(9, 5);
        // A franja meio cinza recebe um pouco de vermelho, sem virar vermelho puro.
        assert!(a[0] > 0.1 && a[0] < 0.9, "{a:?}");
        assert_eq!(doc.layers[0].surface.rgba(12, 5)[3], 0.0);
        let mut hard = Document::new(20, 20);
        for i in 0..20 {
            hard.layers[0]
                .surface
                .write_pixel(10, i, &[0.0, 0.0, 0.0, 1.0]);
            hard.layers[0]
                .surface
                .write_pixel(9, i, &[0.0, 0.0, 0.0, 0.5]);
        }
        let opts = FillOptions {
            antialias: false,
            ..FillOptions::default()
        };
        flood_fill_with(&mut hard, 2, 2, red, &opts);
        assert_eq!(hard.layers[0].surface.rgba(9, 5), red);
    }

    #[test]
    fn current_layer_reference_ignores_lines_on_other_layers() {
        let mut doc = Document::new(20, 20);
        for i in 0..20 {
            doc.layers[0]
                .surface
                .write_pixel(10, i, &[0.0, 0.0, 0.0, 1.0]);
        }
        doc.add_layer();
        let red = [1.0, 0.0, 0.0, 1.0];
        let opts = FillOptions {
            reference: FillRef::Layer,
            ..FillOptions::default()
        };
        flood_fill_with(&mut doc, 2, 2, red, &opts);
        assert_eq!(doc.layers[1].surface.rgba(15, 2), red);
        let mut all = Document::new(20, 20);
        for i in 0..20 {
            all.layers[0]
                .surface
                .write_pixel(10, i, &[0.0, 0.0, 0.0, 1.0]);
        }
        all.add_layer();
        let opts = FillOptions {
            reference: FillRef::All,
            ..FillOptions::default()
        };
        flood_fill_with(&mut all, 2, 2, red, &opts);
        assert_eq!(all.layers[1].surface.rgba(15, 2)[3], 0.0);
        // Um passo só de desfazer.
        assert!(all.undo());
        assert_eq!(all.layers[1].surface.rgba(2, 2)[3], 0.0);
    }

    #[test]
    fn contract_shrinks_the_fill() {
        let mut doc = open_square(0, 1);
        let red = [1.0, 0.0, 0.0, 1.0];
        let opts = FillOptions {
            expand: -3.0,
            antialias: false,
            ..FillOptions::default()
        };
        flood_fill_with(&mut doc, 25, 25, red, &opts);
        let s = &doc.layers[0].surface;
        assert_eq!(s.rgba(25, 25), red);
        assert_eq!(s.rgba(12, 25)[3], 0.0);
    }

    #[test]
    fn near_distance_matches_the_full_transform_inside_the_radius() {
        let (w, h) = (37, 23);
        let on: Vec<bool> = (0..w * h).map(|i| (i * 7919) % 31 == 0).collect();
        let off: Vec<bool> = on.iter().map(|b| !b).collect();
        let full = crate::mask::distance_sq(&on, w, h);
        let near = near_dist_sq(&off, w, h, 4);
        for (a, b) in full.iter().zip(&near) {
            if *a <= 16.0 {
                assert_eq!(*a, f32::from(*b));
            } else {
                assert_eq!(*b, u16::MAX);
            }
        }
    }
}
