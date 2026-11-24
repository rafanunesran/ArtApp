//! Voltar ao mesmo projeto ao abrir o app, inclusive depois de ele fechar com erro.
//!
//! O app guarda em `sessao.json` qual projeto está aberto. Na abertura seguinte reabre esse
//! projeto (o salvamento automático o mantém em dia). Se o arquivo sumiu ou não abre, abre o
//! backup mais recente dele (ver `backup`). Desenhos sem nome já voltam pelo arquivo de
//! recuperação (diálogo "Desenho não salvo" em `projects`).
//!
//! Para não entrar num ciclo (o próprio projeto faz o app fechar ao abrir), um marcador
//! `crash/reabrindo` existe enquanto o projeto reabre; se o app abrir e ele ainda estiver lá,
//! pergunta antes de tentar de novo.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::SystemTime;

use eframe::egui::{self, Align2, RichText, Vec2};

use crate::app::ArtApp;
use crate::platform;
use crate::theme::{color, panel_frame};

/// Depois de abrir, quanto tempo rodando sem erro para tirar o marcador (segundos).
const SAFE_AFTER: f64 = 10.0;

type Found = Result<Option<(PathBuf, Option<SystemTime>)>, String>;

#[derive(Default)]
pub struct Resume {
    started: bool,
    /// O que está gravado em `sessao.json` (para só regravar quando mudar).
    written: Option<Option<PathBuf>>,
    /// Reabrindo este projeto (nome) agora.
    opening: Option<String>,
    /// Quando o projeto reabriu (para tirar o marcador depois de `SAFE_AFTER`).
    opened_at: Option<f64>,
    /// Procurando um backup do projeto (nome, motivo).
    finding: Option<(Receiver<Found>, String, String)>,
    /// Pergunta mostrada quando a última tentativa de reabrir fechou o app.
    pub ask: Option<(PathBuf, String)>,
    /// Abrindo um backup no lugar do projeto: o aviso para quando ele abrir.
    backup_status: Option<String>,
}

fn session_path() -> PathBuf {
    platform::data_dir().join("sessao.json")
}

fn guard_path() -> PathBuf {
    crate::crash::dir().join("reabrindo")
}

/// O projeto aberto na sessão anterior (caminho e nome).
fn read_session() -> Option<(PathBuf, String)> {
    let text = std::fs::read_to_string(session_path()).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let path = PathBuf::from(v.get("path")?.as_str()?);
    let name = v
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| crate::gallery::stem_of(&path), str::to_string);
    Some((path, name))
}

impl ArtApp {
    /// A cada quadro: na abertura reabre o último projeto; depois mantém `sessao.json` em dia.
    pub fn resume_tick(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if !self.resume.started {
            self.resume.started = true;
            self.resume_start();
            crate::crash_view::ask_if_pending(self);
        }
        self.poll_backup_search();
        if let Some(t) = self.resume.opened_at
            && now - t > SAFE_AFTER
        {
            self.resume.opened_at = None;
            let _ = std::fs::remove_file(guard_path());
        }
        if self.resume.opening.is_some() || self.resume.finding.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
            return;
        }
        let path = self
            .projects
            .path
            .clone()
            .filter(|p| crate::projects::kind_of(p) == Some(crate::projects::Kind::Project));
        if self.resume.written.as_ref() != Some(&path) {
            let v = match &path {
                Some(p) => serde_json::json!({
                    "path": p.to_string_lossy(),
                    "name": self.projects.name,
                }),
                None => serde_json::json!({}),
            };
            let _ = platform::write_atomic(&session_path(), v.to_string().as_bytes());
            self.resume.written = Some(path);
        }
    }

    fn resume_start(&mut self) {
        let Some((path, name)) = read_session() else {
            return;
        };
        // Já está abrindo outro arquivo (o desktop abre o arquivo passado ao programa).
        if self.projects.busy() || self.projects.path.is_some() {
            return;
        }
        if guard_path().exists() {
            let _ = std::fs::remove_file(guard_path());
            self.resume.ask = Some((path, name));
            return;
        }
        self.resume_open(path, name);
    }

    fn resume_open(&mut self, path: PathBuf, name: String) {
        if !path.is_file() {
            self.find_backup(name, "o arquivo não foi encontrado".into());
            return;
        }
        let _ = std::fs::write(guard_path(), path.to_string_lossy().as_bytes());
        self.open_path(path);
        if self.projects.busy() {
            self.status = format!("Voltando para “{name}”…");
            self.resume.opening = Some(name);
        }
    }

    /// `projects` terminou de abrir um arquivo. `err` = não abriu.
    pub fn resume_opened(&mut self, now: f64, err: Option<&str>) {
        let Some(name) = self.resume.opening.take() else {
            return;
        };
        let backup = self.resume.backup_status.take();
        match err {
            None => {
                self.resume.opened_at = Some(now);
                if let Some(msg) = backup {
                    self.status = msg;
                    return;
                }
                self.status = if crate::crash::crashed_last_time() {
                    format!(
                        "O app tinha fechado com erro. Voltamos para “{name}” como foi salvo \
                         automaticamente."
                    )
                } else {
                    format!("De volta a “{name}”.")
                };
            }
            Some(e) => {
                let _ = std::fs::remove_file(guard_path());
                if backup.is_some() {
                    self.status = format!("Nem o projeto “{name}” nem o backup dele abriram: {e}");
                } else {
                    self.find_backup(name, e.to_string());
                }
            }
        }
    }

    /// O projeto não abriu: procura o backup mais recente dele.
    fn find_backup(&mut self, name: String, why: String) {
        let dest = self.backup.dest.clone();
        let (tx, rx) = channel();
        let n = name.clone();
        std::thread::spawn(move || {
            let _ = tx.send(crate::backup::latest_working_backup(&dest, &n));
        });
        self.status = format!("“{name}” não abriu ({why}). Procurando o backup…");
        self.resume.finding = Some((rx, name, why));
    }

    fn poll_backup_search(&mut self) {
        let Some((rx, name, why)) = self.resume.finding.take() else {
            return;
        };
        let found = match rx.try_recv() {
            Err(TryRecvError::Empty) => {
                self.resume.finding = Some((rx, name, why));
                return;
            }
            Err(TryRecvError::Disconnected) => Err("erro interno".to_string()),
            Ok(r) => r,
        };
        match found {
            Ok(Some((path, when))) => {
                let _ = std::fs::write(guard_path(), path.to_string_lossy().as_bytes());
                self.open_backup(path, name.clone());
                if self.projects.busy() {
                    let ago = crate::projects::time_ago(when);
                    let ago = if ago.is_empty() {
                        String::new()
                    } else {
                        format!(", feito {ago}")
                    };
                    self.resume.backup_status = Some(format!(
                        "“{name}” não abriu ({why}). Abrimos o backup mais recente{ago}. Salve \
                         para guardá-lo."
                    ));
                    self.resume.opening = Some(name);
                }
            }
            Ok(None) => {
                self.status = format!("“{name}” não abriu ({why}) e não há backup dele.");
            }
            Err(e) => {
                self.status = format!("“{name}” não abriu ({why}); backup indisponível: {e}");
            }
        }
    }
}

/// Pergunta: na última vez o app fechou enquanto reabria o projeto.
pub fn show(app: &mut ArtApp, ctx: &egui::Context) {
    let Some((path, name)) = app.resume.ask.clone() else {
        return;
    };
    let mut done = false;
    egui::Window::new("Voltar ao projeto")
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .order(egui::Order::Foreground)
        .frame(panel_frame())
        .show(ctx, |ui| {
            ui.set_max_width(340.0);
            ui.label(format!(
                "Na última vez, o ArtApp fechou enquanto abria “{name}”. O que fazer?"
            ));
            ui.label(
                RichText::new("O backup é a cópia mais recente guardada na pasta de backup.")
                    .size(12.0)
                    .color(color::TEXT_MUTED),
            );
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(RichText::new("Abrir o backup").strong())
                    .clicked()
                {
                    app.find_backup(name.clone(), "fechou o app ao abrir".into());
                    done = true;
                }
                if ui.button("Tentar o projeto de novo").clicked() {
                    app.resume_open(path.clone(), name.clone());
                    done = true;
                }
                if ui.button("Começar em branco").clicked() {
                    let _ = std::fs::remove_file(session_path());
                    done = true;
                }
            });
        });
    if done {
        app.resume.ask = None;
    }
}
