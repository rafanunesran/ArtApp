//! Organizar a galeria de projetos: pilhas (pastas), renomear, duplicar, mover e apagar.
//! Só mexe em arquivos; a janela fica em `projects.rs`.

use std::path::{Path, PathBuf};

/// Um caminho livre em `dir` para `stem` + `ext` ("Nome", "Nome 2", "Nome 3"…). `ext` vazio
/// para pastas.
pub fn free_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let file = |s: &str| {
        if ext.is_empty() {
            dir.join(s)
        } else {
            dir.join(format!("{s}.{ext}"))
        }
    };
    let first = file(stem);
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|i| file(&format!("{stem} {i}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_string()
}

/// Nome sem extensão (o que aparece para renomear).
pub fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Cria uma pilha (pasta) nova em `dir`.
pub fn new_stack(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let path = free_path(dir, name, "");
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Renomeia um projeto ou pilha, mantendo a extensão. Não substitui outro arquivo.
pub fn rename(path: &Path, name: &str) -> Result<PathBuf, String> {
    let dir = path.parent().ok_or("caminho sem pasta")?;
    let ext = if path.is_dir() {
        String::new()
    } else {
        ext_of(path)
    };
    let target = if ext.is_empty() {
        dir.join(name)
    } else {
        dir.join(format!("{name}.{ext}"))
    };
    if target == path {
        return Ok(target);
    }
    if target.exists() {
        return Err(format!("já existe “{name}” aqui"));
    }
    std::fs::rename(path, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

/// Copia um projeto ao lado dele ("Nome cópia").
pub fn duplicate(path: &Path) -> Result<PathBuf, String> {
    let dir = path.parent().ok_or("caminho sem pasta")?;
    let target = free_path(dir, &format!("{} cópia", stem_of(path)), &ext_of(path));
    std::fs::copy(path, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

/// Move um projeto para dentro de `dir` (sem substituir: ganha um número se o nome existir).
pub fn move_into(path: &Path, dir: &Path) -> Result<PathBuf, String> {
    let target = free_path(dir, &stem_of(path), &ext_of(path));
    if std::fs::rename(path, &target).is_err() {
        // Outro volume: copia e apaga.
        std::fs::copy(path, &target).map_err(|e| e.to_string())?;
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(target)
}

/// Apaga um projeto, ou uma pilha vazia.
pub fn delete(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        std::fs::remove_dir(path).map_err(|_| "a pilha não está vazia".to_string())
    } else {
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }
}

/// O que há numa pilha: quantos arquivos (que `wanted` aceita) e o mais recente deles.
pub fn stack_info(dir: &Path, wanted: impl Fn(&Path) -> bool) -> (usize, Option<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return (0, None);
    };
    let mut n = 0;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() || !wanted(&p) {
            continue;
        }
        n += 1;
        let t = e
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        if newest.as_ref().is_none_or(|(nt, _)| t > *nt) {
            newest = Some((t, p));
        }
    }
    (n, newest.map(|(_, p)| p))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("artapp-gallery-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn stacks_and_files() {
        let d = tmp("ops");
        let a = d.join("Gato.artapp");
        std::fs::write(&a, b"x").unwrap();
        let copy = duplicate(&a).unwrap();
        assert_eq!(copy, d.join("Gato cópia.artapp"));
        assert_eq!(duplicate(&a).unwrap(), d.join("Gato cópia 2.artapp"));
        let s = new_stack(&d, "Bichos").unwrap();
        assert!(s.is_dir());
        assert_eq!(new_stack(&d, "Bichos").unwrap(), d.join("Bichos 2"));
        let moved = move_into(&a, &s).unwrap();
        assert_eq!(moved, s.join("Gato.artapp"));
        assert!(!a.exists());
        let (n, newest) = stack_info(&s, |p| p.extension().is_some());
        assert_eq!((n, newest), (1, Some(moved.clone())));
        assert!(delete(&s).is_err(), "pilha com desenho não é apagada");
        let r = rename(&moved, "Gata").unwrap();
        assert_eq!(r, s.join("Gata.artapp"));
        assert!(
            rename(&copy, "Gato cópia 2").is_err(),
            "não substitui outro arquivo"
        );
        let s2 = rename(&s, "Felinos").unwrap();
        assert!(s2.join("Gata.artapp").exists());
        assert!(d.join("Bichos 2").is_dir());
        delete(&s2.join("Gata.artapp")).unwrap();
        delete(&s2).unwrap();
        assert!(!s2.exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
