//! Heap do kernel.

use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// 8 MiB. Suficiente ate o KV cache virar objeto de kernel proprio (M1),
/// quando ele deixa de sair do heap e passa a ter alocador dedicado.
pub const HEAP_SIZE: usize = 8 * 1024 * 1024;

/// # Safety
/// `virt_start` precisa apontar para `size` bytes mapeados e nao usados.
pub unsafe fn init(virt_start: *mut u8, size: usize) {
    ALLOCATOR.lock().init(virt_start, size);
}

pub fn used_bytes() -> usize {
    ALLOCATOR.lock().used()
}

pub fn free_bytes() -> usize {
    ALLOCATOR.lock().free()
}
