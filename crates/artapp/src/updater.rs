//! Atualizar o app (Android): ao abrir, o ArtApp confere a última release no GitHub e, se houver
//! build mais novo, pergunta "Atualizar agora" ou "Depois". Menu › Atualizar app faz o mesmo com
//! um toque. O APK é baixado para a pasta do app e entregue ao instalador do Android
//! (`platform::update`, que chama o `Updater.java`).
//!
//! O Android sempre exige que a pessoa libere "Instalar apps desconhecidos" para o ArtApp uma vez;
//! depois disso, no Android 12 ou mais novo, a atualização costuma entrar sem outro diálogo.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use eframe::egui::{self, Align2, RichText, Vec2};

use crate::app::ArtApp;
use crate::platform::{self, update};
use crate::theme::{color, panel_frame};

/// O APK publicado em cada release (veja `.github/workflows/android.yml`).
const APK_ASSET: &str = "ArtApp.apk";
/// Com o app aberto, volta a conferir ao voltar para ele depois desse tempo (segundos).
const RECHECK_AFTER: f64 = 3600.0;

/// Uma release do GitHub com APK.
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    /// O número do build (o `versionCode` do APK).
    pub build: u64,
    /// Nome mostrado (ex.: "ArtApp 0.1.0 (build 42)").
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// Lê a resposta de `releases/latest`. A tag é `v<versão>-build.<número>`.
pub fn parse_release(json: &str) -> Result<Release, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let tag = v["tag_name"].as_str().unwrap_or_default();
    let build = tag
        .rsplit_once("build.")
        .and_then(|(_, n)| n.parse::<u64>().ok())
        .ok_or_else(|| format!("tag sem número de build: {tag:?}"))?;
    let asset = v["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| a["name"].as_str() == Some(APK_ASSET))
        .ok_or("a release não tem o APK")?;
    let url = asset["browser_download_url"]
        .as_str()
        .ok_or("APK sem link")?
        .to_string();
    let name = v["name"]
        .as_str()
        .filter(|n| !n.is_empty())
        .map_or_else(|| tag.to_string(), str::to_string);
    Ok(Release {
        build,
        name,
        url,
        size: asset["size"].as_u64().unwrap_or(0),
    })
}

/// Onde fica o APK baixado de um build.
fn apk_path(build: u64) -> PathBuf {
    platform::data_dir().join(format!("atualizacao-{build}.apk"))
}

/// Apaga APKs baixados antes, menos o de `keep`.
fn remove_old_apks(keep: Option<u64>) {
    let Ok(dir) = std::fs::read_dir(platform::data_dir()) else {
        return;
    };
    let keep = keep.map(apk_path);
    for e in dir.flatten() {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("atualizacao-") && Some(&p) != keep.as_ref() {
            let _ = std::fs::remove_file(p);
        }
    }
}

enum Step {
    Idle,
    /// Conferindo a última release. `manual` = veio do menu (sem perguntar, já baixa).
    Checking {
        rx: Receiver<Result<Release, String>>,
        manual: bool,
    },
    /// Há versão nova: "Atualizar agora" ou "Depois".
    Offer(Release),
    Downloading {
        release: Release,
        rx: Receiver<Result<(), String>>,
    },
    /// Esperando a pessoa liberar "Instalar apps desconhecidos".
    Permission(PathBuf),
    /// Gravando o desenho antes de o Android fechar o app. `flushed` = já pediu para gravar.
    Saving {
        apk: PathBuf,
        flushed: bool,
    },
    /// Entregue ao Android.
    Installing,
    Failed(String),
}

pub struct Updater {
    step: Step,
    /// Build instalado (`0` fora do Android).
    installed: u64,
    /// Já conferiu ao abrir o app, e quando (tempo do egui).
    started: bool,
    checked_at: f64,
    was_focused: bool,
    /// Build que a pessoa adiou com "Depois" (não pergunta de novo até reabrir o app).
    dismissed: Option<u64>,
}

impl Default for Updater {
    fn default() -> Self {
        Self {
            step: Step::Idle,
            installed: update::installed_build(),
            started: false,
            checked_at: 0.0,
            was_focused: true,
            dismissed: None,
        }
    }
}

impl Updater {
    pub fn installed(&self) -> u64 {
        self.installed
    }

    fn check(&mut self, manual: bool) {
        if !matches!(self.step, Step::Idle | Step::Failed(_)) {
            return;
        }
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(update::latest_release_json().and_then(|j| parse_release(&j)));
        });
        self.step = Step::Checking { rx, manual };
    }

    fn download(&mut self, release: Release) {
        let dest = apk_path(release.build);
        remove_old_apks(Some(release.build));
        // Já baixado (ex.: o Android fechou o app enquanto a pessoa liberava a permissão).
        let ready =
            std::fs::metadata(&dest).is_ok_and(|m| release.size > 0 && m.len() == release.size);
        let (tx, rx) = channel();
        if ready {
            let _ = tx.send(Ok(()));
        } else {
            let url = release.url.clone();
            std::thread::spawn(move || {
                let _ = tx.send(update::download(&url, &dest));
            });
        }
        self.step = Step::Downloading { release, rx };
    }

    /// Com o APK baixado: pede a permissão, se faltar, e depois instala.
    fn ready(&mut self, apk: PathBuf) {
        if update::can_install() {
            self.step = Step::Saving {
                apk,
                flushed: false,
            };
        } else {
            match update::open_install_settings() {
                Ok(()) => self.step = Step::Permission(apk),
                Err(e) => {
                    self.step = Step::Failed(format!("Não deu para abrir as configurações: {e}"))
                }
            }
        }
    }
}

impl ArtApp {
    /// Menu › Atualizar app.
    pub fn check_update(&mut self) {
        self.updater.check(true);
        if matches!(self.updater.step, Step::Checking { .. }) {
            self.status = "Procurando atualização…".into();
        }
    }

    /// A cada quadro: confere ao abrir o app e avança a atualização em andamento.
    pub fn updater_tick(&mut self, ctx: &egui::Context) {
        if !update::SUPPORTED {
            return;
        }
        let now = ctx.input(|i| i.time);
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        let back = focused && !self.updater.was_focused;
        self.updater.was_focused = focused;
        if !self.updater.started || (back && now - self.updater.checked_at >= RECHECK_AFTER) {
            if !self.updater.started {
                // APKs de atualizações que já foram instaladas.
                remove_old_apks(None);
            }
            self.updater.started = true;
            self.updater.checked_at = now;
            self.updater.check(false);
        }
        let u = &mut self.updater;
        match std::mem::replace(&mut u.step, Step::Idle) {
            Step::Checking { rx, manual } => match rx.try_recv() {
                Err(TryRecvError::Empty) => {
                    u.step = Step::Checking { rx, manual };
                    ctx.request_repaint_after(Duration::from_millis(200));
                }
                Err(TryRecvError::Disconnected) => {}
                Ok(Err(e)) => {
                    log::warn!("Conferir atualização: {e}");
                    if manual {
                        self.status = format!("Não deu para procurar atualização: {e}");
                    }
                }
                Ok(Ok(r)) if r.build > u.installed => {
                    if manual {
                        u.download(r);
                    } else if u.dismissed != Some(r.build) {
                        u.step = Step::Offer(r);
                    }
                }
                Ok(Ok(_)) => {
                    if manual {
                        self.status = format!("O ArtApp está atualizado (build {}).", u.installed);
                    }
                }
            },
            Step::Downloading { release, rx } => match rx.try_recv() {
                Err(TryRecvError::Empty) => {
                    u.step = Step::Downloading { release, rx };
                    ctx.request_repaint_after(Duration::from_millis(200));
                }
                Err(TryRecvError::Disconnected) => {}
                Ok(Err(e)) if e == "cancelado" => {
                    self.status = "Atualização cancelada.".into();
                }
                Ok(Err(e)) => u.step = Step::Failed(format!("O download falhou: {e}")),
                Ok(Ok(())) => u.ready(apk_path(release.build)),
            },
            Step::Permission(apk) => {
                if update::can_install() {
                    u.step = Step::Saving {
                        apk,
                        flushed: false,
                    };
                } else {
                    u.step = Step::Permission(apk);
                    ctx.request_repaint_after(Duration::from_millis(500));
                }
            }
            Step::Saving { apk, flushed } => {
                ctx.request_repaint_after(Duration::from_millis(100));
                if self.projects.busy() {
                    u.step = Step::Saving { apk, flushed };
                } else if !flushed {
                    u.step = Step::Saving { apk, flushed: true };
                    self.flush_autosave();
                } else {
                    match update::install(&apk) {
                        Ok(()) => u.step = Step::Installing,
                        Err(e) => u.step = Step::Failed(format!("A instalação falhou: {e}")),
                    }
                }
            }
            Step::Installing => match update::install_state() {
                update::InstallState::Failed(e) => {
                    u.step = Step::Failed(format!("O Android não instalou: {e}"));
                }
                update::InstallState::Cancelled => {
                    self.status = "Atualização cancelada.".into();
                }
                _ => {
                    u.step = Step::Installing;
                    ctx.request_repaint_after(Duration::from_millis(300));
                }
            },
            other => u.step = other,
        }
    }
}

fn window(title: &str) -> egui::Window<'_> {
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .order(egui::Order::Foreground)
        .frame(panel_frame())
}

fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).size(12.5).color(color::TEXT_MUTED)
}

fn mb(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_048_576.0).replace('.', ",")
}

/// Mostra o diálogo da atualização, se houver.
pub fn show(app: &mut ArtApp, ctx: &egui::Context) {
    let u = &mut app.updater;
    let width = 320.0;
    match &u.step {
        Step::Offer(r) => {
            let r = r.clone();
            let (mut now, mut later) = (false, false);
            window("Nova versão do ArtApp").show(ctx, |ui| {
                ui.set_width(width);
                ui.label(format!("{} está disponível.", r.name));
                ui.label(muted(format!(
                    "Você está com o build {}. O desenho aberto é salvo antes de atualizar.",
                    u.installed
                )));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    now = ui
                        .button(RichText::new("Atualizar agora").strong())
                        .clicked();
                    later = ui.button("Depois").clicked();
                });
            });
            if now {
                u.download(r);
            } else if later {
                u.dismissed = Some(r.build);
                u.step = Step::Idle;
            }
        }
        Step::Downloading { release, .. } => {
            let (done, total) = update::progress();
            let total = total.or((release.size > 0).then_some(release.size));
            let mut cancel = false;
            window("Atualizando o ArtApp").show(ctx, |ui| {
                ui.set_width(width);
                ui.label(format!("Baixando {}…", release.name));
                let frac = total.map_or(0.0, |t| done as f32 / t.max(1) as f32);
                ui.add(egui::ProgressBar::new(frac.clamp(0.0, 1.0)).show_percentage());
                ui.label(muted(match total {
                    Some(t) => format!("{} de {} MB", mb(done), mb(t)),
                    None => format!("{} MB", mb(done)),
                }));
                cancel = ui.button("Cancelar").clicked();
            });
            if cancel {
                update::cancel();
            }
        }
        Step::Permission(_) => {
            let (mut settings, mut cancel) = (false, false);
            window("Permitir instalar").show(ctx, |ui| {
                ui.set_width(width);
                ui.add(
                    egui::Label::new(
                        "O Android pede que você libere \"Instalar apps desconhecidos\" para o \
                         ArtApp. É só na primeira vez: ligue a opção e volte para o app que a \
                         atualização continua sozinha.",
                    )
                    .wrap(),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    settings = ui
                        .button(RichText::new("Abrir configurações").strong())
                        .clicked();
                    cancel = ui.button("Cancelar").clicked();
                });
            });
            if settings && let Err(e) = update::open_install_settings() {
                u.step = Step::Failed(format!("Não deu para abrir as configurações: {e}"));
            } else if cancel {
                u.step = Step::Idle;
            }
        }
        Step::Saving { .. } | Step::Installing => {
            window("Atualizando o ArtApp").show(ctx, |ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Instalando a versão nova…");
                });
                ui.add(
                    egui::Label::new(muted(
                        "O ArtApp fecha sozinho para trocar de versão. Depois é só abrir de novo.",
                    ))
                    .wrap(),
                );
            });
        }
        Step::Failed(e) => {
            let e = e.clone();
            let (mut retry, mut close) = (false, false);
            window("Atualização").show(ctx, |ui| {
                ui.set_width(width);
                ui.add(egui::Label::new(e).wrap());
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    retry = ui.button("Tentar de novo").clicked();
                    close = ui.button("Fechar").clicked();
                });
            });
            if retry {
                u.step = Step::Idle;
                u.check(true);
            } else if close {
                u.step = Step::Idle;
            }
        }
        Step::Idle | Step::Checking { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn reads_latest_release() {
        let json = r#"{
            "tag_name": "v0.1.0-build.42",
            "name": "ArtApp 0.1.0 (build 42)",
            "assets": [
                {"name": "ArtApp-Windows-x64.zip", "size": 1, "browser_download_url": "https://x/w.zip"},
                {"name": "ArtApp.apk", "size": 31457280,
                 "browser_download_url": "https://github.com/rafanunesran/ArtApp/releases/download/v0.1.0-build.42/ArtApp.apk"}
            ]
        }"#;
        let r = parse_release(json).unwrap();
        assert_eq!(r.build, 42);
        assert_eq!(r.name, "ArtApp 0.1.0 (build 42)");
        assert_eq!(r.size, 31457280);
        assert!(r.url.ends_with("/ArtApp.apk"));
    }

    #[test]
    fn rejects_release_without_apk_or_build() {
        assert!(parse_release(r#"{"tag_name": "v0.1.0-build.3", "assets": []}"#).is_err());
        assert!(parse_release(r#"{"tag_name": "v0.1.0", "assets": []}"#).is_err());
        assert!(parse_release("não é json").is_err());
    }
}
