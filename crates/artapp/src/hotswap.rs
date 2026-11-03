//! Atualização rápida (Android): em vez do APK inteiro, baixa só a `libartapp.so` nova da
//! release (compactada, uma por arquitetura), confere o SHA-256 e a deixa em `files/hot/`. Na
//! próxima abertura o carregador do APK (`crates/artapp-boot`) usa a nova; se ela não abrir, volta
//! sozinho para a do APK.
//!
//! Só vale quando a "casca" do APK instalado (Java, manifesto, carregador) é a mesma para a qual
//! a biblioteca nova foi compilada: o `update.json` da release diz qual é (`shell`). Se mudou, o
//! updater cai no APK de sempre.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use sha2::{Digest, Sha256};

#[path = "../../artapp-boot/src/hot.rs"]
#[allow(dead_code)]
mod hot;

/// O manifesto publicado em cada release (veja `.github/workflows/android.yml`).
pub const MANIFEST_ASSET: &str = "update.json";

/// O que o carregador contou ao abrir o app.
#[derive(Debug, Default)]
pub struct Boot {
    /// A casca instalada. `None` = APK antigo, sem carregador (só atualiza por APK).
    pub shell: Option<String>,
    /// Build da atualização rápida em uso, se for o caso.
    pub hot_build: Option<u64>,
    pub dir: Option<PathBuf>,
    /// Aviso do carregador (ex.: atualização desfeita).
    pub note: Option<String>,
}

pub fn boot() -> &'static Boot {
    static BOOT: OnceLock<Boot> = OnceLock::new();
    BOOT.get_or_init(|| {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Boot {
            shell: var("ARTAPP_SHELL"),
            hot_build: var("ARTAPP_BOOT")
                .and_then(|b| b.strip_prefix("hot:").and_then(|n| n.parse().ok())),
            dir: var("ARTAPP_HOT_DIR").map(PathBuf::from),
            note: var("ARTAPP_BOOT_NOTE"),
        }
    })
}

/// O build desta biblioteca (o CI passa `ARTAPP_VERSION_CODE` ao compilar). `None` fora do CI.
pub fn compiled_build() -> Option<u64> {
    option_env!("ARTAPP_VERSION_CODE").and_then(|v| v.parse().ok())
}

/// A pasta de bibliotecas do Android para esta arquitetura.
pub fn abi() -> Option<&'static str> {
    if cfg!(target_arch = "aarch64") {
        Some("arm64-v8a")
    } else if cfg!(target_arch = "x86_64") {
        Some("x86_64")
    } else if cfg!(target_arch = "arm") {
        Some("armeabi-v7a")
    } else {
        None
    }
}

/// A biblioteca de uma release para este aparelho.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fast {
    /// Link do `.so.gz`.
    pub url: String,
    /// SHA-256 (hex) da `.so` descompactada.
    pub sha256: String,
    /// Tamanho descompactado e baixado.
    pub size: u64,
    pub gz_size: u64,
}

/// Lê o `update.json`: `{build, shell, libs: {<abi>: {file, sha256, size, gz_size}}}`. `asset_url`
/// dá o link de um arquivo da mesma release. `Ok(None)` = não serve para este aparelho.
pub fn parse_manifest(
    json: &str,
    build: u64,
    shell: &str,
    abi: &str,
    asset_url: impl Fn(&str) -> Option<String>,
) -> Result<Option<Fast>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if v["build"].as_u64() != Some(build) {
        return Err("o update.json é de outro build".into());
    }
    if v["shell"].as_str() != Some(shell) {
        return Ok(None);
    }
    let lib = &v["libs"][abi];
    let (Some(file), Some(sha256), Some(size)) = (
        lib["file"].as_str(),
        lib["sha256"].as_str(),
        lib["size"].as_u64(),
    ) else {
        return Ok(None);
    };
    let sha256 = sha256.to_ascii_lowercase();
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("hash inválido no update.json".into());
    }
    let url = asset_url(file).ok_or("a release não tem a biblioteca do update.json")?;
    Ok(Some(Fast {
        url,
        sha256,
        size,
        gz_size: lib["gz_size"].as_u64().unwrap_or(0),
    }))
}

/// Dá para atualizar rápido para `build` neste aparelho? (casca com carregador, aparelho que
/// aceita, build que não falhou antes.)
pub fn available(build: u64) -> Option<(&'static str, &'static Path)> {
    let b = boot();
    let shell = b.shell.as_deref()?;
    let dir = b.dir.as_deref()?;
    abi()?;
    (!hot::disabled(dir) && !hot::is_bad(dir, build)).then_some((shell, dir))
}

/// Já há uma atualização baixada (esperando o app reabrir)? Devolve o build.
pub fn pending() -> Option<u64> {
    let b = boot();
    let cur = hot::read_current(b.dir.as_deref()?)?;
    (Some(cur.build) != b.hot_build && Some(cur.shell.as_str()) == b.shell.as_deref())
        .then_some(cur.build)
}

/// Onde baixar o `.so.gz` de um build.
pub fn download_path(dir: &Path, build: u64) -> PathBuf {
    dir.join(format!("dl-{build}.so.gz"))
}

/// Descompacta `gz` em `dir`, confere tamanho e hash e marca como a atualização a usar.
pub fn install(dir: &Path, gz: &Path, build: u64, shell: &str, fast: &Fast) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let name = hot::lib_name(build);
    let part = dir.join(format!("{name}.part"));
    let result = (|| -> Result<(), String> {
        let file = fs::File::open(gz).map_err(|e| e.to_string())?;
        let mut dec = flate2::read::GzDecoder::new(std::io::BufReader::new(file));
        let mut out = fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut buf = vec![0u8; 1 << 16];
        let mut total = 0u64;
        loop {
            let n = dec
                .read(&mut buf)
                .map_err(|e| format!("arquivo corrompido: {e}"))?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
            out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            total += n as u64;
        }
        out.sync_all().map_err(|e| e.to_string())?;
        let got: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if total != fast.size || got != fast.sha256 {
            return Err("o arquivo baixado não confere (hash diferente)".into());
        }
        let lib = dir.join(&name);
        let _ = fs::remove_file(&lib);
        fs::rename(&part, &lib).map_err(|e| e.to_string())?;
        // Código carregado de fora do APK tem que ser só leitura (exigência do Android 14).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&lib, fs::Permissions::from_mode(0o444));
        }
        let cur = hot::Current {
            build,
            shell: shell.to_string(),
            file: name.clone(),
            size: total,
        };
        crate::platform::write_atomic(&dir.join("current"), cur.to_text().as_bytes())?;
        let _ = fs::remove_file(dir.join("trial"));
        Ok(())
    })();
    let _ = fs::remove_file(&part);
    let _ = fs::remove_file(gz);
    if result.is_ok() {
        // A biblioteca em uso pode ser apagada: o sistema a mantém aberta até o app fechar.
        hot::remove_libs(dir, Some(&name));
    }
    result
}

/// O app abriu direito com a atualização rápida: avisa o carregador para não desfazer.
pub fn confirm() {
    let b = boot();
    if b.hot_build.is_some()
        && let Some(dir) = &b.dir
    {
        hot::confirm(dir);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const SHA: &str = "a3f1c2d4e5f60718293a4b5c6d7e8f90112233445566778899aabbccddeeff00";

    fn manifest() -> String {
        format!(
            r#"{{"build": 42, "shell": "s1", "libs": {{
                "arm64-v8a": {{"file": "libartapp-arm64-v8a.so.gz", "sha256": "{SHA}",
                               "size": 100, "gz_size": 40}}}}}}"#
        )
    }

    #[test]
    fn reads_manifest() {
        let url = |f: &str| Some(format!("https://x/{f}"));
        let f = parse_manifest(&manifest(), 42, "s1", "arm64-v8a", url)
            .unwrap()
            .unwrap();
        assert_eq!(f.url, "https://x/libartapp-arm64-v8a.so.gz");
        assert_eq!((f.size, f.gz_size), (100, 40));
        // Outra casca ou arquitetura: não serve (vai por APK).
        assert_eq!(
            parse_manifest(&manifest(), 42, "s2", "arm64-v8a", url).unwrap(),
            None
        );
        assert_eq!(
            parse_manifest(&manifest(), 42, "s1", "x86_64", url).unwrap(),
            None
        );
        assert!(parse_manifest(&manifest(), 43, "s1", "arm64-v8a", url).is_err());
    }

    #[test]
    fn installs_and_checks_hash() {
        let dir = std::env::temp_dir().join(format!("artapp-hotswap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let lib = b"conteudo da biblioteca".repeat(100);
        let sha: String = Sha256::digest(&lib)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let write_gz = |path: &Path| {
            let mut enc =
                flate2::write::GzEncoder::new(fs::File::create(path).unwrap(), Default::default());
            enc.write_all(&lib).unwrap();
            enc.finish().unwrap();
        };
        let mut fast = Fast {
            url: String::new(),
            sha256: SHA.into(),
            size: lib.len() as u64,
            gz_size: 0,
        };
        let gz = download_path(&dir, 7);
        write_gz(&gz);
        assert!(install(&dir, &gz, 7, "s1", &fast).is_err());
        assert!(hot::read_current(&dir).is_none());

        fast.sha256 = sha;
        write_gz(&gz);
        install(&dir, &gz, 7, "s1", &fast).unwrap();
        assert_eq!(fs::read(dir.join("lib-7.so")).unwrap(), lib);
        assert_eq!(hot::read_current(&dir).unwrap().build, 7);
        assert!(!gz.exists());
        assert!(matches!(
            hot::choose(&dir, "s1", 5),
            hot::Choice::Hot { build: 7, .. }
        ));
    }
}
