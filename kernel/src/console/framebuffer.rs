//! Framebuffer linear e console de texto sobre ele.
//!
//! No M0 o texto do kernel saia so' pela serial -- invisivel em hardware
//! real. Um console que fala por uma porta que ninguem escuta nao e' um
//! console, entao o texto passa a sair nos dois lugares.

use super::font::{self, GLYPH_H, GLYPH_W};

#[derive(Clone, Copy)]
pub struct Rgb(pub u8, pub u8, pub u8);

pub const BG: Rgb = Rgb(0x12, 0x12, 0x18);
pub const FG: Rgb = Rgb(0xd0, 0xd4, 0xdc);

/// Paleta da barra de topo -- a mesma do sinal de boot do M0.
const ACCENT: [Rgb; 4] = [
    Rgb(0x1c, 0x1c, 0x22),
    Rgb(0x2d, 0x4f, 0x6b),
    Rgb(0x3f, 0x8e, 0x8e),
    Rgb(0xd8, 0xa0, 0x57),
];

const BAR_H: usize = 6;
const MARGIN_X: usize = 6;
const MARGIN_Y: usize = 4;

pub struct Framebuffer {
    addr: *mut u8,
    width: usize,
    height: usize,
    pitch: usize,
    bpp_bytes: usize,
    red_shift: u32,
    green_shift: u32,
    blue_shift: u32,
}

unsafe impl Send for Framebuffer {}

impl Framebuffer {
    /// Devolve `None` para formatos que este codigo nao sabe pintar.
    /// Nesse caso o kernel segue normalmente -- so' pela serial.
    ///
    /// # Safety
    /// `addr` precisa ser o ponteiro valido entregue pelo bootloader.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn new(
        addr: *mut u8,
        width: usize,
        height: usize,
        pitch: usize,
        bpp: usize,
        red_shift: u32,
        green_shift: u32,
        blue_shift: u32,
    ) -> Option<Self> {
        if bpp != 32 || width == 0 || height == 0 {
            return None;
        }
        Some(Self {
            addr,
            width,
            height,
            pitch,
            bpp_bytes: bpp / 8,
            red_shift,
            green_shift,
            blue_shift,
        })
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Monta o pixel a partir das mascaras que o bootloader informou,
    /// em vez de assumir ordem de canais. Funciona em RGB e em BGR.
    fn pack(&self, c: Rgb) -> u32 {
        ((c.0 as u32) << self.red_shift)
            | ((c.1 as u32) << self.green_shift)
            | ((c.2 as u32) << self.blue_shift)
    }

    #[inline]
    fn put(&mut self, x: usize, y: usize, raw: u32) {
        if x >= self.width || y >= self.height {
            return;
        }
        let off = y * self.pitch + x * self.bpp_bytes;
        unsafe { core::ptr::write_volatile(self.addr.add(off) as *mut u32, raw) };
    }

    pub fn fill_rect(&mut self, x0: usize, y0: usize, w: usize, h: usize, c: Rgb) {
        let raw = self.pack(c);
        for y in y0..(y0 + h).min(self.height) {
            for x in x0..(x0 + w).min(self.width) {
                self.put(x, y, raw);
            }
        }
    }

    pub fn clear(&mut self, c: Rgb) {
        self.fill_rect(0, 0, self.width, self.height, c);
    }

    /// Sobe a regiao a partir de `top` em `amount` pixels.
    fn scroll_up(&mut self, top: usize, amount: usize, bg: Rgb) {
        if amount == 0 || top + amount >= self.height {
            return;
        }
        let row_bytes = self.width * self.bpp_bytes;
        for y in top..(self.height - amount) {
            let dst = y * self.pitch;
            let src = (y + amount) * self.pitch;
            unsafe {
                core::ptr::copy(self.addr.add(src), self.addr.add(dst), row_bytes);
            }
        }
        let cleared = self.height - amount;
        self.fill_rect(0, cleared, self.width, amount, bg);
    }
}

/// Console de texto com cursor e rolagem.
pub struct TextConsole {
    fb: Framebuffer,
    top: usize,
    cols: usize,
    rows: usize,
    cx: usize,
    cy: usize,
}

impl TextConsole {
    pub fn new(mut fb: Framebuffer) -> Self {
        let (w, h) = fb.dimensions();
        fb.clear(BG);

        // Barra de topo: a paleta do sinal de boot, agora como identidade.
        let seg = w / ACCENT.len();
        for (i, c) in ACCENT.iter().enumerate() {
            let x = i * seg;
            let width = if i + 1 == ACCENT.len() { w - x } else { seg };
            fb.fill_rect(x, 0, width, BAR_H, *c);
        }

        let top = BAR_H + MARGIN_Y;
        let cols = (w - MARGIN_X * 2) / GLYPH_W;
        let rows = (h - top - MARGIN_Y) / GLYPH_H;

        Self { fb, top, cols, rows, cx: 0, cy: 0 }
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    pub fn write_byte(&mut self, b: u8) {
        match b {
            b'\n' => self.newline(),
            b'\r' => self.cx = 0,
            b'\t' => {
                let next = (self.cx / 8 + 1) * 8;
                while self.cx < next.min(self.cols) {
                    self.draw(b' ');
                    self.cx += 1;
                }
                if self.cx >= self.cols {
                    self.newline();
                }
            }
            _ => {
                if self.cx >= self.cols {
                    self.newline();
                }
                self.draw(b);
                self.cx += 1;
            }
        }
    }

    fn newline(&mut self) {
        self.cx = 0;
        self.cy += 1;
        if self.cy >= self.rows {
            self.fb.scroll_up(self.top, GLYPH_H, BG);
            self.cy = self.rows - 1;
        }
    }

    fn draw(&mut self, b: u8) {
        let g = font::glyph(b);
        let px = MARGIN_X + self.cx * GLYPH_W;
        let py = self.top + self.cy * GLYPH_H;
        let fg = self.fb.pack(FG);
        let bg = self.fb.pack(BG);
        for (row, bits) in g.iter().enumerate() {
            for col in 0..GLYPH_W {
                let on = bits >> col & 1 != 0;
                self.fb.put(px + col, py + row, if on { fg } else { bg });
            }
        }
    }
}
