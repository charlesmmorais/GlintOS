//! Alocador de quadros fisicos sobre o mapa de memoria do Limine.
//!
//! I2: nenhuma alocacao dinamica no caminho critico. As regioes vivem em
//! um array de tamanho fixo, reservado em tempo de compilacao.

pub const PAGE_SIZE: u64 = 4096;
const MAX_REGIONS: usize = 64;

#[derive(Clone, Copy, Default)]
pub struct Region {
    pub base: u64,
    pub len: u64,
}

pub struct FrameAllocator {
    regions: [Region; MAX_REGIONS],
    count: usize,
    cursor_region: usize,
    cursor_offset: u64,
    total_bytes: u64,
    allocated_bytes: u64,
}

impl FrameAllocator {
    pub const fn new() -> Self {
        Self {
            regions: [Region { base: 0, len: 0 }; MAX_REGIONS],
            count: 0,
            cursor_region: 0,
            cursor_offset: 0,
            total_bytes: 0,
            allocated_bytes: 0,
        }
    }

    pub fn add_region(&mut self, base: u64, len: u64) -> bool {
        if self.count >= MAX_REGIONS || len < PAGE_SIZE {
            return false;
        }
        let base = (base + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let len = len & !(PAGE_SIZE - 1);
        if len == 0 {
            return false;
        }
        self.regions[self.count] = Region { base, len };
        self.count += 1;
        self.total_bytes += len;
        true
    }

    /// Bump allocator. Suficiente para o M0; vira lista livre no M2,
    /// quando o despejo de KV passa a devolver quadros.
    pub fn alloc_frame(&mut self) -> Option<u64> {
        while self.cursor_region < self.count {
            let r = self.regions[self.cursor_region];
            if self.cursor_offset + PAGE_SIZE <= r.len {
                let addr = r.base + self.cursor_offset;
                self.cursor_offset += PAGE_SIZE;
                self.allocated_bytes += PAGE_SIZE;
                return Some(addr);
            }
            self.cursor_region += 1;
            self.cursor_offset = 0;
        }
        None
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    pub fn allocated_bytes(&self) -> u64 {
        self.allocated_bytes
    }

    pub fn free_bytes(&self) -> u64 {
        self.total_bytes - self.allocated_bytes
    }

    pub fn region_count(&self) -> usize {
        self.count
    }
}
