//! Console do kernel: serial e framebuffer ao mesmo tempo.
//!
//! A serial e' o canal do QEMU e do CI; o framebuffer e' o canal do
//! hardware real. Tudo que o kernel imprime sai nos dois.

pub mod font;
pub mod framebuffer;
pub mod serial;

use crate::sync::Spinlock;
use core::fmt::{self, Write};

pub struct Console {
    serial: serial::Serial,
    text: Option<framebuffer::TextConsole>,
}

impl Console {
    pub const fn new() -> Self {
        Self { serial: serial::Serial::new(), text: None }
    }
}

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            // A serial precisa de CR antes do LF; o console de texto
            // trata CR como retorno de carro e nao deve receber um.
            if b == b'\n' {
                self.serial.write_byte(b'\r');
            }
            self.serial.write_byte(b);
            if let Some(t) = &mut self.text {
                t.write_byte(b);
            }
        }
        Ok(())
    }
}

pub static CONSOLE: Spinlock<Console> = Spinlock::new(Console::new());

pub fn init() {
    CONSOLE.lock().serial.init();
}

/// Liga o framebuffer ao console. A partir daqui tudo sai nos dois canais.
pub fn attach(text: framebuffer::TextConsole) {
    CONSOLE.lock().text = Some(text);
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    // Se o lock estiver ocupado, descarta em vez de girar: um console
    // nunca pode ser motivo de travamento do kernel.
    if let Some(mut c) = CONSOLE.try_lock() {
        let _ = c.write_fmt(args);
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::console::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    ()            => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}
