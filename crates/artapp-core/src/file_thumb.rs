//! Miniaturas dos arquivos na janela de abrir/importar: a imagem reduzida de um PNG/JPEG, a
//! imagem achatada de um PSD, a prévia de um projeto, amostras de traço de um pacote de pincéis e
//! as cores de um arquivo de amostras. Roda fora da interface (numa thread), por isso não
//! depende de egui: devolve RGBA de 8 bits e uma linha curta de informação ("1920 × 1080",
//! "12 pincéis"…) para ajudar a escolher o arquivo certo.

use std::path::Path;

/// Maior lado de uma miniatura, em pixels (a lista mostra ~56–72 pt numa tela densa).
pub const THUMB_MAX: u32 = 160;

/// Arquivos maiores que isso não são abertos só para a miniatura.
const MAX_FILE_BYTES: u64 = 256 << 20;

/// Quantos traços de amostra mostrar para um pacote de pincéis.
const BRUSH_SAMPLES: usize = 3;

/// Uma miniatura pronta: RGBA 8 bits (não pré-multiplicado) e um texto curto.
#[derive(Clone, Debug, Default)]
pub struct Thumb {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// "1920 × 1080", "PSD · 3000 × 2000", "12 pincéis", "24 cores"…
    pub note: String,
    /// Traços brancos sobre transparente (pincéis): a lista desenha um fundo escuro atrás.
    pub on_dark: bool,
}

impl Thumb {
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0 || self.rgba.is_empty()
    }
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

/// Lê o arquivo e monta a miniatura. `None` quando não dá (formato sem prévia, arquivo
/// ilegível ou grande demais): a lista fica com o ícone.
pub fn make(path: &Path) -> Option<Thumb> {
    let ext = ext_of(path);
    if ext == "artapp" {
        let png = crate::project::read_preview(path)?;
        let (w, h, px) = crate::project::decode_rgba(&png).ok()?;
        return Some(fit(w, h, &px, String::new()));
    }
    let len = std::fs::metadata(path).ok()?.len();
    if len > MAX_FILE_BYTES {
        return None;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let bytes = std::fs::read(path).ok()?;
    from_bytes(&name, &ext, &bytes)
}

/// Como [`make`], com o arquivo já lido (`ext` em minúsculas, sem ponto).
pub fn from_bytes(name: &str, ext: &str, bytes: &[u8]) -> Option<Thumb> {
    match ext {
        "psd" | "psb" => psd_thumb(bytes),
        "ase" | "aco" => swatches_thumb(bytes),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" => {
            let (w, h, px) = crate::edit::decode_image(bytes).ok()?;
            Some(fit(w, h, &px, format!("{w} × {h}")))
        }
        e if crate::brushes::IMPORT_EXTS.contains(&e) => brushes_thumb(name, bytes),
        _ => None,
    }
}

/// Reduz para caber em [`THUMB_MAX`] (nunca amplia).
fn fit(w: u32, h: u32, px: &[u8], note: String) -> Thumb {
    let (w, h) = (w.max(1), h.max(1));
    let s = (THUMB_MAX as f32 / w.max(h) as f32).min(1.0);
    let (tw, th) = (
        ((w as f32 * s).round() as u32).max(1),
        ((h as f32 * s).round() as u32).max(1),
    );
    let rgba = if (tw, th) == (w, h) {
        px.to_vec()
    } else {
        crate::edit::downscale(px, w, h, tw, th)
    };
    Thumb {
        width: tw,
        height: th,
        rgba,
        note,
        on_dark: false,
    }
}

fn psd_thumb(bytes: &[u8]) -> Option<Thumb> {
    let file = photocraft_psd::PsdFile::from_bytes(bytes).ok()?;
    let (w, h) = (file.header.width, file.header.height);
    let note = format!("PSD · {w} × {h}");
    // A imagem achatada que o próprio PSD guarda é o caminho rápido.
    if let Ok(img) = file.composite_rgba8()
        && img.width > 0
        && img.height > 0
        && img.data.as_chunks::<4>().0.iter().any(|p| p[3] > 0)
    {
        return Some(fit(img.width, img.height, &img.data, note));
    }
    // Sem ela, monta pelas camadas.
    let doc = crate::psd::import(bytes).ok()?;
    let (tw, th, px) = crate::project::preview_rgba(&doc);
    Some(fit(tw, th, &px, note))
}

fn swatches_thumb(bytes: &[u8]) -> Option<Thumb> {
    let colors = crate::palette::read_swatches(bytes).ok()?;
    if colors.is_empty() {
        return None;
    }
    // Grade quadrada de cores, cada uma num quadradinho.
    let n = colors.len();
    let cols = (n as f32).sqrt().ceil().max(1.0) as usize;
    let rows = n.div_ceil(cols);
    let cell = (THUMB_MAX as usize / cols.max(rows)).max(4);
    let (w, h) = (cols * cell, rows * cell);
    let mut rgba = vec![0u8; w * h * 4];
    for (i, c) in colors.iter().enumerate() {
        let (cx, cy) = ((i % cols) * cell, (i / cols) * cell);
        for y in cy..cy + cell {
            for x in cx..cx + cell {
                let o = (y * w + x) * 4;
                rgba[o..o + 3].copy_from_slice(c);
                rgba[o + 3] = 255;
            }
        }
    }
    Some(Thumb {
        width: w as u32,
        height: h as u32,
        rgba,
        note: if n == 1 {
            "1 cor".into()
        } else {
            format!("{n} cores")
        },
        on_dark: false,
    })
}

fn brushes_thumb(name: &str, bytes: &[u8]) -> Option<Thumb> {
    let list = crate::brushes::import(name, bytes).ok()?;
    if list.is_empty() {
        return None;
    }
    let n = list.len();
    let samples = n.min(BRUSH_SAMPLES);
    let (w, sh) = (THUMB_MAX, THUMB_MAX / 2 / BRUSH_SAMPLES as u32);
    let mut rgba = Vec::with_capacity((w * sh * samples as u32 * 4) as usize);
    for b in list.iter().take(samples) {
        for p in crate::brushes::preview(&b.settings, w, sh) {
            rgba.extend_from_slice(&p);
        }
    }
    let mut note = if n == 1 {
        format!("1 pincel · {}", list[0].name)
    } else {
        format!("{n} pincéis")
    };
    if note.chars().count() > 48 {
        note = note.chars().take(47).collect::<String>() + "…";
    }
    Some(Thumb {
        width: w,
        height: sh * samples as u32,
        rgba,
        note,
        on_dark: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_is_reduced_and_described() -> Result<(), String> {
        let (w, h) = (400u32, 200u32);
        let px = vec![200u8; (w * h * 4) as usize];
        let png = crate::project::encode_rgba(w, h, &px)?;
        let t = from_bytes("a.png", "png", &png).ok_or("sem miniatura")?;
        assert_eq!((t.width, t.height), (THUMB_MAX, THUMB_MAX / 2));
        assert_eq!(t.rgba.len(), (t.width * t.height * 4) as usize);
        assert_eq!(t.note, "400 × 200");
        Ok(())
    }

    #[test]
    fn small_image_is_not_enlarged() -> Result<(), String> {
        let px = vec![255u8; 10 * 6 * 4];
        let png = crate::project::encode_rgba(10, 6, &px)?;
        let t = from_bytes("a.png", "png", &png).ok_or("sem miniatura")?;
        assert_eq!((t.width, t.height), (10, 6));
        Ok(())
    }

    #[test]
    fn brush_pack_shows_strokes() -> Result<(), String> {
        let list: Vec<_> = crate::brushes::drawing()
            .into_iter()
            .take(5)
            .map(|mut b| {
                b.group = crate::brushes::CUSTOM_GROUP.into();
                b
            })
            .collect();
        let pack = crate::brushes::export_pack(&list)?;
        let t = from_bytes("meus.artbrushes", "artbrushes", &pack).ok_or("sem miniatura")?;
        assert_eq!(t.note, "5 pincéis");
        assert!(t.on_dark);
        assert_eq!(t.rgba.len(), (t.width * t.height * 4) as usize);
        assert!(t.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 200));
        Ok(())
    }

    #[test]
    fn psd_uses_its_flat_image() -> Result<(), String> {
        let mut doc = crate::document::Document::new(64, 32);
        doc.layers[0].surface.fill_rect(
            photocraft_geom::Rect::new(0, 0, 64, 32),
            &[1.0, 0.0, 0.0, 1.0],
        );
        let psd = crate::psd::export(&doc)?;
        let t = from_bytes("a.psd", "psd", &psd).ok_or("sem miniatura")?;
        assert_eq!(t.note, "PSD · 64 × 32");
        assert_eq!((t.width, t.height), (64, 32));
        let mid = ((16 * 64 + 32) * 4) as usize;
        assert!(
            t.rgba[mid] > 200 && t.rgba[mid + 1] < 50,
            "{:?}",
            &t.rgba[mid..mid + 4]
        );
        Ok(())
    }

    #[test]
    fn unknown_or_broken_files_have_no_thumb() {
        assert!(from_bytes("a.obj", "obj", b"v 0 0 0").is_none());
        assert!(from_bytes("a.png", "png", b"nope").is_none());
        assert!(from_bytes("a.abr", "abr", b"nope").is_none());
    }
}
