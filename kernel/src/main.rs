//! GlintOS -- microkernel em que inferencia, KV cache e aceleradores sao
//! recursos de primeira classe do nucleo.
//!
//! M0: boota via Limine, assume o console, mapeia a memoria fisica e
//! exerce o controle de admissao contra o driver simulado.

#![no_std]
#![no_main]
// Os modulos de esqueleto declaram tipos que so serao usados a partir do M1.
#![allow(dead_code)]

extern crate alloc;

mod accel;
mod arch;
mod boot;
mod console;
mod journal;
mod kv;
mod mem;
mod sched;
mod session;
mod sync;

use core::panic::PanicInfo;

use boot::limine::{
    BaseRevision, EndMarker, FramebufferRequest, HhdmRequest, MemmapRequest, StartMarker,
    BASE_REVISION_REQUESTED, FRAMEBUFFER_ID, HHDM_ID, MEMMAP_ID, MEMMAP_USABLE,
};

use accel::{Accelerator, Batch, Simulated};
use journal::Event;
use session::{Budget, Refused, SessionId};

// --- Requisicoes ao bootloader -------------------------------------------
// Precisam ficar entre os marcadores, na secao .requests do linker script.

#[used]
#[link_section = ".requests"]
static BASE_REVISION: BaseRevision = BaseRevision::new(BASE_REVISION_REQUESTED);

#[used]
#[link_section = ".requests"]
static MEMMAP: MemmapRequest = MemmapRequest::new(MEMMAP_ID, 0);

#[used]
#[link_section = ".requests"]
static HHDM: HhdmRequest = HhdmRequest::new(HHDM_ID, 0);

#[used]
#[link_section = ".requests"]
static FRAMEBUFFER: FramebufferRequest = FramebufferRequest::new(FRAMEBUFFER_ID, 0);

#[used]
#[link_section = ".requests_start_marker"]
static REQ_START: StartMarker = StartMarker::new();

#[used]
#[link_section = ".requests_end_marker"]
static REQ_END: EndMarker = EndMarker::new();

// --- Entrada --------------------------------------------------------------

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    assert!(BASE_REVISION.is_supported());

    console::init();
    init_framebuffer();
    banner();

    init_memory();

    journal::record(0, Event::Boot);

    demo_admission();

    println!();
    println!("[glint] M0 concluido. Ocioso.");
    arch::target::halt_loop();
}

fn banner() {
    println!();
    println!("  GlintOS 0.1.0 -- marco M0");
    println!("  inferencia, KV cache e aceleradores como recursos de nucleo");
    println!();
}

// --- Memoria --------------------------------------------------------------

fn init_memory() {
    let hhdm = HHDM.response().expect("Limine nao devolveu HHDM");
    mem::set_hhdm_offset(hhdm.data.offset);

    let memmap = MEMMAP.response().expect("Limine nao devolveu mapa de memoria");

    // Escolhe a maior regiao utilizavel para o heap; o resto vira quadros.
    let mut heap_phys = 0u64;
    let mut best_len = 0u64;
    for entry in memmap.data.entries() {
        if entry.type_ == MEMMAP_USABLE && entry.length > best_len {
            heap_phys = entry.base;
            best_len = entry.length;
        }
    }

    assert!(
        best_len as usize > mem::heap::HEAP_SIZE,
        "nenhuma regiao utilizavel comporta o heap"
    );

    let heap_virt = mem::phys_to_virt(heap_phys) as *mut u8;
    unsafe { mem::heap::init(heap_virt, mem::heap::HEAP_SIZE) };

    let regions = {
        let mut frames = mem::FRAMES.lock();
        for entry in memmap.data.entries() {
            if entry.type_ != MEMMAP_USABLE {
                continue;
            }
            if entry.base == heap_phys {
                // Pula o pedaco cedido ao heap.
                let base = entry.base + mem::heap::HEAP_SIZE as u64;
                let len = entry.length - mem::heap::HEAP_SIZE as u64;
                frames.add_region(base, len);
            } else {
                frames.add_region(entry.base, entry.length);
            }
        }
        frames.region_count()
    };

    let s = mem::stats();
    let phys_mib = s.phys_free / (1024 * 1024);
    let heap_used_kib = s.heap_used / 1024;
    let heap_free_kib = s.heap_free / 1024;

    println!("[mem]  hhdm     0x{:016x}", mem::hhdm_offset());
    println!("[mem]  fisica   {} MiB livres em {} regioes", phys_mib, regions);
    println!("[mem]  heap     {} KiB usados / {} KiB livres", heap_used_kib, heap_free_kib);
}

fn init_framebuffer() {
    let Some(resp) = FRAMEBUFFER.response() else {
        println!("[fb]   ausente -- seguindo so na serial");
        return;
    };
    let fbs = resp.data.framebuffers();
    let Some(fb) = fbs.first() else {
        println!("[fb]   nenhum framebuffer");
        return;
    };

    let surface = unsafe {
        console::framebuffer::Framebuffer::new(
            fb.address,
            fb.width as usize,
            fb.height as usize,
            fb.pitch as usize,
            fb.bpp as usize,
            fb.red_mask_shift as u32,
            fb.green_mask_shift as u32,
            fb.blue_mask_shift as u32,
        )
    };

    let Some(surface) = surface else {
        println!("[fb]   {} bpp nao suportado -- seguindo so na serial", fb.bpp);
        return;
    };

    let (w, h) = surface.dimensions();
    let text = console::framebuffer::TextConsole::new(surface);
    let (cols, rows) = text.dimensions();
    console::attach(text);

    println!("[fb]   {}x{} -- console de texto {}x{}", w, h, cols, rows);
}

// --- Demonstracao do controle de admissao ---------------------------------

/// Exerce a tese na primeira tela: tres sessoes disputam um dispositivo
/// com teto conhecido, e a terceira e' RECUSADA na entrada, com motivo.
///
/// Principio 3: recusar e' melhor que enfileirar.
fn demo_admission() {
    // 8 ms por token, 2 KiB de KV por token, teto de 240 tokens/s.
    let mut accel = Simulated::new(8_000_000, 2048, 240);
    let cost = accel.cost_model();

    let mut sched = sched::Scheduler::new();
    sched.bind(&accel);

    println!(
        "[accel] simulado: {} ns/token, teto {} tok/s",
        cost.ns_per_token, cost.max_tokens_per_sec
    );
    println!();

    // (id, tokens, tokens/s pedidos, prazo em ns)
    let pedidos: [(SessionId, u32, u32, Option<u64>); 3] = [
        (1, 128, 100, Some(2_000_000_000)),
        (2, 64, 100, Some(1_000_000_000)),
        (3, 256, 100, Some(1_500_000_000)),
    ];

    for (id, tokens, want_tps, deadline) in pedidos {
        let budget = Budget::new(tokens, 10_000_000_000);
        match sched.admit(&budget, deadline, want_tps, &cost) {
            Ok(tps) => {
                journal::record(0, Event::SessionAdmitted { id, reserved_tps: tps });
                let uso = sched.utilization_pct();
                println!(
                    "[sched] sessao {} ADMITIDA   {} tokens, reserva {} tok/s, uso {}%",
                    id, tokens, tps, uso
                );
                let _ = accel.submit(Batch { session: id, tokens }, deadline.unwrap_or(u64::MAX));
            }
            Err(reason) => {
                journal::record(0, Event::SessionRefused { id, reason });
                println!("[sched] sessao {} RECUSADA   {}", id, motivo(reason));
            }
        }
    }

    let eventos = journal::JOURNAL.lock().len();
    println!();
    println!("[journal] {} eventos registrados", eventos);
}

/// Motivo sempre legivel. Nunca ha recusa silenciosa (secao 7).
fn motivo(r: Refused) -> &'static str {
    match r {
        Refused::DeadlineUnreachable => "prazo inalcancavel para o numero de tokens pedido",
        Refused::NoThroughputReserve => "sem reserva de vazao disponivel no dispositivo",
        Refused::InsufficientKvMemory => "memoria insuficiente para o KV minimo",
        Refused::WeightsUnavailable => "pesos nao residentes e sem via de carga",
    }
}

// --- Panico ---------------------------------------------------------------

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    arch::target::cli();
    println!();
    println!("[PANICO] {}", info);
    arch::target::halt_loop();
}
