# epaint 0.37.0 (cópia com correção)

Cópia do `epaint` 0.37.0 publicado no crates.io (https://github.com/emilk/egui, MIT OR Apache-2.0,
veja os arquivos de licença do egui) usada pelo `[patch.crates-io]` do `Cargo.toml` da raiz.

Única mudança: `src/text/font_face.rs` cria os contextos de rasterização de glifo com
`glyph_render_context` (um thread só). Sem isso, como o VectorCraft liga o recurso `multithreading`
do `vello_cpu`, o app entra em pânico ("attempted to rasterize before flushing") ao desenhar texto.
A mesma correção está no fork do egui usado pelo VectorCraft (storytold/egui@6d518fc).

Remover esta pasta e o patch quando o egui publicado trouxer a correção.
