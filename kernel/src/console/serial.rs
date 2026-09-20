//! UART 16550 em 0x3F8 (COM1). Console de texto do M0.

use crate::arch::target::{inb, outb};
use core::fmt;

const COM1: u16 = 0x3F8;

pub struct Serial {
    port: u16,
}

impl Serial {
    pub const fn new() -> Self {
        Self { port: COM1 }
    }

    pub fn init(&mut self) {
        unsafe {
            outb(self.port + 1, 0x00); // desliga interrupcoes
            outb(self.port + 3, 0x80); // habilita DLAB
            outb(self.port, 0x03);     // divisor baixo = 3 (38400 baud)
            outb(self.port + 1, 0x00); // divisor alto
            outb(self.port + 3, 0x03); // 8 bits, sem paridade, 1 stop
            outb(self.port + 2, 0xC7); // FIFO, limpa, threshold 14
            outb(self.port + 4, 0x0B); // IRQs ligadas, RTS/DSR
        }
    }

    fn can_send(&self) -> bool {
        unsafe { inb(self.port + 5) & 0x20 != 0 }
    }

    pub fn write_byte(&mut self, b: u8) {
        // I1: espera limitada por tentativas, nunca infinita.
        let mut spins = 0u32;
        while !self.can_send() {
            spins += 1;
            if spins > 100_000 {
                return; // desiste em vez de travar o kernel
            }
            core::hint::spin_loop();
        }
        unsafe { outb(self.port, b) };
    }
}

impl fmt::Write for Serial {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(b);
        }
        Ok(())
    }
}
