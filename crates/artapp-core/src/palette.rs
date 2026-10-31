//! Paleta a partir de uma foto (as "paletas de câmera e de arquivo" do Procreate): as cores
//! principais da imagem por k-médias, da mais escura para a mais clara.

/// Até `n` cores que resumem `rgba` (RGBA 8 bits). Pixels quase transparentes ficam de fora.
pub fn from_pixels(rgba: &[u8], n: usize) -> Vec<[u8; 3]> {
    let all: Vec<[f32; 3]> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] >= 128)
        .map(|p| [f32::from(p[0]), f32::from(p[1]), f32::from(p[2])])
        .collect();
    if all.is_empty() || n == 0 {
        return Vec::new();
    }
    // Uma amostra espalhada basta (e deixa rápido em fotos grandes).
    let step = (all.len() / 20_000).max(1);
    let samples: Vec<[f32; 3]> = all.iter().step_by(step).copied().collect();
    let d2 = |a: [f32; 3], b: [f32; 3]| {
        (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
    };
    // Começo: a primeira cor e depois sempre a mais longe das já escolhidas.
    let mut centers = vec![samples[0]];
    while centers.len() < n {
        let (far, dist) = samples
            .iter()
            .map(|s| centers.iter().map(|c| d2(*s, *c)).fold(f32::MAX, f32::min))
            .enumerate()
            .fold((0, 0.0f32), |a, (i, d)| if d > a.1 { (i, d) } else { a });
        if dist < 1.0 {
            break; // Menos cores distintas que `n`.
        }
        centers.push(samples[far]);
    }
    for _ in 0..12 {
        let mut sum = vec![[0.0f64; 4]; centers.len()];
        for s in &samples {
            let j = centers
                .iter()
                .enumerate()
                .map(|(j, c)| (j, d2(*s, *c)))
                .fold((0, f32::MAX), |a, b| if b.1 < a.1 { b } else { a })
                .0;
            for c in 0..3 {
                sum[j][c] += f64::from(s[c]);
            }
            sum[j][3] += 1.0;
        }
        for (c, s) in centers.iter_mut().zip(&sum) {
            if s[3] > 0.0 {
                *c = [0, 1, 2].map(|k| (s[k] / s[3]) as f32);
            }
        }
    }
    let mut out: Vec<[u8; 3]> = centers
        .iter()
        .map(|c| c.map(|v| v.round().clamp(0.0, 255.0) as u8))
        .collect();
    out.sort_by_key(|c| 299 * u32::from(c[0]) + 587 * u32::from(c[1]) + 114 * u32::from(c[2]));
    out.dedup();
    out
}

/// Lê cores de um arquivo de amostras: ASE (Adobe Swatch Exchange) ou ACO (amostras do
/// Photoshop). Formatos públicos; só RGB, cinza e CMYK (convertido de forma simples).
pub fn read_swatches(bytes: &[u8]) -> Result<Vec<[u8; 3]>, String> {
    let colors = if bytes.starts_with(b"ASEF") {
        read_ase(bytes)
    } else {
        read_aco(bytes)
    }?;
    if colors.is_empty() {
        return Err("o arquivo não tem cores RGB, cinza ou CMYK".into());
    }
    Ok(colors)
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.b.len());
        let Some(end) = end else {
            return Err("arquivo cortado".into());
        };
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16, String> {
        let s = self.take(2)?;
        Ok(u16::from_be_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Result<u32, String> {
        let s = self.take(4)?;
        Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_bits(self.u32()?))
    }
}

fn unit(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn cmyk([c, m, y, k]: [f32; 4]) -> [u8; 3] {
    [c, m, y].map(|v| unit((1.0 - v) * (1.0 - k)))
}

fn read_ase(bytes: &[u8]) -> Result<Vec<[u8; 3]>, String> {
    let mut r = Reader { b: bytes, at: 4 };
    r.take(4)?; // versão
    let blocks = r.u32()?;
    let mut out = Vec::new();
    for _ in 0..blocks.min(100_000) {
        let kind = r.u16()?;
        let len = r.u32()? as usize;
        let body = r.take(len)?;
        if kind != 0x0001 {
            continue; // começo ou fim de grupo
        }
        let mut c = Reader { b: body, at: 0 };
        let name_len = c.u16()? as usize;
        c.take(name_len * 2)?;
        let model = c.take(4)?.to_vec();
        let color = match &model[..] {
            b"RGB " => Some([c.f32()?, c.f32()?, c.f32()?].map(unit)),
            b"Gray" => {
                let g = unit(c.f32()?);
                Some([g, g, g])
            }
            b"CMYK" => Some(cmyk([c.f32()?, c.f32()?, c.f32()?, c.f32()?])),
            _ => None,
        };
        out.extend(color);
    }
    Ok(out)
}

fn read_aco(bytes: &[u8]) -> Result<Vec<[u8; 3]>, String> {
    let mut r = Reader { b: bytes, at: 0 };
    let version = r.u16()?;
    if version != 1 && version != 2 {
        return Err("não é um arquivo ASE nem ACO".into());
    }
    let count = r.u16()?;
    let mut out = Vec::new();
    for _ in 0..count {
        let space = r.u16()?;
        let v = [r.u16()?, r.u16()?, r.u16()?, r.u16()?];
        let f = |x: u16| f32::from(x) / 65535.0;
        match space {
            0 => out.push([v[0], v[1], v[2]].map(|x| (x >> 8) as u8)),
            2 => out.push(cmyk([
                1.0 - f(v[0]),
                1.0 - f(v[1]),
                1.0 - f(v[2]),
                1.0 - f(v[3]),
            ])),
            8 => {
                let g = unit(1.0 - f32::from(v[0]) / 10000.0);
                out.push([g, g, g]);
            }
            _ => {}
        }
        if version == 2 {
            let n = r.u32()? as usize;
            r.take(n * 2)?;
        }
    }
    Ok(out)
}

fn utf16_name(name: &str) -> Vec<u16> {
    name.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Grava as cores como ASE, num grupo chamado `name`.
pub fn write_ase(name: &str, colors: &[[u8; 3]]) -> Vec<u8> {
    let mut b = b"ASEF".to_vec();
    b.extend_from_slice(&[0, 1, 0, 0]);
    b.extend_from_slice(&(colors.len() as u32 + 2).to_be_bytes());
    let group = utf16_name(name);
    b.extend_from_slice(&0xC001u16.to_be_bytes());
    b.extend_from_slice(&(2 + group.len() as u32 * 2).to_be_bytes());
    b.extend_from_slice(&(group.len() as u16).to_be_bytes());
    group
        .iter()
        .for_each(|u| b.extend_from_slice(&u.to_be_bytes()));
    for c in colors {
        let n = utf16_name(&format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]));
        b.extend_from_slice(&0x0001u16.to_be_bytes());
        b.extend_from_slice(&(2 + n.len() as u32 * 2 + 4 + 12 + 2).to_be_bytes());
        b.extend_from_slice(&(n.len() as u16).to_be_bytes());
        n.iter().for_each(|u| b.extend_from_slice(&u.to_be_bytes()));
        b.extend_from_slice(b"RGB ");
        for v in c {
            b.extend_from_slice(&(f32::from(*v) / 255.0).to_be_bytes());
        }
        b.extend_from_slice(&2u16.to_be_bytes());
    }
    b.extend_from_slice(&0xC002u16.to_be_bytes());
    b.extend_from_slice(&0u32.to_be_bytes());
    b
}

/// Grava as cores como ACO (versão 1 seguida da 2, com nomes), como o Photoshop faz.
pub fn write_aco(colors: &[[u8; 3]]) -> Vec<u8> {
    let mut b = Vec::new();
    for version in [1u16, 2] {
        b.extend_from_slice(&version.to_be_bytes());
        b.extend_from_slice(&(colors.len() as u16).to_be_bytes());
        for c in colors {
            b.extend_from_slice(&0u16.to_be_bytes());
            for v in c {
                b.extend_from_slice(&(u16::from(*v) * 257).to_be_bytes());
            }
            b.extend_from_slice(&0u16.to_be_bytes());
            if version == 2 {
                let n = utf16_name(&format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]));
                b.extend_from_slice(&(n.len() as u32).to_be_bytes());
                n.iter().for_each(|u| b.extend_from_slice(&u.to_be_bytes()));
            }
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_main_colors() {
        let mut px = Vec::new();
        for i in 0..300 {
            let c = match i % 3 {
                0 => [250, 10, 10, 255],
                1 => [10, 240, 20, 255],
                _ => [5, 5, 200, 255],
            };
            px.extend_from_slice(&c);
        }
        px.extend_from_slice(&[0, 0, 0, 0]);
        let p = from_pixels(&px, 3);
        assert_eq!(p.len(), 3);
        assert!(
            p.contains(&[250, 10, 10]) && p.contains(&[10, 240, 20]) && p.contains(&[5, 5, 200])
        );
        assert_eq!(from_pixels(&px, 8).len(), 3);
    }

    #[test]
    fn ase_and_aco_round_trip() {
        let colors = [[255, 0, 0], [12, 200, 99], [0, 0, 0]];
        assert_eq!(
            read_swatches(&write_ase("Teste", &colors)).ok(),
            Some(colors.to_vec())
        );
        assert_eq!(
            read_swatches(&write_aco(&colors)).ok(),
            Some(colors.to_vec())
        );
        assert!(read_swatches(b"lixo").is_err());
        assert!(read_swatches(&[0, 1, 0, 5]).is_err(), "cortado");
    }
}
