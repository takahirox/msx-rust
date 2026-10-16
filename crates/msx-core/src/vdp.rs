//! TMS9918A ports, read-ahead, NTSC scanline timing and four documented modes.
pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 192;
pub const PALETTE: [[u8; 3]; 16] = [
    [0, 0, 0],
    [0, 0, 0],
    [33, 200, 66],
    [94, 220, 120],
    [84, 85, 237],
    [125, 118, 252],
    [212, 82, 77],
    [66, 235, 245],
    [252, 85, 84],
    [255, 121, 120],
    [212, 193, 84],
    [230, 206, 128],
    [33, 176, 59],
    [201, 91, 186],
    [204, 204, 204],
    [255, 255, 255],
];
pub struct Vdp {
    pub vram: [u8; 16384],
    pub registers: [u8; 8],
    pub pixels: Vec<u8>,
    pub status: u8,
    address: u16,
    latch: Option<u8>,
    buffer: u8,
    cycles: u32,
    pub line: u16,
    pub frames: u64,
}
impl Default for Vdp {
    fn default() -> Self {
        Self::with_pixels(vec![0; WIDTH * HEIGHT * 4])
    }
}
impl Vdp {
    fn with_pixels(pixels: Vec<u8>) -> Self {
        Self {
            vram: [0; 16384],
            registers: [0; 8],
            pixels,
            status: 0x1f,
            address: 0,
            latch: None,
            buffer: 0,
            cycles: 0,
            line: 0,
            frames: 0,
        }
    }
    pub(crate) fn reset(&mut self) {
        let mut pixels = std::mem::take(&mut self.pixels);
        pixels.fill(0);
        *self = Self::with_pixels(pixels);
    }
    pub fn irq(&self) -> bool {
        self.status & 0x80 != 0 && self.registers[1] & 0x20 != 0
    }
    pub fn control(&mut self, v: u8) {
        if let Some(lo) = self.latch.take() {
            if v & 0x80 != 0 {
                self.registers[(v & 7) as usize] = lo;
            } else {
                self.address = u16::from_le_bytes([lo, v & 0x3f]);
                if v & 0x40 == 0 {
                    self.buffer = self.vram[self.address as usize];
                    self.advance();
                }
            }
        } else {
            self.latch = Some(v);
        }
    }
    fn advance(&mut self) {
        self.address = (self.address + 1) & 0x3fff;
    }
    pub fn write(&mut self, v: u8) {
        self.latch = None;
        self.vram[self.address as usize] = v;
        self.buffer = v;
        self.advance();
    }
    pub fn read(&mut self) -> u8 {
        self.latch = None;
        let v = self.buffer;
        self.buffer = self.vram[self.address as usize];
        self.advance();
        v
    }
    pub fn read_status(&mut self) -> u8 {
        self.latch = None;
        let v = self.status;
        self.status &= 0x1f;
        v
    }
    fn at(&self, a: usize) -> u8 {
        self.vram[a & 0x3fff]
    }
    pub fn tick(&mut self, cycles: u32) {
        self.cycles += cycles;
        while self.cycles >= 228 {
            self.cycles -= 228;
            if self.line < 192 {
                self.render_line(self.line as usize);
            }
            self.line += 1;
            if self.line == 192 {
                self.status |= 0x80;
            }
            if self.line == 262 {
                self.line = 0;
                self.frames += 1;
            }
        }
    }
    /// Snapshot rendering for tools/tests; normal emulation renders at each scanline.
    pub fn render(&mut self) {
        for y in 0..HEIGHT {
            self.render_line(y);
        }
    }
    fn render_line(&mut self, y: usize) {
        let r = self.registers;
        let backdrop = r[7] & 15;
        let mut colors = [backdrop; WIDTH];
        let text = r[1] & 0x10 != 0;
        if r[1] & 0x40 != 0 {
            let name_base = ((r[2] & 15) as usize) << 10;
            let pattern_base = ((r[4] & 7) as usize) << 11;
            let color_base = (r[3] as usize) << 6;
            for (x, color) in colors.iter_mut().enumerate() {
                let (fg, bg, pattern) = if text {
                    if !(8..248).contains(&x) {
                        continue;
                    }
                    let c = self.at(name_base + (y / 8) * 40 + (x - 8) / 6) as usize;
                    (
                        r[7] >> 4,
                        backdrop,
                        self.at(pattern_base + c * 8 + y % 8) & (0x80 >> ((x - 8) % 6)) != 0,
                    )
                } else {
                    let c = self.at(name_base + (y / 8) * 32 + x / 8) as usize;
                    if r[1] & 8 != 0 {
                        let v = self.at(pattern_base + c * 8 + (y / 4) % 8);
                        (v >> 4, v & 15, x % 8 < 4)
                    } else if r[0] & 2 != 0 {
                        let offset = (y / 64) * 2048 + c * 8 + y % 8;
                        let pb = ((r[4] & 4) as usize) << 11;
                        let pm = (((r[4] & 3) as usize) << 11) | 0x7ff;
                        let cb = ((r[3] & 0x80) as usize) << 6;
                        let cm = (((r[3] & 0x7f) as usize) << 6) | 63;
                        let v = self.at(cb + (offset & cm));
                        (
                            v >> 4,
                            v & 15,
                            self.at(pb + (offset & pm)) & (0x80 >> (x % 8)) != 0,
                        )
                    } else {
                        let v = self.at(color_base + c / 8);
                        (
                            v >> 4,
                            v & 15,
                            self.at(pattern_base + c * 8 + y % 8) & (0x80 >> (x % 8)) != 0,
                        )
                    }
                };
                let v = if pattern { fg } else { bg };
                *color = if v == 0 { backdrop } else { v };
            }
            if !text {
                self.sprites(y, &mut colors);
            }
        }
        for (x, c) in colors.iter().enumerate() {
            let offset = (y * WIDTH + x) * 4;
            self.pixels[offset..offset + 3].copy_from_slice(&PALETTE[*c as usize]);
            self.pixels[offset + 3] = 255;
        }
    }
    fn sprites(&mut self, y: usize, colors: &mut [u8; WIDTH]) {
        let r = self.registers;
        let base = ((r[5] & 0x7f) as usize) << 7;
        let pattern_base = ((r[6] & 7) as usize) << 11;
        let size = if r[1] & 2 != 0 { 16 } else { 8 };
        let scale = if r[1] & 1 != 0 { 2 } else { 1 };
        let mut count = 0;
        let mut occupied = [false; WIDTH];
        let mut painted = [false; WIDTH];
        for n in 0..32 {
            let sy = self.at(base + n * 4);
            if sy == 208 {
                break;
            }
            let top = if sy > 224 {
                sy as i32 - 255
            } else {
                sy as i32 + 1
            };
            let dy = y as i32 - top;
            if dy < 0 || dy >= size * scale {
                continue;
            }
            count += 1;
            if count == 5 {
                if self.status & 0x40 == 0 {
                    self.status = (self.status & 0xa0) | 0x40 | n as u8;
                }
                break;
            }
            let attr = self.at(base + n * 4 + 3);
            let left = self.at(base + n * 4 + 1) as i32 - if attr & 0x80 != 0 { 32 } else { 0 };
            let tile = self.at(base + n * 4 + 2) as usize & if size == 16 { 0xfc } else { 0xff };
            let row = (dy / scale) as usize;
            for dx in 0..size * scale {
                let x = left + dx;
                if !(0..256).contains(&x) {
                    continue;
                }
                let col = (dx / scale) as usize;
                let offset = if size == 16 {
                    (col / 8) * 16 + (row / 8) * 8 + row % 8
                } else {
                    row
                };
                if self.at(pattern_base + tile * 8 + offset) & (0x80 >> (col % 8)) == 0 {
                    continue;
                }
                if occupied[x as usize] {
                    self.status |= 0x20;
                }
                occupied[x as usize] = true;
                if attr & 15 != 0 && !painted[x as usize] {
                    colors[x as usize] = attr & 15;
                    painted[x as usize] = true;
                }
            }
        }
    }
}
