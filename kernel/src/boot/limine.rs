//! Protocolo Limine, implementado a mao.
//!
//! A crate `limine` exige nightly (`feature(ptr_metadata)`). Como o M0 e' um
//! punhado de structs `repr(C)` e magic numbers publicados, implementar aqui
//! mantem o kernel em Rust estavel e remove uma dependencia -- que era
//! justamente o ponto de maior risco do build.
//!
//! Referencia: PROTOCOL.md do limine-bootloader.

use core::cell::UnsafeCell;

pub const COMMON_MAGIC: [u64; 2] = [0xc7b1dd30df4c8b88, 0x0a82e883a194f07b];

// --- Revisao base ---------------------------------------------------------

/// Revisao base pedida ao bootloader.
///
/// 2 e' amplamente suportada (Limine 7+) e tem semantica estavel: ponteiros
/// de resposta virtuais e HHDM disponivel. Pode ser elevada depois.
pub const BASE_REVISION_REQUESTED: u64 = 2;

#[repr(C)]
pub struct BaseRevision {
    magic: UnsafeCell<[u64; 3]>,
}

unsafe impl Sync for BaseRevision {}

impl BaseRevision {
    pub const fn new(revision: u64) -> Self {
        Self {
            magic: UnsafeCell::new([0xf9562b2d5c95a6c8, 0x6a7b384944536bdc, revision]),
        }
    }

    /// O bootloader zera a terceira palavra quando aceita a revisao pedida.
    pub fn is_supported(&self) -> bool {
        unsafe { (self.magic.get() as *const u64).add(2).read_volatile() == 0 }
    }
}

// --- Marcadores -----------------------------------------------------------

#[repr(C)]
pub struct StartMarker([u64; 4]);

impl StartMarker {
    pub const fn new() -> Self {
        Self([
            0xf6b8f4b39de7d1ae,
            0xfab91a6940fcb9cf,
            0x785c6ed015d3e316,
            0x181e920a7852b9d9,
        ])
    }
}

#[repr(C)]
pub struct EndMarker([u64; 2]);

impl EndMarker {
    pub const fn new() -> Self {
        Self([0xadc0e0531bb10d03, 0x9572709f31764c62])
    }
}

// --- Requisicao e resposta ------------------------------------------------

#[repr(C)]
pub struct Response<T> {
    pub revision: u64,
    pub data: T,
}

#[repr(C)]
pub struct Request<T> {
    magic: [u64; 2],
    id: [u64; 2],
    revision: u64,
    response: UnsafeCell<*mut Response<T>>,
}

unsafe impl<T> Sync for Request<T> {}
unsafe impl<T> Send for Request<T> {}

impl<T> Request<T> {
    pub const fn new(id: [u64; 2], revision: u64) -> Self {
        Self {
            magic: COMMON_MAGIC,
            id,
            revision,
            response: UnsafeCell::new(core::ptr::null_mut()),
        }
    }

    /// Leitura volatil: o bootloader preenche isto depois da carga.
    pub fn response(&self) -> Option<&'static Response<T>> {
        let p = unsafe { self.response.get().read_volatile() };
        if p.is_null() {
            None
        } else {
            Some(unsafe { &*p })
        }
    }
}

// --- Mapa de memoria ------------------------------------------------------

pub const MEMMAP_USABLE: u64 = 0;
pub const MEMMAP_RESERVED: u64 = 1;
pub const MEMMAP_ACPI_RECLAIMABLE: u64 = 2;
pub const MEMMAP_ACPI_NVS: u64 = 3;
pub const MEMMAP_BAD_MEMORY: u64 = 4;
pub const MEMMAP_BOOTLOADER_RECLAIMABLE: u64 = 5;
pub const MEMMAP_EXECUTABLE_AND_MODULES: u64 = 6;
pub const MEMMAP_FRAMEBUFFER: u64 = 7;

#[repr(C)]
pub struct MemmapEntry {
    pub base: u64,
    pub length: u64,
    pub type_: u64,
}

#[repr(C)]
pub struct MemmapRespData {
    entry_count: u64,
    entries: *const *const MemmapEntry,
}

impl MemmapRespData {
    pub fn entries(&self) -> &'static [&'static MemmapEntry] {
        unsafe {
            core::slice::from_raw_parts(
                self.entries as *const &'static MemmapEntry,
                self.entry_count as usize,
            )
        }
    }
}

pub const MEMMAP_ID: [u64; 2] = [0x67cf3d9d378a806f, 0xe304acdfc50c3c62];
pub type MemmapRequest = Request<MemmapRespData>;

// --- HHDM -----------------------------------------------------------------

#[repr(C)]
pub struct HhdmRespData {
    pub offset: u64,
}

pub const HHDM_ID: [u64; 2] = [0x48dcf1cb8ad2b852, 0x63984e959a98244b];
pub type HhdmRequest = Request<HhdmRespData>;

// --- Framebuffer ----------------------------------------------------------

#[repr(C)]
pub struct FramebufferInfo {
    pub address: *mut u8,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub memory_model: u8,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
    _resvd: [u8; 7],
    edid_size: u64,
    edid: *const u8,
}

#[repr(C)]
pub struct FramebufferRespData {
    count: u64,
    framebuffers: *const *const FramebufferInfo,
}

impl FramebufferRespData {
    pub fn framebuffers(&self) -> &'static [&'static FramebufferInfo] {
        unsafe {
            core::slice::from_raw_parts(
                self.framebuffers as *const &'static FramebufferInfo,
                self.count as usize,
            )
        }
    }
}

pub const FRAMEBUFFER_ID: [u64; 2] = [0x9d5827dcd881dd75, 0xa3148604f6fab11b];
pub type FramebufferRequest = Request<FramebufferRespData>;
