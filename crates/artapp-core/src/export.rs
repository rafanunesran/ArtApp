//! Exportação PNG.

use crate::composite::composite_rgba8;
use crate::document::Document;

/// Codifica a tela inteira como PNG RGBA.
pub fn png(doc: &Document, with_paper: bool) -> Result<Vec<u8>, String> {
    let (r, px) = composite_rgba8(doc, doc.bounds(), with_paper);
    encode_png(r.width(), r.height(), &px)
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
}
