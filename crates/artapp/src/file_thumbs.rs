//! Miniaturas da janela de abrir/importar, feitas em segundo plano.
//!
//! A lista pede a miniatura só das linhas que aparecem na tela; duas threads leem e reduzem os
//! arquivos ([`artapp_core::file_thumb`]) e pedem um novo quadro quando cada uma fica pronta.
//! O pedido mais recente é atendido primeiro (quem rola a lista vê logo o que está olhando). O
//! resultado fica num cache do processo, por caminho + data de modificação + tamanho, então
//! voltar a uma pasta ou reabrir a janela não lê tudo de novo.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::SystemTime;

use artapp_core::file_thumb::{self, Thumb};
use eframe::egui;

/// Quantas miniaturas prontas guardar (cada uma tem até 160×160 px: ~100 KB).
const CACHE_MAX: usize = 240;
/// Threads que fazem miniaturas.
const WORKERS: usize = 2;
/// Pedidos esperando: os mais antigos (de linhas que já saíram da tela) são descartados.
const QUEUE_MAX: usize = 64;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    path: PathBuf,
    modified: Option<SystemTime>,
    len: u64,
}

impl Key {
    fn of(path: &Path) -> Option<Self> {
        let m = std::fs::metadata(path).ok()?;
        Some(Self {
            path: path.to_path_buf(),
            modified: m.modified().ok(),
            len: m.len(),
        })
    }
}

#[derive(Default)]
struct State {
    done: HashMap<Key, Option<Arc<Thumb>>>,
    order: VecDeque<Key>,
    queued: HashSet<Key>,
    /// Pilha de pedidos: o último a entrar sai primeiro.
    queue: Vec<Key>,
    ctx: Option<egui::Context>,
    started: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

static SHARED: LazyLock<Shared> = LazyLock::new(|| Shared {
    state: Mutex::new(State::default()),
    wake: Condvar::new(),
});

/// Como está a miniatura de um arquivo.
pub enum Lookup {
    /// Pronta (`None` = o arquivo não tem miniatura: fica o ícone).
    Ready(Option<Arc<Thumb>>),
    /// Sendo feita; a tela é redesenhada quando ficar pronta.
    Pending,
}

/// A miniatura de `path`, pedindo-a se ainda não existe.
pub fn request(ctx: &egui::Context, path: &Path) -> Lookup {
    let Some(key) = Key::of(path) else {
        return Lookup::Ready(None);
    };
    let mut st = SHARED.state.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(t) = st.done.get(&key) {
        return Lookup::Ready(t.clone());
    }
    if st.ctx.is_none() {
        st.ctx = Some(ctx.clone());
    }
    if st.queued.insert(key.clone()) {
        st.queue.push(key);
        if st.queue.len() > QUEUE_MAX {
            let old = st.queue.remove(0);
            st.queued.remove(&old);
        }
    } else if let Some(i) = st.queue.iter().position(|k| *k == key) {
        // Pedido de novo (a linha continua na tela): passa para a frente.
        let k = st.queue.remove(i);
        st.queue.push(k);
    }
    if !st.started {
        st.started = true;
        for i in 0..WORKERS {
            let _ = std::thread::Builder::new()
                .name(format!("thumbs-{i}"))
                .spawn(worker);
        }
    }
    drop(st);
    SHARED.wake.notify_one();
    Lookup::Pending
}

fn worker() {
    loop {
        let key = {
            let mut st = SHARED.state.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if let Some(k) = st.queue.pop() {
                    break k;
                }
                st = SHARED.wake.wait(st).unwrap_or_else(|e| e.into_inner());
            }
        };
        // Um arquivo estranho não pode derrubar o app por causa de uma miniatura.
        let thumb = std::panic::catch_unwind(|| file_thumb::make(&key.path))
            .ok()
            .flatten()
            .filter(|t| !t.is_empty())
            .map(Arc::new);
        let ctx = {
            let mut st = SHARED.state.lock().unwrap_or_else(|e| e.into_inner());
            st.queued.remove(&key);
            st.done.insert(key.clone(), thumb);
            st.order.push_back(key);
            while st.order.len() > CACHE_MAX {
                if let Some(old) = st.order.pop_front() {
                    st.done.remove(&old);
                }
            }
            st.ctx.clone()
        };
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
    }
}
