//! A pasta `hot/` (dentro da pasta privada do app) guarda a atualização rápida: a
//! `libartapp.so` de um build mais novo, baixada de uma release, que o carregador (`artapp-boot`)
//! abre no lugar da que veio no APK.
//!
//! Este arquivo é compartilhado: o carregador o usa para escolher a biblioteca ao abrir o app e o
//! `artapp` (por `#[path]`) o usa para gravar a atualização baixada. Só usa a `std`.
//!
//! Arquivos:
//! - `current`: a atualização instalada (`build=`, `shell=`, `file=`, `size=`, uma por linha).
//! - `lib-<build>.so`: a biblioteca.
//! - `trial`: `<build> <tentativas>`; o carregador soma uma a cada abertura e o app apaga quando
//!   abriu direito ([`confirm`]). Se o app fechar antes de confirmar [`MAX_TRIALS`] vezes, o
//!   build é marcado como ruim e o app volta para a biblioteca do APK.
//! - `bad-<build>`: build que não abriu (não é usado de novo).
//! - `disabled`: o aparelho não deixou carregar a biblioteca baixada; daqui em diante só APK.

use std::fs;
use std::path::{Path, PathBuf};

pub const DIR: &str = "hot";
/// Aberturas seguidas sem confirmar antes de desistir de um build.
pub const MAX_TRIALS: u32 = 2;

/// O que está em `current`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Current {
    pub build: u64,
    /// A "casca" (Java, manifesto, carregador) para a qual a biblioteca foi feita.
    pub shell: String,
    /// Nome do arquivo dentro de `hot/`.
    pub file: String,
    pub size: u64,
}

impl Current {
    pub fn parse(text: &str) -> Option<Self> {
        let mut build = None;
        let mut shell = None;
        let mut file = None;
        let mut size = None;
        for line in text.lines() {
            match line.trim().split_once('=') {
                Some(("build", v)) => build = v.parse().ok(),
                Some(("shell", v)) => shell = Some(v.to_string()),
                Some(("file", v)) => file = Some(v.to_string()),
                Some(("size", v)) => size = v.parse().ok(),
                _ => {}
            }
        }
        let file: String = file?;
        // Só um nome simples dentro de hot/.
        if file.is_empty() || file.contains('/') || file.starts_with('.') {
            return None;
        }
        Some(Self {
            build: build?,
            shell: shell?,
            file,
            size: size?,
        })
    }

    pub fn to_text(&self) -> String {
        format!(
            "build={}\nshell={}\nfile={}\nsize={}\n",
            self.build, self.shell, self.file, self.size
        )
    }
}

/// O nome do arquivo da biblioteca de um build.
pub fn lib_name(build: u64) -> String {
    format!("lib-{build}.so")
}

/// A escolha do carregador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// Abrir a biblioteca baixada.
    Hot { path: PathBuf, build: u64 },
    /// Abrir a do APK. `note` explica, quando havia uma atualização que não foi usada.
    Bundled { note: Option<String> },
}

fn read(dir: &Path, name: &str) -> Option<String> {
    fs::read_to_string(dir.join(name)).ok()
}

pub fn read_current(dir: &Path) -> Option<Current> {
    read(dir, "current").and_then(|t| Current::parse(&t))
}

pub fn is_bad(dir: &Path, build: u64) -> bool {
    dir.join(format!("bad-{build}")).exists()
}

pub fn disabled(dir: &Path) -> bool {
    dir.join("disabled").exists()
}

/// Tira a atualização instalada (o APK passa a valer). Mantém as marcas de builds ruins.
pub fn clear(dir: &Path) {
    let _ = fs::remove_file(dir.join("current"));
    let _ = fs::remove_file(dir.join("trial"));
    remove_libs(dir, None);
}

/// Apaga as bibliotecas baixadas, menos `keep`.
pub fn remove_libs(dir: &Path, keep: Option<&str>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        let lib = name.starts_with("lib-") || name.starts_with("dl-");
        if lib && Some(name.as_ref()) != keep {
            let _ = fs::remove_file(e.path());
        }
    }
}

/// Marca `build` como ruim e volta para o APK. `device` = o aparelho recusou carregar a
/// biblioteca (não adianta tentar outras).
pub fn mark_bad(dir: &Path, build: u64, device: bool) {
    let _ = fs::write(dir.join(format!("bad-{build}")), b"");
    if device {
        let _ = fs::write(dir.join("disabled"), b"");
    }
    clear(dir);
}

/// O app abriu direito com a biblioteca baixada: zera as tentativas.
pub fn confirm(dir: &Path) {
    let _ = fs::remove_file(dir.join("trial"));
}

/// Decide qual biblioteca abrir. `shell` e `bundled` são os da casca instalada (o APK).
pub fn choose(dir: &Path, shell: &str, bundled: u64) -> Choice {
    let Some(cur) = read_current(dir) else {
        return Choice::Bundled { note: None };
    };
    if cur.build <= bundled || cur.shell != shell {
        // O APK foi atualizado (é igual ou mais novo) ou a casca mudou: a baixada não serve mais.
        clear(dir);
        return Choice::Bundled { note: None };
    }
    if is_bad(dir, cur.build) || disabled(dir) {
        clear(dir);
        return Choice::Bundled { note: None };
    }
    let path = dir.join(&cur.file);
    if fs::metadata(&path).map(|m| m.len()).ok() != Some(cur.size) {
        clear(dir);
        return Choice::Bundled {
            note: Some(format!(
                "A atualização rápida (build {}) estava incompleta; abrindo a versão do APK.",
                cur.build
            )),
        };
    }
    let trials = read(dir, "trial")
        .and_then(|t| {
            let (b, n) = t.trim().split_once(' ')?;
            (b.parse::<u64>().ok()? == cur.build).then(|| n.parse::<u32>().ok())?
        })
        .unwrap_or(0);
    if trials >= MAX_TRIALS {
        mark_bad(dir, cur.build, false);
        return Choice::Bundled {
            note: Some(format!(
                "A atualização rápida (build {}) não abriu direito e foi desfeita; abrindo a \
                 versão do APK.",
                cur.build
            )),
        };
    }
    let _ = fs::write(dir.join("trial"), format!("{} {}", cur.build, trials + 1));
    Choice::Hot {
        path,
        build: cur.build,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("artapp-hot-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn install(dir: &Path, build: u64, shell: &str) {
        let file = lib_name(build);
        fs::write(dir.join(&file), b"12345").unwrap();
        let cur = Current {
            build,
            shell: shell.into(),
            file,
            size: 5,
        };
        fs::write(dir.join("current"), cur.to_text()).unwrap();
    }

    #[test]
    fn current_round_trip_and_rejects_paths() {
        let c = Current {
            build: 7,
            shell: "abc".into(),
            file: "lib-7.so".into(),
            size: 9,
        };
        assert_eq!(Current::parse(&c.to_text()), Some(c));
        assert!(Current::parse("build=1\nshell=a\nfile=../x.so\nsize=1").is_none());
        assert!(Current::parse("build=1\nshell=a\nsize=1").is_none());
    }

    #[test]
    fn picks_newer_matching_build() {
        let d = tmp("pick");
        install(&d, 50, "s1");
        assert_eq!(
            choose(&d, "s1", 40),
            Choice::Hot {
                path: d.join("lib-50.so"),
                build: 50
            }
        );
    }

    #[test]
    fn ignores_older_or_other_shell() {
        let d = tmp("old");
        install(&d, 50, "s1");
        assert_eq!(choose(&d, "s1", 50), Choice::Bundled { note: None });
        assert!(!d.join("current").exists());
        install(&d, 60, "s1");
        assert_eq!(choose(&d, "s2", 40), Choice::Bundled { note: None });
        assert!(!d.join("lib-60.so").exists());
    }

    #[test]
    fn rolls_back_after_failed_starts() {
        let d = tmp("trial");
        install(&d, 50, "s1");
        for _ in 0..MAX_TRIALS {
            assert!(matches!(choose(&d, "s1", 40), Choice::Hot { .. }));
        }
        assert!(matches!(
            choose(&d, "s1", 40),
            Choice::Bundled { note: Some(_) }
        ));
        assert!(is_bad(&d, 50));
        // Confirmado: não conta como falha.
        install(&d, 51, "s1");
        for _ in 0..5 {
            assert!(matches!(choose(&d, "s1", 40), Choice::Hot { .. }));
            confirm(&d);
        }
    }

    #[test]
    fn incomplete_file_falls_back() {
        let d = tmp("size");
        install(&d, 50, "s1");
        fs::write(d.join("lib-50.so"), b"12").unwrap();
        assert!(matches!(
            choose(&d, "s1", 40),
            Choice::Bundled { note: Some(_) }
        ));
    }
}
