//! Exportar PDF: o desenho numa página ou, com as páginas ligadas (Page Assist), uma página
//! do PDF por página do documento. Cada página vai como imagem JPEG no tamanho de impressão
//! dado pela resolução (dpi) do documento.

use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};

use crate::composite::{Pass, composite_pass};
use crate::document::Document;

/// Lado maior das imagens no PDF (acima disso a página é reduzida).
pub const MAX_SIDE: u32 = 6000;
const JPEG_QUALITY: u8 = 92;

/// As páginas do documento em RGBA 8 bits, com o papel.
fn page_pixels(doc: &Document) -> Vec<Vec<u8>> {
    if doc.animation.enabled && doc.animation.pages {
        (0..doc.frames().len())
            .map(|k| doc.render_frame(k, true).as_flattened().to_vec())
            .collect()
    } else {
        let (_, px) = composite_pass(doc, doc.bounds(), true, Pass::Final);
        vec![px.as_flattened().to_vec()]
    }
}

fn jpeg(w: u32, h: u32, rgba: Vec<u8>) -> Result<(u32, u32, Vec<u8>), String> {
    let img = image::RgbaImage::from_raw(w, h, rgba).ok_or("página com tamanho errado")?;
    let img = if w.max(h) > MAX_SIDE {
        let s = f64::from(MAX_SIDE) / f64::from(w.max(h));
        let (tw, th) = (
            ((f64::from(w) * s).round() as u32).max(1),
            ((f64::from(h) * s).round() as u32).max(1),
        );
        image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let (tw, th) = img.dimensions();
    let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .encode_image(&rgb)
        .map_err(|e| e.to_string())?;
    Ok((tw, th, out))
}

/// O documento como PDF.
pub fn export(doc: &Document) -> Result<Vec<u8>, String> {
    let pages = page_pixels(doc);
    if pages.is_empty() {
        return Err("o documento não tem páginas".into());
    }
    // Tamanho da página em pontos (1/72 pol), pela resolução do documento.
    let dpi = if doc.dpi.is_finite() && doc.dpi > 0.0 {
        doc.dpi
    } else {
        crate::document::DEFAULT_DPI
    };
    let (pw, ph) = (
        doc.width as f32 / dpi * 72.0,
        doc.height as f32 / dpi * 72.0,
    );
    let mut pdf = Pdf::new();
    let mut next = 1;
    let mut alloc = || {
        let r = Ref::new(next);
        next += 1;
        r
    };
    let catalog = alloc();
    let tree = alloc();
    let mut kids = Vec::with_capacity(pages.len());
    let mut bodies = Vec::with_capacity(pages.len());
    for px in pages {
        let (page, image, content) = (alloc(), alloc(), alloc());
        kids.push(page);
        bodies.push((page, image, content, jpeg(doc.width, doc.height, px)?));
    }
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree)
        .kids(kids.iter().copied())
        .count(kids.len() as i32);
    for (page, image, content, (iw, ih, data)) in bodies {
        let mut p = pdf.page(page);
        p.media_box(Rect::new(0.0, 0.0, pw, ph));
        p.parent(tree);
        p.contents(content);
        p.resources().x_objects().pair(Name(b"Im"), image);
        p.finish();
        let mut x = pdf.image_xobject(image, &data);
        x.filter(Filter::DctDecode);
        x.width(iw as i32);
        x.height(ih as i32);
        x.color_space().device_rgb();
        x.bits_per_component(8);
        x.finish();
        let mut c = Content::new();
        c.save_state();
        c.transform([pw, 0.0, 0.0, ph, 0.0, 0.0]);
        c.x_object(Name(b"Im"));
        c.restore_state();
        pdf.stream(content, &c.finish());
    }
    Ok(pdf.finish())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn count_pages(pdf: &[u8]) -> usize {
        let s = String::from_utf8_lossy(pdf);
        s.matches("/Type /Page").count() - s.matches("/Type /Pages").count()
    }

    #[test]
    fn one_page_or_one_per_page() {
        let mut d = Document::new(300, 200);
        let pdf = export(&d).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        assert_eq!(count_pages(&pdf), 1);
        d.add_frame();
        d.add_frame();
        d.animation.enabled = true;
        d.animation.pages = true;
        let pdf = export(&d).unwrap();
        assert_eq!(count_pages(&pdf), 3);
    }
}
