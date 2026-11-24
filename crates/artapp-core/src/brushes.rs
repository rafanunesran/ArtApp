//! Pincéis prontos. Os de pintura vêm dos presets embutidos do PhotoCraft; os de desenho
//! (lápis, caneta-tinteiro, marcador) são ajustes nossos para trabalho de linha, no estilo Clip
//! Studio, feitos sobre o mesmo motor.

/// Maior tamanho de pincel aceito pelo motor do PhotoCraft.
pub const MAX_SIZE: f32 = photocraft_paint::MAX_BRUSH_SIZE;

use crate::brush_sort::BrushSort;
use photocraft_paint::{
    BrushPreset, BrushSettings, Control, Dynamic, GrayTile, MaskMode, MixerSettings, Pattern,
    PatternStyle, ShapeDynamics, Smoothing, Texture, Transfer, presets,
};

/// Um pincel da paleta: nome mostrado e configurações.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BrushEntry {
    pub name: String,
    pub group: String,
    pub settings: BrushSettings,
    /// A tinta mistura com a da tela (os da categoria Mistura e as cópias deles em Meus pincéis).
    #[serde(default)]
    pub mixes: bool,
    /// Subconjunto dentro de "Meus pincéis" (vazio = fora de pasta), como os conjuntos do
    /// Procreate e as etiquetas do Krita, para separar "Lineart", "Pintura", "Cliente X"…
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
    /// Categoria de um pincel instalado (onde ele aparece além de Meus pincéis), ver
    /// [`crate::brush_sort`].
    #[serde(default, skip_serializing_if = "BrushSort::is_empty")]
    pub sort: BrushSort,
}

impl BrushEntry {
    /// Categoria em que o pincel aparece: a dele, ou, para os de Meus pincéis já classificados,
    /// a que a classificação deu (Esboço, Arte-final…). Decide também a ferramenta pronta.
    pub fn shown_group(&self) -> &str {
        if self.group == CUSTOM_GROUP && !self.sort.category.is_empty() {
            &self.sort.category
        } else {
            &self.group
        }
    }

    /// Etiquetas tiradas dos ajustes, para a busca achar por comportamento ("pressão",
    /// "textura"…) além do nome e da categoria, como as etiquetas do Krita.
    pub fn tags(&self) -> Vec<&'static str> {
        let b = &self.settings;
        let mut t = Vec::new();
        let pressure = |d: &Dynamic| d.control == Control::PenPressure;
        if (b.shape_dynamics.enabled && pressure(&b.shape_dynamics.size))
            || (b.transfer.enabled && pressure(&b.transfer.opacity))
        {
            t.push("pressão");
        }
        if b.texture.enabled || b.dual_brush.enabled || b.noise {
            t.push("textura");
        }
        if b.erase {
            t.push("apagar");
        }
        if self.mixes_paint() {
            t.push("misturar");
        }
        if b.wet_edges {
            t.push("molhado");
        }
        if b.build_up {
            t.push("acumular");
        }
        if b.scattering.enabled {
            t.push("dispersão");
        }
        if !matches!(b.tip, photocraft_paint::TipShape::Round) {
            t.push("carimbo");
        }
        if b.hardness >= 0.95 {
            t.push("duro");
        } else if b.hardness <= 0.2 {
            t.push("macio");
        }
        t
    }

    /// O pincel aparece na busca por `query` (já em minúsculas): nome, categoria ou etiqueta.
    pub fn matches(&self, query: &str) -> bool {
        self.name.to_lowercase().contains(query)
            || self.group.to_lowercase().contains(query)
            || self.sort.category.to_lowercase().contains(query)
            || self.tags().iter().any(|t| t.contains(query))
    }

    /// Pinta misturando com a tela (ver [`MIX_GROUP`]).
    pub fn mixes_paint(&self) -> bool {
        self.mixes || self.group == MIX_GROUP
    }
}

fn pressure_size(minimum: f32) -> ShapeDynamics {
    ShapeDynamics {
        enabled: true,
        size: Dynamic {
            minimum,
            ..Dynamic::controlled(Control::PenPressure)
        },
        ..Default::default()
    }
}

fn ours(name: &str, group: &str, settings: BrushSettings) -> BrushEntry {
    BrushEntry {
        name: name.into(),
        group: group.into(),
        settings,
        mixes: group == MIX_GROUP,
        folder: String::new(),
        sort: BrushSort::default(),
    }
}

/// Categorias da biblioteca de pincéis, na ordem mostrada (como os grupos de sub-ferramenta do
/// Clip Studio e a biblioteca do Procreate). O grupo de cada [`BrushEntry`] é uma delas; o painel
/// esconde as que estão vazias.
///
/// Onde entra um pincel novo: lápis, grafite, giz, carvão, pastel → "Esboço"; linha de contorno,
/// nanquim, caneta G → "Arte-final"; marcador, caneta de ponta porosa → "Marcadores"; ponta chata
/// de letra → "Caligrafia"; redondos, óleo, guache, cerdas → "Pintura"; aquarela e aguadas →
/// "Aquarela"; spray, respingo, pontilhado → "Aerógrafo"; dedo/mistura/borrar → "Mistura";
/// papel, tela, esponja, granulado → "Texturas"; folhas, brilhos, carimbos de forma →
/// "Decoração"; retícula, hachura, linhas de efeito → "Quadrinhos"; borrachas → "Borrachas".
/// Pincéis do usuário e importados ficam em [`CUSTOM_GROUP`].
pub const CATEGORIES: [&str; 13] = [
    "Esboço",
    "Arte-final",
    "Marcadores",
    "Caligrafia",
    "Pintura",
    "Aquarela",
    "Aerógrafo",
    "Mistura",
    "Texturas",
    "Decoração",
    "Quadrinhos",
    "Borrachas",
    CUSTOM_GROUP,
];

/// Nome em português e categoria de um preset embutido do PhotoCraft (pelo nome original).
fn localize(name: &str, group: &str) -> (String, &'static str) {
    let (pt, cat) = match name {
        "Hard Round" => ("Redondo duro", "Pintura"),
        "Soft Round" => ("Redondo macio", "Pintura"),
        "Hard Round Fine" => ("Redondo duro fino", "Arte-final"),
        "Soft Round Large" => ("Redondo macio grande", "Pintura"),
        "Hard Round Pressure Size" => ("Caneta redonda (pressão no tamanho)", "Arte-final"),
        "Soft Round Pressure Size" => ("Redondo macio (pressão no tamanho)", "Pintura"),
        "Hard Round Pressure Opacity" => ("Redondo duro (pressão na opacidade)", "Pintura"),
        "Soft Round Pressure Opacity" => ("Redondo macio (pressão na opacidade)", "Pintura"),
        "Hard Round Pressure Opacity and Flow" => {
            ("Redondo duro (pressão na opacidade e fluxo)", "Pintura")
        }
        "Airbrush Soft" => ("Aerógrafo macio", "Aerógrafo"),
        "Airbrush Pressure Flow" => ("Aerógrafo (pressão no fluxo)", "Aerógrafo"),
        "Calligraphy Flat" => ("Caligrafia chata", "Caligrafia"),
        "Hard Pencil" => ("Lápis duro", "Esboço"),
        "Soft Pencil" => ("Lápis macio", "Esboço"),
        "Chalk" => ("Giz", "Esboço"),
        "Charcoal" => ("Carvão", "Esboço"),
        "Pastel" => ("Pastel", "Esboço"),
        "Wax Crayon" => ("Giz de cera", "Esboço"),
        "Dry Bristle" => ("Cerdas secas", "Pintura"),
        "Rake" => ("Rastelo", "Pintura"),
        "Watercolor Wet Edges" => ("Aquarela bordas molhadas", "Aquarela"),
        "Watercolor Wash" => ("Aquarela lavada", "Aquarela"),
        "Ink Pen" => ("Caneta nanquim", "Arte-final"),
        "Oil Bristle" => ("Óleo (cerdas)", "Pintura"),
        "Gouache Flat" => ("Guache chato", "Pintura"),
        "Ink Wash Sponge" => ("Aguada com esponja", "Aquarela"),
        "Spatter" => ("Respingo", "Aerógrafo"),
        "Stipple" => ("Pontilhado", "Aerógrafo"),
        "Confetti" => ("Confete", "Decoração"),
        "Foliage" => ("Folhagem", "Decoração"),
        "Grass" => ("Grama", "Decoração"),
        "Sparkle" => ("Brilhos", "Decoração"),
        "Sponge Texture" => ("Esponja", "Texturas"),
        "Canvas Texture" => ("Tela de pintura", "Texturas"),
        "Halftone Dots" => ("Retícula", "Quadrinhos"),
        "Dual Grain" => ("Granulado duplo", "Texturas"),
        _ => {
            let cat = match group {
                "Dry Media" => "Esboço",
                "Special Effects" => "Decoração",
                _ => "Pintura",
            };
            return (name.to_string(), cat);
        }
    };
    (pt.to_string(), cat)
}

/// Uma frase sobre para que serve o pincel (os do conjunto padrão; `None` para os outros).
pub fn describe(name: &str) -> Option<&'static str> {
    Some(match name {
        "Caneta G" => "Contorno de quadrinho: linha firme que afina com pouca pressão.",
        "Lápis" => "Esboço solto, com borda irregular e opacidade pela pressão.",
        "Lapiseira" => "Linha fina e clara para estudo e construção.",
        "Lápis de cor" => "Colorir com o grão do papel aparecendo.",
        "Marcador" => "Cor chapada semitransparente para preencher rápido.",
        "Marcador de álcool" => {
            "Ponta larga inclinada; onde o traço passa de novo, até no mesmo traço, a cor escurece."
        }
        "Marca-texto" => "Multiplica sobre o desenho: destaca sem cobrir a linha.",
        "Caneta de feltro" => "Linha média e quase opaca, boa para lettering e rabiscos.",
        "Caneta fina" => "Detalhes e hachuras finas, com estabilização.",
        "Caneta técnica" => "Linha de largura constante, sem pressão.",
        "Pincel de nanquim" => "Traço de pincel com grande variação de largura.",
        "Caneta chanfrada" => "Ponta chata a 45°: grossa num sentido, fina no outro.",
        "Monoline" => "Lettering de largura igual, bem estabilizado.",
        "Aquarela" => "Aguada transparente com borda molhada.",
        "Aerógrafo" => "Sombra e brilho macios; segurar parado acumula tinta.",
        "Óleo que mistura" => "Pinta com a cor escolhida e mistura com a tinta da tela.",
        "Pincel úmido" => "Bem molhado: pega muita cor da tela e funde as áreas.",
        "Esfumar com cor" => "Quase sem tinta própria: espalha e suaviza o que já está pintado.",
        "Hachura" => "Linhas diagonais presas à tela: sombra de quadrinho que não se desalinha.",
        "Hachura cruzada" => "Hachura nas duas diagonais, para sombras mais escuras.",
        "Borracha dura" => "Apaga com borda limpa.",
        "Borracha macia" => "Apaga suave, para clarear áreas.",
        "Borracha de lápis" => "Apaga como borracha de papel, pela pressão.",
        "Borracha de textura" => "Apaga deixando o grão do papel.",
        "Aquarela queimada" => {
            "Aguada com a borda escura onde cruza outra tinta, como pigmento que seca na beirada."
        }
        "Marcador uniforme" => "Camada lisa: passar de novo no mesmo traço não escurece.",
        "Guache intenso" => "Começa leve e cobre de vez se você insistir no mesmo lugar.",
        "Folhas variadas" => "Folhas soltas que alternam formatos e tons, para árvores e arbustos.",
        "Confete de formas" => "Estrelas, gotas e pontinhos coloridos espalhados.",
        _ => return crate::decoration::describe(name),
    })
}

/// Posição da categoria em [`CATEGORIES`] (desconhecidas vão para o fim).
pub fn category_order(group: &str) -> usize {
    CATEGORIES
        .iter()
        .position(|c| *c == group)
        .unwrap_or(CATEGORIES.len())
}

/// Pincéis de linha do ArtApp.
pub fn drawing() -> Vec<BrushEntry> {
    let base = BrushSettings::default();
    vec![
        ours(
            "Caneta G",
            "Arte-final",
            BrushSettings {
                size: 8.0,
                hardness: 1.0,
                spacing: 0.05,
                shape_dynamics: pressure_size(0.05),
                smoothing: Smoothing {
                    amount: 0.3,
                    ..Default::default()
                },
                ..base.clone()
            },
        ),
        ours(
            "Lápis",
            "Esboço",
            BrushSettings {
                size: 4.0,
                hardness: 0.6,
                spacing: 0.08,
                opacity: 0.85,
                shape_dynamics: pressure_size(0.4),
                transfer: Transfer {
                    enabled: true,
                    opacity: Dynamic::controlled(Control::PenPressure),
                    ..Default::default()
                },
                noise: true,
                ..base.clone()
            },
        ),
        ours(
            "Marcador",
            "Marcadores",
            BrushSettings {
                size: 18.0,
                hardness: 0.9,
                spacing: 0.05,
                opacity: 0.6,
                shape_dynamics: ShapeDynamics::default(),
                ..base.clone()
            },
        ),
        ours(
            "Aquarela",
            "Aquarela",
            BrushSettings {
                size: 40.0,
                hardness: 0.0,
                flow: 0.25,
                opacity: 0.8,
                wet_edges: true,
                shape_dynamics: pressure_size(0.3),
                ..base.clone()
            },
        ),
        ours(
            "Aerógrafo",
            "Aerógrafo",
            BrushSettings {
                size: 80.0,
                hardness: 0.0,
                flow: 0.08,
                spacing: 0.1,
                build_up: true,
                ..base
            },
        ),
    ]
}

/// Pincel que apaga com a própria forma (categoria Borrachas). Funciona com a ferramenta Pincel,
/// como no Procreate, onde qualquer pincel pode apagar.
fn eraser(size: f32, hardness: f32, settings: BrushSettings) -> BrushSettings {
    BrushSettings {
        size,
        hardness,
        erase: true,
        ..settings
    }
}

fn smooth(amount: f32) -> Smoothing {
    Smoothing {
        amount,
        ..Default::default()
    }
}

fn pressure_opacity() -> Transfer {
    Transfer {
        enabled: true,
        opacity: Dynamic::controlled(Control::PenPressure),
        ..Default::default()
    }
}

/// Linhas diagonais a cada 8 px (e as da outra diagonal, se `cross`), 2 px de largura.
fn hatch_tile(cross: bool) -> GrayTile {
    GrayTile::from_fn(32, 32, move |x, y| {
        let a = (x + y) % 8 < 2;
        let b = cross && (x + 32 - y) % 8 < 2;
        if a || b { 1.0 } else { 0.0 }
    })
}

fn hatch_brush(cross: bool) -> BrushSettings {
    BrushSettings {
        size: 60.0,
        hardness: 0.8,
        spacing: 0.1,
        texture: Texture {
            enabled: true,
            pattern: Pattern::Tile(hatch_tile(cross)),
            mode: MaskMode::HardMix,
            depth: 1.0,
            ..Default::default()
        },
        ..BrushSettings::default()
    }
}

/// Mais pincéis do conjunto padrão, um por necessidade comum de cada categoria (rodada B2 do
/// plano de pincéis). Todos feitos aqui, sobre o motor do PhotoCraft.
pub fn extra() -> Vec<BrushEntry> {
    let base = BrushSettings::default();
    let paper = Texture {
        enabled: true,
        pattern: Pattern::Procedural {
            style: PatternStyle::Paper,
            size: 128,
            seed: 11,
        },
        scale: 1.0,
        depth: 0.7,
        ..Default::default()
    };
    let mut v = vec![
        // Esboço
        ours(
            "Lapiseira",
            "Esboço",
            BrushSettings {
                size: 2.0,
                hardness: 0.85,
                spacing: 0.08,
                opacity: 0.9,
                shape_dynamics: pressure_size(0.6),
                transfer: pressure_opacity(),
                noise: true,
                ..base.clone()
            },
        ),
        ours(
            "Lápis de cor",
            "Esboço",
            BrushSettings {
                size: 7.0,
                hardness: 0.5,
                spacing: 0.08,
                shape_dynamics: pressure_size(0.5),
                transfer: pressure_opacity(),
                texture: paper.clone(),
                ..base.clone()
            },
        ),
        // Arte-final
        ours(
            "Caneta fina",
            "Arte-final",
            BrushSettings {
                size: 3.0,
                hardness: 1.0,
                spacing: 0.04,
                shape_dynamics: pressure_size(0.2),
                smoothing: smooth(0.4),
                ..base.clone()
            },
        ),
        ours(
            "Caneta técnica",
            "Arte-final",
            BrushSettings {
                size: 4.0,
                hardness: 1.0,
                spacing: 0.04,
                shape_dynamics: ShapeDynamics::default(),
                smoothing: smooth(0.5),
                ..base.clone()
            },
        ),
        ours(
            "Pincel de nanquim",
            "Arte-final",
            BrushSettings {
                size: 16.0,
                hardness: 0.95,
                spacing: 0.04,
                roundness: 0.8,
                angle: 30.0,
                shape_dynamics: pressure_size(0.02),
                smoothing: smooth(0.35),
                ..base.clone()
            },
        ),
        // Marcadores
        // Cada marca pinta direto sobre a anterior (pelo misturador, seco e sem pegar cor), então
        // onde o traço passa de novo, até dentro do mesmo traço, a cor escurece, como no papel.
        BrushEntry {
            mixes: true,
            ..ours(
                "Marcador de álcool",
                "Marcadores",
                BrushSettings {
                    size: 26.0,
                    hardness: 0.85,
                    spacing: 0.2,
                    roundness: 0.6,
                    angle: 30.0,
                    shape_dynamics: ShapeDynamics::default(),
                    mixer: MixerSettings {
                        wet: 0.0,
                        load: 1.0,
                        mix: 0.0,
                        flow: 0.1,
                        ..Default::default()
                    },
                    ..base.clone()
                },
            )
        },
        ours(
            "Marca-texto",
            "Marcadores",
            BrushSettings {
                size: 24.0,
                hardness: 1.0,
                spacing: 0.04,
                opacity: 0.5,
                roundness: 0.35,
                mode: photocraft_color::BlendMode::Multiply,
                shape_dynamics: ShapeDynamics::default(),
                ..base.clone()
            },
        ),
        ours(
            "Caneta de feltro",
            "Marcadores",
            BrushSettings {
                size: 6.0,
                hardness: 0.85,
                spacing: 0.05,
                opacity: 0.95,
                shape_dynamics: pressure_size(0.6),
                ..base.clone()
            },
        ),
        // Caligrafia
        ours(
            "Caneta chanfrada",
            "Caligrafia",
            BrushSettings {
                size: 18.0,
                hardness: 1.0,
                spacing: 0.03,
                roundness: 0.15,
                angle: 45.0,
                shape_dynamics: pressure_size(0.5),
                smoothing: smooth(0.3),
                ..base.clone()
            },
        ),
        ours(
            "Monoline",
            "Caligrafia",
            BrushSettings {
                size: 7.0,
                hardness: 1.0,
                spacing: 0.04,
                shape_dynamics: ShapeDynamics::default(),
                smoothing: smooth(0.6),
                ..base.clone()
            },
        ),
        // Mistura: pintam com a cor escolhida e misturam com a tinta que já está na tela.
        ours(
            "Óleo que mistura",
            MIX_GROUP,
            BrushSettings {
                size: 30.0,
                hardness: 0.7,
                spacing: 0.08,
                shape_dynamics: pressure_size(0.4),
                mixer: MixerSettings {
                    wet: 0.6,
                    load: 0.6,
                    mix: 0.5,
                    flow: 1.0,
                    ..Default::default()
                },
                ..base.clone()
            },
        ),
        ours(
            "Pincel úmido",
            MIX_GROUP,
            BrushSettings {
                size: 40.0,
                hardness: 0.3,
                spacing: 0.08,
                mixer: MixerSettings {
                    wet: 0.9,
                    load: 0.3,
                    mix: 0.8,
                    flow: 0.8,
                    ..Default::default()
                },
                ..base.clone()
            },
        ),
        ours(
            "Esfumar com cor",
            MIX_GROUP,
            BrushSettings {
                size: 36.0,
                hardness: 0.0,
                spacing: 0.08,
                mixer: MixerSettings {
                    wet: 1.0,
                    load: 0.0,
                    mix: 0.95,
                    flow: 0.6,
                    ..Default::default()
                },
                ..base.clone()
            },
        ),
        // Quadrinhos: o padrão fica preso à tela, então traços vizinhos continuam a mesma hachura.
        ours("Hachura", "Quadrinhos", hatch_brush(false)),
        ours("Hachura cruzada", "Quadrinhos", hatch_brush(true)),
        // Borrachas
        ours(
            "Borracha dura",
            "Borrachas",
            eraser(20.0, 1.0, base.clone()),
        ),
        ours(
            "Borracha macia",
            "Borrachas",
            eraser(
                60.0,
                0.0,
                BrushSettings {
                    flow: 0.5,
                    ..base.clone()
                },
            ),
        ),
        ours(
            "Borracha de lápis",
            "Borrachas",
            eraser(
                8.0,
                0.6,
                BrushSettings {
                    noise: true,
                    transfer: pressure_opacity(),
                    shape_dynamics: pressure_size(0.5),
                    ..base.clone()
                },
            ),
        ),
        ours(
            "Borracha de textura",
            "Borrachas",
            eraser(
                30.0,
                0.4,
                BrushSettings {
                    texture: paper,
                    ..base.clone()
                },
            ),
        ),
    ];
    v.extend(engine_brushes(&base));
    v
}

/// Pincéis que usam as mudanças do ArtApp no motor: bordas queimadas, vidrado e pontas múltiplas.
fn engine_brushes(base: &BrushSettings) -> Vec<BrushEntry> {
    use crate::decoration::{loose, scatter};
    use photocraft_paint::{ColorDynamics, Glaze, TipShape};
    let tips = |names: &[&str]| -> (TipShape, Vec<GrayTile>) {
        let mut t: Vec<GrayTile> = names.iter().filter_map(|n| stamp(n)).collect();
        if t.is_empty() {
            return (TipShape::Round, t);
        }
        let first = t.remove(0);
        (TipShape::Sampled(first), t)
    };
    let (leaf, leaves) = tips(&["Folha", "Gota", "Folha"]);
    let (star, confetti) = tips(&["Estrela", "Gota", "Grão"]);
    vec![
        ours(
            "Aquarela queimada",
            "Aquarela",
            BrushSettings {
                size: 40.0,
                hardness: 0.45,
                flow: 0.35,
                opacity: 0.75,
                spacing: 0.08,
                wet_edges: true,
                burnt_edges: 0.8,
                shape_dynamics: pressure_size(0.5),
                ..base.clone()
            },
        ),
        ours(
            "Marcador uniforme",
            "Marcadores",
            BrushSettings {
                size: 18.0,
                hardness: 0.9,
                opacity: 0.6,
                spacing: 0.05,
                glaze: Glaze::Uniform,
                ..base.clone()
            },
        ),
        ours(
            "Guache intenso",
            "Pintura",
            BrushSettings {
                size: 26.0,
                hardness: 0.7,
                flow: 0.25,
                spacing: 0.06,
                glaze: Glaze::Intense,
                shape_dynamics: pressure_size(0.4),
                ..base.clone()
            },
        ),
        ours(
            "Folhas variadas",
            "Decoração",
            BrushSettings {
                size: 30.0,
                spacing: 0.6,
                tip: leaf,
                extra_tips: leaves,
                tip_random: true,
                shape_dynamics: loose(0.5, 1.0),
                scattering: scatter(1.5),
                color_dynamics: ColorDynamics {
                    enabled: true,
                    brightness_jitter: 0.15,
                    per_tip: true,
                    ..Default::default()
                },
                ..base.clone()
            },
        ),
        ours(
            "Confete de formas",
            "Decoração",
            BrushSettings {
                size: 22.0,
                spacing: 0.9,
                tip: star,
                extra_tips: confetti,
                tip_random: true,
                shape_dynamics: loose(0.6, 1.0),
                scattering: scatter(2.5),
                color_dynamics: ColorDynamics {
                    enabled: true,
                    hue_jitter: 0.5,
                    per_tip: true,
                    ..Default::default()
                },
                ..base.clone()
            },
        ),
    ]
}

/// Todos os pincéis: os de desenho e os presets embutidos do PhotoCraft, com nome em português,
/// ordenados por categoria ([`CATEGORIES`]).
pub fn all() -> Vec<BrushEntry> {
    let mut v = drawing();
    v.extend(extra());
    v.extend(crate::decoration::brushes());
    v.extend(presets::builtin().into_iter().map(|p: BrushPreset| {
        let (name, group) = localize(&p.name, &p.group);
        BrushEntry {
            name,
            group: group.into(),
            settings: p.brush,
            mixes: false,
            folder: String::new(),
            sort: BrushSort::default(),
        }
    }));
    // Ordenação estável: dentro da categoria, os nossos vêm antes dos do PhotoCraft.
    v.sort_by_key(|b| category_order(&b.group));
    v
}

/// Categoria dos pincéis que misturam com a tela: com a ferramenta Pincel eles usam os ajustes de
/// `BrushSettings::mixer` (molhado, carga, mistura, fluxo) em vez de só depositar tinta.
pub const MIX_GROUP: &str = "Mistura";

/// Grupo dos pincéis criados pelo usuário.
pub const CUSTOM_GROUP: &str = "Meus pincéis";

/// Pastas usadas em "Meus pincéis", em ordem alfabética.
pub fn folders(brushes: &[BrushEntry]) -> Vec<String> {
    let mut v: Vec<String> = brushes
        .iter()
        .filter(|b| b.group == CUSTOM_GROUP && !b.folder.is_empty())
        .map(|b| b.folder.clone())
        .collect();
    v.sort_by_key(|f| f.to_lowercase());
    v.dedup();
    v
}

/// Lê os pincéis salvos pelo usuário (lista vazia se o arquivo não existe ou não abre).
pub fn load_custom(path: &std::path::Path) -> Vec<BrushEntry> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<BrushEntry>>(&b).ok())
        .unwrap_or_default()
}

/// Grava os pincéis do usuário.
/// Extensão do pacote de pincéis do ArtApp (JSON com os ajustes de cada pincel).
pub const PACK_EXT: &str = "artbrushes";

#[derive(serde::Serialize, serde::Deserialize)]
struct Pack {
    format: String,
    version: u32,
    brushes: Vec<BrushEntry>,
}

const PACK_FORMAT: &str = "artapp-brushes";

/// Pacote com os pincéis de "Meus pincéis", para levar a outro aparelho ou mandar a alguém.
pub fn export_pack(brushes: &[BrushEntry]) -> Result<Vec<u8>, String> {
    let pack = Pack {
        format: PACK_FORMAT.into(),
        version: 1,
        brushes: brushes
            .iter()
            .filter(|b| b.group == CUSTOM_GROUP)
            .cloned()
            .collect(),
    };
    if pack.brushes.is_empty() {
        return Err("não há pincéis em Meus pincéis".into());
    }
    serde_json::to_vec_pretty(&pack).map_err(|e| e.to_string())
}

fn import_pack(bytes: &[u8]) -> Result<Vec<BrushEntry>, String> {
    let pack: Pack =
        serde_json::from_slice(bytes).map_err(|e| format!("pacote de pincéis inválido ({e})"))?;
    if pack.format != PACK_FORMAT {
        return Err("não é um pacote de pincéis do ArtApp".into());
    }
    if pack.brushes.is_empty() {
        return Err("o pacote não tem pincéis".into());
    }
    Ok(pack
        .brushes
        .into_iter()
        .map(|mut b| {
            b.group = CUSTOM_GROUP.into();
            b
        })
        .collect())
}

pub fn save_custom(path: &std::path::Path, brushes: &[BrushEntry]) -> Result<(), String> {
    let custom: Vec<&BrushEntry> = brushes.iter().filter(|b| b.group == CUSTOM_GROUP).collect();
    let json = serde_json::to_vec_pretty(&custom).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brush_pack_round_trips_only_my_brushes() -> Result<(), String> {
        let mut b = drawing();
        b[1].group = CUSTOM_GROUP.into();
        b[1].name = "Meu lápis".into();
        b[1].settings.size = 12.0;
        b[1].mixes = true;
        b[1].folder = "Lineart".into();
        let bytes = export_pack(&b)?;
        let back = import(&format!("x.{PACK_EXT}"), &bytes)?;
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "Meu lápis");
        assert_eq!(back[0].group, CUSTOM_GROUP);
        assert_eq!(back[0].settings.size, 12.0);
        assert!(
            back[0].mixes_paint(),
            "cópia de pincel de Mistura continua misturando"
        );
        assert_eq!(back[0].folder, "Lineart", "a pasta vai junto no pacote");
        assert_eq!(folders(&back), vec!["Lineart".to_string()]);
        assert!(folders(&drawing()).is_empty());
        assert!(export_pack(&drawing()).is_err());
        assert!(import(&format!("x.{PACK_EXT}"), b"{}").is_err());
        Ok(())
    }

    #[test]
    fn custom_brushes_round_trip() {
        let dir = std::env::temp_dir().join(format!("artapp-test-{}", std::process::id()));
        let path = dir.join("brushes.json");
        let mut b = drawing();
        b[0].group = CUSTOM_GROUP.into();
        b[0].name = "Meu".into();
        b[0].settings.size = 33.0;
        save_custom(&path, &b).unwrap_or_default();
        let back = load_custom(&path);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "Meu");
        assert_eq!(back[0].settings.size, 33.0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn hatch_leaves_gaps_between_the_lines() {
        let all = all();
        let ink = |name: &str| {
            let b = all
                .iter()
                .find(|b| b.name == name)
                .map(|b| b.settings.clone());
            let px = preview(&b.unwrap_or_default(), 200, 60);
            // Faixa do meio do traço.
            let mid: Vec<_> = px[60 * 200 / 2 - 100..60 * 200 / 2 + 100].to_vec();
            mid.iter().filter(|p| p[3] > 128).count() as f32 / mid.len() as f32
        };
        let single = ink("Hachura");
        let cross = ink("Hachura cruzada");
        assert!(single > 0.1 && single < 0.6, "hachura cobre {single}");
        assert!(cross > single, "cruzada {cross} <= simples {single}");
    }

    #[test]
    fn search_finds_brushes_by_behavior() {
        let all = all();
        let find = |q: &str| all.iter().filter(|b| b.matches(q)).count();
        assert!(find("apagar") >= 4);
        assert!(find("misturar") >= 3);
        assert!(find("pressão") > find("apagar"));
        assert!(
            all.iter()
                .any(|b| b.name == "Lápis de cor" && b.matches("textura"))
        );
    }

    #[test]
    fn our_brushes_have_a_description() {
        for b in drawing().iter().chain(extra().iter()) {
            assert!(describe(&b.name).is_some(), "{} sem descrição", b.name);
        }
    }

    #[test]
    fn default_set_covers_the_categories_and_names_are_unique() {
        let all = all();
        for c in ["Marcadores", "Caligrafia", "Borrachas"] {
            assert!(all.iter().filter(|b| b.group == c).count() >= 2, "{c}");
        }
        assert!(
            all.iter()
                .filter(|b| b.group == "Borrachas")
                .all(|b| b.settings.erase)
        );
        let mut keys: Vec<_> = all.iter().map(|b| (&b.group, &b.name)).collect();
        let n = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(
            keys.len(),
            n,
            "dois pincéis com o mesmo nome na mesma categoria"
        );
        // Todo pincel deixa marca na prévia (as borrachas apagam, então ficam de fora).
        for b in all.iter().filter(|b| !b.settings.erase) {
            let px = preview(&b.settings, 96, 24);
            assert!(
                px.iter().any(|p| p[3] > 0),
                "{} sem tinta na prévia",
                b.name
            );
        }
    }

    #[test]
    fn has_drawing_and_photocraft_brushes() {
        let all = all();
        assert!(all.iter().any(|b| b.name == "Caneta G"));
        assert!(all.iter().all(|b| CATEGORIES.contains(&b.group.as_str())));
        assert!(
            all.iter()
                .any(|b| b.name == "Caneta nanquim" && b.group == "Arte-final")
        );
        for c in [
            "Esboço",
            "Arte-final",
            "Pintura",
            "Aquarela",
            "Aerógrafo",
            "Texturas",
            "Decoração",
        ] {
            assert!(all.iter().any(|b| b.group == *c), "{c}");
        }
    }
}

/// Amostra de um pincel para a lista: um traço em S de `width × height`, em RGBA 8 bits
/// (branco sobre transparente, para o tema escuro). O tamanho do pincel é limitado à altura da amostra.
pub fn preview(settings: &BrushSettings, width: u32, height: u32) -> Vec<[u8; 4]> {
    use photocraft_color::PixelFormat;
    use photocraft_geom::Rect;
    use photocraft_raster::Surface;

    let (w, h) = (width.max(8), height.max(8));
    let mut brush = settings.clone();
    brush.size = brush.size.clamp(2.0, h as f32 * 0.45);
    brush.color = [1.0, 1.0, 1.0, 1.0];
    brush.erase = false;
    let n = 48;
    let pts: Vec<photocraft_paint::StrokePoint> = (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let x = 8.0 + t * (w as f64 - 16.0);
            let y = h as f64 * 0.5 + (t * std::f64::consts::TAU).sin() * h as f64 * 0.22;
            // Pressão sobe e desce, para mostrar a dinâmica do pincel.
            let p = (t * std::f64::consts::PI).sin() as f32;
            photocraft_paint::StrokePoint {
                time: t * 400.0,
                ..photocraft_paint::StrokePoint::new(x, y, 0.15 + 0.85 * p)
            }
        })
        .collect();
    let mut s = Surface::new(PixelFormat::RGBA8);
    photocraft_paint::render_stroke(&mut s, &brush.bounded_for_render(), &pts, None, false, 1.0);
    let r = Rect::new(0, 0, w as i32, h as i32);
    let mut out = vec![[0u8; 4]; (w * h) as usize];
    s.read_rgba8_into(r, &mut out);
    out
}

#[cfg(test)]
mod preview_tests {
    use super::*;

    #[test]
    fn preview_has_ink() {
        let b = drawing()
            .into_iter()
            .next()
            .map(|b| b.settings)
            .unwrap_or_default();
        let px = preview(&b, 120, 32);
        assert_eq!(px.len(), 120 * 32);
        assert!(px.iter().any(|p| p[3] > 200));
    }
}

/// Maior lado de uma ponta feita de imagem (as maiores são reduzidas).
const MAX_TIP_SIDE: u32 = 512;

/// Importa pincéis de um arquivo: `.abr` (Photoshop) ou uma imagem, que vira a ponta de um
/// pincel (escuro ou opaco = tinta). Os pincéis entram em "Meus pincéis".
///
/// Cada pincel instalado já sai classificado numa categoria (e com isso numa ferramenta pronta:
/// Lápis, Caneta, Spray, Pincel…) pelo nome, pelo nome do arquivo e pastas do pacote e, sem
/// palavra conhecida, pelos ajustes ([`crate::brush_sort`]). Os de um `.artbrushes` que já
/// têm categoria (escolhida à mão ou não) ficam com ela.
pub fn import(file_name: &str, bytes: &[u8]) -> Result<Vec<BrushEntry>, String> {
    let mut list = import_in(file_name, bytes, None)?;
    crate::brush_sort::sort_unsorted(&mut list);
    Ok(list)
}

/// Extensões de pincel que dá para instalar (fora imagens para ponta).
pub const IMPORT_EXTS: [&str; 4] = ["abr", "sut", "zip", PACK_EXT];

/// Importa `file_name`; `folder` é a pasta em Meus pincéis (de um .zip, o nome do pacote). Sem
/// ela, os pincéis de um .abr ou .sut ficam numa pasta com o nome do arquivo.
fn import_in(
    file_name: &str,
    bytes: &[u8],
    folder: Option<&str>,
) -> Result<Vec<BrushEntry>, String> {
    let lower = file_name.to_ascii_lowercase();
    let stem = std::path::Path::new(file_name)
        .file_stem()
        .map_or_else(|| "Pincel".into(), |s| s.to_string_lossy().into_owned());
    let folder = folder.unwrap_or(&stem).to_string();
    // A origem (caminho do arquivo, até dentro do .zip, e pastas/nós do pacote) ajuda a
    // classificar: "Pencils/Soft.abr", "鉛筆"…
    let entry = |name: String, settings: BrushSettings, extra: &str| BrushEntry {
        name,
        group: CUSTOM_GROUP.into(),
        settings,
        mixes: false,
        folder: folder.clone(),
        sort: BrushSort {
            origin: format!("{file_name} {extra}").trim().to_string(),
            ..BrushSort::default()
        },
    };
    if lower.ends_with(&format!(".{PACK_EXT}")) {
        return import_pack(bytes);
    }
    // Pelo conteúdo também: arquivos baixados às vezes chegam com outro nome.
    if lower.ends_with(".zip") || crate::zip_lite::is_zip(bytes) {
        if lower.ends_with(".brushset") || lower.ends_with(".brush") {
            return Err(
                "pincéis do Procreate (.brushset) ainda não dá para instalar; use .abr ou .sut"
                    .into(),
            );
        }
        let files = crate::zip_lite::extract(bytes, |n| {
            let n = n.to_ascii_lowercase();
            // Pastas escondidas do Mac (__MACOSX) só têm lixo.
            !n.contains("__macosx")
                && IMPORT_EXTS
                    .iter()
                    .any(|e| *e != "zip" && n.ends_with(&format!(".{e}")))
        });
        if files.is_empty() {
            return Err("o .zip não tem pincéis (.abr, .sut)".into());
        }
        let mut out = Vec::new();
        let mut errors = Vec::new();
        for (name, data) in files {
            let base = name.rsplit('/').next().unwrap_or(&name).to_string();
            // Com o caminho inteiro: as pastas do .zip ("Lápis/", "Inking/") dizem o tipo.
            match import_in(&name, &data, Some(&stem)) {
                Ok(list) => out.extend(list),
                Err(e) => errors.push(format!("{base}: {e}")),
            }
        }
        if out.is_empty() {
            return Err(errors.join("; "));
        }
        return Ok(out);
    }
    if lower.ends_with(".sut") || bytes.starts_with(b"SQLite format 3\0") {
        return Ok(crate::sut::read_sut(bytes, &stem)?
            .into_iter()
            .map(|b| entry(b.name, b.settings, &b.hint))
            .collect());
    }
    if lower.ends_with(".abr") {
        let imp = crate::abr::read_abr(bytes, &stem)?;
        if imp.presets.is_empty() {
            return Err("o arquivo não tem pincéis".into());
        }
        return Ok(imp
            .presets
            .into_iter()
            .map(|p| {
                let extra = p.folder.join(" ");
                entry(p.name, p.brush, &extra)
            })
            .collect());
    }
    let (w, h, px) = crate::edit::decode_image(bytes)?;
    let tip = tip_from_rgba(w, h, &px).ok_or("a imagem está vazia")?;
    let size = tip.width.max(tip.height).min(120) as f32;
    Ok(vec![BrushEntry {
        name: stem,
        group: CUSTOM_GROUP.into(),
        mixes: false,
        folder: String::new(),
        sort: BrushSort::default(),
        settings: BrushSettings {
            tip: photocraft_paint::TipShape::Sampled(tip),
            size,
            spacing: 0.25,
            ..BrushSettings::default()
        },
    }])
}

/// Ponta tirada de uma imagem (PNG/JPEG), para somar às pontas de um pincel de pontas múltiplas.
pub fn tip_from_image(bytes: &[u8]) -> Result<photocraft_paint::GrayTile, String> {
    let (w, h, px) = crate::edit::decode_image(bytes)?;
    tip_from_rgba(w, h, &px).ok_or_else(|| "a imagem está vazia".to_string())
}

/// Carimbos prontos para pontas múltiplas (desenhados por fórmula, 64 × 64).
pub const STAMPS: [&str; 4] = ["Folha", "Gota", "Estrela", "Grão"];

/// O carimbo `name` de [`STAMPS`].
pub fn stamp(name: &str) -> Option<photocraft_paint::GrayTile> {
    const S: u32 = 64;
    let c = |v: u32| (v as f32 + 0.5) / S as f32 * 2.0 - 1.0;
    let soft = |d: f32| (1.0 - d).clamp(0.0, 0.08) / 0.08;
    let f: Box<dyn Fn(f32, f32) -> f32> = match name {
        // Folha: elipse fina com a nervura do meio mais clara.
        "Folha" => Box::new(move |x, y| {
            let d = (x / 0.35).powi(2) + (y / 0.95).powi(2);
            soft(d.sqrt()) * if x.abs() < 0.04 { 0.55 } else { 1.0 }
        }),
        // Gota: redonda embaixo, pontuda em cima.
        "Gota" => Box::new(move |x, y| {
            let w = 0.6 * ((y + 1.0) / 2.0).clamp(0.0, 1.0).sqrt();
            let d = if y > 0.2 {
                ((x / 0.6).powi(2) + ((y - 0.2) / 0.75).powi(2)).sqrt()
            } else if w > 0.0 {
                (x.abs() / w).max(0.0)
            } else {
                2.0
            };
            soft(d)
        }),
        // Estrela de cinco pontas.
        "Estrela" => Box::new(move |x, y| {
            let a = y.atan2(x);
            let r = (x * x + y * y).sqrt();
            let k = (a * 2.5).cos().abs();
            let edge = 0.42 + 0.53 * k.powf(6.0);
            soft(r / edge)
        }),
        // Grão: pontinhos espalhados (ruído fixo), para texturas de folhagem e areia.
        "Grão" => Box::new(move |x, y| {
            let cell = |v: f32| ((v + 1.0) * 6.0).floor();
            let h = (cell(x) * 12.9898 + cell(y) * 78.233).sin() * 43_758.547;
            let r = (x * x + y * y).sqrt();
            if h.fract().abs() > 0.6 && r < 1.0 {
                1.0
            } else {
                0.0
            }
        }),
        _ => return None,
    };
    Some(photocraft_paint::GrayTile::from_fn(S, S, |x, y| {
        f(c(x), c(y))
    }))
}

/// A ponta em tons de cinza de uma imagem RGBA: tinta = opaco e escuro. Reduzida até
/// [`MAX_TIP_SIDE`]. `None` se não sobra tinta nenhuma.
pub(crate) fn tip_from_rgba(w: u32, h: u32, px: &[u8]) -> Option<photocraft_paint::GrayTile> {
    if w == 0 || h == 0 || px.len() < (w * h * 4) as usize {
        return None;
    }
    let ink = |x: u32, y: u32| -> f32 {
        let i = ((y * w + x) * 4) as usize;
        let p = |k: usize| f32::from(px.get(i + k).copied().unwrap_or(0)) / 255.0;
        let luma = 0.299 * p(0) + 0.587 * p(1) + 0.114 * p(2);
        p(3) * (1.0 - luma)
    };
    let step = w.max(h).div_ceil(MAX_TIP_SIDE).max(1);
    let (tw, th) = (w.div_ceil(step), h.div_ceil(step));
    let tile = photocraft_paint::GrayTile::from_fn(tw, th, |x, y| {
        let mut acc = 0.0;
        let mut n = 0.0;
        for dy in 0..step {
            for dx in 0..step {
                let (sx, sy) = (x * step + dx, y * step + dy);
                if sx < w && sy < h {
                    acc += ink(sx, sy);
                    n += 1.0;
                }
            }
        }
        if n > 0.0 { acc / n } else { 0.0 }
    });
    let any = (0..th).any(|y| (0..tw).any(|x| tile.get(x, y) > 0.02));
    any.then_some(tile)
}

#[cfg(test)]
mod import_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn image_becomes_a_sampled_tip() {
        // Círculo preto em fundo branco, 64×64.
        let mut img = image::RgbaImage::from_pixel(64, 64, image::Rgba([255, 255, 255, 255]));
        for y in 0..64 {
            for x in 0..64 {
                let (dx, dy) = (x as f32 - 32.0, y as f32 - 32.0);
                if dx * dx + dy * dy < 400.0 {
                    img.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
                }
            }
        }
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let v = import("bolinha.png", &png).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name, "bolinha");
        assert_eq!(v[0].group, CUSTOM_GROUP);
        let t = match &v[0].settings.tip {
            photocraft_paint::TipShape::Sampled(t) => t.clone(),
            _ => photocraft_paint::GrayTile::from_fn(1, 1, |_, _| 0.0),
        };
        assert_eq!((t.width, t.height), (64, 64));
        assert!(t.get(32, 32) > 0.9);
        assert!(t.get(2, 2) < 0.05);
        assert!(
            import("vazio.png", &{
                let blank = image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 255, 255, 255]));
                let mut b = Vec::new();
                blank
                    .write_to(&mut std::io::Cursor::new(&mut b), image::ImageFormat::Png)
                    .unwrap();
                b
            })
            .is_err()
        );
        assert!(import("x.abr", b"lixo").is_err());
    }
}
