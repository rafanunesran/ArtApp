//! Medição das ações mais pesadas do ArtApp (sem interface): `cargo run --release -p artapp-core
//! --example perf`. Simula uma tela A4 a 300 dpi com várias camadas pintadas e mede traço,
//! composição da tela, desfazer, salvar e abrir.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Instant;

use artapp_core::composite;
use artapp_core::project;
use artapp_core::stroke::PaintSession;
use artapp_core::{BlendMode, Document, Rect};
use photocraft_paint::{BrushSettings, StrokePoint};

const W: u32 = 2480;
const H: u32 = 3508;
const LAYERS: usize = 8;

fn painted_doc() -> Document {
    let mut doc = Document::new(W, H);
    for i in 0..LAYERS {
        if i > 0 {
            doc.add_layer();
        }
        let l = doc.layers.len() - 1;
        let k = i as f32 / LAYERS as f32;
        // Fundo cheio na de baixo, manchas grandes nas outras.
        let r = if i == 0 {
            doc.bounds()
        } else {
            let x = (i as i32 * 211) % (W as i32 / 2);
            let y = (i as i32 * 397) % (H as i32 / 2);
            Rect::new(x, y, x + W as i32 / 2, y + H as i32 / 2)
        };
        doc.layers[l]
            .surface
            .fill_rect(r, &[k, 1.0 - k, 0.5, if i == 0 { 1.0 } else { 0.6 }]);
        if i == 3 {
            doc.layers[l].blend = BlendMode::Multiply;
        }
    }
    // Pinta na penúltima (uma camada por cima, como é comum).
    doc.active = LAYERS - 2;
    doc
}

/// Muitas camadas com pouca tinta cada (linhas, sombras, detalhes), como num desenho real.
fn sparse_doc() -> Document {
    let mut doc = Document::new(W, H);
    let b = doc.bounds();
    doc.layers[0].surface.fill_rect(b, &[0.9, 0.9, 0.85, 1.0]);
    for i in 1..24 {
        doc.add_layer();
        let l = doc.layers.len() - 1;
        let x = (i * 331) % (W as i32 - 600);
        let y = (i * 577) % (H as i32 - 600);
        doc.layers[l]
            .surface
            .fill_rect(Rect::new(x, y, x + 500, y + 400), &[0.2, 0.1, 0.6, 0.8]);
    }
    doc
}

fn stroke_points(n: usize) -> Vec<StrokePoint> {
    (0..n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let x = 300.0 + t * 1800.0;
            let y = 1700.0 + (t * 12.0).sin() * 600.0;
            StrokePoint::new(x, y, 0.4 + 0.6 * (t * 3.0).sin().abs() as f32)
        })
        .collect()
}

struct Times {
    paint: f64,
    display: f64,
    frames: usize,
}

/// Um traço como o app faz: alguns pontos por quadro, desenha e recompõe só o retângulo sujo.
fn stroke(doc: &mut Document, cache: &mut composite::DisplayCache, size: f32) -> Times {
    let brush = BrushSettings {
        size,
        color: [0.1, 0.2, 0.8, 1.0],
        ..Default::default()
    };
    let pts = stroke_points(600);
    let mut s = PaintSession::begin(doc, &brush, 0.25).unwrap();
    let mut t = Times {
        paint: 0.0,
        display: 0.0,
        frames: 0,
    };
    for chunk in pts.chunks(3) {
        let a = Instant::now();
        let dirty = s.add(doc, chunk);
        let b = Instant::now();
        let (_, px) = cache.composite(doc, dirty);
        std::hint::black_box(px);
        let c = Instant::now();
        t.paint += (b - a).as_secs_f64();
        t.display += (c - b).as_secs_f64();
        t.frames += 1;
    }
    let dirty = s.end(doc);
    cache.composite(doc, dirty);
    t
}

fn ms(d: f64) -> String {
    format!("{:8.1} ms", d * 1000.0)
}

fn time<T>(name: &str, reps: usize, mut f: impl FnMut() -> T) -> f64 {
    let a = Instant::now();
    for _ in 0..reps {
        std::hint::black_box(f());
    }
    let d = a.elapsed().as_secs_f64() / reps as f64;
    println!("{name:<44}{}", ms(d));
    d
}

fn main() {
    println!(
        "Tela {W}×{H}, {LAYERS} camadas, {} threads",
        std::thread::available_parallelism().map_or(1, |n| n.get())
    );
    let mut doc = painted_doc();
    let mut cache = composite::DisplayCache::default();
    time("Compor a tela inteira (abrir/desfazer)", 3, || {
        composite::composite_rgba8(&doc, doc.bounds(), true)
    });
    let sparse = sparse_doc();
    time("Compor a tela inteira, 24 camadas esparsas", 3, || {
        composite::composite_rgba8(&sparse, sparse.bounds(), true)
    });
    for size in [12.0, 60.0, 200.0] {
        let t = stroke(&mut doc, &mut cache, size);
        println!(
            "Traço pincel {size:>3} px: {} quadros, pintar {} · compor tela {} · por quadro {:.2} ms",
            t.frames,
            ms(t.paint),
            ms(t.display),
            (t.paint + t.display) * 1000.0 / t.frames as f64
        );
    }
    time("Desfazer + recompor", 3, || {
        doc.undo();
        let r = doc.take_history_dirty();
        let px = cache.composite(&doc, r);
        doc.redo();
        let r = doc.take_history_dirty();
        (px, cache.composite(&doc, r))
    });
    let bytes = project::save(&doc).unwrap();
    time("Salvar projeto", 2, || project::save(&doc).unwrap());
    time("Abrir projeto", 2, || project::load(&bytes).unwrap());
    println!("Tamanho do projeto: {} KB", bytes.len() >> 10);
}
