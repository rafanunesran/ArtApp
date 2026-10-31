//! Assistente de animação (como o Animation Assist do Procreate, refeito do zero): cada camada de
//! cima, ou pasta, é um quadro, de baixo para cima. A tela mostra só o quadro atual, com a pele
//! de cebola dos vizinhos; a exportação gera um quadro por vez.

use serde::{Deserialize, Serialize};

use crate::document::Document;

/// Como a reprodução continua no fim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Playback {
    #[default]
    Loop,
    PingPong,
    Once,
}

/// Quadros por segundo aceitos.
pub const FPS_RANGE: std::ops::RangeInclusive<f32> = 1.0..=60.0;
/// Quadros de pele de cebola de cada lado, no máximo.
pub const ONION_MAX: u8 = 12;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Animation {
    pub enabled: bool,
    pub fps: f32,
    pub playback: Playback,
    /// Quadros de pele de cebola antes e depois do atual.
    pub onion_before: u8,
    pub onion_after: u8,
    /// Opacidade da pele de cebola mais próxima (as outras vão sumindo).
    pub onion_opacity: f32,
    /// Pele de cebola colorida: vermelha antes, verde depois.
    pub onion_colors: bool,
    /// O primeiro quadro fica sempre à vista (fundo).
    pub background: bool,
    /// O último quadro fica sempre à vista (primeiro plano).
    pub foreground: bool,
    /// Quadro mostrado (índice em [`Document::frames`]).
    #[serde(skip)]
    pub current: usize,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            enabled: false,
            fps: 12.0,
            playback: Playback::Loop,
            onion_before: 1,
            onion_after: 1,
            onion_opacity: 0.35,
            onion_colors: true,
            background: false,
            foreground: false,
            current: 0,
        }
    }
}

/// Como uma camada entra na tela com a animação ligada.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameLook {
    /// Normal (quadro atual, fundo ou primeiro plano).
    Shown,
    /// Pele de cebola: opacidade e, se houver, a cor que tinge.
    Onion(f32, Option<[f32; 3]>),
    Hidden,
}

const BEFORE_TINT: [f32; 3] = [0.9, 0.15, 0.15];
const AFTER_TINT: [f32; 3] = [0.1, 0.65, 0.2];

impl Document {
    /// Os quadros: índice da camada de cima de cada um (pasta ou camada fora de pasta, sem
    /// máscaras), de baixo para cima.
    pub fn frames(&self) -> Vec<usize> {
        self.layers
            .iter()
            .enumerate()
            .filter(|(_, l)| l.parent.is_none() && !l.mask)
            .map(|(i, _)| i)
            .collect()
    }

    /// O quadro (posição em [`Document::frames`]) a que a camada `i` pertence.
    pub fn frame_of(&self, i: usize) -> Option<usize> {
        let layer = self.layers.get(i)?;
        // Máscara: a da camada de baixo.
        if layer.mask {
            return i.checked_sub(1).and_then(|j| self.frame_of(j));
        }
        let top = match layer.parent {
            Some(p) => self.layers.iter().position(|l| l.id == p)?,
            None => i,
        };
        self.frames().iter().position(|&f| f == top)
    }

    /// Como a camada `i` aparece mostrando o quadro `current` (com pele de cebola se `onion`).
    pub fn frame_look(&self, i: usize, current: usize, onion: bool) -> FrameLook {
        let a = &self.animation;
        let n = self.frames().len();
        let Some(k) = self.frame_of(i) else {
            return FrameLook::Shown;
        };
        if k == current || (a.background && k == 0) || (a.foreground && n > 1 && k == n - 1) {
            return FrameLook::Shown;
        }
        if !onion {
            return FrameLook::Hidden;
        }
        let (dist, reach, tint) = if k < current {
            (current - k, a.onion_before, BEFORE_TINT)
        } else {
            (k - current, a.onion_after, AFTER_TINT)
        };
        if dist == 0 || dist > usize::from(reach) {
            return FrameLook::Hidden;
        }
        let fade = 1.0 - (dist - 1) as f32 / f32::from(reach.max(1));
        let opacity = (a.onion_opacity * fade).clamp(0.0, 1.0);
        FrameLook::Onion(opacity, a.onion_colors.then_some(tint))
    }

    /// Mostra o quadro da camada ativa (ao escolher uma camada, a tela vai para o quadro dela).
    pub fn follow_active_frame(&mut self) {
        if let Some(k) = self.frame_of(self.active) {
            self.animation.current = k;
        }
    }

    /// Próximo quadro na reprodução, a partir de `current` indo na direção `dir` (1 ou -1).
    /// Devolve o quadro e a direção nova; `None` quando a reprodução "uma vez" acabou.
    pub fn next_frame(&self, current: usize, dir: i8) -> Option<(usize, i8)> {
        let n = self.frames().len();
        if n <= 1 {
            return None;
        }
        let last = n - 1;
        match self.animation.playback {
            Playback::Loop => Some(((current + 1) % n, 1)),
            Playback::Once => (current < last).then_some((current + 1, 1)),
            Playback::PingPong => {
                if dir >= 0 {
                    if current >= last {
                        Some((last - 1, -1))
                    } else {
                        Some((current + 1, 1))
                    }
                } else if current == 0 {
                    Some((1, 1))
                } else {
                    Some((current - 1, -1))
                }
            }
        }
    }

    /// Quadro novo: uma camada vazia acima de tudo, já ativa.
    pub fn add_frame(&mut self) {
        self.active = self.layers.len().saturating_sub(1);
        self.add_layer();
        let n = self.frames().len();
        if let Some(l) = self.active_layer_mut() {
            l.name = format!("Quadro {n}");
        }
        self.follow_active_frame();
    }

    /// Pixels RGBA 8 bits do quadro `k` (sem pele de cebola), para exportar.
    pub fn render_frame(&self, k: usize, with_paper: bool) -> Vec<[u8; 4]> {
        crate::composite::composite_frame(self, k, with_paper)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use photocraft_geom::Rect;

    fn three_frames() -> Document {
        let mut d = Document::new(10, 10);
        for (i, c) in [
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
        ]
        .iter()
        .enumerate()
        {
            if i > 0 {
                d.add_frame();
            }
            let a = d.active;
            d.layers[a].surface.fill_rect(Rect::new(0, 0, 10, 10), c);
        }
        d.animation.enabled = true;
        d
    }

    #[test]
    fn frames_and_onion_skin() {
        let mut d = three_frames();
        assert_eq!(d.frames().len(), 3);
        assert_eq!(d.animation.current, 2);
        // Exportar o quadro 1: só verde.
        let px = d.render_frame(1, true);
        assert_eq!(px[0], [0, 255, 0, 255]);
        // Mostrando o quadro 1, o 0 é pele de cebola, o 2 também.
        assert!(matches!(d.frame_look(0, 1, true), FrameLook::Onion(..)));
        assert!(matches!(d.frame_look(2, 1, true), FrameLook::Onion(..)));
        assert_eq!(d.frame_look(1, 1, true), FrameLook::Shown);
        d.animation.onion_after = 0;
        assert_eq!(d.frame_look(2, 1, true), FrameLook::Hidden);
        // Fundo sempre à vista.
        d.animation.background = true;
        assert_eq!(d.frame_look(0, 2, false), FrameLook::Shown);
    }

    #[test]
    fn playback_modes() {
        let mut d = three_frames();
        assert_eq!(d.next_frame(2, 1), Some((0, 1)));
        d.animation.playback = Playback::Once;
        assert_eq!(d.next_frame(2, 1), None);
        d.animation.playback = Playback::PingPong;
        assert_eq!(d.next_frame(2, 1), Some((1, -1)));
        assert_eq!(d.next_frame(0, -1), Some((1, 1)));
    }
}

/// A ordem dos quadros num arquivo animado: ida e volta repete os do meio de trás para frente.
pub fn sequence(n: usize, playback: Playback) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    if playback == Playback::PingPong && n > 2 {
        v.extend((1..n - 1).rev());
    }
    v
}

/// Os quadros prontos para exportar, reduzidos para caber em `max_side` (0 = tamanho da tela).
fn export_frames(doc: &Document, max_side: u32, with_paper: bool) -> (u32, u32, Vec<Vec<u8>>) {
    let (w, h) = (doc.width, doc.height);
    let scale = if max_side > 0 && w.max(h) > max_side {
        f64::from(max_side) / f64::from(w.max(h))
    } else {
        1.0
    };
    let (tw, th) = (
        ((f64::from(w) * scale).round() as u32).max(1),
        ((f64::from(h) * scale).round() as u32).max(1),
    );
    let n = doc.frames().len();
    let frames = (0..n)
        .map(|k| {
            let px = doc.render_frame(k, with_paper);
            let raw = px.as_flattened().to_vec();
            if (tw, th) == (w, h) {
                return raw;
            }
            image::RgbaImage::from_raw(w, h, raw).map_or_else(Vec::new, |img| {
                image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle)
                    .into_raw()
            })
        })
        .collect();
    (tw, th, frames)
}

/// A animação como GIF (com o papel; o GIF não tem transparência parcial).
pub fn export_gif(doc: &Document, max_side: u32) -> Result<Vec<u8>, String> {
    use image::codecs::gif::{GifEncoder, Repeat};
    let a = &doc.animation;
    let (w, h, frames) = export_frames(doc, max_side, true);
    if frames.is_empty() {
        return Err("a animação não tem quadros".into());
    }
    let delay_ms = (1000.0 / a.fps.clamp(*FPS_RANGE.start(), *FPS_RANGE.end())).round() as u32;
    let mut out = Vec::new();
    {
        let mut enc = GifEncoder::new_with_speed(&mut out, 10);
        if a.playback != Playback::Once {
            enc.set_repeat(Repeat::Infinite)
                .map_err(|e| e.to_string())?;
        }
        for k in sequence(frames.len(), a.playback) {
            let img = image::RgbaImage::from_raw(w, h, frames[k].clone())
                .ok_or("quadro com tamanho errado")?;
            let frame =
                image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(delay_ms, 1));
            enc.encode_frame(frame).map_err(|e| e.to_string())?;
        }
    }
    Ok(out)
}

/// A animação como PNG animado (APNG), com transparência se `with_paper` for falso.
pub fn export_apng(doc: &Document, max_side: u32, with_paper: bool) -> Result<Vec<u8>, String> {
    let a = &doc.animation;
    let (w, h, frames) = export_frames(doc, max_side, with_paper);
    if frames.is_empty() {
        return Err("a animação não tem quadros".into());
    }
    let seq = sequence(frames.len(), a.playback);
    let fps = a.fps.clamp(*FPS_RANGE.start(), *FPS_RANGE.end());
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let plays = if a.playback == Playback::Once { 1 } else { 0 };
        enc.set_animated(seq.len() as u32, plays)
            .map_err(|e| e.to_string())?;
        enc.set_frame_delay(100, (fps * 100.0).round() as u16)
            .map_err(|e| e.to_string())?;
        let mut wr = enc.write_header().map_err(|e| e.to_string())?;
        for k in seq {
            wr.write_image_data(&frames[k]).map_err(|e| e.to_string())?;
        }
        wr.finish().map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod export_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use photocraft_geom::Rect;

    #[test]
    fn gif_and_apng_have_every_frame() {
        let mut d = Document::new(40, 20);
        d.layers[0]
            .surface
            .fill_rect(Rect::new(0, 0, 10, 10), &[1.0, 0.0, 0.0, 1.0]);
        d.add_frame();
        let a = d.active;
        d.layers[a]
            .surface
            .fill_rect(Rect::new(20, 0, 30, 10), &[0.0, 0.0, 1.0, 1.0]);
        d.animation.playback = Playback::PingPong;
        assert_eq!(sequence(3, Playback::PingPong), vec![0, 1, 2, 1]);
        let gif = export_gif(&d, 20).unwrap();
        use image::AnimationDecoder;
        let dec = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(gif)).unwrap();
        let frames = dec.into_frames().collect_frames().unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].buffer().dimensions(), (20, 10));
        let apng = export_apng(&d, 0, false).unwrap();
        let reader = png::Decoder::new(std::io::Cursor::new(apng))
            .read_info()
            .unwrap();
        assert_eq!(reader.info().animation_control().unwrap().num_frames, 2);
    }
}
