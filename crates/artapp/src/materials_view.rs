//! Janela de materiais: guarda a camada ativa como material, mostra os materiais guardados e
//! põe um deles no desenho com um toque (a biblioteca de materiais do Clip Studio).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use artapp_core::{materials, project};
use eframe::egui::{self, Align2, RichText, TextureHandle, Vec2};

use crate::app::ArtApp;
use crate::platform;
use crate::theme::{color, icon, panel_frame};

const THUMB: f32 = 96.0;

#[derive(Default)]
pub struct MaterialsState {
    pub open: bool,
    /// Os arquivos da pasta (material ou imagem), do mais novo ao mais velho.
    entries: Vec<PathBuf>,
    scanned: bool,
    thumbs: HashMap<PathBuf, Option<TextureHandle>>,
    /// Nome para o próximo material guardado.
    name: String,
    /// Material esperando confirmação para apagar.
    deleting: Option<PathBuf>,
}

/// Onde os materiais ficam.
pub fn dir() -> PathBuf {
    let d = platform::data_dir().join("materiais");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn is_material(p: &Path) -> bool {
    p.extension().is_some_and(|e| {
        e.eq_ignore_ascii_case(materials::EXT)
            || ["png", "jpg", "jpeg", "webp"]
                .iter()
                .any(|x| e.eq_ignore_ascii_case(x))
    })
}

impl MaterialsState {
    fn scan(&mut self) {
        let mut v: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir())
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| is_material(&e.path()))
            .map(|e| {
                let t = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                (t, e.path())
            })
            .collect();
        v.sort_by_key(|e| std::cmp::Reverse(e.0));
        self.entries = v.into_iter().map(|(_, p)| p).collect();
        self.scanned = true;
    }

    fn thumb(&mut self, ctx: &egui::Context, path: &Path) -> Option<TextureHandle> {
        self.thumbs
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                let (w, h, px) = if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case(materials::EXT))
                {
                    let png = project::read_preview(path)?;
                    project::decode_rgba(&png).ok()?
                } else {
                    let bytes = std::fs::read(path).ok()?;
                    artapp_core::edit::decode_image(&bytes).ok()?
                };
                let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px);
                Some(ctx.load_texture(
                    format!("mat:{}", path.display()),
                    img,
                    egui::TextureOptions::LINEAR,
                ))
            })
            .clone()
    }
}

impl ArtApp {
    /// Guarda a camada ativa como material.
    pub fn save_material(&mut self, name: &str) {
        let Some(m) = materials::from_active(&self.doc) else {
            self.status = "A camada ativa está vazia (ou é pasta/máscara).".into();
            return;
        };
        let name = crate::projects::clean_name(if name.trim().is_empty() {
            &m.layers[0].name
        } else {
            name
        });
        let path = crate::gallery::free_path(&dir(), &name, materials::EXT);
        self.status = match materials::encode(&m)
            .and_then(|b| std::fs::write(&path, b).map_err(|e| e.to_string()))
        {
            Ok(()) => format!("Material “{name}” guardado."),
            Err(e) => format!("Não foi possível guardar o material: {e}"),
        };
        self.materials.scanned = false;
    }

    /// Põe o material (ou a imagem) do arquivo numa camada nova.
    pub fn place_material(&mut self, path: &Path) {
        let name = crate::gallery::stem_of(path);
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(materials::EXT))
        {
            self.import_image(path);
            return;
        }
        let res = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| materials::decode(&b));
        match res {
            Ok(m) => {
                let r = self.doc.place_material(&m, &name);
                self.invalidate(r);
                self.kit.reset();
                self.status = format!("Material “{name}” posto numa camada nova.");
            }
            Err(e) => self.status = format!("Não foi possível abrir o material: {e}"),
        }
    }

    /// Copia uma imagem para a pasta de materiais.
    pub fn import_material_image(&mut self, path: &Path) {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| "png".into());
        let target = crate::gallery::free_path(&dir(), &crate::gallery::stem_of(path), &ext);
        self.status = match std::fs::copy(path, &target) {
            Ok(_) => "Imagem guardada nos materiais.".into(),
            Err(e) => format!("Não foi possível copiar: {e}"),
        };
        self.materials.scanned = false;
        self.materials.open = true;
    }
}

pub fn show(app: &mut ArtApp, ctx: &egui::Context) {
    if !app.materials.open {
        return;
    }
    if !app.materials.scanned {
        app.materials.scan();
        app.materials.thumbs.clear();
    }
    let mut open = true;
    let mut place = None;
    let mut save = false;
    let mut import = false;
    let mut store = false;
    let mut delete = None;
    let max_h = ctx.content_rect().height() * 0.7;
    egui::Window::new("Materiais")
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(panel_frame())
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(THUMB * 4.0 + 40.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.materials.name)
                        .hint_text("Nome do material")
                        .desired_width(170.0),
                );
                if ui
                    .button(format!("{}  Guardar camada", icon::PLUS))
                    .on_hover_text("Guarda a camada ativa (tinta ou vetor) como material")
                    .clicked()
                {
                    save = true;
                }
                if ui.button("Importar imagem…").clicked() {
                    import = true;
                }
                if ui
                    .button("Mais materiais…")
                    .on_hover_text("Página de pincéis compatíveis do site; o que baixar lá é instalado sozinho")
                    .clicked()
                {
                    store = true;
                }
            });
            ui.separator();
            if app.materials.entries.is_empty() {
                ui.label(
                    RichText::new(
                        "Nenhum material ainda. Desenhe algo numa camada e toque em \
                         Guardar camada para reusar em outros desenhos.",
                    )
                    .color(color::TEXT_MUTED),
                );
                return;
            }
            egui::ScrollArea::vertical()
                .max_height(max_h)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let entries = app.materials.entries.clone();
                    for row in entries.chunks(4) {
                        ui.horizontal(|ui| {
                            for path in row {
                                ui.vertical(|ui| {
                                    ui.set_width(THUMB);
                                    let tex = app.materials.thumb(ui.ctx(), path);
                                    let (rect, resp) = ui.allocate_exact_size(
                                        Vec2::splat(THUMB),
                                        egui::Sense::click(),
                                    );
                                    ui.painter().rect_filled(rect, 6.0, color::SURFACE);
                                    if let Some(t) = tex {
                                        let s = t.size_vec2();
                                        let k = (THUMB - 8.0) / s.x.max(s.y);
                                        let r = egui::Rect::from_center_size(rect.center(), s * k);
                                        egui::Image::new((t.id(), s * k)).paint_at(ui, r);
                                    }
                                    let name = crate::gallery::stem_of(path);
                                    if resp
                                        .on_hover_text(format!("Pôr “{name}” no desenho"))
                                        .clicked()
                                    {
                                        place = Some(path.clone());
                                    }
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::Label::new(RichText::new(&name).size(12.0))
                                                .truncate(),
                                        );
                                        if ui
                                            .small_button(icon::TRASH.to_string())
                                            .on_hover_text("Apagar o material")
                                            .clicked()
                                        {
                                            delete = Some(path.clone());
                                        }
                                    });
                                });
                            }
                        });
                    }
                });
            if let Some(p) = app.materials.deleting.clone() {
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("Apagar “{}”?", crate::gallery::stem_of(&p)));
                    if ui.button("Apagar").clicked() {
                        let _ = std::fs::remove_file(&p);
                        app.materials.deleting = None;
                        app.materials.scanned = false;
                    }
                    if ui.button("Cancelar").clicked() {
                        app.materials.deleting = None;
                    }
                });
            }
        });
    if let Some(p) = delete {
        app.materials.deleting = Some(p);
    }
    if save {
        let name = std::mem::take(&mut app.materials.name);
        app.save_material(&name);
    }
    if import {
        app.pick_image_for(crate::projects::ImageUse::Material);
    }
    if store {
        app.open_brush_store();
    }
    if let Some(p) = place {
        app.place_material(&p);
        app.materials.open = false;
    }
    if !open {
        app.materials.open = false;
    }
}
