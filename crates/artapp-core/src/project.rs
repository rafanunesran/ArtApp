//! Projeto do ArtApp (`.artapp`): o documento inteiro, para continuar depois.
//!
//! Formato (versão 1), tudo little-endian:
//! ```text
//! MAGIC (8 bytes) | versão u32 | tamanho do manifesto u32 | manifesto JSON | dados
//! ```
//! O manifesto descreve a tela e cada camada (nome, visível, opacidade, modo de mesclagem,
//! travas, recorte, referência, rascunho). Os pixels de cada camada vão nos dados como PNG RGBA
//! do retângulo pintado, e a arte das camadas vetoriais no formato nativo do VectorCraft. Um
//! [`Blob`] diz onde cada pedaço está (a partir do começo dos dados). A miniatura (`preview`) é o
//! primeiro pedaço, para a lista de projetos ler só o começo do arquivo.
//!
//! Leitores recusam versões mais novas que [`VERSION`]; campos que não conhecem são ignorados e
//! campos que faltam ficam com o valor padrão, então versões novas podem acrescentar coisas.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use photocraft_color::{BlendMode, PixelFormat};
use photocraft_geom::Rect;
use photocraft_raster::Surface;
use serde::{Deserialize, Serialize};

use crate::composite::{Pass, composite_pass};
use crate::document::{Document, Layer};
use crate::vector::{VectorArt, next_rev};

pub const MAGIC: &[u8; 8] = b"ARTAPP\x1a\n";
pub const VERSION: u32 = 1;
/// Extensão dos arquivos de projeto.
pub const EXT: &str = "artapp";
/// Maior lado da miniatura guardada no projeto.
pub const PREVIEW_MAX: u32 = 256;
const HEADER_LEN: usize = 16;
/// Manifestos maiores que isso são recusados (um projeto real fica em poucos KB).
const MANIFEST_MAX: usize = 64 << 20;

/// Onde um pedaço está nos dados depois do manifesto.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Blob {
    pub offset: u64,
    pub len: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    #[serde(default)]
    generator: String,
    width: u32,
    height: u32,
    #[serde(default = "white")]
    paper: [f32; 4],
    #[serde(default)]
    active: usize,
    #[serde(default)]
    preview: Option<Blob>,
    /// Resolução de impressão.
    #[serde(default = "default_dpi")]
    dpi: f32,
    /// Assistente de animação (ligado, fps, pele de cebola…).
    #[serde(default)]
    animation: crate::animation::Animation,
    #[serde(default)]
    layers: Vec<LayerEntry>,
    /// Gravação do processo: se está gravando, e as fotos (JPEG), da mais antiga à mais nova.
    #[serde(default)]
    timelapse: crate::timelapse::Timelapse,
    #[serde(default)]
    timelapse_frames: Vec<Blob>,
    /// Modelo 3D (OBJ) pintado com o desenho como textura.
    #[serde(default)]
    model_obj: Option<Blob>,
}

fn default_dpi() -> f32 {
    crate::document::DEFAULT_DPI
}

fn white() -> [f32; 4] {
    [1.0; 4]
}

fn yes() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Serialize, Deserialize)]
struct LayerEntry {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default = "yes")]
    visible: bool,
    #[serde(default = "one")]
    opacity: f32,
    /// Nome do modo (`"Multiply"`…). Um modo desconhecido vira Normal.
    #[serde(default)]
    blend: String,
    #[serde(default)]
    lock_alpha: bool,
    #[serde(default)]
    clip: bool,
    #[serde(default)]
    reference: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    mask: bool,
    #[serde(default)]
    folder: bool,
    #[serde(default)]
    collapsed: bool,
    #[serde(default)]
    parent: Option<u64>,
    /// Borda da camada: espessura (0 = sem) e cor.
    #[serde(default)]
    border_width: f32,
    #[serde(default)]
    border_color: [f32; 3],
    /// Retícula: espaço entre os pontos (0 = sem) e ângulo.
    #[serde(default)]
    tone_period: f32,
    #[serde(default)]
    tone_angle: f32,
    /// Pixels: o retângulo pintado e o PNG dele. `None` = camada vazia.
    #[serde(default)]
    pixels: Option<Pixels>,
    /// Arte vetorial (`.vectorcraft`).
    #[serde(default)]
    vector: Option<Blob>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Pixels {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    png: Blob,
}

/// Junta os pedaços dos dados.
#[derive(Default)]
struct Blobs(Vec<u8>);

impl Blobs {
    fn push(&mut self, bytes: &[u8]) -> Blob {
        let b = Blob {
            offset: self.0.len() as u64,
            len: bytes.len() as u64,
        };
        self.0.extend_from_slice(bytes);
        b
    }
}

fn blend_name(b: BlendMode) -> String {
    match serde_json::to_value(b) {
        Ok(serde_json::Value::String(s)) => s,
        _ => "Normal".into(),
    }
}

fn blend_from_name(s: &str) -> BlendMode {
    serde_json::from_value(serde_json::Value::String(s.into())).unwrap_or(BlendMode::Normal)
}

/// Codifica o documento como projeto `.artapp`.
pub fn save(doc: &Document) -> Result<Vec<u8>, String> {
    let mut blobs = Blobs::default();
    let preview = preview_png(doc).ok().map(|png| blobs.push(&png));
    let mut layers = Vec::with_capacity(doc.layers.len());
    for l in &doc.layers {
        let r = l.surface.content_bounds();
        let pixels = if r.is_empty() {
            None
        } else {
            let bytes = l.surface.to_interleaved(r);
            let png = encode_rgba(r.width(), r.height(), &bytes)?;
            Some(Pixels {
                x: r.x0,
                y: r.y0,
                w: r.width(),
                h: r.height(),
                png: blobs.push(&png),
            })
        };
        let vector = l
            .vector
            .as_ref()
            .map(|v| blobs.push(&vectorcraft_format::save_file(&v.doc)));
        layers.push(LayerEntry {
            id: l.id,
            name: l.name.clone(),
            visible: l.visible,
            opacity: l.opacity,
            blend: blend_name(l.blend),
            lock_alpha: l.lock_alpha,
            clip: l.clip,
            reference: l.reference,
            draft: l.draft,
            mask: l.mask,
            folder: l.folder,
            collapsed: l.collapsed,
            parent: l.parent,
            border_width: l.border.map_or(0.0, |b| b.width),
            border_color: l.border.map_or([0.0; 3], |b| b.color),
            tone_period: l.tone.map_or(0.0, |t| t.period),
            tone_angle: l.tone.map_or(0.0, |t| t.angle),
            pixels,
            vector,
        });
    }
    let manifest = Manifest {
        generator: format!("ArtApp {}", env!("CARGO_PKG_VERSION")),
        width: doc.width,
        height: doc.height,
        paper: doc.paper,
        active: doc.active,
        preview,
        dpi: doc.dpi,
        animation: doc.animation.clone(),
        layers,
        timelapse: doc.timelapse.clone(),
        timelapse_frames: doc.timelapse.frames.iter().map(|f| blobs.push(f)).collect(),
        model_obj: doc.model.as_ref().map(|m| blobs.push(m.as_bytes())),
    };
    let json = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(HEADER_LEN + json.len() + blobs.0.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&json);
    out.extend_from_slice(&blobs.0);
    Ok(out)
}

/// Um arquivo começa como projeto do ArtApp?
pub fn sniff(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// Lê o cabeçalho: (versão, tamanho do manifesto).
fn header(h: &[u8]) -> Result<(u32, usize), String> {
    if !sniff(h) {
        return Err("não é um projeto do ArtApp".into());
    }
    let word = |i: usize| -> u32 {
        let mut b = [0u8; 4];
        if let Some(s) = h.get(i..i + 4) {
            b.copy_from_slice(s);
        }
        u32::from_le_bytes(b)
    };
    let version = word(8);
    if version > VERSION {
        return Err(format!(
            "projeto feito por uma versão mais nova do ArtApp (formato {version}); atualize o app"
        ));
    }
    let len = word(12) as usize;
    if len > MANIFEST_MAX {
        return Err("projeto corrompido (manifesto grande demais)".into());
    }
    Ok((version, len))
}

fn slice<'a>(data: &'a [u8], b: &Blob) -> Result<&'a [u8], String> {
    let start = usize::try_from(b.offset).map_err(|e| e.to_string())?;
    let len = usize::try_from(b.len).map_err(|e| e.to_string())?;
    start
        .checked_add(len)
        .and_then(|end| data.get(start..end))
        .ok_or_else(|| "projeto corrompido (dados cortados)".to_string())
}

/// Abre um projeto `.artapp`.
pub fn load(bytes: &[u8]) -> Result<Document, String> {
    let (_, len) = header(bytes)?;
    let json = bytes
        .get(HEADER_LEN..HEADER_LEN + len)
        .ok_or("projeto corrompido (arquivo cortado)")?;
    let m: Manifest =
        serde_json::from_slice(json).map_err(|e| format!("projeto corrompido: {e}"))?;
    let data = bytes.get(HEADER_LEN + len..).unwrap_or_default();
    let mut layers = Vec::with_capacity(m.layers.len());
    let mut seen = std::collections::HashSet::new();
    let mut next_free = m.layers.iter().map(|l| l.id).max().unwrap_or(0) + 1;
    for e in m.layers {
        // Ids repetidos (arquivo mexido à mão) confundiriam as miniaturas e o kit vetorial.
        let id = if seen.insert(e.id) {
            e.id
        } else {
            next_free += 1;
            next_free - 1
        };
        let mut layer = Layer::new(id, e.name);
        layer.visible = e.visible;
        layer.opacity = e.opacity.clamp(0.0, 1.0);
        layer.blend = blend_from_name(&e.blend);
        layer.lock_alpha = e.lock_alpha;
        layer.clip = e.clip;
        layer.reference = e.reference;
        layer.draft = e.draft;
        layer.mask = e.mask;
        layer.folder = e.folder;
        layer.collapsed = e.collapsed;
        layer.parent = e.parent;
        layer.tone = (e.tone_period > 0.0).then(|| crate::tone::Tone {
            period: e
                .tone_period
                .clamp(crate::tone::MIN_PERIOD, crate::tone::MAX_PERIOD),
            angle: e.tone_angle,
        });
        layer.border = (e.border_width > 0.0).then(|| crate::border::Border {
            width: e.border_width.min(crate::border::MAX_WIDTH),
            color: e.border_color.map(|c| c.clamp(0.0, 1.0)),
        });
        if let Some(p) = &e.pixels {
            let (w, h, px) = decode_rgba(slice(data, &p.png)?)?;
            if (w, h) != (p.w, p.h) {
                return Err("projeto corrompido (pixels de tamanho errado)".into());
            }
            let r = Rect::new(p.x, p.y, p.x + w as i32, p.y + h as i32);
            layer.surface = Surface::from_interleaved(PixelFormat::RGBA8, r, &px);
            layer.surface.prune();
        }
        if let Some(b) = &e.vector {
            let vdoc = vectorcraft_format::load(slice(data, b)?)
                .map_err(|e| format!("arte vetorial corrompida: {e}"))?;
            layer.vector = Some(VectorArt {
                doc: vdoc,
                rev: next_rev(),
            });
        }
        layers.push(layer);
    }
    let mut doc = Document::from_parts(m.width, m.height, m.paper, layers, m.active);
    doc.animation = m.animation.clone();
    doc.follow_active_frame();
    doc.timelapse.enabled = m.timelapse.enabled;
    if let Some(b) = &m.model_obj {
        let text = String::from_utf8_lossy(slice(data, b)?).into_owned();
        doc.model = Some(std::sync::Arc::new(text));
    }
    for b in &m.timelapse_frames {
        // Uma foto estragada não impede de abrir o desenho.
        if let Ok(f) = slice(data, b) {
            doc.timelapse.frames.push(std::sync::Arc::new(f.to_vec()));
        }
    }
    if m.dpi.is_finite() && m.dpi > 0.0 {
        doc.dpi = m.dpi.clamp(1.0, 10_000.0);
    }
    Ok(doc)
}

/// A miniatura de um projeto no disco (PNG), lendo só o começo do arquivo.
pub fn read_preview(path: &Path) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = [0u8; HEADER_LEN];
    f.read_exact(&mut h).ok()?;
    let (_, len) = header(&h).ok()?;
    let mut json = vec![0u8; len];
    f.read_exact(&mut json).ok()?;
    let m: Manifest = serde_json::from_slice(&json).ok()?;
    let b = m.preview?;
    if b.len > 16 << 20 {
        return None;
    }
    f.seek(SeekFrom::Current(i64::try_from(b.offset).ok()?))
        .ok()?;
    let mut png = vec![0u8; usize::try_from(b.len).ok()?];
    f.read_exact(&mut png).ok()?;
    Some(png)
}

/// PNG pequeno da imagem final (com o papel), para a lista de projetos.
fn preview_png(doc: &Document) -> Result<Vec<u8>, String> {
    let (r, px) = composite_pass(doc, doc.bounds(), true, Pass::Final);
    let (w, h) = (r.width().max(1), r.height().max(1));
    let s = (PREVIEW_MAX as f32 / w.max(h) as f32).min(1.0);
    let (tw, th) = (
        ((w as f32 * s) as u32).max(1),
        ((h as f32 * s) as u32).max(1),
    );
    let mut out = Vec::with_capacity((tw * th * 4) as usize);
    for y in 0..th {
        for x in 0..tw {
            let sx = ((x as f32 + 0.5) * w as f32 / tw as f32) as usize;
            let sy = ((y as f32 + 0.5) * h as f32 / th as f32) as usize;
            let p = px.get(sy * w as usize + sx).copied().unwrap_or_default();
            out.extend_from_slice(&p);
        }
    }
    encode_rgba(tw, th, &out)
}

pub fn encode_rgba(width: u32, height: u32, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(bytes).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Decodifica um PNG para RGBA de 8 bits: (largura, altura, bytes).
pub fn decode_rgba(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let size = reader.output_buffer_size().ok_or("imagem grande demais")?;
    let mut buf = vec![0u8; size];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    let (w, h) = (info.width, info.height);
    let n = (w as usize) * (h as usize);
    let px = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("PNG indexado sem expandir".into()),
    };
    if px.len() != n * 4 {
        return Err("PNG com tamanho inesperado".into());
    }
    Ok((w, h, px))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::vector::{Shape, VectorStyle};

    fn painted() -> Document {
        let mut doc = Document::new(64, 48);
        doc.paper = [0.9, 0.8, 0.7, 1.0];
        if let Some(l) = doc.active_layer_mut() {
            let r = Rect::new(3, 5, 20, 30);
            let px: Vec<u8> = (0..r.width() * r.height())
                .flat_map(|i| [(i % 251) as u8, 10, 200, 128])
                .collect();
            l.surface = Surface::from_interleaved(PixelFormat::RGBA8, r, &px);
            l.name = "Lineart".into();
            l.blend = BlendMode::Multiply;
            l.opacity = 0.5;
            l.reference = true;
            l.lock_alpha = true;
        }
        doc.add_layer();
        if let Some(l) = doc.active_layer_mut() {
            l.visible = false;
            l.draft = true;
            l.clip = true;
        }
        doc.add_vector_layer();
        doc.vector_add(
            &Shape::Rectangle(
                vectorcraft_geom::Point::new(4.0, 4.0),
                vectorcraft_geom::Point::new(30.0, 20.0),
            ),
            &VectorStyle::default(),
        );
        doc
    }

    #[test]
    fn round_trips_layers() {
        let mut doc = painted();
        doc.dpi = 350.0;
        let bytes = save(&doc).unwrap();
        assert!(sniff(&bytes));
        let back = load(&bytes).unwrap();
        assert_eq!((back.width, back.height), (64, 48));
        assert_eq!(back.dpi, 350.0);
        assert_eq!(back.paper, doc.paper);
        assert_eq!(back.layers.len(), doc.layers.len());
        assert_eq!(back.active, doc.active);
        for (a, b) in doc.layers.iter().zip(&back.layers) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.name, b.name);
            assert_eq!(
                (
                    a.visible,
                    a.opacity,
                    a.blend,
                    a.lock_alpha,
                    a.clip,
                    a.reference,
                    a.draft
                ),
                (
                    b.visible,
                    b.opacity,
                    b.blend,
                    b.lock_alpha,
                    b.clip,
                    b.reference,
                    b.draft
                )
            );
            assert_eq!(a.vector.is_some(), b.vector.is_some());
            let r = doc.bounds();
            assert_eq!(a.surface.to_interleaved(r), b.surface.to_interleaved(r));
        }
        let (va, vb) = (
            doc.layers[2].vector.as_ref().unwrap(),
            back.layers[2].vector.as_ref().unwrap(),
        );
        assert_eq!(
            vectorcraft_format::save_file(&va.doc),
            vectorcraft_format::save_file(&vb.doc)
        );
    }

    #[test]
    fn preview_is_readable_from_disk() {
        let dir = std::env::temp_dir().join(format!("artapp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.artapp");
        std::fs::write(&path, save(&painted()).unwrap()).unwrap();
        let png = read_preview(&path).unwrap();
        let (w, h, _) = decode_rgba(&png).unwrap();
        assert_eq!((w, h), (64, 48));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_garbage_and_newer_versions() {
        assert!(load(b"nope").is_err());
        let mut bytes = save(&Document::new(4, 4)).unwrap();
        bytes[8] = 99;
        assert!(load(&bytes).err().unwrap().contains("mais nova"));
        let bytes = save(&painted()).unwrap();
        assert!(load(&bytes[..bytes.len() - 3]).is_err());
    }
}
