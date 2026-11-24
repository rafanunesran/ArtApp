//! Pincéis do Clip Studio Paint (`.sut`, sub-ferramenta exportada) → pincéis do ArtApp.
//!
//! O `.sut` é um banco SQLite (lido por [`crate::sqlite_lite`]). Leitura de interoperabilidade
//! pelo que é observável no próprio arquivo, sem nada do programa: a tabela `Node` dá o nome, a
//! `Variant` os ajustes (tamanho, opacidade, dureza, espaçamento, fluxo, espessura, ângulo,
//! spray, borda de aquarela) e a `MaterialFile` guarda as imagens da ponta, das quais pegamos os
//! PNGs embutidos. O que não tem equivalente (as curvas de pressão próprias do Clip, a textura
//! do papel) fica no padrão do ArtApp: pressão no tamanho ligada.

use photocraft_paint::{BrushSettings, TipShape};

use crate::sqlite_lite::{Db, Table, Value};

/// Um pincel lido do `.sut`.
pub struct SutBrush {
    pub name: String,
    pub settings: BrushSettings,
    /// Nomes dos outros nós do arquivo (grupo e ferramenta do Clip, como "鉛筆" ou "Pen"), usados
    /// para classificar o pincel ([`crate::brush_sort`]).
    pub hint: String,
}

/// Lê os pincéis (normalmente um só) de um `.sut`. `stem` é o nome do arquivo, usado quando o
/// pincel não tem nome.
pub fn read_sut(bytes: &[u8], stem: &str) -> Result<Vec<SutBrush>, String> {
    let db = Db::open(bytes).map_err(|e| format!("não é um pincel do Clip Studio (.sut): {e}"))?;
    let variants = db
        .table("Variant")
        .ok_or("o arquivo .sut não tem a tabela de ajustes (Variant)")?;
    let nodes = db.table("Node").unwrap_or_default();
    let tips = material_tips(&db);
    let node_names: Vec<String> = nodes
        .rows
        .iter()
        .filter_map(|n| nodes.get(n, "NodeName").and_then(Value::as_text))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    let mut out = Vec::new();
    for row in &variants.rows {
        let id = variants
            .get(row, "VariantID")
            .and_then(Value::as_f64)
            .map(|v| v as i64);
        // O nome vem do nó que usa esta variante (ou do arquivo).
        let name = nodes
            .rows
            .iter()
            .find(|n| {
                id.is_some()
                    && nodes
                        .get(n, "NodeVariantID")
                        .and_then(Value::as_f64)
                        .map(|v| v as i64)
                        == id
            })
            .or_else(|| {
                (variants.rows.len() == 1)
                    .then(|| nodes.rows.first())
                    .flatten()
            })
            .and_then(|n| nodes.get(n, "NodeName"))
            .and_then(Value::as_text)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map_or_else(|| stem.to_string(), str::to_string);
        let hint = node_names
            .iter()
            .filter(|n| **n != name)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        out.push(SutBrush {
            name,
            settings: map_variant(&variants, row, &tips),
            hint,
        });
    }
    if out.is_empty() {
        return Err("o arquivo .sut não tem pincéis".into());
    }
    Ok(out)
}

/// Lê um número da variante: o primeiro nome de coluna que existir.
fn num(t: &Table, row: &[Value], names: &[&str]) -> Option<f32> {
    names
        .iter()
        .find_map(|n| t.get(row, n).and_then(Value::as_f64))
        .map(|v| v as f32)
        .filter(|v| v.is_finite())
}

/// Porcentagem guardada como 0–100 (ou já 0–1) → 0–1.
fn unit(v: f32) -> f32 {
    if v > 1.0 { v / 100.0 } else { v }.clamp(0.0, 1.0)
}

fn on(t: &Table, row: &[Value], name: &str) -> bool {
    t.get(row, name)
        .and_then(Value::as_f64)
        .is_some_and(|v| v != 0.0)
}

fn map_variant(t: &Table, row: &[Value], tips: &[photocraft_paint::GrayTile]) -> BrushSettings {
    let mut b = BrushSettings::default();
    if let Some(s) = num(t, row, &["BrushSize"]) {
        b.size = s.clamp(1.0, 1000.0);
    }
    if let Some(o) = num(t, row, &["Opacity", "BrushOpacity"]) {
        b.opacity = unit(o).max(0.01);
    }
    if let Some(f) = num(t, row, &["BrushFlow"]) {
        b.flow = unit(f).max(0.01);
    }
    if let Some(h) = num(t, row, &["BrushHardness"]) {
        b.hardness = unit(h);
    }
    if let Some(i) = num(t, row, &["BrushInterval"]) {
        // Intervalo do Clip: porcentagem do tamanho.
        b.spacing = unit(i).clamp(0.01, 2.0);
        b.spacing_enabled = true;
    }
    if let Some(th) = num(t, row, &["BrushThickness"]) {
        b.roundness = unit(th).max(0.02);
    }
    if let Some(a) = num(t, row, &["BrushRotation"]) {
        b.angle = a;
    }
    // O Clip liga a pressão no tamanho em quase todo pincel; sem ler as curvas dele, fica ligada
    // quando o pincel tem algum efeito no tamanho (e no padrão, se a coluna não existe).
    b.pressure_size = t.col("BrushSizeEffector").is_none()
        || t.get(row, "BrushSizeEffector")
            .is_some_and(|v| *v != Value::Null);
    b.pressure_opacity = t
        .get(row, "OpacityEffector")
        .and_then(Value::as_blob)
        .is_some_and(|v| !v.is_empty())
        && b.pressure_size;
    if on(t, row, "BrushUseWaterEdge") {
        b.wet_edges = true;
    }
    if on(t, row, "BrushUseSpray") {
        b.scattering.enabled = true;
        b.scattering.both_axes = true;
        let density = num(t, row, &["BrushSprayDensity"]).map_or(0.5, unit);
        b.scattering.scatter.jitter = 1.0 + 3.0 * density;
        b.scattering.count = ((density * 8.0).round() as u32).clamp(1, 16);
    }
    // Ponta de imagem (pincéis de forma, texturas, carimbos).
    let pattern = t.col("BrushUsePatternImage").is_none() || on(t, row, "BrushUsePatternImage");
    if pattern && let Some((first, rest)) = tips.split_first() {
        b.tip = TipShape::Sampled(first.clone());
        b.extra_tips = rest.iter().take(15).cloned().collect();
        b.tip_random = !b.extra_tips.is_empty();
    }
    b
}

/// As imagens de ponta guardadas no arquivo: os PNGs dentro de cada material.
fn material_tips(db: &Db<'_>) -> Vec<photocraft_paint::GrayTile> {
    let Some(t) = db.table("MaterialFile") else {
        return Vec::new();
    };
    let Some(col) = t.col("FileData").or_else(|| {
        t.rows
            .first()
            .and_then(|r| r.iter().position(|v| v.as_blob().is_some()))
    }) else {
        return Vec::new();
    };
    t.rows
        .iter()
        .filter_map(|r| r.get(col)?.as_blob())
        .filter_map(|blob| {
            embedded_pngs(blob)
                .into_iter()
                // A maior imagem do material é a ponta; as pequenas são miniaturas.
                .filter_map(|png| crate::edit::decode_image(png).ok())
                .max_by_key(|(w, h, _)| w * h)
        })
        .filter_map(|(w, h, px)| crate::brushes::tip_from_rgba(w, h, &px))
        .collect()
}

/// Os PNGs inteiros dentro de `blob` (da assinatura até o bloco IEND).
pub(crate) fn embedded_pngs(blob: &[u8]) -> Vec<&[u8]> {
    const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = find(&blob[from..], SIG) {
        let start = from + rel;
        let Some(end_rel) = find(&blob[start..], b"IEND") else {
            break;
        };
        // "IEND" + CRC de 4 bytes.
        let end = (start + end_rel + 8).min(blob.len());
        out.push(&blob[start..end]);
        from = end;
    }
    out
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// Um `.sut` de teste feito com o sqlite3 do Python: tabelas Node, Variant e MaterialFile
    /// com as colunas que o Clip usa e um PNG de ponta no meio de bytes quaisquer (e grande o
    /// bastante para cair em páginas de transbordo).
    #[test]
    fn reads_a_sut_with_tip() {
        let bytes = include_bytes!("../tests/data/teste.sut");
        let list = read_sut(bytes, "arquivo").unwrap();
        assert_eq!(list.len(), 1);
        let b = &list[0];
        assert_eq!(b.name, "Giz áspero");
        assert!((b.settings.size - 42.0).abs() < 0.01);
        assert!((b.settings.opacity - 0.8).abs() < 0.01);
        assert!((b.settings.hardness - 0.3).abs() < 0.01);
        assert!((b.settings.spacing - 0.15).abs() < 0.01);
        assert!(b.settings.wet_edges);
        assert!(b.settings.pressure_size);
        assert!(
            matches!(&b.settings.tip, TipShape::Sampled(t) if (t.width, t.height) == (64, 32)),
            "a ponta devia vir da imagem maior (64 × 32)"
        );
    }

    #[test]
    fn finds_pngs_in_noise() {
        let mut blob = vec![1u8, 2, 3];
        blob.extend_from_slice(b"\x89PNG\r\n\x1a\nxxxxIEND1234");
        blob.extend_from_slice(b"tail");
        let p = embedded_pngs(&blob);
        assert_eq!(p.len(), 1);
        assert!(p[0].ends_with(b"IEND1234"));
    }

    #[test]
    fn installs_from_a_zip_pack() {
        let zip = include_bytes!("../tests/data/pacote.zip");
        let list = crate::brushes::import("pacote.zip", zip).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Giz áspero");
        assert_eq!(list[0].folder, "pacote");
        assert_eq!(list[0].group, crate::brushes::CUSTOM_GROUP);
        // "Giz" → Esboço, ou seja, a ferramenta Lápis.
        assert_eq!(list[0].sort.category, "Esboço");
        assert_eq!(list[0].shown_group(), "Esboço");
        assert!(!list[0].sort.by_hand);
    }
}
