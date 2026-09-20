//! Journal append-only: toda decisao de admissao, escalonamento e despejo,
//! com a razao (secao 4.3).
//!
//! Entrega tres coisas de uma vez: auditoria, rollback por compensacao e
//! replay deterministico para depuracao -- voce nao reproduz o modelo,
//! reproduz o journal.
//!
//! I2: buffer de tamanho fixo, sem alocacao dinamica.

use crate::sync::Spinlock;

const CAPACITY: usize = 1024;

#[derive(Clone, Copy, Debug)]
pub enum Event {
    Boot,
    SessionAdmitted { id: u32, reserved_tps: u32 },
    SessionRefused { id: u32, reason: crate::session::Refused },
    SessionPreempted { id: u32, tokens_done: u32 },
    KvEvicted { session: u32, blocks: u32, score: u64 },
    SessionDone { id: u32, tokens: u32 },
    WatchdogFired { id: u32 },
}

#[derive(Clone, Copy)]
pub struct Entry {
    pub seq: u64,
    pub at_ns: u64,
    pub event: Event,
}

pub struct Journal {
    buf: [Option<Entry>; CAPACITY],
    head: usize,
    seq: u64,
}

impl Journal {
    pub const fn new() -> Self {
        Self { buf: [None; CAPACITY], head: 0, seq: 0 }
    }

    pub fn record(&mut self, at_ns: u64, event: Event) -> u64 {
        self.seq += 1;
        self.buf[self.head] = Some(Entry { seq: self.seq, at_ns, event });
        self.head = (self.head + 1) % CAPACITY;
        self.seq
    }

    /// Itera do mais antigo ao mais recente. Base do comando `glint why`.
    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        (0..CAPACITY)
            .map(move |i| &self.buf[(self.head + i) % CAPACITY])
            .filter_map(|e| e.as_ref())
    }

    pub fn len(&self) -> u64 {
        self.seq
    }

    pub fn is_empty(&self) -> bool {
        self.seq == 0
    }
}

pub static JOURNAL: Spinlock<Journal> = Spinlock::new(Journal::new());

pub fn record(at_ns: u64, event: Event) {
    if let Some(mut j) = JOURNAL.try_lock() {
        j.record(at_ns, event);
    }
}
