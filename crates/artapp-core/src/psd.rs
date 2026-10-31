//! Abrir e exportar PSD (Photoshop) com camadas, sobre o `photocraft-psd`.
//!
//! O ArtApp não tem pastas de camadas: ao abrir, os grupos do PSD são achatados na pilha (o nome
//! da camada ganha o caminho das pastas, `Pasta / Camada`), e a visibilidade e a opacidade da pasta
//! passam para as camadas de dentro. As máscaras de camada entram no alfa. Ao exportar, o papel
//! vira a camada de baixo, as camadas vetoriais vão rasterizadas e as de rascunho vão ocultas.

use photocraft_color::{BlendMode, PixelFormat};
use photocraft_geom::Rect;
use photocraft_psd::{LayerNode, LayerSpec, PixelData, PsdBuilder, PsdFile, SectionType};
use photocraft_raster::Surface;

use crate::composite::{Pass, composite_pass};
use crate::document::{Document, Layer, MAX_CANVAS_SIDE};

/// Um arquivo começa como PSD/PSB?
pub fn sniff(bytes: &[u8]) -> bool {
    bytes.starts_with(b"8BPS")
}

fn to_psd_blend(b: BlendMode) -> photocraft_psd::BlendMode {
    photocraft_psd::BlendMode::from_key(b.psd_key())
}

fn from_psd_blend(b: photocraft_psd::BlendMode) -> BlendMode {
    match BlendMode::from_psd_key(b.key()) {
        // Pass-through só existe em pastas; numa camada solta vale como Normal.
        Some(BlendMode::PassThrough) | None => BlendMode::Normal,
        Some(m) => m,
    }
}

fn q(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Codifica o documento como PSD com uma camada por camada do ArtApp.
pub fn export(doc: &Document) -> Result<Vec<u8>, String> {
    let (w, h) = (doc.width, doc.height);
    let mut b = PsdBuilder::new(w, h);
    if doc.paper[3] > 0.0 {
        let [r, g, bl, a] = doc.paper.map(q);
        let px: Vec<u8> = std::iter::repeat_n([r, g, bl, a], w as usize * h as usize)
            .flatten()
            .collect();
        b.push_layer(LayerSpec::new("Papel", 0, 0, w, h, PixelData::Rgba8(px)));
    }
    for l in &doc.layers {
        let r = l.surface.content_bounds();
        let (x, y, lw, lh, px) = if r.is_empty() {
            (0, 0, 0, 0, Vec::new())
        } else {
            (
                r.x0,
                r.y0,
                r.width(),
                r.height(),
                l.surface.to_interleaved(r),
            )
        };
        let mut spec = LayerSpec::new(l.name.clone(), x, y, lw, lh, PixelData::Rgba8(px));
        spec.blend_mode = to_psd_blend(l.blend);
        spec.opacity = q(l.opacity);
        spec.visible = l.visible && !l.draft;
        spec.clipping = l.clip;
        b.push_layer(spec);
    }
    let (_, comp) = composite_pass(doc, doc.bounds(), true, Pass::Final);
    b.composite(PixelData::Rgba8(comp.as_flattened().to_vec()));
    b.to_bytes().map_err(|e| e.to_string())
}

/// O que uma pasta passa para as camadas de dentro.
#[derive(Clone)]
struct Inherit {
    visible: bool,
    opacity: f32,
    path: String,
}

/// Abre um PSD/PSB como documento com camadas.
pub fn import(bytes: &[u8]) -> Result<Document, String> {
    let file = PsdFile::from_bytes(bytes).map_err(|e| format!("PSD inválido: {e}"))?;
    let (w, h) = (file.header.width, file.header.height);
    if w > MAX_CANVAS_SIDE || h > MAX_CANVAS_SIDE {
        return Err(format!(
            "PSD de {w} × {h} px: o maior lado aceito é {MAX_CANVAS_SIDE} px"
        ));
    }
    let mut layers = Vec::new();
    let mut warnings = 0usize;
    let root = Inherit {
        visible: true,
        opacity: 1.0,
        path: String::new(),
    };
    for node in file.layer_tree() {
        walk(&file, &node, &root, &mut layers, &mut warnings);
    }
    if layers.is_empty() {
        // PSD sem camadas (só a imagem achatada).
        let img = file
            .composite_rgba8()
            .map_err(|e| format!("PSD sem imagem legível: {e}"))?;
        let mut l = Layer::new(1, "Fundo");
        l.surface = surface_from(img.left, img.top, img.width, img.height, &img.data);
        layers.push(l);
    } else if warnings > 0 {
        log::warn!("PSD: {warnings} camada(s) não puderam ser lidas e ficaram de fora");
    }
    let active = layers.len().saturating_sub(1);
    // O fundo do PSD é a própria camada de baixo; o papel fica transparente.
    Ok(Document::from_parts(
        w,
        h,
        [1.0, 1.0, 1.0, 0.0],
        layers,
        active,
    ))
}

fn walk(
    file: &PsdFile,
    node: &LayerNode,
    inh: &Inherit,
    out: &mut Vec<Layer>,
    warnings: &mut usize,
) {
    let Some(rec) = file.layers().get(node.index()) else {
        return;
    };
    match node {
        LayerNode::Group { children, .. } => {
            let name = rec.name();
            let inner = Inherit {
                visible: inh.visible && rec.is_visible(),
                opacity: inh.opacity * rec.opacity as f32 / 255.0,
                path: if inh.path.is_empty() {
                    name
                } else {
                    format!("{} / {name}", inh.path)
                },
            };
            for c in children {
                walk(file, c, &inner, out, warnings);
            }
        }
        LayerNode::Layer { index } => {
            if rec.section_type() != SectionType::Other {
                return; // marcador de fim de pasta
            }
            let Some(view) = file.layer(*index) else {
                return;
            };
            let img = match view.rgba8() {
                Ok(img) => img,
                Err(e) => {
                    log::warn!("PSD: camada {:?}: {e}", rec.name());
                    *warnings += 1;
                    return;
                }
            };
            let mut data = img.data;
            if let Some(Ok(mask)) = view.user_mask() {
                let disabled = rec.layer_mask().is_some_and(|m| m.disabled());
                if !disabled {
                    apply_mask(&mut data, img.left, img.top, img.width, &mask);
                }
            }
            let name = if inh.path.is_empty() {
                rec.name()
            } else {
                format!("{} / {}", inh.path, rec.name())
            };
            let mut l = Layer::new(out.len() as u64 + 1, name);
            l.visible = inh.visible && rec.is_visible();
            l.opacity =
                inh.opacity * rec.opacity as f32 / 255.0 * rec.fill_opacity() as f32 / 255.0;
            l.blend = from_psd_blend(rec.blend_mode);
            l.clip = rec.clipping != 0;
            l.lock_alpha = rec.flags.transparency_protected();
            l.surface = surface_from(img.left, img.top, img.width, img.height, &data);
            out.push(l);
        }
    }
}

/// Multiplica o alfa (RGBA em `left, top, width`) pela máscara de camada.
fn apply_mask(data: &mut [u8], left: i32, top: i32, width: u32, mask: &photocraft_psd::GrayImage) {
    let width = width.max(1) as usize;
    for (i, px) in data.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = left + (i % width) as i32 - mask.left;
        let y = top + (i / width) as i32 - mask.top;
        let m = if x >= 0 && y >= 0 && (x as u32) < mask.width && (y as u32) < mask.height {
            mask.data
                .get(y as usize * mask.width as usize + x as usize)
                .copied()
                .unwrap_or(mask.default_value)
        } else {
            mask.default_value
        };
        px[3] = ((px[3] as u16 * m as u16 + 127) / 255) as u8;
    }
}

fn surface_from(left: i32, top: i32, w: u32, h: u32, data: &[u8]) -> Surface {
    if w == 0 || h == 0 || data.len() != w as usize * h as usize * 4 {
        return Surface::new(PixelFormat::RGBA8);
    }
    let r = Rect::new(left, top, left + w as i32, top + h as i32);
    let mut s = Surface::from_interleaved(PixelFormat::RGBA8, r, data);
    s.prune();
    s
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn export_then_import_keeps_layers() {
        let mut doc = Document::new(32, 20);
        if let Some(l) = doc.active_layer_mut() {
            let r = Rect::new(2, 3, 12, 9);
            let px: Vec<u8> = (0..r.width() * r.height())
                .flat_map(|i| [(i * 3 % 256) as u8, 40, 90, 255])
                .collect();
            l.surface = Surface::from_interleaved(PixelFormat::RGBA8, r, &px);
            l.name = "Tinta".into();
        }
        doc.add_layer();
        if let Some(l) = doc.active_layer_mut() {
            l.blend = BlendMode::Multiply;
            l.opacity = 0.4;
            l.visible = false;
            l.clip = true;
            l.name = "Sombra".into();
        }
        let bytes = export(&doc).unwrap();
        assert!(sniff(&bytes));
        let back = import(&bytes).unwrap();
        assert_eq!((back.width, back.height), (32, 20));
        // Papel + 2 camadas.
        assert_eq!(back.layers.len(), 3);
        assert_eq!(back.layers[0].name, "Papel");
        let (a, b) = (&back.layers[1], &back.layers[2]);
        assert_eq!(a.name, "Tinta");
        let r = doc.bounds();
        assert_eq!(
            a.surface.to_interleaved(r),
            doc.layers[0].surface.to_interleaved(r)
        );
        assert_eq!(b.name, "Sombra");
        assert_eq!(b.blend, BlendMode::Multiply);
        assert!((b.opacity - 0.4).abs() < 0.01);
        assert!(!b.visible);
        assert!(b.clip);
    }

    #[test]
    fn groups_are_flattened_with_their_path() {
        let mut g = photocraft_psd::GroupSpec::new("Personagem");
        g.visible = false;
        g.opacity = 128;
        let mut b2 = PsdBuilder::new(4, 4);
        b2.push_layer(LayerSpec::new(
            "Fundo",
            0,
            0,
            1,
            1,
            PixelData::Rgba8(vec![1, 2, 3, 255]),
        ));
        b2.begin_group(g);
        b2.push_layer(LayerSpec::new(
            "Olhos",
            1,
            1,
            1,
            1,
            PixelData::Rgba8(vec![9, 9, 9, 255]),
        ));
        b2.end_group().unwrap();
        b2.composite(PixelData::Rgba8(vec![0; 64]));
        let doc = import(&b2.to_bytes().unwrap()).unwrap();
        let names: Vec<_> = doc.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Fundo", "Personagem / Olhos"]);
        assert!(!doc.layers[1].visible);
        assert!((doc.layers[1].opacity - 128.0 / 255.0).abs() < 0.01);
    }
}
