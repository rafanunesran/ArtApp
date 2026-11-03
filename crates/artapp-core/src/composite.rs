//! Composição das camadas para exibição e exportação, com os modos de mesclagem do PhotoCraft.

use photocraft_color::{BlendMode, blend};
use photocraft_geom::Rect;
use photocraft_raster::Surface;

use crate::document::{Document, Layer};

/// Quais camadas entram na composição.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    /// Tudo que está visível (a tela de desenho, o conta-gotas).
    Display,
    /// Sem as camadas de rascunho (exportar, balde olhando todas as camadas).
    Final,
    /// Só as camadas de referência (balde quando alguma camada é de referência).
    Reference,
    /// Tudo menos as camadas de referência (conta-gotas que ignora a referência).
    NoReference,
    /// Só a camada de índice dado (conta-gotas na camada atual).
    Only(usize),
}

/// De onde o conta-gotas pega a cor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SampleFrom {
    /// A imagem como aparece na tela.
    #[default]
    All,
    /// Tudo, menos as camadas de referência.
    NoReference,
    /// Só a camada ativa (sem o papel).
    Active,
}

/// Compõe `r` (recortado à tela) em RGBA 8 bits não pré-multiplicado, linha a linha.
/// Com `with_paper = false` o papel fica transparente (exportar com fundo transparente).
/// Devolve o retângulo efetivamente composto e os pixels.
pub fn composite_rgba8(doc: &Document, r: Rect, with_paper: bool) -> (Rect, Vec<[u8; 4]>) {
    composite_pass(doc, r, with_paper, Pass::Display)
}

/// Como [`composite_rgba8`], escolhendo as camadas pelo `pass`. Com a animação ligada, a tela
/// (`Pass::Display`) mostra só o quadro atual e a pele de cebola.
pub fn composite_pass(
    doc: &Document,
    r: Rect,
    with_paper: bool,
    pass: Pass,
) -> (Rect, Vec<[u8; 4]>) {
    let a = &doc.animation;
    let view = (pass == Pass::Display && a.enabled).then_some((a.current, !a.pages));
    composite_view(doc, r, with_paper, pass, view)
}

/// O quadro `k` inteiro da animação, sem pele de cebola (exportar).
pub fn composite_frame(doc: &Document, k: usize, with_paper: bool) -> Vec<[u8; 4]> {
    composite_view(doc, doc.bounds(), with_paper, Pass::Final, Some((k, false))).1
}

/// `view`: quadro mostrado e se há pele de cebola.
pub(crate) fn composite_view(
    doc: &Document,
    r: Rect,
    with_paper: bool,
    pass: Pass,
    view: Option<(usize, bool)>,
) -> (Rect, Vec<[u8; 4]>) {
    let r = r.intersect(&doc.bounds());
    if r.is_empty() {
        return (r, Vec::new());
    }
    let base = if with_paper { doc.paper } else { [0.0; 4] };
    let job = Job { doc, pass, view };
    let px = in_bands(r, |band, acc| {
        acc.fill(base);
        job.run(band, 0, acc);
    });
    (r, px)
}

/// Linhas por faixa na composição em paralelo (cada faixa vai para um núcleo).
const BAND_ROWS: i32 = 64;
/// Abaixo disso (em pixels) compõe num núcleo só: dividir custaria mais que ganharia.
const PARALLEL_MIN: usize = 96 * 1024;

/// Compõe `r` em faixas de linhas, em paralelo quando a área é grande. `fill` deixa em `acc` os
/// pixels compostos (não pré-multiplicados) da faixa, e cada faixa vira RGBA 8 bits.
fn in_bands(r: Rect, fill: impl Fn(Rect, &mut [[f32; 4]]) + Sync) -> Vec<[u8; 4]> {
    let w = r.width() as usize;
    let n = w * r.height() as usize;
    let mut out = vec![[0u8; 4]; n];
    let one = |band: Rect, chunk: &mut [[u8; 4]]| {
        let mut acc = vec![[0.0f32; 4]; chunk.len()];
        fill(band, &mut acc);
        quantize(&acc, chunk);
    };
    if n < PARALLEL_MIN {
        one(r, &mut out);
    } else {
        use rayon::prelude::*;
        out.par_chunks_mut(w * BAND_ROWS as usize)
            .enumerate()
            .for_each(|(k, chunk)| {
                let y0 = r.y0 + k as i32 * BAND_ROWS;
                let band = Rect::new(r.x0, y0, r.x1, y0 + (chunk.len() / w) as i32);
                one(band, chunk);
            });
    }
    out
}

fn quantize(acc: &[[f32; 4]], out: &mut [[u8; 4]]) {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    for (o, p) in out.iter_mut().zip(acc) {
        *o = [q(p[0]), q(p[1]), q(p[2]), q(p[3])];
    }
}

/// Mesclagem normal ("por cima"), a mais comum, sem passar pela tabela de modos.
#[inline]
fn over(dst: [f32; 4], src: [f32; 4], opacity: f32) -> [f32; 4] {
    let a = src[3] * opacity;
    if a >= 1.0 {
        return [src[0], src[1], src[2], 1.0];
    }
    let keep = dst[3] * (1.0 - a);
    let ao = a + keep;
    if ao <= 0.0 {
        return [0.0; 4];
    }
    let inv = 1.0 / ao;
    [
        (keep * dst[0] + a * src[0]) * inv,
        (keep * dst[1] + a * src[1]) * inv,
        (keep * dst[2] + a * src[2]) * inv,
        ao,
    ]
}

#[inline]
fn mix(mode: BlendMode, dst: [f32; 4], src: [f32; 4], opacity: f32) -> [f32; 4] {
    if mode == BlendMode::Normal {
        over(dst, src, opacity)
    } else {
        blend::composite(mode, dst, src, opacity)
    }
}

/// Como a camada `i` aparece: opacidade final (com pasta e quadro) e a cor da pele de cebola.
fn layer_look(doc: &Document, i: usize, view: Option<(usize, bool)>) -> (f32, Option<[f32; 3]>) {
    use crate::animation::FrameLook;
    let layer = &doc.layers[i];
    let fold = doc.folder_mix(i);
    let look = view.map_or(FrameLook::Shown, |(k, onion)| doc.frame_look(i, k, onion));
    let (look_k, tint) = match look {
        FrameLook::Shown => (1.0, None),
        FrameLook::Onion(o, t) => (o, t),
        FrameLook::Hidden => (0.0, None),
    };
    (layer.opacity * fold.unwrap_or(0.0) * look_k, tint)
}

/// O que compor: o documento, as camadas que entram e o quadro da animação.
struct Job<'a> {
    doc: &'a Document,
    pass: Pass,
    view: Option<(usize, bool)>,
}

impl Job<'_> {
    fn included(&self, i: usize) -> bool {
        let layer = &self.doc.layers[i];
        match self.pass {
            Pass::Display => true,
            Pass::Final => !layer.draft,
            Pass::Reference => layer.reference,
            Pass::NoReference => !layer.reference,
            Pass::Only(k) => i == k,
        }
    }

    /// Compõe as camadas de `from` para cima sobre `acc` (o retângulo `r`). `from` tem que ser 0
    /// ou uma camada comum (sem recorte, máscara ou pasta), onde o grupo de recorte recomeça.
    fn run(&self, r: Rect, from: usize, acc: &mut [[f32; 4]]) {
        self.run_range(r, from, self.doc.layers.len(), acc);
    }

    /// Como [`Self::run`], parando antes da camada `to`.
    fn run_range(&self, r: Rect, from: usize, to: usize, acc: &mut [[f32; 4]]) {
        let doc = self.doc;
        let n = acc.len();
        let mut row = vec![[0.0f32; 4]; n];
        // Alfa da camada base do grupo de recorte atual (`false` = a base não aparece aqui, e
        // as camadas recortadas nela somem).
        let mut clip_alpha = vec![0.0f32; n];
        let mut clip_on = false;
        let mut mask_row = Vec::new();
        let mut pieces = Vec::new();
        let w = r.width() as usize;
        for i in from..to.min(doc.layers.len()) {
            let layer = &doc.layers[i];
            // A máscara entra junto com a camada de baixo, não sozinha. Pastas não têm tinta:
            // só escondem e esmaecem o que está dentro.
            if layer.mask || layer.folder {
                continue;
            }
            let (opacity, tint) = layer_look(doc, i, self.view);
            let shown = self.included(i) && layer.visible && opacity > 0.0;
            if !layer.clip {
                clip_on = false;
            }
            if !shown {
                continue;
            }
            let reach = layer.border.map_or(r, |b| r.inflate(b.pad()));
            if !layer.surface.has_tiles_in(reach) {
                // Base vazia neste trecho: as camadas recortadas nela também somem aqui.
                continue;
            }
            if layer.clip && !clip_on {
                continue;
            }
            // Só os pedaços onde a camada tem tiles: o resto é transparente e não muda nada
            // (camadas de traço costumam cobrir uma parte pequena da tela). A borda passa da
            // tinta, então essas camadas vão inteiras.
            pieces.clear();
            if layer.border.is_some() {
                pieces.push(r);
            } else {
                pieces.extend(
                    r.tiles()
                        .filter(|c| layer.surface.tile(*c).is_some())
                        .map(|c| c.rect().intersect(&r)),
                );
            }
            if !layer.clip {
                clip_on = true;
                if pieces.len() != 1 || pieces[0] != r {
                    clip_alpha.fill(0.0);
                }
            }
            for &p in &pieces {
                let pn = p.width() as usize * p.height() as usize;
                let row = &mut row[..pn];
                match &layer.border {
                    Some(b) => {
                        crate::border::read_with_border(&layer.surface, b, p, doc.bounds(), row)
                    }
                    None => layer.surface.read_rgba_into(p, row),
                }
                if let Some(t) = &layer.tone {
                    crate::tone::apply(t, p, row);
                }
                if let Some(c) = tint {
                    // Pele de cebola colorida: a tinta vira a cor do lado (mantendo o alfa).
                    for px in row.iter_mut() {
                        px[0] = c[0];
                        px[1] = c[1];
                        px[2] = c[2];
                    }
                }
                if let Some(m) = doc.mask_above(i) {
                    mask_row.resize(pn, [0.0f32; 4]);
                    m.surface.read_rgba_into(p, &mut mask_row[..pn]);
                    for (px, k) in row.iter_mut().zip(&mask_row) {
                        px[3] *= mask_visibility(*k);
                    }
                }
                let mode = layer.blend;
                let pw = p.width() as usize;
                for y in p.y0..p.y1 {
                    let o = ((y - r.y0) as usize) * w + (p.x0 - r.x0) as usize;
                    let src = &row[((y - p.y0) as usize) * pw..][..pw];
                    let dst = &mut acc[o..o + pw];
                    let ca = &mut clip_alpha[o..o + pw];
                    if layer.clip {
                        for ((d, s), m) in dst.iter_mut().zip(src).zip(ca.iter()) {
                            let a = s[3] * m;
                            if a > 0.0 {
                                *d = mix(mode, *d, [s[0], s[1], s[2], a], opacity);
                            }
                        }
                    } else {
                        for ((d, s), m) in dst.iter_mut().zip(src).zip(ca.iter_mut()) {
                            *m = s[3];
                            if s[3] > 0.0 {
                                *d = mix(mode, *d, *s, opacity);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Onde a tela pode ser dividida para guardar o que fica abaixo da camada ativa: a camada comum
/// mais alta até a ativa (se a ativa está recortada, a base do grupo dela).
fn split_index(doc: &Document) -> usize {
    let mut s = doc.active.min(doc.layers.len().saturating_sub(1));
    while s > 0 {
        let l = &doc.layers[s];
        if !(l.clip || l.mask || l.folder) {
            break;
        }
        s -= 1;
    }
    s
}

/// Identifica os tiles de uma superfície (os ponteiros, que são copy-on-write: mudar a tinta
/// troca o tile). Barato: uma tela A4 tem algumas dezenas de tiles por camada.
pub fn surface_key(s: &photocraft_raster::Surface) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.tile_count().hash(&mut h);
    for (c, t) in s.tiles() {
        (c.tx, c.ty, std::sync::Arc::as_ptr(t) as usize).hash(&mut h);
    }
    h.finish()
}

/// Lado dos blocos guardados pelo [`DisplayCache`].
const BLOCK: i32 = 64;
const BLOCK_BYTES: usize = (BLOCK * BLOCK) as usize * 16;

/// Guarda a composição das camadas abaixo da ativa enquanto se pinta nela, para que cada quadro
/// do traço só misture a camada ativa e as de cima. Com muitas camadas é a maior parte do
/// trabalho da tela. Os blocos (64 × 64) são feitos na hora em que o traço passa e jogados fora
/// quando qualquer coisa abaixo muda (camadas, propriedades, histórico, papel ou quadro).
pub struct DisplayCache {
    key: u64,
    split: usize,
    blocks: std::collections::HashMap<(i32, i32), Box<[[f32; 4]]>>,
    /// Memória máxima dos blocos, em bytes. Passou disso, recomeça do zero.
    pub budget: usize,
}

impl Default for DisplayCache {
    fn default() -> Self {
        Self {
            key: 0,
            split: 0,
            blocks: Default::default(),
            budget: 64 << 20,
        }
    }
}

impl DisplayCache {
    /// Libera a memória dos blocos (ex.: no fim do traço).
    pub fn clear(&mut self) {
        self.blocks = Default::default();
        self.key = 0;
    }

    /// Bytes ocupados agora.
    pub fn bytes(&self) -> usize {
        self.blocks.len() * BLOCK_BYTES
    }

    fn key_for(doc: &Document, split: usize, view: Option<(usize, bool)>) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (doc.history_version(), split, doc.width, doc.height, view).hash(&mut h);
        doc.paper.map(f32::to_bits).hash(&mut h);
        for i in 0..split {
            let l = &doc.layers[i];
            let (opacity, tint) = layer_look(doc, i, view);
            (l.id, l.visible, l.blend, l.clip, l.mask, l.folder, l.draft).hash(&mut h);
            opacity.to_bits().hash(&mut h);
            tint.map(|c| c.map(f32::to_bits)).hash(&mut h);
            l.border
                .map(|b| (b.width.to_bits(), b.color.map(f32::to_bits)))
                .hash(&mut h);
            l.tone
                .map(|t| (t.period.to_bits(), t.angle.to_bits()))
                .hash(&mut h);
            surface_key(&l.surface).hash(&mut h);
        }
        h.finish()
    }

    /// Como [`composite_rgba8`] com papel (a tela), reaproveitando o que já foi composto abaixo
    /// da camada ativa. Áreas grandes (abrir, desfazer tudo) vão direto, sem guardar nada.
    pub fn composite(&mut self, doc: &Document, r: Rect) -> (Rect, Vec<[u8; 4]>) {
        let r = r.intersect(&doc.bounds());
        if r.is_empty() {
            return (r, Vec::new());
        }
        let a = &doc.animation;
        let view = a.enabled.then_some((a.current, !a.pages));
        let split = split_index(doc);
        let area = r.width() as usize * r.height() as usize * 16;
        if split == 0 || area * 4 > self.budget {
            return composite_view(doc, r, true, Pass::Display, view);
        }
        let key = Self::key_for(doc, split, view);
        if key != self.key || split != self.split {
            self.blocks.clear();
            self.key = key;
            self.split = split;
        }
        let (bx0, by0) = (r.x0.div_euclid(BLOCK), r.y0.div_euclid(BLOCK));
        let (bx1, by1) = ((r.x1 - 1).div_euclid(BLOCK), (r.y1 - 1).div_euclid(BLOCK));
        let missing: Vec<(i32, i32)> = (by0..=by1)
            .flat_map(|by| (bx0..=bx1).map(move |bx| (bx, by)))
            .filter(|b| !self.blocks.contains_key(b))
            .collect();
        if (self.blocks.len() + missing.len()) * BLOCK_BYTES > self.budget {
            // Cheio: recomeça (o traço segue em outro canto da tela).
            self.blocks.clear();
            return self.composite(doc, r);
        }
        let job = Job {
            doc,
            pass: Pass::Display,
            view,
        };
        let bounds = doc.bounds();
        let make = |&(bx, by): &(i32, i32)| {
            let br = Rect::new(bx * BLOCK, by * BLOCK, (bx + 1) * BLOCK, (by + 1) * BLOCK)
                .intersect(&bounds);
            let mut acc = vec![doc.paper; (BLOCK * BLOCK) as usize];
            let n = br.width() as usize * br.height() as usize;
            job.run_range(br, 0, split, &mut acc[..n]);
            ((bx, by), acc.into_boxed_slice())
        };
        let made: Vec<_> = if missing.len() > 2 {
            use rayon::prelude::*;
            missing.par_iter().map(make).collect()
        } else {
            missing.iter().map(make).collect()
        };
        self.blocks.extend(made);
        let blocks = &self.blocks;
        let px = in_bands(r, |band, acc| {
            let w = band.width() as usize;
            for (bk, block) in blocks.iter().filter(|(k, _)| {
                k.1 >= band.y0.div_euclid(BLOCK)
                    && k.1 <= (band.y1 - 1).div_euclid(BLOCK)
                    && k.0 >= bx0
                    && k.0 <= bx1
            }) {
                let br = Rect::new(
                    bk.0 * BLOCK,
                    bk.1 * BLOCK,
                    (bk.0 + 1) * BLOCK,
                    (bk.1 + 1) * BLOCK,
                )
                .intersect(&bounds);
                let bw = br.width() as usize;
                let part = br.intersect(&band);
                if part.is_empty() {
                    continue;
                }
                let span = part.width() as usize;
                for y in part.y0..part.y1 {
                    let src = ((y - br.y0) as usize) * bw + (part.x0 - br.x0) as usize;
                    let dst = ((y - band.y0) as usize) * w + (part.x0 - band.x0) as usize;
                    acc[dst..dst + span].copy_from_slice(&block[src..src + span]);
                }
            }
            job.run(band, split, acc);
        });
        (r, px)
    }
}

/// Quanto um pixel da máscara deixa ver: o cinza dele sobre branco (sem tinta = tudo aparece,
/// preto = some).
pub fn mask_visibility(p: [f32; 4]) -> f32 {
    let l = 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
    (l * p[3] + 1.0 - p[3]).clamp(0.0, 1.0)
}

/// Cor composta (com papel) num pixel da tela, para o conta-gotas.
pub fn sample(doc: &Document, x: i32, y: i32) -> Option<[f32; 4]> {
    sample_from(doc, x, y, SampleFrom::All)
}

/// Cor no pixel `(x, y)` olhando só as camadas de `from`. Na camada ativa o papel não entra:
/// alfa 0 = lugar transparente.
pub fn sample_from(doc: &Document, x: i32, y: i32, from: SampleFrom) -> Option<[f32; 4]> {
    let r = Rect::new(x, y, x + 1, y + 1);
    let (_, px) = match from {
        SampleFrom::All => composite_rgba8(doc, r, true),
        SampleFrom::NoReference => {
            let a = &doc.animation;
            let view = a.enabled.then_some((a.current, !a.pages));
            composite_view(doc, r, true, Pass::NoReference, view)
        }
        SampleFrom::Active => composite_view(doc, r, false, Pass::Only(doc.active), None),
    };
    px.first().map(|p| {
        [
            p[0] as f32 / 255.0,
            p[1] as f32 / 255.0,
            p[2] as f32 / 255.0,
            p[3] as f32 / 255.0,
        ]
    })
}

/// Mescla `top` em `dst` dentro da tela `width × height`.
pub fn merge_into(dst: &mut Surface, top: &Layer, width: u32, height: u32) {
    if !top.visible {
        return;
    }
    let r = top
        .surface
        .content_bounds()
        .intersect(&Rect::new(0, 0, width as i32, height as i32));
    if r.is_empty() {
        return;
    }
    let n = r.width() as usize * r.height() as usize;
    let mut a = vec![[0.0f32; 4]; n];
    let mut b = vec![[0.0f32; 4]; n];
    dst.read_rgba_into(r, &mut a);
    top.surface.read_rgba_into(r, &mut b);
    let mut out = Vec::with_capacity(n * 4);
    for (d, s) in a.iter().zip(&b) {
        // Uma camada recortada mesclada na base só fica onde a base tem tinta.
        let s = if top.clip {
            [s[0], s[1], s[2], s[3] * d[3]]
        } else {
            *s
        };
        let p = if s[3] > 0.0 {
            blend::composite(top.blend, *d, s, top.opacity)
        } else {
            *d
        };
        out.extend_from_slice(&p);
    }
    dst.write_region(r, &out);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::BlendMode;

    fn paint(doc: &mut Document, layer: usize, r: Rect, c: [f32; 4]) {
        doc.layers[layer].surface.fill_rect(r, &c);
    }

    #[test]
    fn layers_stack_over_paper() {
        let mut doc = Document::new(10, 10);
        paint(&mut doc, 0, Rect::new(0, 0, 5, 10), [1.0, 0.0, 0.0, 1.0]);
        let (_, px) = composite_rgba8(&doc, doc.bounds(), true);
        assert_eq!(px[0], [255, 0, 0, 255]);
        assert_eq!(px[9], [255, 255, 255, 255]);
        let (_, px) = composite_rgba8(&doc, doc.bounds(), false);
        assert_eq!(px[9][3], 0);
    }

    #[test]
    fn multiply_and_opacity() {
        let mut doc = Document::new(4, 4);
        let b = doc.bounds();
        paint(&mut doc, 0, b, [1.0, 1.0, 0.0, 1.0]);
        doc.add_layer();
        paint(&mut doc, 1, b, [0.0, 1.0, 1.0, 1.0]);
        doc.layers[1].blend = BlendMode::Multiply;
        assert_eq!(
            composite_rgba8(&doc, doc.bounds(), true).1[0],
            [0, 255, 0, 255]
        );
        doc.layers[1].blend = BlendMode::Normal;
        doc.layers[1].opacity = 0.5;
        let p = composite_rgba8(&doc, doc.bounds(), true).1[0];
        assert!((p[0] as i32 - 128).abs() <= 1 && p[1] == 255, "{p:?}");
    }

    #[test]
    fn clipped_layer_shows_only_over_its_base() {
        let mut doc = Document::new(4, 1);
        paint(&mut doc, 0, Rect::new(0, 0, 2, 1), [0.0, 0.0, 1.0, 1.0]);
        doc.add_layer();
        paint(&mut doc, 1, Rect::new(0, 0, 4, 1), [1.0, 0.0, 0.0, 1.0]);
        doc.layers[1].clip = true;
        let px = composite_rgba8(&doc, doc.bounds(), true).1;
        assert_eq!(px[0], [255, 0, 0, 255]);
        assert_eq!(px[3], [255, 255, 255, 255]);
        // Base oculta: o recorte some junto.
        doc.layers[0].visible = false;
        assert_eq!(
            composite_rgba8(&doc, doc.bounds(), true).1[0],
            [255, 255, 255, 255]
        );
    }

    #[test]
    fn eyedropper_can_skip_reference_or_read_only_the_active_layer() {
        let mut doc = Document::new(2, 1);
        let b = doc.bounds();
        paint(&mut doc, 0, b, [1.0, 0.0, 0.0, 1.0]);
        doc.add_layer();
        paint(&mut doc, 1, Rect::new(0, 0, 1, 1), [0.0, 0.0, 1.0, 1.0]);
        doc.layers[1].reference = true;
        doc.active = 0;
        let near = |c: Option<[f32; 4]>, want: [f32; 4]| {
            let c = c.unwrap();
            assert!(
                c.iter().zip(want).all(|(a, b)| (a - b).abs() < 0.01),
                "{c:?}"
            );
        };
        near(sample(&doc, 0, 0), [0.0, 0.0, 1.0, 1.0]);
        near(
            sample_from(&doc, 0, 0, SampleFrom::NoReference),
            [1.0, 0.0, 0.0, 1.0],
        );
        doc.active = 1;
        near(
            sample_from(&doc, 0, 0, SampleFrom::Active),
            [0.0, 0.0, 1.0, 1.0],
        );
        // Fora da tinta da camada ativa: transparente, mesmo com papel e camada embaixo.
        assert_eq!(sample_from(&doc, 1, 0, SampleFrom::Active).unwrap()[3], 0.0);
    }

    #[test]
    fn drafts_stay_out_of_the_final_image() {
        let mut doc = Document::new(2, 1);
        let b = doc.bounds();
        paint(&mut doc, 0, b, [0.0, 0.0, 0.0, 1.0]);
        doc.layers[0].draft = true;
        assert_eq!(
            composite_rgba8(&doc, doc.bounds(), true).1[0],
            [0, 0, 0, 255]
        );
        assert_eq!(
            composite_pass(&doc, doc.bounds(), true, Pass::Final).1[0],
            [255, 255, 255, 255]
        );
    }

    #[test]
    fn merge_down_keeps_the_look() {
        let mut doc = Document::new(8, 8);
        let b = doc.bounds();
        paint(&mut doc, 0, b, [0.2, 0.4, 0.6, 1.0]);
        doc.add_layer();
        paint(&mut doc, 1, Rect::new(0, 0, 4, 4), [1.0, 1.0, 1.0, 1.0]);
        doc.layers[1].blend = BlendMode::Screen;
        doc.layers[1].opacity = 0.7;
        let before = composite_rgba8(&doc, doc.bounds(), true).1;
        doc.merge_down();
        assert_eq!(doc.layers.len(), 1);
        let after = composite_rgba8(&doc, doc.bounds(), true).1;
        for (a, b) in before.iter().zip(&after) {
            for c in 0..4 {
                assert!((a[c] as i32 - b[c] as i32).abs() <= 1, "{a:?} {b:?}");
            }
        }
    }

    /// A tela com e sem o guardado das camadas de baixo tem que ser a mesma, com recorte,
    /// máscara, pasta e modos de mesclagem no meio.
    #[test]
    fn display_cache_matches_direct_composite() {
        let mut doc = Document::new(300, 200);
        let b = doc.bounds();
        paint(&mut doc, 0, b, [0.3, 0.6, 0.9, 1.0]);
        doc.add_layer();
        paint(
            &mut doc,
            1,
            Rect::new(10, 10, 200, 150),
            [1.0, 0.0, 0.0, 0.5],
        );
        doc.layers[1].blend = BlendMode::Multiply;
        doc.add_layer();
        paint(
            &mut doc,
            2,
            Rect::new(50, 20, 250, 180),
            [0.0, 1.0, 0.0, 0.7],
        );
        doc.add_layer();
        paint(&mut doc, 3, Rect::new(0, 0, 300, 100), [0.0, 0.0, 1.0, 1.0]);
        doc.layers[3].clip = true;
        doc.add_layer();
        paint(
            &mut doc,
            4,
            Rect::new(100, 50, 280, 190),
            [1.0, 1.0, 0.0, 0.8],
        );
        doc.layers[4].opacity = 0.6;
        let mut cache = DisplayCache::default();
        for active in 0..doc.layers.len() {
            doc.active = active;
            for r in [
                Rect::new(0, 0, 300, 200),
                Rect::new(37, 61, 140, 77),
                Rect::new(250, 150, 300, 200),
            ] {
                let want = composite_rgba8(&doc, r, true);
                assert_eq!(cache.composite(&doc, r), want, "ativa {active}, {r:?}");
            }
            // Mudar uma camada de baixo joga o guardado fora.
            if active > 1 {
                doc.checkpoint();
                paint(&mut doc, 0, Rect::new(0, 0, 40, 40), [0.0, 0.0, 0.0, 1.0]);
                let r = Rect::new(0, 0, 80, 80);
                assert_eq!(cache.composite(&doc, r), composite_rgba8(&doc, r, true));
            }
        }
    }

    #[test]
    fn undo_reports_only_the_changed_tiles() {
        let mut doc = Document::new(2000, 2000);
        doc.add_layer();
        doc.checkpoint();
        paint(
            &mut doc,
            1,
            Rect::new(1500, 1500, 1510, 1510),
            [1.0, 0.0, 0.0, 1.0],
        );
        assert!(doc.undo());
        let r = doc.take_history_dirty();
        assert!(r.contains(1505, 1505), "{r:?}");
        assert!(r.width() <= 512 && r.height() <= 512, "{r:?}");
        // Mudar a ordem das camadas recompõe tudo.
        doc.checkpoint();
        doc.move_layer(false);
        assert!(doc.undo());
        assert_eq!(doc.take_history_dirty(), doc.bounds());
    }
}
