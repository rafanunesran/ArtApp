//! Leitor mínimo de `.zip` (só leitura), para instalar pacotes de pincéis baixados da internet:
//! diretório central, arquivos guardados (sem compressão) ou comprimidos com deflate.

/// Maior arquivo descompactado aceito (protege contra "bombas" de zip).
const MAX_ENTRY: usize = 256 << 20;

fn u16le(b: &[u8], at: usize) -> Option<usize> {
    Some(usize::from(u16::from_le_bytes(
        b.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn u32le(b: &[u8], at: usize) -> Option<usize> {
    usize::try_from(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?)).ok()
}

/// Os nomes dos arquivos (não pastas) do zip.
pub fn names(zip: &[u8]) -> Vec<String> {
    entries(zip).into_iter().map(|e| e.name).collect()
}

struct Entry {
    name: String,
    method: usize,
    comp: usize,
    size: usize,
    local: usize,
}

fn entries(zip: &[u8]) -> Vec<Entry> {
    // Fim do diretório central: nos últimos 64 KB + 22 bytes.
    let from = zip.len().saturating_sub(65_557);
    let Some(eocd) = (from..zip.len().saturating_sub(21))
        .rev()
        .find(|&i| zip.get(i..i + 4) == Some(b"PK\x05\x06"))
    else {
        return Vec::new();
    };
    let (Some(count), Some(mut at)) = (u16le(zip, eocd + 10), u32le(zip, eocd + 16)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for _ in 0..count {
        if zip.get(at..at + 4) != Some(b"PK\x01\x02") {
            break;
        }
        let (Some(method), Some(comp), Some(size), Some(n), Some(x), Some(c), Some(local)) = (
            u16le(zip, at + 10),
            u32le(zip, at + 20),
            u32le(zip, at + 24),
            u16le(zip, at + 28),
            u16le(zip, at + 30),
            u16le(zip, at + 32),
            u32le(zip, at + 42),
        ) else {
            break;
        };
        let Some(name) = zip.get(at + 46..at + 46 + n) else {
            break;
        };
        let name = String::from_utf8_lossy(name).into_owned();
        if !name.ends_with('/') {
            out.push(Entry {
                name,
                method,
                comp,
                size,
                local,
            });
        }
        at += 46 + n + x + c;
    }
    out
}

/// Os arquivos do zip cujo nome passa em `want`: (nome, conteúdo). Arquivos que não dá para ler
/// (criptografados ou com outra compressão) ficam de fora.
pub fn extract(zip: &[u8], want: impl Fn(&str) -> bool) -> Vec<(String, Vec<u8>)> {
    entries(zip)
        .into_iter()
        .filter(|e| want(&e.name) && e.size <= MAX_ENTRY)
        .filter_map(|e| {
            let l = e.local;
            if zip.get(l..l + 4) != Some(b"PK\x03\x04") {
                return None;
            }
            let start = l + 30 + u16le(zip, l + 26)? + u16le(zip, l + 28)?;
            let data = zip.get(start..start.checked_add(e.comp)?)?;
            let bytes = match e.method {
                0 => data.to_vec(),
                8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, MAX_ENTRY).ok()?,
                _ => return None,
            };
            Some((e.name, bytes))
        })
        .collect()
}

pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn reads_stored_and_deflated() {
        let zip = include_bytes!("../tests/data/pacote.zip");
        assert!(is_zip(zip));
        let mut n = names(zip);
        n.sort();
        assert_eq!(n, ["leia.txt", "pincéis/teste.sut"]);
        let got = extract(zip, |n| n.ends_with(".txt"));
        assert_eq!(got[0].1, b"ola ola ola ola ola ola");
        let sut = extract(zip, |n| n.ends_with(".sut"));
        assert_eq!(sut[0].1, include_bytes!("../tests/data/teste.sut"));
    }
}
