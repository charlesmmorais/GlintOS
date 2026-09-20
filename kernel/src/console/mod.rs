//! Console do kernel. No M0 o texto sai pela serial; o framebuffer e' sinal visual.

pub mod framebuffer;
pub mod serial;

use crate::sync::Spinlock;
use core::fmt::{self, Write};

pub static CONSOLE: Spinlock<serial::Serial> = Spinlock::new(serial::Serial::new());

pub fn init() {
    CONSOLE.lock().init();
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
    ()               => ($crate::print!("\n"));
    ($($arg:tt)*)    => ($crate::print!("{}\n", format_args!($($arg)*)));
}
