//! Pintura 3D: um modelo OBJ com coordenadas de textura (UV). O desenho do documento é a
//! textura; a vista 3D é desenhada por um rasterizador simples na CPU, que também diz, para
//! cada pixel da vista, qual ponto da textura está ali (para pintar direto no modelo).

/// Uma malha em triângulos, já centrada e com raio 1.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Cada triângulo: índices de posição e de UV dos três cantos.
    pub tris: Vec<[(u32, u32); 3]>,
    /// Normal de cada posição (média das faces em volta), para o sombreado suave.
    pub normals: Vec<[f32; 3]>,
    /// Arquivo de textura citado no `.mtl` (`map_Kd`), se houver.
    pub texture_hint: Option<String>,
    pub mtllib: Option<String>,
}

/// Lê um OBJ (texto). Faces com mais de 3 cantos viram leque de triângulos; cantos sem UV
/// ficam com UV (0, 0). Erro se não houver triângulos ou nenhuma UV.
pub fn parse_obj(text: &str) -> Result<Mesh, String> {
    let mut m = Mesh::default();
    let num = |s: Option<&str>| s.and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
    for line in text.lines() {
        let line = line.trim();
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => m
                .positions
                .push([num(it.next()), num(it.next()), num(it.next())]),
            Some("vt") => m.uvs.push([num(it.next()), num(it.next())]),
            Some("mtllib") => m.mtllib = Some(line[6..].trim().to_string()),
            Some("f") => {
                let corners: Vec<(u32, u32)> = it
                    .filter_map(|c| {
                        let mut p = c.split('/');
                        let vi = resolve(p.next()?, m.positions.len())?;
                        let ti = p
                            .next()
                            .filter(|s| !s.is_empty())
                            .and_then(|s| resolve(s, m.uvs.len()))
                            .unwrap_or(u32::MAX);
                        Some((vi, ti))
                    })
                    .collect();
                for k in 1..corners.len().saturating_sub(1) {
                    m.tris.push([corners[0], corners[k], corners[k + 1]]);
                }
            }
            _ => {}
        }
    }
    if m.tris.is_empty() {
        return Err("o arquivo não tem faces".into());
    }
    if m.uvs.is_empty() {
        return Err("o modelo não tem coordenadas de textura (UV) para pintar".into());
    }
    // Cantos sem UV apontam para uma UV extra (0, 0).
    let none = m.uvs.len() as u32;
    let mut used_none = false;
    for t in &mut m.tris {
        for c in t.iter_mut() {
            if c.1 == u32::MAX || c.1 as usize >= m.uvs.len() {
                c.1 = none;
                used_none = true;
            }
        }
    }
    if used_none {
        m.uvs.push([0.0, 0.0]);
    }
    m.normalize();
    m.compute_normals();
    Ok(m)
}

/// Índice do OBJ (começa em 1; negativo conta do fim) para índice de vetor.
fn resolve(s: &str, len: usize) -> Option<u32> {
    let i: i64 = s.parse().ok()?;
    let idx = if i < 0 { len as i64 + i } else { i - 1 };
    (0..len as i64).contains(&idx).then_some(idx as u32)
}

/// Lê `map_Kd` de um `.mtl`.
pub fn mtl_texture(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let l = l.trim();
        l.strip_prefix("map_Kd")
            .map(|r| r.split_whitespace().last().unwrap_or_default().to_string())
            .filter(|s| !s.is_empty())
    })
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ]
}

impl Mesh {
    fn compute_normals(&mut self) {
        let mut n = vec![[0.0f32; 3]; self.positions.len()];
        for t in &self.tris {
            let p = t.map(|c| self.positions[c.0 as usize]);
            let f = face_normal(p[0], p[1], p[2]);
            for c in t {
                for k in 0..3 {
                    n[c.0 as usize][k] += f[k];
                }
            }
        }
        for v in &mut n {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
            *v = v.map(|x| x / l);
        }
        self.normals = n;
    }

    /// Centra na origem e escala para caber numa esfera de raio 1.
    fn normalize(&mut self) {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &self.positions {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let c = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0);
        let r = self
            .positions
            .iter()
            .map(|p| ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt())
            .fold(0.0f32, f32::max)
            .max(1e-6);
        for p in &mut self.positions {
            *p = [0, 1, 2].map(|k| (p[k] - c[k]) / r);
        }
    }
}

/// Câmera em órbita em volta do modelo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Giro em volta do eixo vertical e inclinação, em radianos.
    pub yaw: f32,
    pub pitch: f32,
    /// Distância ao centro (o modelo tem raio 1).
    pub dist: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 0.6,
            pitch: 0.35,
            dist: 3.2,
        }
    }
}

impl Camera {
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw += dx;
        self.pitch = (self.pitch + dy).clamp(-1.5, 1.5);
    }

    pub fn zoom(&mut self, factor: f32) {
        self.dist = (self.dist / factor.max(0.01)).clamp(1.3, 12.0);
    }

    /// Ponto do mundo na vista: (x, y) em pixels e profundidade (maior = mais longe).
    fn project(&self, p: [f32; 3], w: f32, h: f32) -> Option<[f32; 3]> {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        // Gira o mundo: primeiro em volta de Y (yaw), depois de X (pitch).
        let x1 = cy * p[0] + sy * p[2];
        let z1 = -sy * p[0] + cy * p[2];
        let y2 = cp * p[1] - sp * z1;
        let z2 = sp * p[1] + cp * z1;
        let depth = self.dist - z2;
        if depth < 0.05 {
            return None;
        }
        let f = 0.5 * h.min(w) / 0.45; // campo de visão ~ 50°
        Some([w / 2.0 + x1 * f / depth, h / 2.0 - y2 * f / depth, depth])
    }

    fn light_dir(&self) -> [f32; 3] {
        // Luz vinda de perto da câmera, um pouco de cima.
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = (self.pitch + 0.4).sin_cos();
        [sy * cp, sp, cy * cp]
    }
}

/// A vista desenhada: cores e, por pixel, a UV que está ali (NaN onde não há modelo).
pub struct View {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub uv: Vec<[f32; 2]>,
}

impl View {
    /// A UV no pixel (x, y) da vista, se o modelo está ali.
    pub fn pick(&self, x: f32, y: f32) -> Option<[f32; 2]> {
        let (xi, yi) = (x.floor() as i64, y.floor() as i64);
        if xi < 0 || yi < 0 || xi >= i64::from(self.width) || yi >= i64::from(self.height) {
            return None;
        }
        let uv = self.uv[(yi as u32 * self.width + xi as u32) as usize];
        (!uv[0].is_nan()).then_some(uv)
    }
}

/// UV (OBJ: v para cima) para pixel da textura `tw`×`th`. A textura repete fora de 0..1.
pub fn uv_to_texel(uv: [f32; 2], tw: u32, th: u32) -> (f64, f64) {
    let u = uv[0] - uv[0].floor();
    let v = uv[1] - uv[1].floor();
    (
        f64::from(u) * f64::from(tw),
        f64::from(1.0 - v) * f64::from(th),
    )
}

/// O desenho como textura RGBA 8 bits, reduzido para lado maior até `max_side`.
pub fn texture_of(doc: &crate::Document, max_side: u32) -> (u32, u32, Vec<u8>) {
    let (_, px) = crate::composite::composite_rgba8(doc, doc.bounds(), true);
    let (w, h) = (doc.width, doc.height);
    let raw = px.as_flattened().to_vec();
    if w.max(h) <= max_side {
        return (w, h, raw);
    }
    let s = f64::from(max_side) / f64::from(w.max(h));
    let (tw, th) = (
        ((f64::from(w) * s).round() as u32).max(1),
        ((f64::from(h) * s).round() as u32).max(1),
    );
    match image::RgbaImage::from_raw(w, h, raw) {
        Some(img) => (
            tw,
            th,
            image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle).into_raw(),
        ),
        None => (0, 0, Vec::new()),
    }
}

/// Desenha o modelo com a textura `tex` (RGBA 8 bits, `tw`×`th`) numa vista `w`×`h`.
pub fn render(mesh: &Mesh, tex: &[u8], tw: u32, th: u32, cam: &Camera, w: u32, h: u32) -> View {
    let n = (w * h) as usize;
    let bg = [38u8, 40, 46, 255];
    let mut rgba: Vec<u8> = bg.iter().copied().cycle().take(n * 4).collect();
    let mut uvbuf = vec![[f32::NAN; 2]; n];
    let mut zbuf = vec![f32::INFINITY; n];
    let (wf, hf) = (w as f32, h as f32);
    let screen: Vec<Option<[f32; 3]>> = mesh
        .positions
        .iter()
        .map(|p| cam.project(*p, wf, hf))
        .collect();
    let light = cam.light_dir();
    let texel = |uv: [f32; 2]| -> [u8; 4] {
        if tw == 0 || th == 0 || tex.len() < (tw * th * 4) as usize {
            return [200, 200, 200, 255];
        }
        let (x, y) = uv_to_texel(uv, tw, th);
        let (x, y) = ((x as u32).min(tw - 1), (y as u32).min(th - 1));
        let i = ((y * tw + x) * 4) as usize;
        [tex[i], tex[i + 1], tex[i + 2], tex[i + 3]]
    };
    for t in &mesh.tris {
        let (Some(a), Some(b), Some(c)) = (
            screen[t[0].0 as usize],
            screen[t[1].0 as usize],
            screen[t[2].0 as usize],
        ) else {
            continue;
        };
        // Sombreado suave: luz em cada canto pela normal dele (os dois lados contam).
        let lit = |c: (u32, u32)| {
            let n = mesh
                .normals
                .get(c.0 as usize)
                .copied()
                .unwrap_or([0.0, 0.0, 1.0]);
            0.45 + 0.55 * (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).abs()
        };
        let shades = [lit(t[0]), lit(t[1]), lit(t[2])];
        let uvs = [
            mesh.uvs[t[0].1 as usize],
            mesh.uvs[t[1].1 as usize],
            mesh.uvs[t[2].1 as usize],
        ];
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        if area.abs() < 1e-9 {
            continue;
        }
        let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.0) as u32;
        let x1 = (a[0].max(b[0]).max(c[0]).ceil() as i64).clamp(0, i64::from(w) - 1) as u32;
        let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.0) as u32;
        let y1 = (a[1].max(b[1]).max(c[1]).ceil() as i64).clamp(0, i64::from(h) - 1) as u32;
        if x0 > x1 || y0 > y1 {
            continue;
        }
        // Interpolação correta em perspectiva: 1/z e uv/z.
        let iz = [1.0 / a[2], 1.0 / b[2], 1.0 / c[2]];
        for y in y0..=y1 {
            let py = y as f32 + 0.5;
            for x in x0..=x1 {
                let px = x as f32 + 0.5;
                let w0 = ((b[0] - px) * (c[1] - py) - (c[0] - px) * (b[1] - py)) / area;
                let w1 = ((c[0] - px) * (a[1] - py) - (a[0] - px) * (c[1] - py)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let inv = w0 * iz[0] + w1 * iz[1] + w2 * iz[2];
                let z = 1.0 / inv;
                let i = (y * w + x) as usize;
                if z >= zbuf[i] {
                    continue;
                }
                zbuf[i] = z;
                let uv = [
                    (w0 * uvs[0][0] * iz[0] + w1 * uvs[1][0] * iz[1] + w2 * uvs[2][0] * iz[2]) * z,
                    (w0 * uvs[0][1] * iz[0] + w1 * uvs[1][1] * iz[1] + w2 * uvs[2][1] * iz[2]) * z,
                ];
                uvbuf[i] = uv;
                let t = texel(uv);
                let shade = w0 * shades[0] + w1 * shades[1] + w2 * shades[2];
                let al = f32::from(t[3]) / 255.0;
                for k in 0..3 {
                    // Textura transparente mostra um cinza claro (o "material" do modelo).
                    let base = f32::from(t[k]) * al + 210.0 * (1.0 - al);
                    rgba[i * 4 + k] = (base * shade).clamp(0.0, 255.0) as u8;
                }
                rgba[i * 4 + 3] = 255;
            }
        }
    }
    View {
        width: w,
        height: h,
        rgba,
        uv: uvbuf,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const QUAD: &str = "mtllib q.mtl\nv -1 -1 0\nv 1 -1 0\nv 1 1 0\nv -1 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3 4/4\n";

    #[test]
    fn parses_and_triangulates() {
        let m = parse_obj(QUAD).unwrap();
        assert_eq!(m.tris.len(), 2);
        assert_eq!(m.uvs.len(), 4);
        assert_eq!(m.mtllib.as_deref(), Some("q.mtl"));
        assert!(parse_obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").is_err());
        assert_eq!(
            mtl_texture("newmtl a\nmap_Kd -bm 1 tex/pele.png\n").as_deref(),
            Some("tex/pele.png")
        );
        // Índices negativos.
        let m = parse_obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nf -3/-1 -2/-1 -1/-1\n").unwrap();
        assert_eq!(m.tris.len(), 1);
    }

    #[test]
    fn model_is_saved_in_the_project() {
        let mut d = crate::Document::new(32, 32);
        d.model = Some(std::sync::Arc::new(QUAD.to_string()));
        let back = crate::project::load(&crate::project::save(&d.snapshot()).unwrap()).unwrap();
        assert_eq!(back.model.as_deref().map(String::as_str), Some(QUAD));
    }

    #[test]
    fn renders_and_picks_texture_points() {
        let m = parse_obj(QUAD).unwrap();
        // Textura 2×2: vermelho em cima à esquerda, resto azul.
        let mut tex = vec![0u8; 16];
        for (i, px) in tex.chunks_mut(4).enumerate() {
            px.copy_from_slice(if i == 0 {
                &[255, 0, 0, 255]
            } else {
                &[0, 0, 255, 255]
            });
        }
        let cam = Camera {
            yaw: 0.0,
            pitch: 0.0,
            dist: 3.0,
        };
        let v = render(&m, &tex, 2, 2, &cam, 200, 200);
        // O centro da vista é o centro do quadrado (UV 0.5, 0.5).
        let uv = v.pick(100.0, 100.0).unwrap();
        assert!(
            (uv[0] - 0.5).abs() < 0.02 && (uv[1] - 0.5).abs() < 0.02,
            "{uv:?}"
        );
        // Em cima à esquerda da vista: UV perto de (0, 1) → texel (0, 0), vermelho.
        let uv = v.pick(70.0, 70.0).unwrap();
        let (tx, ty) = uv_to_texel(uv, 2, 2);
        assert!(tx < 1.0 && ty < 1.0, "{tx} {ty}");
        let i = ((70 * 200 + 70) * 4) as usize;
        assert!(v.rgba[i] > 100 && v.rgba[i + 2] < 50);
        // Fora do modelo.
        assert!(v.pick(2.0, 2.0).is_none());
    }
}
