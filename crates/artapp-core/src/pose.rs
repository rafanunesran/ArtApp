//! Boneco 3D de pose (o "boneco de desenho 3D" do Clip Studio): um manequim articulado, feito
//! aqui mesmo de elipsoides e juntas em bola, que dá para posar parte por parte, olhar de
//! qualquer ângulo e pôr no desenho como camada de referência. A pose fica guardada na camada
//! para editar de novo depois.
//!
//! Cada osso gira em volta da junta dele, no espaço do osso pai, por três ângulos: dobrar (X),
//! girar em volta de si (Y) e abrir para o lado (Z). O boneco olha para +Z, com +Y para cima e
//! a esquerda dele em +X.

use serde::{Deserialize, Serialize};

use crate::model3d::{self, Camera, Mesh, View};

type V3 = [f32; 3];
type M3 = [[f32; 3]; 3];

pub const BONES: usize = 17;

pub const PELVIS: usize = 0;
pub const BELLY: usize = 1;
pub const CHEST: usize = 2;
pub const NECK: usize = 3;
pub const HEAD: usize = 4;
pub const UPPER_ARM_L: usize = 5;
pub const FOREARM_L: usize = 6;
pub const HAND_L: usize = 7;
pub const UPPER_ARM_R: usize = 8;
pub const FOREARM_R: usize = 9;
pub const HAND_R: usize = 10;
pub const THIGH_L: usize = 11;
pub const SHIN_L: usize = 12;
pub const FOOT_L: usize = 13;
pub const THIGH_R: usize = 14;
pub const SHIN_R: usize = 15;
pub const FOOT_R: usize = 16;

/// O nome de cada osso, para o painel.
pub const NAMES: [&str; BONES] = [
    "Quadril",
    "Barriga",
    "Peito",
    "Pescoço",
    "Cabeça",
    "Braço esquerdo",
    "Antebraço esquerdo",
    "Mão esquerda",
    "Braço direito",
    "Antebraço direito",
    "Mão direita",
    "Coxa esquerda",
    "Canela esquerda",
    "Pé esquerdo",
    "Coxa direita",
    "Canela direita",
    "Pé direito",
];

/// O osso pai de cada um (os pais vêm antes dos filhos).
const PARENT: [Option<usize>; BONES] = [
    None,
    Some(PELVIS),
    Some(BELLY),
    Some(CHEST),
    Some(NECK),
    Some(CHEST),
    Some(UPPER_ARM_L),
    Some(FOREARM_L),
    Some(CHEST),
    Some(UPPER_ARM_R),
    Some(FOREARM_R),
    Some(PELVIS),
    Some(THIGH_L),
    Some(SHIN_L),
    Some(PELVIS),
    Some(THIGH_R),
    Some(SHIN_R),
];

/// O osso do outro lado (o do meio é ele mesmo).
const MIRROR: [usize; BONES] = [0, 1, 2, 3, 4, 8, 9, 10, 5, 6, 7, 14, 15, 16, 11, 12, 13];

/// Altura do boneco no mundo (os pés ficam em `FLOOR`).
const HEIGHT: f32 = 1.8;
const FLOOR: f32 = -0.9;

/// Como o boneco vai para o desenho.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Style {
    /// Cinza com luz e sombra.
    #[default]
    Shaded,
    /// Só o contorno das partes, para desenhar por cima.
    Outline,
    /// Sombreado com contorno.
    Both,
}

/// O boneco: proporções, pose, a câmera de onde é visto e o estilo.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Figure {
    /// Altura em cabeças (7 a 8 é adulto; 2 a 4, estilo chibi).
    pub heads: f32,
    /// Grossura do corpo (1 = normal).
    pub build: f32,
    /// Ângulos de cada osso em radianos: dobrar (X), girar (Y), abrir (Z).
    pub pose: Vec<V3>,
    pub camera: Camera,
    /// Desce ou sobe o boneco para a parte mais baixa ficar no chão.
    pub grounded: bool,
    pub style: Style,
}

pub const HEADS: std::ops::RangeInclusive<f32> = 2.0..=9.0;
pub const BUILD: std::ops::RangeInclusive<f32> = 0.6..=1.5;

impl Default for Figure {
    fn default() -> Self {
        Self {
            heads: 7.0,
            build: 1.0,
            pose: presets()[0].1.clone(),
            camera: Camera {
                yaw: 0.45,
                pitch: 0.12,
                dist: 2.7,
            },
            grounded: true,
            style: Style::Shaded,
        }
    }
}

/// As poses que vêm com o app: nome e ângulos.
pub fn presets() -> Vec<(&'static str, Vec<V3>)> {
    let pose = |items: &[(usize, V3)]| {
        let mut p = vec![[0.0; 3]; BONES];
        for (b, a) in items {
            p[*b] = *a;
        }
        p
    };
    vec![
        (
            "Em pé",
            pose(&[
                (UPPER_ARM_L, [0.0, 0.0, 0.12]),
                (UPPER_ARM_R, [0.0, 0.0, -0.12]),
                (FOREARM_L, [-0.15, 0.0, 0.0]),
                (FOREARM_R, [-0.15, 0.0, 0.0]),
            ]),
        ),
        (
            "Pose T",
            pose(&[
                (UPPER_ARM_L, [0.0, 0.0, 1.57]),
                (UPPER_ARM_R, [0.0, 0.0, -1.57]),
            ]),
        ),
        (
            "Andando",
            pose(&[
                (THIGH_L, [-0.45, 0.0, 0.0]),
                (SHIN_L, [0.2, 0.0, 0.0]),
                (THIGH_R, [0.3, 0.0, 0.0]),
                (SHIN_R, [0.4, 0.0, 0.0]),
                (FOOT_R, [-0.2, 0.0, 0.0]),
                (UPPER_ARM_L, [0.35, 0.0, 0.08]),
                (UPPER_ARM_R, [-0.35, 0.0, -0.08]),
                (FOREARM_L, [-0.25, 0.0, 0.0]),
                (FOREARM_R, [-0.45, 0.0, 0.0]),
            ]),
        ),
        (
            "Correndo",
            pose(&[
                (BELLY, [0.15, 0.0, 0.0]),
                (CHEST, [0.1, 0.0, 0.0]),
                (HEAD, [-0.2, 0.0, 0.0]),
                (THIGH_L, [-1.1, 0.0, 0.0]),
                (SHIN_L, [1.2, 0.0, 0.0]),
                (THIGH_R, [0.5, 0.0, 0.0]),
                (SHIN_R, [1.1, 0.0, 0.0]),
                (UPPER_ARM_L, [0.75, 0.0, 0.1]),
                (FOREARM_L, [-1.4, 0.0, 0.0]),
                (UPPER_ARM_R, [-0.85, 0.0, -0.1]),
                (FOREARM_R, [-1.4, 0.0, 0.0]),
            ]),
        ),
        (
            "Sentado",
            pose(&[
                (THIGH_L, [-1.57, 0.0, 0.08]),
                (THIGH_R, [-1.57, 0.0, -0.08]),
                (SHIN_L, [1.57, 0.0, 0.0]),
                (SHIN_R, [1.57, 0.0, 0.0]),
                (UPPER_ARM_L, [-0.3, 0.0, 0.1]),
                (UPPER_ARM_R, [-0.3, 0.0, -0.1]),
                (FOREARM_L, [-0.9, 0.0, 0.0]),
                (FOREARM_R, [-0.9, 0.0, 0.0]),
            ]),
        ),
        (
            "Acenando",
            pose(&[
                (UPPER_ARM_L, [0.0, 0.0, 0.12]),
                (FOREARM_L, [-0.15, 0.0, 0.0]),
                (UPPER_ARM_R, [0.0, 0.0, -1.35]),
                (FOREARM_R, [0.0, 0.0, -1.4]),
                (HEAD, [0.0, 0.0, -0.12]),
                (PELVIS, [0.0, 0.0, 0.03]),
            ]),
        ),
        (
            "Pulando",
            pose(&[
                (BELLY, [-0.12, 0.0, 0.0]),
                (THIGH_L, [-0.9, 0.0, 0.1]),
                (THIGH_R, [-0.3, 0.0, -0.1]),
                (SHIN_L, [1.4, 0.0, 0.0]),
                (SHIN_R, [1.0, 0.0, 0.0]),
                (UPPER_ARM_L, [0.0, 0.0, 2.6]),
                (UPPER_ARM_R, [0.0, 0.0, -2.6]),
            ]),
        ),
        (
            "Agachado",
            pose(&[
                (BELLY, [0.35, 0.0, 0.0]),
                (CHEST, [0.15, 0.0, 0.0]),
                (HEAD, [-0.4, 0.0, 0.0]),
                (THIGH_L, [-2.0, 0.0, 0.15]),
                (THIGH_R, [-2.0, 0.0, -0.15]),
                (SHIN_L, [2.2, 0.0, 0.0]),
                (SHIN_R, [2.2, 0.0, 0.0]),
                (FOOT_L, [-0.2, 0.0, 0.0]),
                (FOOT_R, [-0.2, 0.0, 0.0]),
                (UPPER_ARM_L, [-0.9, 0.0, 0.1]),
                (UPPER_ARM_R, [-0.9, 0.0, -0.1]),
                (FOREARM_L, [-0.5, 0.0, 0.0]),
                (FOREARM_R, [-0.5, 0.0, 0.0]),
            ]),
        ),
    ]
}

// ---------- contas de matriz 3×3 ----------

const ID: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn mul(a: &M3, b: &M3) -> M3 {
    let mut m = [[0.0; 3]; 3];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    m
}

fn apply(m: &M3, v: V3) -> V3 {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

fn transpose(m: &M3) -> M3 {
    [0, 1, 2].map(|i| [m[0][i], m[1][i], m[2][i]])
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Rotação Z·X·Y dos ângulos `[x, y, z]` (o giro em Y acontece primeiro, em volta do osso).
fn euler(a: V3) -> M3 {
    let (sa, ca) = a[0].sin_cos();
    let (sb, cb) = a[1].sin_cos();
    let (sc, cc) = a[2].sin_cos();
    let rx = [[1.0, 0.0, 0.0], [0.0, ca, -sa], [0.0, sa, ca]];
    let ry = [[cb, 0.0, sb], [0.0, 1.0, 0.0], [-sb, 0.0, cb]];
    let rz = [[cc, -sc, 0.0], [sc, cc, 0.0], [0.0, 0.0, 1.0]];
    mul(&rz, &mul(&rx, &ry))
}

/// Os ângulos `[x, y, z]` de uma rotação (o inverso de [`euler`]).
fn to_euler(m: &M3) -> V3 {
    let a = m[2][1].clamp(-1.0, 1.0).asin();
    if a.cos() > 1e-4 {
        [a, (-m[2][0]).atan2(m[2][2]), (-m[0][1]).atan2(m[1][1])]
    } else {
        [a, 0.0, m[1][0].atan2(m[0][0])]
    }
}

/// Rotação de `angle` em volta do eixo `axis` (unitário).
fn axis_angle(axis: V3, angle: f32) -> M3 {
    let l = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2])
        .sqrt()
        .max(1e-9);
    let [x, y, z] = axis.map(|v| v / l);
    let (s, c) = angle.sin_cos();
    let t = 1.0 - c;
    [
        [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
        [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
        [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
    ]
}

// ---------- corpo ----------

/// Uma peça do boneco: elipsoide no espaço do osso, afinando para cima ou para baixo.
struct Part {
    bone: usize,
    ball: bool,
    center: V3,
    half: V3,
    /// Largura multiplicada por `1 + taper·y` (y de −1 embaixo a 1 em cima).
    taper: f32,
}

/// O boneco montado: malha e posição das juntas no mundo.
pub struct Built {
    pub mesh: Mesh,
    pub joints: [V3; BONES],
}

/// Peças por osso: corpo e bola da junta.
const IDS: usize = BONES * 2;

impl Figure {
    pub fn angles(&self, b: usize) -> V3 {
        self.pose.get(b).copied().unwrap_or([0.0; 3])
    }

    pub fn set_angles(&mut self, b: usize, a: V3) {
        if self.pose.len() < BONES {
            self.pose.resize(BONES, [0.0; 3]);
        }
        if b < BONES {
            self.pose[b] = a.map(wrap);
        }
    }

    /// Unidade de cabeça e unidade de corpo (no adulto de 7,5 cabeças as duas são iguais).
    fn units(&self) -> (f32, f32, f32) {
        let heads = self.heads.clamp(*HEADS.start(), *HEADS.end());
        let hu = HEIGHT / heads;
        let bu = (HEIGHT - hu) / 6.5;
        (hu, bu, self.build.clamp(*BUILD.start(), *BUILD.end()))
    }

    /// Onde fica a junta do osso, no espaço do pai (o quadril: no mundo).
    fn offset(&self, b: usize) -> V3 {
        let (_, u, w) = self.units();
        let side = if matches!(
            b,
            UPPER_ARM_R | FOREARM_R | HAND_R | THIGH_R | SHIN_R | FOOT_R
        ) {
            -1.0
        } else {
            1.0
        };
        let v = match b {
            PELVIS => return [0.0, FLOOR + 3.9 * u, 0.0],
            BELLY => [0.0, 0.2, 0.0],
            CHEST => [0.0, 0.95, 0.0],
            NECK => [0.0, 1.15, 0.0],
            HEAD => [0.0, 0.3, 0.0],
            UPPER_ARM_L | UPPER_ARM_R => [side * (0.78 + 0.17 * w), 0.95, 0.0],
            FOREARM_L | FOREARM_R => [0.0, -1.45, 0.0],
            HAND_L | HAND_R => [0.0, -1.2, 0.0],
            THIGH_L | THIGH_R => [side * (0.28 + 0.14 * w), -0.3, 0.0],
            SHIN_L | SHIN_R => [0.0, -1.75, 0.0],
            _ => [0.0, -1.65, 0.0],
        };
        v.map(|x| x * u)
    }

    fn parts(&self) -> Vec<Part> {
        let (h, u, w) = self.units();
        let part = |bone, center: V3, half: V3, taper| Part {
            bone,
            ball: false,
            center: center.map(|x| x * u),
            half: half.map(|x| x * u),
            taper,
        };
        let ball = |bone, r: f32| Part {
            bone,
            ball: true,
            center: [0.0; 3],
            half: [r * u; 3],
            taper: 0.0,
        };
        let mut v = vec![
            part(PELVIS, [0.0, -0.12, 0.0], [0.72 * w, 0.42, 0.48 * w], -0.08),
            part(BELLY, [0.0, 0.5, 0.0], [0.58 * w, 0.6, 0.4 * w], 0.05),
            part(CHEST, [0.0, 0.62, 0.02], [0.82 * w, 0.68, 0.5 * w], 0.15),
            part(NECK, [0.0, 0.18, 0.0], [0.19 * w, 0.3, 0.19 * w], 0.0),
            // A cabeça e o nariz (que mostra para onde o boneco olha) seguem a unidade de cabeça.
            Part {
                bone: HEAD,
                ball: false,
                center: [0.0, 0.5 * h, 0.02 * h],
                half: [0.36 * h, 0.52 * h, 0.44 * h],
                taper: 0.12,
            },
            Part {
                bone: HEAD,
                ball: false,
                center: [0.0, 0.4 * h, 0.44 * h],
                half: [0.07 * h, 0.09 * h, 0.08 * h],
                taper: 0.0,
            },
        ];
        for (ua, fa, hand) in [
            (UPPER_ARM_L, FOREARM_L, HAND_L),
            (UPPER_ARM_R, FOREARM_R, HAND_R),
        ] {
            v.push(ball(ua, 0.21 * w));
            v.push(part(
                ua,
                [0.0, -0.72, 0.0],
                [0.19 * w, 0.82, 0.19 * w],
                0.15,
            ));
            v.push(ball(fa, 0.16 * w));
            v.push(part(
                fa,
                [0.0, -0.6, 0.0],
                [0.155 * w, 0.68, 0.155 * w],
                0.22,
            ));
            v.push(ball(hand, 0.11 * w));
            v.push(part(hand, [0.0, -0.36, 0.0], [0.075, 0.36, 0.17], 0.0));
            // O polegar, para a frente com a palma virada para o corpo.
            v.push(part(hand, [0.0, -0.2, 0.17], [0.06, 0.17, 0.06], 0.0));
        }
        for (thigh, shin, foot) in [(THIGH_L, SHIN_L, FOOT_L), (THIGH_R, SHIN_R, FOOT_R)] {
            v.push(ball(thigh, 0.27 * w));
            v.push(part(
                thigh,
                [0.0, -0.85, 0.0],
                [0.28 * w, 0.98, 0.28 * w],
                0.22,
            ));
            v.push(ball(shin, 0.19 * w));
            v.push(part(
                shin,
                [0.0, -0.8, 0.0],
                [0.19 * w, 0.92, 0.19 * w],
                0.3,
            ));
            v.push(ball(foot, 0.12 * w));
            v.push(part(foot, [0.0, -0.13, 0.25], [0.15, 0.1, 0.42], 0.0));
        }
        v
    }

    /// Rotação no mundo e posição da junta de cada osso (antes de pôr no chão).
    fn rig(&self) -> ([M3; BONES], [V3; BONES]) {
        let mut rots = [ID; BONES];
        let mut joints = [[0.0; 3]; BONES];
        for b in 0..BONES {
            let local = euler(self.angles(b));
            match PARENT[b] {
                None => {
                    rots[b] = local;
                    joints[b] = self.offset(b);
                }
                Some(p) => {
                    rots[b] = mul(&rots[p], &local);
                    joints[b] = add(joints[p], apply(&rots[p], self.offset(b)));
                }
            }
        }
        (rots, joints)
    }

    /// Monta o boneco na pose.
    pub fn build(&self) -> Built {
        const RINGS: usize = 10;
        const SEGS: usize = 14;
        let (rots, mut joints) = self.rig();
        let mut mesh = Mesh {
            uvs: (0..IDS)
                .map(|i| [(i as f32 + 0.5) / IDS as f32, 0.5])
                .collect(),
            ..Mesh::default()
        };
        for p in self.parts() {
            let uv = (p.bone * 2 + usize::from(p.ball)) as u32;
            let base = mesh.positions.len() as u32;
            for r in 0..=RINGS {
                let phi = std::f32::consts::PI * r as f32 / RINGS as f32;
                let (sp, y) = phi.sin_cos();
                let k = 1.0 + p.taper * y;
                for s in 0..SEGS {
                    let th = std::f32::consts::TAU * s as f32 / SEGS as f32;
                    let (st, ct) = th.sin_cos();
                    let local = [
                        p.center[0] + sp * ct * p.half[0] * k,
                        p.center[1] + y * p.half[1],
                        p.center[2] + sp * st * p.half[2] * k,
                    ];
                    mesh.positions
                        .push(add(joints[p.bone], apply(&rots[p.bone], local)));
                }
            }
            let at = |r: usize, s: usize| base + (r * SEGS + s % SEGS) as u32;
            for r in 0..RINGS {
                for s in 0..SEGS {
                    let (a, b, c, d) = (at(r, s), at(r, s + 1), at(r + 1, s), at(r + 1, s + 1));
                    mesh.tris.push([(a, uv), (c, uv), (b, uv)]);
                    mesh.tris.push([(b, uv), (c, uv), (d, uv)]);
                }
            }
        }
        if self.grounded {
            let low = mesh
                .positions
                .iter()
                .map(|p| p[1])
                .fold(f32::INFINITY, f32::min);
            let dy = if low.is_finite() { FLOOR - low } else { 0.0 };
            for p in &mut mesh.positions {
                p[1] += dy;
            }
            for j in &mut joints {
                j[1] += dy;
            }
        }
        mesh.compute_normals();
        Built { mesh, joints }
    }

    /// Gira o osso `b` no mundo, em volta do eixo `axis` (que passa pela junta dele).
    pub fn rotate_world(&mut self, b: usize, axis: V3, angle: f32) {
        if b >= BONES {
            return;
        }
        let (rots, _) = self.rig();
        let world = mul(&axis_angle(axis, angle), &rots[b]);
        let local = match PARENT[b] {
            None => world,
            Some(p) => mul(&transpose(&rots[p]), &world),
        };
        self.set_angles(b, to_euler(&local));
    }

    /// A mesma pose trocando esquerda e direita.
    pub fn mirrored(&self) -> Figure {
        let mut f = self.clone();
        for (b, &other) in MIRROR.iter().enumerate() {
            let [x, y, z] = self.angles(other);
            f.set_angles(b, [x, -y, -z]);
        }
        f
    }

    /// Copia o lado de `from` (esquerdo ou direito) para o outro, espelhado.
    pub fn copy_side(&mut self, left_to_right: bool) {
        let m = self.mirrored();
        let side = if left_to_right {
            [UPPER_ARM_R, FOREARM_R, HAND_R, THIGH_R, SHIN_R, FOOT_R]
        } else {
            [UPPER_ARM_L, FOREARM_L, HAND_L, THIGH_L, SHIN_L, FOOT_L]
        };
        for b in side {
            self.set_angles(b, m.angles(b));
        }
    }

    /// Vista do boneco para o editor, com o osso `sel` em destaque.
    pub fn render_view(&self, built: &Built, sel: Option<usize>, w: u32, h: u32) -> View {
        let tex = palette(sel, false);
        model3d::render_on(
            &built.mesh,
            &tex,
            IDS as u32,
            1,
            &self.camera,
            w,
            h,
            [44, 46, 52, 255],
        )
    }

    /// O boneco desenhado em `w`×`h` com fundo transparente, no estilo dele (RGBA 8 bits).
    pub fn render_layer(&self, w: u32, h: u32) -> Vec<u8> {
        let built = self.build();
        // Desenha maior e reduz, para a borda sair lisa.
        let k = (4096.0 / w.max(h).max(1) as f32).min(2.0);
        let (rw, rh) = (
            ((w as f32 * k).round() as u32).max(1),
            ((h as f32 * k).round() as u32).max(1),
        );
        let tex = palette(None, self.style == Style::Outline);
        let bg = if self.style == Style::Outline {
            [0, 0, 0, 0]
        } else {
            [200, 200, 204, 0]
        };
        let view = model3d::render_on(&built.mesh, &tex, IDS as u32, 1, &self.camera, rw, rh, bg);
        let mut px = view.rgba.clone();
        if self.style == Style::Outline {
            for p in px.as_chunks_mut::<4>().0 {
                p[3] = 0;
            }
        }
        if self.style != Style::Shaded {
            let lw = (w.min(h) as f32 / 450.0).max(1.5) * k;
            outline(&view, lw, &mut px);
        }
        if (rw, rh) == (w, h) {
            return px;
        }
        match image::RgbaImage::from_raw(rw, rh, px) {
            Some(img) => image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle)
                .into_raw(),
            None => vec![0; (w * h * 4) as usize],
        }
    }
}

/// Ângulo de volta para −π..π.
fn wrap(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    if a.is_finite() {
        (a + PI).rem_euclid(TAU) - PI
    } else {
        0.0
    }
}

/// Cores das peças (uma coluna por peça).
fn palette(sel: Option<usize>, plain: bool) -> Vec<u8> {
    (0..IDS)
        .flat_map(|i| {
            let (bone, ball) = (i / 2, i % 2 == 1);
            let c = match (Some(bone) == sel, ball) {
                _ if plain => [235, 235, 238],
                (true, false) => [255, 178, 102],
                (true, true) => [230, 140, 70],
                (false, false) => [226, 226, 230],
                (false, true) => [168, 174, 186],
            };
            [c[0], c[1], c[2], 255]
        })
        .collect()
}

/// O osso no pixel `(x, y)` da vista.
pub fn pick(view: &View, x: f32, y: f32) -> Option<usize> {
    let uv = view.pick(x, y)?;
    Some(((uv[0] * IDS as f32) as usize).min(IDS - 1) / 2)
}

/// Traça linhas de largura `lw` onde muda o osso (ou entre o boneco e o fundo).
fn outline(view: &View, lw: f32, px: &mut [u8]) {
    let (w, h) = (view.width as usize, view.height as usize);
    let id: Vec<i32> = view
        .uv
        .iter()
        .map(|uv| {
            if uv[0].is_nan() {
                -1
            } else {
                (((uv[0] * IDS as f32) as usize).min(IDS - 1) / 2) as i32
            }
        })
        .collect();
    let r = (lw / 2.0).max(0.5);
    let ri = r.ceil() as i64;
    let mut ink = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let here = id[y * w + x];
            let edge = (x + 1 < w && id[y * w + x + 1] != here)
                || (y + 1 < h && id[(y + 1) * w + x] != here);
            if !edge {
                continue;
            }
            for dy in -ri..=ri {
                for dx in -ri..=ri {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let d = ((dx * dx + dy * dy) as f32).sqrt();
                    let a = (r + 0.5 - d).clamp(0.0, 1.0);
                    let i = ny as usize * w + nx as usize;
                    ink[i] = ink[i].max(a);
                }
            }
        }
    }
    for (i, a) in ink.into_iter().enumerate() {
        if a <= 0.0 {
            continue;
        }
        let p = &mut px[i * 4..i * 4 + 4];
        let under = f32::from(p[3]) / 255.0;
        let out = a + under * (1.0 - a);
        for (c, line) in p[..3].iter_mut().zip([40.0f32, 40.0, 48.0]) {
            let mixed = (line * a + f32::from(*c) * under * (1.0 - a)) / out.max(1e-6);
            *c = mixed.round().clamp(0.0, 255.0) as u8;
        }
        p[3] = (out * 255.0).round() as u8;
    }
}

impl crate::Document {
    /// Põe o boneco no desenho: numa camada nova, ou refaz a camada `layer` (que já tinha um
    /// boneco). Devolve a área que mudou.
    pub fn place_figure(&mut self, fig: &Figure, layer: Option<u64>) -> photocraft_geom::Rect {
        use photocraft_color::PixelFormat;
        use photocraft_raster::Surface;
        let (w, h) = (self.width, self.height);
        let px = fig.render_layer(w, h);
        let r = photocraft_geom::Rect::new(0, 0, w as i32, h as i32);
        let mut surface = Surface::from_interleaved(PixelFormat::RGBA8, r, &px);
        surface.prune();
        let at = layer.and_then(|id| self.layers.iter().position(|l| l.id == id));
        let old = match at {
            Some(i) => {
                self.checkpoint();
                self.active = i;
                self.layers[i].surface.content_bounds()
            }
            None => {
                self.add_layer();
                photocraft_geom::Rect::EMPTY
            }
        };
        let Some(l) = self.active_layer_mut() else {
            return photocraft_geom::Rect::EMPTY;
        };
        if at.is_none() {
            l.name = "Boneco 3D".into();
        }
        let new = surface.content_bounds();
        l.surface = surface;
        l.pose = Some(fig.clone());
        old.union(&new).intersect(&r)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn euler_round_trips() {
        for a in [[0.3, -0.7, 1.1], [-1.2, 2.5, -0.4], [0.0, 0.0, 0.0]] {
            let back = to_euler(&euler(a));
            let (m1, m2) = (euler(a), euler(back));
            for i in 0..3 {
                for j in 0..3 {
                    assert!((m1[i][j] - m2[i][j]).abs() < 1e-4, "{a:?} → {back:?}");
                }
            }
        }
    }

    #[test]
    fn stands_on_the_floor_with_the_right_height() {
        let f = Figure {
            pose: vec![[0.0; 3]; BONES],
            ..Figure::default()
        };
        let b = f.build();
        let (lo, hi) = b
            .mesh
            .positions
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), p| (l.min(p[1]), h.max(p[1])));
        assert!((lo - FLOOR).abs() < 1e-4);
        assert!((hi - lo - HEIGHT).abs() < 0.12, "altura {}", hi - lo);
        // A esquerda do boneco fica em +X.
        assert!(b.joints[HAND_L][0] > 0.0 && b.joints[HAND_R][0] < 0.0);
        // Chibi: cabeça bem maior em relação ao corpo.
        let c = Figure {
            heads: 3.0,
            ..f.clone()
        };
        let head = |f: &Figure| f.units().0 / f.units().1;
        assert!(head(&c) > 2.0 * head(&f));
    }

    #[test]
    fn dragging_a_bone_rotates_it_around_its_joint() {
        let mut f = Figure {
            pose: vec![[0.0; 3]; BONES],
            grounded: false,
            ..Figure::default()
        };
        let before = f.build();
        // Gira o braço esquerdo 90° em volta de Z (do mundo): a mão vai para o lado.
        f.rotate_world(UPPER_ARM_L, [0.0, 0.0, 1.0], std::f32::consts::FRAC_PI_2);
        let after = f.build();
        let sh = after.joints[UPPER_ARM_L];
        assert!(
            (sh[0] - before.joints[UPPER_ARM_L][0]).abs() < 1e-5,
            "junta parada"
        );
        let hand = after.joints[HAND_L];
        assert!(hand[0] - sh[0] > 0.5, "mão para fora: {hand:?}");
        assert!((hand[1] - sh[1]).abs() < 0.05);
        // O antebraço segue o braço (filho), o outro braço não muda.
        assert_eq!(after.joints[HAND_R], before.joints[HAND_R]);
        // Espelhar leva o braço levantado para o lado direito.
        let m = f.mirrored().build();
        assert!(m.joints[HAND_R][0] - m.joints[UPPER_ARM_R][0] < -0.5);
    }

    #[test]
    fn renders_picks_and_places_a_layer() {
        let f = Figure::default();
        let b = f.build();
        let v = f.render_view(&b, Some(CHEST), 200, 240);
        // O meio da vista é o corpo do boneco.
        let hit = pick(&v, 100.0, 120.0);
        assert!(hit.is_some());
        assert!(pick(&v, 2.0, 2.0).is_none());

        for style in [Style::Shaded, Style::Outline, Style::Both] {
            let f = Figure {
                style,
                ..Figure::default()
            };
            let mut d = crate::Document::new(300, 400);
            let n = d.layers.len();
            let r = d.place_figure(&f, None);
            assert_eq!(d.layers.len(), n + 1);
            assert!(!r.is_empty(), "{style:?}");
            assert!(d.layers[d.active].pose.is_some());
            // Refazer na mesma camada não cria outra.
            let id = d.layers[d.active].id;
            let g = f.mirrored();
            d.place_figure(&g, Some(id));
            assert_eq!(d.layers.len(), n + 1);
            assert_eq!(d.layers[d.active].pose.as_ref(), Some(&g));
            assert!(d.can_undo());
        }
    }

    #[test]
    fn figure_survives_the_project_file() {
        let mut d = crate::Document::new(64, 64);
        let mut f = Figure::default();
        f.set_angles(THIGH_L, [-1.0, 0.2, 0.1]);
        d.place_figure(&f, None);
        let back = crate::project::load(&crate::project::save(&d.snapshot()).unwrap()).unwrap();
        assert_eq!(back.layers[back.active].pose.as_ref(), Some(&f));
    }
}
