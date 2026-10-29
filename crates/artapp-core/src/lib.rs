//! Núcleo do ArtApp: documento com camadas raster, traços de pincel, histórico (desfazer/refazer),
//! composição e exportação. Não depende de nenhuma biblioteca de interface, então roda igual no
//! Android, no desktop e nos testes.
//!
//! Os pincéis e as superfícies em tiles vêm do PhotoCraft (`photocraft-paint`,
//! `photocraft-raster`). As superfícies são copy-on-write: clonar uma camada só copia ponteiros
//! para os tiles, e é isso que deixa o histórico barato (cada passo guarda a pilha de camadas
//! inteira).
#![forbid(unsafe_code)]

pub mod brushes;
pub mod composite;
pub mod document;
pub mod export;
pub mod fill;
pub mod history;
pub mod stroke;

pub use document::{Document, Layer};
pub use photocraft_color::BlendMode;
pub use photocraft_geom::Rect;
pub use photocraft_paint::{BrushSettings, StrokePoint};
pub use stroke::PaintSession;
