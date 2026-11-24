//! Gravação do processo (timelapse): uma foto pequena da tela depois de cada alteração,
//! guardada no projeto e exportável como GIF ou PNG animado.
//!
//! As fotos são JPEG com o lado maior até [`SIDE`]. Passando de [`MAX_FRAMES`], uma foto sim,
//! outra não é descartada (a primeira e a última ficam), então o vídeo cobre o desenho inteiro
//! com tamanho limitado.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::composite::{Pass, composite_view};
use crate::document::Document;

/// Lado maior das fotos guardadas.
pub const SIDE: u32 = 640;
/// Fotos guardadas no máximo (depois disso a gravação fica mais espaçada).
pub const MAX_FRAMES: usize = 900;
/// Velocidades de reprodução e exportação, em fotos por segundo.
pub const FPS_RANGE: std::ops::RangeInclusive<u32> = 5..=60;
const JPEG_QUALITY: u8 = 85;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Timelapse {
    /// Gravando (vale por projeto).
    pub enabled: bool,
    /// Fotos em JPEG, da mais antiga para a mais nova.
    #[serde(skip)]
    pub frames: Vec<Arc<Vec<u8>>>,
}

impl Default for Timelapse {
    fn default() -> Self {
        Self {
            enabled: true,
            frames: Vec::new(),
        }
    }
}

impl Timelapse {
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Acrescenta uma foto (de [`capture`]) e afina a gravação se passou do limite.
    pub fn push(&mut self, jpeg: Vec<u8>) {
        // Nada mudou na tela (ex.: só trocou a camada ativa): não repete a foto.
        if self.frames.last().is_some_and(|f| **f == jpeg) {
            return;
        }
        self.frames.push(Arc::new(jpeg));
        if self.frames.len() > MAX_FRAMES {
            let last = self.frames.len() - 1;
            let mut k = 0;
            self.frames.retain(|_| {
                let keep = k % 2 == 0 || k == last;
                k += 1;
                keep
            });
        }
    }

    pub fn clear(&mut self) {
        self.frames.clear();
    }

    /// A foto `i` em RGBA (para mostrar ou exportar).
    pub fn decode(&self, i: usize) -> Option<image::RgbaImage> {
        let bytes = self.frames.get(i)?;
        image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
            .ok()
            .map(|img| img.to_rgba8())
    }
}

/// O tamanho de uma foto da tela `w`×`h` (lado maior até [`SIDE`]).
pub fn frame_size(w: u32, h: u32) -> (u32, u32) {
    let s = (f64::from(SIDE) / f64::from(w.max(h).max(1))).min(1.0);
    (
        ((f64::from(w) * s).round() as u32).max(1),
        ((f64::from(h) * s).round() as u32).max(1),
    )
}

/// Fotografa a tela como ela aparece (com o papel; na animação, o quadro atual) e devolve o
/// JPEG. Pode rodar fora da linha da interface, com uma cópia do documento.
pub fn capture(doc: &Document) -> Result<Vec<u8>, String> {
    let view = doc
        .animation
        .enabled
        .then_some((doc.animation.current, false));
    let (_, px) = composite_view(doc, doc.bounds(), true, Pass::Final, view);
    let full = image::RgbaImage::from_raw(doc.width, doc.height, px.as_flattened().to_vec())
        .ok_or("tela com tamanho errado")?;
    let (tw, th) = frame_size(doc.width, doc.height);
    let small = if (tw, th) == (doc.width, doc.height) {
        full
    } else {
        image::imageops::resize(&full, tw, th, image::imageops::FilterType::Triangle)
    };
    let rgb = image::DynamicImage::ImageRgba8(small).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .encode_image(&rgb)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// Todas as fotos no tamanho da última (a tela pode ter mudado de tamanho no meio), centradas
/// em fundo branco, mais a última repetida por um segundo no fim.
fn frames_for_export(t: &Timelapse, fps: u32) -> Result<(u32, u32, Vec<image::RgbaImage>), String> {
    let last = t
        .decode(t.len().saturating_sub(1))
        .ok_or("ainda não há nada gravado")?;
    let (w, h) = last.dimensions();
    let mut out = Vec::with_capacity(t.len() + fps as usize);
    for i in 0..t.len() {
        let Some(img) = t.decode(i) else {
            continue;
        };
        if img.dimensions() == (w, h) {
            out.push(img);
            continue;
        }
        let s = (f64::from(w) / f64::from(img.width())).min(f64::from(h) / f64::from(img.height()));
        let (iw, ih) = (
            ((f64::from(img.width()) * s).round() as u32).clamp(1, w),
            ((f64::from(img.height()) * s).round() as u32).clamp(1, h),
        );
        let small = image::imageops::resize(&img, iw, ih, image::imageops::FilterType::Triangle);
        let mut canvas = image::RgbaImage::from_pixel(w, h, image::Rgba([255, 255, 255, 255]));
        image::imageops::overlay(
            &mut canvas,
            &small,
            i64::from((w - iw) / 2),
            i64::from((h - ih) / 2),
        );
        out.push(canvas);
    }
    for _ in 0..fps {
        out.push(last.clone());
    }
    Ok((w, h, out))
}

/// A gravação como GIF em `fps` fotos por segundo, repetindo.
pub fn export_gif(t: &Timelapse, fps: u32) -> Result<Vec<u8>, String> {
    use image::codecs::gif::{GifEncoder, Repeat};
    let fps = fps.clamp(*FPS_RANGE.start(), *FPS_RANGE.end());
    let (_, _, frames) = frames_for_export(t, fps)?;
    let delay = image::Delay::from_numer_denom_ms(1000, fps);
    let mut out = Vec::new();
    {
        let mut enc = GifEncoder::new_with_speed(&mut out, 20);
        enc.set_repeat(Repeat::Infinite)
            .map_err(|e| e.to_string())?;
        for img in frames {
            enc.encode_frame(image::Frame::from_parts(img, 0, 0, delay))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(out)
}

/// A gravação como PNG animado (APNG) em `fps` fotos por segundo, repetindo.
pub fn export_apng(t: &Timelapse, fps: u32) -> Result<Vec<u8>, String> {
    let fps = fps.clamp(*FPS_RANGE.start(), *FPS_RANGE.end());
    let (w, h, frames) = frames_for_export(t, fps)?;
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        enc.set_animated(frames.len() as u32, 0)
            .map_err(|e| e.to_string())?;
        enc.set_frame_delay(1, fps as u16)
            .map_err(|e| e.to_string())?;
        let mut wr = enc.write_header().map_err(|e| e.to_string())?;
        for img in frames {
            wr.write_image_data(img.as_raw())
                .map_err(|e| e.to_string())?;
        }
        wr.finish().map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use photocraft_geom::Rect;

    #[test]
    fn records_and_exports() {
        let mut d = Document::new(1280, 640);
        d.timelapse.push(capture(&d).unwrap());
        d.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 640, 640), &[1.0, 0.0, 0.0, 1.0]);
        d.timelapse.push(capture(&d).unwrap());
        assert_eq!(d.timelapse.len(), 2);
        let img = d.timelapse.decode(1).unwrap();
        assert_eq!(img.dimensions(), (640, 320));
        let p = img.get_pixel(10, 10).0;
        assert!(p[0] > 200 && p[1] < 60, "{p:?}");
        let p = img.get_pixel(600, 10).0;
        assert!(p[0] > 200 && p[1] > 200, "{p:?}");

        let gif = export_gif(&d.timelapse, 10).unwrap();
        let n = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(gif)).unwrap();
        use image::AnimationDecoder;
        assert_eq!(n.into_frames().count(), 2 + 10);
        let apng = export_apng(&d.timelapse, 10).unwrap();
        let dec = png::Decoder::new(std::io::Cursor::new(apng));
        let r = dec.read_info().unwrap();
        assert_eq!(r.info().animation_control().unwrap().num_frames, 12);
    }

    #[test]
    fn saved_in_the_project() {
        let mut d = Document::new(64, 64);
        d.timelapse.push(capture(&d).unwrap());
        d.timelapse.push(capture(&d).unwrap());
        d.timelapse.enabled = false;
        let back = crate::project::load(&crate::project::save(&d.snapshot()).unwrap()).unwrap();
        assert_eq!(back.timelapse, d.timelapse);
    }

    #[test]
    fn thins_out_when_full() {
        let mut t = Timelapse::default();
        for i in 0..=MAX_FRAMES {
            t.push(vec![(i % 256) as u8, (i / 256) as u8]);
        }
        let n = t.len();
        t.push(t.frames[n - 1].to_vec());
        assert_eq!(t.len(), n, "foto igual à anterior não entra");
        assert!(t.len() <= MAX_FRAMES / 2 + 2);
        assert_eq!(*t.frames[0], vec![0, 0]);
        let last = MAX_FRAMES;
        assert_eq!(
            **t.frames.last().unwrap(),
            vec![(last % 256) as u8, (last / 256) as u8]
        );
    }
}
