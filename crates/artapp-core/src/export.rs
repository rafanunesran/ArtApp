//! Exportação PNG e JPEG.

use crate::composite::{Pass, composite_pass};
use crate::document::Document;

/// Codifica a tela inteira como PNG RGBA (sem as camadas de rascunho).
pub fn png(doc: &Document, with_paper: bool) -> Result<Vec<u8>, String> {
    let (r, px) = composite_pass(doc, doc.bounds(), with_paper, Pass::Final);
    encode_png(r.width(), r.height(), &px)
}

/// Codifica a tela inteira como JPEG (`quality` 1..=100). O JPEG não tem transparência: o que
/// for transparente fica sobre o papel, ou branco se o papel for transparente.
pub fn jpeg(doc: &Document, quality: u8) -> Result<Vec<u8>, String> {
    let (r, px) = composite_pass(doc, doc.bounds(), true, Pass::Final);
    let rgb: Vec<u8> = px
        .iter()
        .flat_map(|p| {
            let a = u32::from(p[3]);
            [0, 1, 2].map(|c| ((u32::from(p[c]) * a + 255 * (255 - a) + 127) / 255) as u8)
        })
        .collect();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100))
        .encode(&rgb, r.width(), r.height(), image::ExtendedColorType::Rgb8)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

pub fn encode_png(width: u32, height: u32, px: &[[u8; 4]]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(px.as_flattened())
            .map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn png_round_trips() {
        let doc = Document::new(16, 9);
        let bytes = png(&doc, true).unwrap();
        let dec = png::Decoder::new(std::io::Cursor::new(bytes));
        let reader = dec.read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (16, 9));
    }

    #[test]
    fn jpeg_decodes_back() {
        let doc = Document::new(16, 9);
        let bytes = jpeg(&doc, 90).unwrap();
        let (w, h, px) = crate::edit::decode_image(&bytes).unwrap();
        assert_eq!((w, h), (16, 9));
        assert!(px[0] > 240);
    }
}
