//! Primitivas de x86_64 usadas no M0: I/O de porta e parada.

use core::arch::asm;

/// # Safety
/// Escrever em porta arbitraria pode afetar hardware.
pub unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

/// # Safety
/// Ler porta arbitraria pode ter efeito colateral em hardware.
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
    value
}

/// Para o processador ate a proxima interrupcao. Laco de ociosidade.
pub fn halt_loop() -> ! {
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

/// Desabilita interrupcoes.
pub fn cli() {
    unsafe { asm!("cli", options(nomem, nostack)) };
}
