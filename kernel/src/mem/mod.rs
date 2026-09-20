//! Gerencia de memoria: quadros fisicos e heap do kernel.

pub mod heap;
pub mod phys;

use crate::sync::Spinlock;
use core::sync::atomic::{AtomicU64, Ordering};

pub static FRAMES: Spinlock<phys::FrameAllocator> = Spinlock::new(phys::FrameAllocator::new());

/// Deslocamento do mapeamento direto de memoria fisica (HHDM do Limine).
static HHDM_OFFSET: AtomicU64 = AtomicU64::new(0);

pub fn set_hhdm_offset(off: u64) {
    HHDM_OFFSET.store(off, Ordering::Relaxed);
}

pub fn hhdm_offset() -> u64 {
    HHDM_OFFSET.load(Ordering::Relaxed)
}

/// Converte endereco fisico para virtual pelo mapeamento direto.
pub fn phys_to_virt(phys: u64) -> u64 {
    phys + hhdm_offset()
}

pub struct MemStats {
    pub phys_total: u64,
    pub phys_free: u64,
    pub heap_used: usize,
    pub heap_free: usize,
}

pub fn stats() -> MemStats {
    let f = FRAMES.lock();
    MemStats {
        phys_total: f.total_bytes(),
        phys_free: f.free_bytes(),
        heap_used: heap::used_bytes(),
        heap_free: heap::free_bytes(),
    }
}
