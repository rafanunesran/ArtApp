//! Classificação automática dos pincéis instalados (.sut, .abr, .zip, .artbrushes) entre as
//! categorias da biblioteca ([`crate::brushes::CATEGORIES`]), e com isso entre as ferramentas
//! prontas do trilho (Lápis, Caneta, Spray, Pincel, Texturas e especiais).
//!
//! Os pincéis instalados ficam sempre em "Meus pincéis" (é o que é gravado), mas cada um ganha
//! uma categoria ([`BrushSort::category`]) que diz onde mais ele aparece. A categoria sai de:
//!
//! 1. palavras do nome do pincel (peso dobrado) e de onde ele veio: nome do arquivo baixado,
//!    pastas do `.abr`, caminho dentro do `.zip`, nomes dos nós do `.sut` — em português, inglês e
//!    japonês (muitos `.sut` do Clip vêm com nomes em japonês);
//! 2. sem nenhuma palavra conhecida, dos ajustes: apagar, mistura, borda molhada, espalhamento,
//!    aerógrafo, textura, dureza e tamanho.
//!
//! Categoria escolhida à mão ([`BrushSort::by_hand`]) nunca é trocada por aqui.

use photocraft_paint::BrushSettings;

use crate::brushes::{BrushEntry, CUSTOM_GROUP};

/// Onde um pincel de "Meus pincéis" aparece além dela.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BrushSort {
    /// Categoria da biblioteca (Esboço, Arte-final…); vazia = ainda não classificado (só em Meus
    /// pincéis, na ferramenta Texturas e especiais).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    /// A categoria foi escolhida pelo usuário: "Organizar automaticamente" não mexe.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_hand: bool,
    /// De onde veio (arquivo, pastas do pacote), guardado para classificar de novo depois.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origin: String,
}

impl BrushSort {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Categorias que dá para escolher para um pincel instalado (todas menos "Meus pincéis").
pub fn choices() -> impl Iterator<Item = &'static str> {
    crate::brushes::CATEGORIES
        .iter()
        .copied()
        .filter(|c| *c != CUSTOM_GROUP)
}

/// Uma regra: categoria, peso de cada palavra e as palavras.
///
/// Palavras: com `*` no fim valem como começo de palavra ("pencil*" pega "pencils",
/// "pencilbox"); com espaço, uma sequência de palavras; com caractere não ASCII (japonês), um
/// trecho do texto; as outras, a palavra inteira (com ou sem "s"/"es" no fim).
struct Rule {
    category: &'static str,
    weight: u32,
    words: &'static [&'static str],
}

/// Ordem = desempate (a primeira ganha).
const RULES: &[Rule] = &[
    Rule {
        category: "Borrachas",
        weight: 6,
        words: &[
            "eraser*",
            "erase",
            "borracha*",
            "apagador*",
            "apagar",
            "消しゴム",
            "消し",
        ],
    },
    Rule {
        category: "Mistura",
        weight: 5,
        words: &[
            "blend*",
            "smudg*",
            "blur*",
            "borrar",
            "esfum*",
            "mix",
            "mixer*",
            "mixing",
            "mistur*",
            "smear*",
            "finger",
            "dedo",
            "fundir",
            "色混ぜ",
            "ぼかし",
            "指先",
            "混色",
        ],
    },
    Rule {
        category: "Aquarela",
        weight: 4,
        words: &[
            "watercolor*",
            "watercolour*",
            "aquarel*",
            "wash",
            "aguada*",
            "wet",
            "molhad*",
            "sumi",
            "水彩",
            "にじみ",
            "滲み",
        ],
    },
    Rule {
        category: "Aerógrafo",
        weight: 4,
        words: &[
            "airbrush*",
            "aerograf*",
            "spray*",
            "splatter*",
            "spatter*",
            "respingo*",
            "salpic*",
            "stipple*",
            "stippling",
            "pontilh*",
            "エアブラシ",
            "スプレー",
            "飛沫",
            "噴霧",
        ],
    },
    Rule {
        category: "Quadrinhos",
        weight: 4,
        words: &[
            "halftone*",
            "screentone*",
            "tone",
            "retic*",
            "hatch*",
            "crosshatch*",
            "hachur*",
            "manga",
            "comic*",
            "quadrinho*",
            "hq",
            "speedline*",
            "speed line",
            "focus line",
            "linhas de efeito",
            "トーン",
            "集中線",
            "流線",
            "網点",
            "カケアミ",
        ],
    },
    Rule {
        category: "Caligrafia",
        weight: 4,
        words: &[
            "calligraph*",
            "caligraf*",
            "lettering",
            "letter",
            "letra*",
            "chisel*",
            "chanfr*",
            "nib",
            "monoline",
            "script",
            "brush lettering",
            "カリグラフィ",
            "レタリング",
        ],
    },
    Rule {
        category: "Marcadores",
        weight: 4,
        words: &[
            "marker*",
            "marcador*",
            "highlighter*",
            "marca texto",
            "copic",
            "felt*",
            "feltro",
            "alcohol",
            "alcool",
            "promarker",
            "マーカー",
            "サインペン",
        ],
    },
    Rule {
        category: "Esboço",
        weight: 4,
        words: &[
            "pencil*",
            "lapis",
            "lapiseira*",
            "graphite",
            "grafite",
            "sketch*",
            "esboc*",
            "rascunh*",
            "charcoal*",
            "carvao",
            "chalk*",
            "giz",
            "pastel*",
            "crayon*",
            "conte",
            "mechanical",
            "draft*",
            "drawing",
            "desenho",
            "鉛筆",
            "シャーペン",
            "シャープペン",
            "木炭",
            "クレヨン",
            "パステル",
            "ラフ",
            "デッサン",
            "色鉛筆",
        ],
    },
    Rule {
        category: "Arte-final",
        weight: 4,
        words: &[
            "ink",
            "inking",
            "inker*",
            "inked",
            "nanquim",
            "liner*",
            "lineart*",
            "line art",
            "arte final",
            "outline*",
            "contorno*",
            "gpen",
            "g pen",
            "maru",
            "kabura",
            "mapping pen",
            "technical",
            "tecnica",
            "fineliner*",
            "millipen",
            "micron",
            "pen",
            "caneta*",
            "brush pen",
            "ペン",
            "インク",
            "ミリペン",
            "線画",
            "主線",
        ],
    },
    Rule {
        category: "Decoração",
        weight: 3,
        words: &[
            "leaf",
            "leaves",
            "folha*",
            "foliage",
            "folhag*",
            "grass*",
            "grama",
            "flower*",
            "flor",
            "flores",
            "floral",
            "star*",
            "estrela*",
            "sparkl*",
            "glitter*",
            "brilho*",
            "cloud*",
            "nuvem",
            "nuvens",
            "tree*",
            "arvore*",
            "stamp*",
            "carimbo*",
            "pattern*",
            "decor*",
            "confett*",
            "confete*",
            "snow*",
            "neve",
            "bubble*",
            "bolha*",
            "heart*",
            "coraca*",
            "coraco*",
            "costura*",
            "stitch*",
            "corda*",
            "rope*",
            "bolinha*",
            "dots",
            "polka",
            "lace",
            "renda",
            "rain",
            "chuva",
            "fur",
            "hair",
            "cabelo*",
            "chain*",
            "corrente*",
            "ribbon*",
            "fita*",
            "bokeh",
            "装飾",
            "草",
            "花",
            "葉",
            "雲",
            "星",
            "キラキラ",
            "レース",
            "模様",
        ],
    },
    Rule {
        category: "Texturas",
        weight: 2,
        words: &[
            "textur*",
            "tela",
            "grain*",
            "granul*",
            "paper",
            "papel",
            "canvas",
            "noise",
            "ruido",
            "grunge*",
            "wood*",
            "madeira*",
            "burn*",
            "queim*",
            "concrete",
            "concreto",
            "stone*",
            "pedra*",
            "rock*",
            "sponge*",
            "esponja*",
            "rough*",
            "aspero*",
            "dirt*",
            "dust*",
            "poeira",
            "fabric*",
            "tecido*",
            "rust*",
            "ferrugem",
            "scratch*",
            "risco*",
            "テクスチャ",
            "質感",
            "紙",
        ],
    },
    Rule {
        category: "Pintura",
        weight: 3,
        words: &[
            "oil",
            "oleo",
            "gouache*",
            "guache",
            "acrylic*",
            "acrilic*",
            "impasto*",
            "bristle*",
            "cerda*",
            "rake",
            "rastelo",
            "tempera",
            "油彩",
            "厚塗り",
            "ガッシュ",
            "アクリル",
            "グリザイユ",
        ],
    },
    Rule {
        category: "Pintura",
        weight: 1,
        words: &[
            "paint*",
            "pintur*",
            "round",
            "redond*",
            "flat",
            "chato",
            "fan",
            "leque",
            "knife",
            "espatula",
            "brush*",
            "pincel*",
            "render*",
            "blocking",
            "thick",
            "筆",
            "ブラシ",
        ],
    },
];

/// Texto em minúsculas, sem acento, com as palavras separadas por um espaço (e "SoftPencil" vira
/// "soft pencil"), com um espaço no começo e no fim.
fn normalize(text: &str) -> String {
    let mut out = String::from(" ");
    let mut prev_lower = false;
    for c in text.chars() {
        if c.is_uppercase() && prev_lower {
            out.push(' ');
        }
        prev_lower = c.is_lowercase();
        let c = match c.to_lowercase().next().unwrap_or(c) {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c => c,
        };
        if c.is_alphabetic() {
            out.push(c);
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }
    if !out.ends_with(' ') {
        out.push(' ');
    }
    out
}

fn word_matches(text: &str, word: &str) -> bool {
    if !word.is_ascii() {
        return text.contains(word);
    }
    if word.contains(' ') {
        return text.contains(&format!(" {word}"));
    }
    if let Some(stem) = word.strip_suffix('*') {
        return text.contains(&format!(" {stem}"));
    }
    text.split(' ').any(|t| {
        t == word || t.strip_suffix('s') == Some(word) || t.strip_suffix("es") == Some(word)
    })
}

/// Pontos de cada categoria pelas palavras de `text`.
fn score(text: &str, scores: &mut Vec<(&'static str, u32)>, factor: u32) {
    let text = normalize(text);
    for rule in RULES {
        let hits = rule.words.iter().filter(|w| word_matches(&text, w)).count() as u32;
        if hits == 0 {
            continue;
        }
        let pts = hits.min(2) * rule.weight * factor;
        match scores.iter_mut().find(|(c, _)| *c == rule.category) {
            Some((_, p)) => *p += pts,
            None => scores.push((rule.category, pts)),
        }
    }
}

/// Categoria pelas palavras do nome e das dicas (nome do arquivo, pastas), ou `None` se nenhuma
/// palavra conhecida aparece.
pub fn by_words(name: &str, hints: &[&str]) -> Option<&'static str> {
    let mut scores = Vec::new();
    score(name, &mut scores, 2);
    for h in hints {
        score(h, &mut scores, 1);
    }
    // Maior pontuação; no empate, a regra que vem antes em RULES (a ordem de inserção).
    let mut best: Option<(&'static str, u32)> = None;
    for (c, p) in scores {
        if best.is_none_or(|(_, bp)| p > bp) {
            best = Some((c, p));
        }
    }
    best.map(|(c, _)| c)
}

/// Categoria pelo jeito do pincel, quando o nome não diz nada.
pub fn by_settings(s: &BrushSettings, mixes: bool) -> &'static str {
    let scatter =
        s.scattering.enabled && (s.scattering.scatter.jitter > 1.0 || s.scattering.count > 2);
    let textured = s.texture.enabled || s.dual_brush.enabled || s.noise;
    if s.erase {
        "Borrachas"
    } else if mixes {
        "Mistura"
    } else if s.wet_edges {
        "Aquarela"
    } else if scatter || (s.build_up && s.hardness < 0.6) || (s.hardness < 0.25 && s.size >= 30.0) {
        "Aerógrafo"
    } else if textured && s.size <= 20.0 {
        "Esboço"
    } else if textured {
        "Texturas"
    } else if s.hardness >= 0.8 && s.size <= 24.0 {
        "Arte-final"
    } else if s.size <= 10.0 {
        "Esboço"
    } else {
        "Pintura"
    }
}

/// Categoria de um pincel instalado: apagar vale mais que o nome (uma "borracha de lápis" é
/// borracha); depois as palavras; por fim os ajustes (misturar com a tela → Mistura).
pub fn classify(name: &str, hints: &[&str], s: &BrushSettings, mixes: bool) -> &'static str {
    if s.erase {
        return "Borrachas";
    }
    by_words(name, hints).unwrap_or_else(|| by_settings(s, mixes))
}

/// Classifica um pincel de "Meus pincéis" pelo nome, pasta e origem (se não foi escolhido à mão).
/// Devolve se a categoria mudou.
pub fn auto_sort(b: &mut BrushEntry) -> bool {
    if b.group != CUSTOM_GROUP || b.sort.by_hand {
        return false;
    }
    let hints = [b.folder.as_str(), b.sort.origin.as_str()];
    let cat = classify(&b.name, &hints, &b.settings, b.mixes);
    if b.sort.category == cat {
        return false;
    }
    b.sort.category = cat.to_string();
    true
}

/// "Organizar automaticamente": classifica de novo todos os de Meus pincéis que não foram
/// escolhidos à mão. Devolve quantos mudaram de categoria.
pub fn auto_sort_all(brushes: &mut [BrushEntry]) -> usize {
    brushes.iter_mut().map(auto_sort).filter(|c| *c).count()
}

/// Classifica só os que ainda não têm categoria (pincéis instalados antes desta função existir).
pub fn sort_unsorted(brushes: &mut [BrushEntry]) -> usize {
    brushes
        .iter_mut()
        .filter(|b| b.sort.category.is_empty())
        .map(auto_sort)
        .filter(|c| *c)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(name: &str, hints: &[&str]) -> &'static str {
        classify(name, hints, &BrushSettings::default(), false)
    }

    #[test]
    fn names_in_english_portuguese_and_japanese() {
        assert_eq!(cat("Soft Pencil", &[]), "Esboço");
        assert_eq!(cat("Lápis 6B", &[]), "Esboço");
        assert_eq!(cat("鉛筆", &[]), "Esboço");
        assert_eq!(cat("Gペン", &[]), "Arte-final");
        assert_eq!(cat("Fineliner 0.3", &[]), "Arte-final");
        assert_eq!(cat("Ink brush", &[]), "Arte-final");
        assert_eq!(cat("Alcohol marker", &[]), "Marcadores");
        assert_eq!(cat("Watercolor brush", &[]), "Aquarela");
        assert_eq!(cat("水彩 にじみ", &[]), "Aquarela");
        assert_eq!(cat("Airbrush soft", &[]), "Aerógrafo");
        assert_eq!(cat("Splatter 03", &[]), "Aerógrafo");
        assert_eq!(cat("Gouache flat", &[]), "Pintura");
        assert_eq!(cat("Oil Bristle", &[]), "Pintura");
        assert_eq!(cat("Paper texture", &[]), "Texturas");
        assert_eq!(cat("Halftone dots", &[]), "Quadrinhos");
        assert_eq!(cat("Leaves", &[]), "Decoração");
        assert_eq!(cat("Calligraphy chisel", &[]), "Caligrafia");
        assert_eq!(cat("Soft blender", &[]), "Mistura");
        assert_eq!(cat("Kneaded eraser", &[]), "Borrachas");
        assert_eq!(cat("SoftPencilTexture", &[]), "Esboço");
    }

    #[test]
    fn pen_is_not_pencil_and_word_beats_pack() {
        assert_eq!(cat("Pen", &["Amika Pencilbox"]), "Arte-final");
        assert_eq!(cat("Pencil", &["Inking pack"]), "Esboço");
        // Nome sem dica: o nome do pacote decide.
        assert_eq!(
            cat("Nº 3", &["16 Free Graphite & Pencil Brushes"]),
            "Esboço"
        );
        assert_eq!(
            cat("Brush 12", &["64 Free Watercolor Splatter Brushes"]),
            "Aquarela"
        );
    }

    #[test]
    fn falls_back_to_settings() {
        let mut s = BrushSettings {
            size: 60.0,
            hardness: 0.1,
            ..BrushSettings::default()
        };
        assert_eq!(classify("A1", &[], &s, false), "Aerógrafo");
        s.hardness = 0.95;
        s.size = 6.0;
        assert_eq!(classify("A1", &[], &s, false), "Arte-final");
        s.wet_edges = true;
        assert_eq!(classify("A1", &[], &s, false), "Aquarela");
        s.erase = true;
        assert_eq!(classify("Pencil", &[], &s, false), "Borrachas");
        assert_eq!(
            classify("A1", &[], &BrushSettings::default(), true),
            "Mistura"
        );
    }

    #[test]
    fn by_hand_is_kept() {
        let mut b = crate::brushes::BrushEntry {
            name: "Watercolor".into(),
            group: CUSTOM_GROUP.into(),
            settings: BrushSettings::default(),
            mixes: false,
            folder: String::new(),
            sort: BrushSort {
                category: "Esboço".into(),
                by_hand: true,
                origin: String::new(),
            },
        };
        assert!(!auto_sort(&mut b));
        assert_eq!(b.sort.category, "Esboço");
        b.sort.by_hand = false;
        assert!(auto_sort(&mut b));
        assert_eq!(b.sort.category, "Aquarela");
    }
}

/// Os pincéis que vêm com o app, tratados como se fossem instalados só pelo nome e ajustes,
/// caem quase todos na categoria em que estão de propósito.
#[cfg(test)]
mod survey {
    #[test]
    fn builtin_brushes_mostly_land_in_their_own_category() {
        let all = crate::brushes::all();
        let wrong: Vec<String> = all
            .iter()
            .filter(|b| super::classify(&b.name, &[], &b.settings, b.mixes_paint()) != b.group)
            .map(|b| b.name.clone())
            .collect();
        assert!(wrong.len() * 10 <= all.len(), "fora do lugar: {wrong:?}");
    }
}
