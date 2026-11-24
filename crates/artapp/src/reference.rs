//! Janela de referência (do Procreate): uma imagem ou a própria tela flutuando ao lado do
//! desenho, para olhar enquanto pinta.

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions, Vec2};

use crate::app::ArtApp;
use crate::projects::ImageUse;
use crate::theme::panel_frame;

/// Maior lado da textura (imagens maiores são reduzidas).
const MAX_SIDE: u32 = 2048;

pub struct Reference {
    w: u32,
    h: u32,
    px: Vec<u8>,
    tex: Option<TextureHandle>,
}

impl Reference {
    /// `px` em RGBA 8 bits, `w` × `h`.
    pub fn new(w: u32, h: u32, px: Vec<u8>) -> Self {
        let (w, h, px) = shrink(w, h, px);
        Self {
            w,
            h,
            px,
            tex: None,
        }
    }

    fn texture(&mut self, ctx: &egui::Context) -> Option<TextureHandle> {
        if self.tex.is_none() && self.w > 0 && self.h > 0 {
            let img =
                ColorImage::from_rgba_unmultiplied([self.w as usize, self.h as usize], &self.px);
            self.tex = Some(ctx.load_texture("reference", img, TextureOptions::LINEAR));
        }
        self.tex.clone()
    }
}

/// Reduz pelo vizinho mais perto até o maior lado caber em [`MAX_SIDE`].
fn shrink(w: u32, h: u32, px: Vec<u8>) -> (u32, u32, Vec<u8>) {
    let side = w.max(h);
    if side <= MAX_SIDE || px.len() < w as usize * h as usize * 4 {
        return (w, h, px);
    }
    let k = f64::from(side) / f64::from(MAX_SIDE);
    let (nw, nh) = (
        ((f64::from(w) / k) as u32).max(1),
        ((f64::from(h) / k) as u32).max(1),
    );
    let mut out = Vec::with_capacity(nw as usize * nh as usize * 4);
    for y in 0..nh {
        let sy = ((f64::from(y) * k) as u32).min(h - 1) as usize;
        for x in 0..nw {
            let sx = ((f64::from(x) * k) as u32).min(w - 1) as usize;
            let i = (sy * w as usize + sx) * 4;
            out.extend_from_slice(&px[i..i + 4]);
        }
    }
    (nw, nh, out)
}

impl ArtApp {
    /// Põe na referência a tela como está agora.
    pub fn reference_from_canvas(&mut self) {
        let (r, px) = artapp_core::composite::composite_rgba8(&self.doc, self.doc.bounds(), true);
        self.reference = Some(Reference::new(
            r.width(),
            r.height(),
            px.as_flattened().to_vec(),
        ));
    }
}

/// A janela flutuante, se aberta.
pub fn show(app: &mut ArtApp, ctx: &egui::Context) {
    if !app.reference_open {
        return;
    }
    let mut open = true;
    let mut action = None;
    let mut picked = None;
    egui::Window::new("Referência")
        .frame(panel_frame())
        .default_size(Vec2::new(280.0, 300.0))
        // Começa no canto de cima à direita, longe das ferramentas.
        .default_pos(ctx.content_rect().right_top() + Vec2::new(-300.0, 60.0))
        .order(egui::Order::Foreground)
        .resizable(true)
        .collapsible(true)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Imagem…").clicked() {
                    action = Some(ImageUse::Reference);
                }
                if ui.button("Tela atual").clicked() {
                    action = Some(ImageUse::Layer);
                }
            });
            let Some(r) = app.reference.as_mut() else {
                ui.label("Escolha uma imagem ou use a tela atual.");
                return;
            };
            ui.label(
                egui::RichText::new("Toque na imagem para pegar a cor.")
                    .size(12.0)
                    .color(crate::theme::color::TEXT_MUTED),
            );
            let (w, h) = (r.w as f32, r.h as f32);
            if let Some(tex) = r.texture(ctx) {
                let avail = ui.available_size().max(Vec2::splat(80.0));
                let k = (avail.x / w).min(avail.y / h);
                let resp = ui.add(
                    egui::Image::new(&tex)
                        .fit_to_exact_size(Vec2::new(w * k, h * k))
                        .sense(egui::Sense::click()),
                );
                // Tocar na imagem pega a cor (como o conta-gotas).
                if resp.clicked()
                    && let Some(pos) = resp.interact_pointer_pos()
                {
                    let uv = (pos - resp.rect.min) / resp.rect.size();
                    let x = ((uv.x * w) as u32).min(r.w - 1) as usize;
                    let y = ((uv.y * h) as u32).min(r.h - 1) as usize;
                    let i = (y * r.w as usize + x) * 4;
                    if let Some(p) = r.px.get(i..i + 3) {
                        picked = Some(egui::Color32::from_rgb(p[0], p[1], p[2]));
                    }
                }
            }
        });
    if let Some(c) = picked {
        app.color = c;
        app.transparent = false;
        app.remember_color();
    }
    if !open {
        app.reference_open = false;
    }
    match action {
        Some(ImageUse::Reference) => app.pick_image_for(ImageUse::Reference),
        Some(_) => app.reference_from_canvas(),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_images_are_shrunk() {
        let (w, h, px) = shrink(4096, 100, vec![7; 4096 * 100 * 4]);
        assert_eq!((w, h), (2048, 50));
        assert_eq!(px.len(), 2048 * 50 * 4);
        let (w, h, _) = shrink(30, 20, vec![0; 30 * 20 * 4]);
        assert_eq!((w, h), (30, 20));
    }
}
