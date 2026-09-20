//! Framebuffer linear entregue pelo Limine.
//!
//! No M0 serve apenas como sinal visual de boot. Texto no framebuffer
//! (necessario para a CLI do M2) exige uma fonte bitmap e ainda nao existe.

pub struct Framebuffer {
    addr: *mut u8,
    width: usize,
    height: usize,
    pitch: usize,
    bpp: usize,
}

unsafe impl Send for Framebuffer {}

impl Framebuffer {
    /// # Safety
    /// `addr` precisa ser o ponteiro valido entregue pelo bootloader.
    pub unsafe fn new(addr: *mut u8, width: usize, height: usize, pitch: usize, bpp: usize) -> Self {
        Self { addr, width, height, pitch, bpp }
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn fill(&mut self, color: u32) {
        let bytes = self.bpp / 8;
        for y in 0..self.height {
            for x in 0..self.width {
                let off = y * self.pitch + x * bytes;
                unsafe {
                    core::ptr::write_volatile(self.addr.add(off) as *mut u32, color);
                }
            }
        }
    }

    /// Barra de sinal de boot: prova visual de que o kernel assumiu o controle.
    pub fn boot_signal(&mut self) {
        let bytes = self.bpp / 8;
        let band = self.height / 16;
        let colors = [0x00_1c_1c_22u32, 0x00_2d_4f_6b, 0x00_3f_8e_8e, 0x00_d8_a0_57];
        for (i, color) in colors.iter().enumerate() {
            for y in (i * band)..((i + 1) * band) {
                for x in 0..self.width {
                    let off = y * self.pitch + x * bytes;
                    unsafe {
                        core::ptr::write_volatile(self.addr.add(off) as *mut u32, *color);
                    }
                }
            }
        }
    }
}
