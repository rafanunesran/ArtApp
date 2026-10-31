//! Núcleo do ArtApp: documento com camadas raster, traços de pincel, histórico (desfazer/refazer),
//! composição e exportação. Não depende de nenhuma biblioteca de interface, então roda igual no
//! Android, no desktop e nos testes.
//!
//! Os pincéis e as superfícies em tiles vêm do PhotoCraft (`photocraft-paint`,
//! `photocraft-raster`). As superfícies são copy-on-write: clonar uma camada só copia ponteiros
//! para os tiles, e é isso que deixa o histórico barato (cada passo guarda a pilha de camadas
//! inteira).
#![forbid(unsafe_code)]

pub mod border;
pub mod brushes;
pub mod canvas_size;
pub mod composite;
pub mod document;
pub mod edit;
pub mod export;
pub mod fill;
pub mod folders;
pub mod heal;
pub mod history;
pub mod layer_mask;
pub mod liquify;
pub mod mask;
pub mod mix;
pub mod pressure;
pub mod project;
pub mod psd;
pub mod quickshape;
pub mod ruler;
pub mod selection;
pub mod stroke;
pub mod transform;
pub mod vector;
pub mod vectorkit;
pub mod warp;

pub use document::{Document, Layer};
pub use photocraft_algo::FilterParams;
pub use photocraft_color::BlendMode;
pub use photocraft_geom::Rect;
/// O motor de pincéis do PhotoCraft (o editor de pincel mexe em todos os ajustes dele).
pub use photocraft_paint as paint;
pub use photocraft_paint::{BrushSettings, StrokePoint};
pub use selection::Selection;
pub use stroke::{PaintSession, StrokeOptions, Taper};
pub use vectorcraft_doc::NodeId;
pub use vectorcraft_geom::path::{Anchor, SubPath};
pub use vectorcraft_geom::{BezPath, Point as VPoint};
