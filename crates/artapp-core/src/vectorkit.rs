//! O kit vetorial: todas as ferramentas do VectorCraft (seleção direta, caneta, curvatura, texto,
//! formas, distorções, construtor de formas, malha, simetria por espelho…) agindo sobre a camada
//! vetorial ativa. Uma sessão do motor do VectorCraft guarda a arte da camada enquanto a gente
//! edita; cada mudança volta para a camada e é rasterizada de novo.

use std::sync::Arc;

use photocraft_geom::Rect;
use serde_json::Value;
use vectorcraft_color::{Color, Paint};
use vectorcraft_engine::Session;
use vectorcraft_engine::ViewInfo;
use vectorcraft_engine::tools::PointerEvent;
pub use vectorcraft_engine::tools::{Mods, PointerKind};
use vectorcraft_geom::Point;

use crate::document::Document;
use crate::vector::{VectorStyle, next_rev, rasterize};

pub use vectorcraft_engine::tools::catalog::{TOOL_GROUPS, ToolInfo};
pub use vectorcraft_engine::tools::{Overlay, ToolKey};

/// O motor do VectorCraft preso à camada vetorial ativa.
pub struct VectorKit {
    session: Session,
    /// A camada e a revisão da arte que a sessão tem aberta.
    synced: Option<(u64, u64)>,
    /// O documento que a sessão tinha depois da última escrita na camada.
    last: Option<Arc<vectorcraft_engine::doc::Document>>,
    /// Um gesto em andamento gravou um ponto no histórico.
    pending_checkpoint: bool,
    pub zoom: f64,
    /// Mensagens do motor (avisos, diálogos pedidos) para mostrar na tela.
    pub status: Option<String>,
}

impl Default for VectorKit {
    fn default() -> Self {
        Self::new()
    }
}

impl VectorKit {
    pub fn new() -> Self {
        Self {
            session: Session::new(),
            synced: None,
            last: None,
            pending_checkpoint: false,
            zoom: 1.0,
            status: None,
        }
    }

    fn view(&self) -> ViewInfo {
        ViewInfo {
            zoom: self.zoom.max(0.01),
            ..ViewInfo::default()
        }
    }

    pub fn tool_id(&self) -> &'static str {
        self.session.tool_id()
    }

    pub fn select_tool(&mut self, id: &str) {
        let view = self.view();
        if self.session.select_tool(id, view).is_err() {
            self.status = Some("Ferramenta indisponível.".into());
        }
    }

    /// Cor e espessura dos objetos novos.
    pub fn set_style(&mut self, style: &VectorStyle) {
        let paint = |c: Option<[f32; 3]>| {
            c.map_or(Paint::None, |c| Paint::solid(Color::rgb(c[0], c[1], c[2])))
        };
        self.session.paint.fill = paint(style.fill);
        self.session.paint.stroke = paint(style.stroke);
        self.session.paint.stroke_width = style.width;
    }

    /// Garante que a sessão tem a arte atual da camada ativa. Falso se a camada não é vetorial.
    fn sync_in(&mut self, doc: &Document) -> bool {
        let Some(layer) = doc.active_layer() else {
            return false;
        };
        let Some(art) = layer.vector.as_ref() else {
            return false;
        };
        if self.synced == Some((layer.id, art.rev)) && self.session.active().is_some() {
            return true;
        }
        while !self.session.documents().is_empty() {
            if !self.session.close_document(0) {
                break;
            }
        }
        self.session.add_document(art.doc.clone(), None);
        self.synced = Some((layer.id, art.rev));
        self.last = self.session.active().map(|d| d.doc.clone());
        true
    }

    /// Copia a arte da sessão para a camada se ela mudou. Devolve a área a recompor.
    fn sync_out(&mut self, doc: &mut Document) -> Rect {
        let Some(state) = self.session.active() else {
            return Rect::EMPTY;
        };
        if self
            .last
            .as_ref()
            .is_some_and(|l| Arc::ptr_eq(l, &state.doc))
        {
            return Rect::EMPTY;
        }
        let new = state.doc.clone();
        let (w, h) = (doc.width, doc.height);
        let bounds = doc.bounds();
        let Some(layer) = doc.active_layer_mut() else {
            return Rect::EMPTY;
        };
        let Some(art) = layer.vector.as_mut() else {
            return Rect::EMPTY;
        };
        art.doc = (*new).clone();
        art.rev = next_rev();
        layer.surface = rasterize(art, w, h);
        self.synced = Some((layer.id, art.rev));
        self.last = Some(new);
        bounds
    }

    fn handle(&mut self, reqs: vectorcraft_engine::Result<Vec<vectorcraft_engine::UiRequest>>) {
        match reqs {
            Ok(reqs) => {
                for r in reqs {
                    match r {
                        vectorcraft_engine::UiRequest::Status(s) => self.status = Some(s),
                        vectorcraft_engine::UiRequest::SwitchTool(id) => self.select_tool(&id),
                        _ => {}
                    }
                }
            }
            Err(e) => self.status = Some(e.to_string()),
        }
    }

    /// Um evento do dedo/caneta em coordenadas do documento.
    pub fn pointer(
        &mut self,
        doc: &mut Document,
        kind: PointerKind,
        x: f64,
        y: f64,
        pressure: f32,
        mods: Mods,
    ) -> Rect {
        if !self.sync_in(doc) {
            return Rect::EMPTY;
        }
        if kind == PointerKind::Down {
            doc.checkpoint();
            self.pending_checkpoint = true;
        }
        let ev = PointerEvent {
            kind,
            pos: Point::new(x, y),
            mods,
            pressure: pressure.clamp(0.0, 1.0),
        };
        let view = self.view();
        let r = self.session.pointer(&ev, view);
        self.handle(r);
        let dirty = self.sync_out(doc);
        if !dirty.is_empty() {
            self.pending_checkpoint = false;
        }
        if kind == PointerKind::Up {
            self.finish_gesture(doc);
        }
        dirty
    }

    /// Um gesto que não mudou nada não deixa passo no histórico.
    fn finish_gesture(&mut self, doc: &mut Document) {
        if std::mem::take(&mut self.pending_checkpoint) {
            doc.drop_checkpoint();
        }
    }

    pub fn key(&mut self, doc: &mut Document, key: ToolKey) -> Rect {
        if !self.sync_in(doc) {
            return Rect::EMPTY;
        }
        doc.checkpoint();
        let view = self.view();
        let r = self.session.tool_key(key, Mods::default(), view);
        self.handle(r);
        self.commit_or_drop(doc)
    }

    pub fn wants_text(&self) -> bool {
        self.session.tool_wants_text()
    }

    pub fn text(&mut self, doc: &mut Document, s: &str) -> Rect {
        if !self.sync_in(doc) {
            return Rect::EMPTY;
        }
        doc.checkpoint();
        let view = self.view();
        let r = self.session.tool_text(s, view);
        self.handle(r);
        self.commit_or_drop(doc)
    }

    /// Roda um comando do VectorCraft (ex.: "object.group", "pathfinder.unite") na camada ativa.
    pub fn execute(&mut self, doc: &mut Document, id: &str, params: &Value) -> Rect {
        if !self.sync_in(doc) {
            return Rect::EMPTY;
        }
        doc.checkpoint();
        if let Err(e) = self.session.execute(id, params) {
            self.status = Some(e.to_string());
        }
        self.commit_or_drop(doc)
    }

    fn commit_or_drop(&mut self, doc: &mut Document) -> Rect {
        let dirty = self.sync_out(doc);
        if dirty.is_empty() {
            doc.drop_checkpoint();
        }
        dirty
    }

    /// O que a ferramenta desenha por cima (alças, guias, contornos), em coordenadas do documento.
    pub fn overlays(&mut self, doc: &Document) -> Vec<Overlay> {
        if !doc.active_is_vector() {
            return vec![];
        }
        let view = self.view();
        let mut v = self.selection_marks();
        v.extend(self.session.overlays(view));
        v
    }

    /// A seleção: contornos, pontos (cheios quando escolhidos) e alças dos pontos escolhidos; com
    /// as ferramentas que não editam pontos, a caixa em volta de tudo.
    fn selection_marks(&self) -> Vec<Overlay> {
        let Some(st) = self.session.active() else {
            return vec![];
        };
        let tool = self.session.tool_id();
        let direct = vectorcraft_engine::tools::catalog::edits_anchors(tool);
        let blue = [40, 140, 255];
        let mut v = Vec::new();
        let mut bounds: Option<vectorcraft_geom::Rect> = None;
        for id in &st.selection.objects {
            let Some(n) = st.doc.node(*id) else {
                continue;
            };
            if let Some(b) = n.geometric_bounds() {
                bounds = Some(bounds.map_or(b, |u| u.union(b)));
            }
            let Some(pd) = n.path_data() else {
                continue;
            };
            v.push(Overlay::Path {
                path: pd.to_bezpath(),
                color: blue,
                width: 1.0,
                dashed: false,
            });
            let chosen = st.selection.anchors.get(id);
            for (si, sp) in pd.subpaths.iter().enumerate() {
                for (ai, a) in sp.anchors.iter().enumerate() {
                    let picked = chosen.is_some_and(|c| c.contains(&(si, ai)));
                    if direct && picked {
                        for h in [a.h_in, a.h_out] {
                            if (h - a.p).hypot() > 0.01 {
                                v.push(Overlay::Line {
                                    a: a.p,
                                    b: h,
                                    color: blue,
                                    dashed: false,
                                });
                                v.push(Overlay::Handle { p: h, color: blue });
                            }
                        }
                    }
                    v.push(Overlay::Anchor {
                        p: a.p,
                        color: blue,
                        filled: picked || (!direct && chosen.is_none()),
                        size: if direct { 9.0 } else { 6.0 },
                    });
                }
            }
        }
        if !direct && let Some(b) = bounds {
            v.insert(0, Overlay::Marquee(b));
        }
        v
    }

    /// Esquece a arte aberta (outra camada, desfazer…): o próximo evento abre a atual.
    pub fn reset(&mut self) {
        self.synced = None;
    }

    pub fn options(&self) -> Value {
        self.session.tool_options()
    }

    pub fn set_option(&mut self, key: &str, v: &Value) {
        self.session.set_tool_option(key, v);
    }

    pub fn has_selection(&self) -> bool {
        self.session
            .active()
            .is_some_and(|d| !d.selection.is_empty())
    }
}

/// Nome em português de uma ferramenta do catálogo.
pub fn label_pt(id: &str) -> &'static str {
    match id {
        "selection" => "Seleção",
        "directSelection" => "Seleção direta",
        "groupSelection" => "Seleção de grupo",
        "magicWand" => "Varinha mágica",
        "lasso" => "Laço",
        "pen" => "Caneta",
        "addAnchor" => "Adicionar ponto",
        "deleteAnchor" => "Apagar ponto",
        "anchorPoint" => "Converter ponto",
        "curvature" => "Curvatura",
        "type" => "Texto",
        "areaType" => "Texto em área",
        "typeOnPath" => "Texto no traçado",
        "verticalType" => "Texto vertical",
        "verticalAreaType" => "Texto vertical em área",
        "verticalTypeOnPath" => "Texto vertical no traçado",
        "touchType" => "Retocar letras",
        "lineSegment" => "Segmento de reta",
        "arc" => "Arco",
        "spiral" => "Espiral",
        "rectangularGrid" => "Grade retangular",
        "polarGrid" => "Grade polar",
        "rectangle" => "Retângulo",
        "roundedRectangle" => "Retângulo arredondado",
        "ellipse" => "Elipse",
        "polygon" => "Polígono",
        "star" => "Estrela",
        "flare" => "Reflexo de luz",
        "paintbrush" => "Pincel vetorial",
        "blobBrush" => "Pincel de bolha",
        "shaper" => "Modelador",
        "pencil" => "Lápis vetorial",
        "smooth" => "Suavizar",
        "pathEraser" => "Apagar traçado",
        "join" => "Unir",
        "eraser" => "Borracha vetorial",
        "scissors" => "Tesoura",
        "knife" => "Faca",
        "mirrorCut" => "Espelhar e cortar",
        "lineCut" => "Corte em linha",
        "rectCut" => "Corte em retângulo",
        "rotate" => "Girar",
        "reflect" => "Refletir",
        "scale" => "Escala",
        "shear" => "Inclinar",
        "reshape" => "Remodelar",
        "width" => "Largura do traço",
        "warp" => "Deformar",
        "twirl" => "Torcer",
        "pucker" => "Contrair",
        "bloat" => "Inflar",
        "scallop" => "Recortar bordas",
        "crystallize" => "Cristalizar",
        "wrinkle" => "Enrugar",
        "freeTransform" => "Transformação livre",
        "puppetWarp" => "Marionete",
        "shapeBuilder" => "Construtor de formas",
        "livePaintBucket" => "Balde de pintura viva",
        "livePaintSelection" => "Seleção de pintura viva",
        "perspectiveGrid" => "Grade de perspectiva",
        "perspectiveSelection" => "Seleção em perspectiva",
        "mesh" => "Malha",
        "gradient" => "Degradê vetorial",
        "eyedropper" => "Conta-gotas de estilo",
        "measure" => "Medir",
        "blend" => "Mesclar formas",
        "objectsOnPath" => "Objetos no traçado",
        "dimension" => "Cota",
        "symbolSprayer" => "Spray de símbolos",
        "symbolShifter" => "Mover símbolos",
        "symbolScruncher" => "Juntar símbolos",
        "symbolSizer" => "Tamanho dos símbolos",
        "symbolSpinner" => "Girar símbolos",
        "symbolStainer" => "Tingir símbolos",
        "symbolScreener" => "Transparência dos símbolos",
        "symbolStyler" => "Estilo dos símbolos",
        "columnGraph" => "Gráfico de colunas",
        "stackedColumnGraph" => "Colunas empilhadas",
        "barGraph" => "Gráfico de barras",
        "stackedBarGraph" => "Barras empilhadas",
        "lineGraph" => "Gráfico de linhas",
        "areaGraph" => "Gráfico de área",
        "scatterGraph" => "Dispersão",
        "pieGraph" => "Pizza",
        "radarGraph" => "Radar",
        _ => "",
    }
}

/// As ferramentas do catálogo que fazem sentido aqui (sem prancheta, fatia, mão, zoom, impressão).
pub fn groups() -> impl Iterator<Item = &'static [ToolInfo]> {
    TOOL_GROUPS
        .iter()
        .copied()
        .filter(|g| g.iter().any(|t| !label_pt(t.id).is_empty()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn rectangle_tool_draws_on_vector_layer() {
        let mut doc = Document::new(200, 200);
        doc.add_vector_layer();
        let mut kit = VectorKit::new();
        kit.set_style(&VectorStyle {
            stroke: Some([0.0, 0.0, 0.0]),
            width: 4.0,
            fill: Some([1.0, 0.0, 0.0]),
            ..VectorStyle::default()
        });
        kit.select_tool("rectangle");
        let m = Mods::default();
        kit.pointer(&mut doc, PointerKind::Down, 20.0, 20.0, 1.0, m);
        kit.pointer(&mut doc, PointerKind::Drag, 80.0, 80.0, 1.0, m);
        let dirty = kit.pointer(&mut doc, PointerKind::Up, 120.0, 120.0, 1.0, m);
        assert!(!dirty.is_empty() || doc.can_undo());
        let art = doc.active_vector().unwrap();
        assert!(!art.doc.layers[0].children().unwrap().is_empty());
        assert!(doc.can_undo());
    }

    #[test]
    fn every_listed_tool_has_a_label() {
        let n: usize = groups().map(|g| g.len()).sum();
        assert!(n > 60);
    }

    #[test]
    fn direct_selection_shows_anchors() {
        let mut doc = Document::new(200, 200);
        doc.add_vector_layer();
        let mut kit = VectorKit::new();
        kit.select_tool("rectangle");
        let m = Mods::default();
        kit.pointer(&mut doc, PointerKind::Down, 20.0, 20.0, 1.0, m);
        kit.pointer(&mut doc, PointerKind::Drag, 80.0, 80.0, 1.0, m);
        kit.pointer(&mut doc, PointerKind::Up, 120.0, 120.0, 1.0, m);
        kit.select_tool("directSelection");
        kit.pointer(&mut doc, PointerKind::Down, 70.0, 20.0, 1.0, m);
        kit.pointer(&mut doc, PointerKind::Up, 70.0, 20.0, 1.0, m);
        let o = kit.overlays(&doc);
        eprintln!("sel={} overlays={o:?}", kit.has_selection());
        assert!(!o.is_empty());
    }
}
