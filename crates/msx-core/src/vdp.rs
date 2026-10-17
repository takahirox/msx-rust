//! TMS9918A and V9938 ports, scanline video, palettes and command execution.
#[path = "vdp_command.rs"]
mod command;
use command::Command;
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
    pub vram: Vec<u8>,
    pub registers: [u8; 64],
    pub msx2: bool,
    pub palette: [[u8; 3]; 16],
    statuses: [u8; 10],
    palette_latch: Option<u8>,
    command: Option<Command>,
    command_cycles: u32,
    pub pixels: Vec<u8>,
    pub status: u8,
    address: usize,
    latch: Option<u8>,
    buffer: u8,
    cycles: u32,
    pub line: u16,
    pub frames: u64,
}
impl Default for Vdp {
    fn default() -> Self {
        Self::new(false)
    }
}
impl Vdp {
    pub fn new(msx2: bool) -> Self {
        let pixels = vec![
            0;
            if msx2 {
                512 * 424 * 4
            } else {
                WIDTH * HEIGHT * 4
            }
        ];
        Self::with_buffers(pixels, vec![0; if msx2 { 131072 } else { 16384 }], msx2)
    }
    fn with_buffers(pixels: Vec<u8>, vram: Vec<u8>, msx2: bool) -> Self {
        Self {
            vram,
            registers: [0; 64],
            msx2,
            palette: if msx2 { default_palette() } else { PALETTE },
            statuses: [0; 10],
            palette_latch: None,
            command: None,
            command_cycles: 0,
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
        let msx2 = self.msx2;
        let mut vram = std::mem::take(&mut self.vram);
        vram.fill(0);
        // Keep the large buffers stable across resets.
        *self = Self::with_buffers(pixels, vram, msx2);
    }
    pub fn irq(&self) -> bool {
        (self.status & 0x80 != 0 && self.registers[1] & 0x20 != 0)
            || (self.msx2 && self.statuses[1] & 1 != 0 && self.registers[0] & 0x10 != 0)
    }
    pub fn control(&mut self, v: u8) {
        if let Some(lo) = self.latch.take() {
            if v & 0x80 != 0 {
                self.set_register((v & if self.msx2 { 63 } else { 7 }) as usize, lo);
            } else {
                self.address = u16::from_le_bytes([lo, v & 0x3f]) as usize
                    | if self.msx2 {
                        ((self.registers[14] & 7) as usize) << 14
                    } else {
                        0
                    };
                if v & 0x40 == 0 {
                    self.buffer = if self.msx2 && self.registers[45] & 0x40 != 0 {
                        255
                    } else {
                        self.vram[self.address]
                    };
                    self.advance();
                }
            }
        } else {
            self.latch = Some(v);
        }
    }
    fn advance(&mut self) {
        let mask = if self.msx2 && self.mode() >= 4 {
            0x1ffff
        } else {
            0x3fff
        };
        let high = self.address & !mask;
        self.address = high | ((self.address + 1) & mask);
        if self.msx2 {
            self.registers[14] = (self.address >> 14) as u8;
        }
    }
    pub fn write(&mut self, v: u8) {
        self.latch = None;
        if !self.msx2 || self.registers[45] & 0x40 == 0 {
            self.vram[self.address] = v;
        }
        self.buffer = v;
        self.advance();
    }
    pub fn read(&mut self) -> u8 {
        self.latch = None;
        let v = self.buffer;
        self.buffer = if self.msx2 && self.registers[45] & 0x40 != 0 {
            255
        } else {
            self.vram[self.address]
        };
        self.advance();
        v
    }
    pub fn read_status(&mut self) -> u8 {
        self.latch = None;
        let selected = if self.msx2 {
            self.registers[15] & 15
        } else {
            0
        };
        match selected {
            0 => {
                let v = self.status;
                self.status &= 0x1f;
                v
            }
            1 => {
                let v = self.statuses[1];
                self.statuses[1] &= !1;
                v
            }
            2 => {
                self.statuses[2]
                    | if self.line >= self.visible_lines() as u16 {
                        0x40
                    } else {
                        0
                    }
                    | if self.cycles >= 171 { 0x20 } else { 0 }
                    | ((self.frames as u8 & 1) << 1)
                    | 0x0c
            }
            7 => self.command_read(),
            4 => self.statuses[4] | 0xfe,
            5 => {
                let v = self.statuses[5];
                self.statuses[3..7].fill(0);
                v
            }
            6 => self.statuses[6] | 0xfc,
            9 => self.statuses[9] | 0xfe,
            3 | 8 => self.statuses[selected as usize],
            _ => 255,
        }
    }
    fn at(&self, a: usize) -> u8 {
        self.vram[a & (self.vram.len() - 1)]
    }
    pub fn tick(&mut self, cycles: u32) {
        self.tick_command(cycles);
        self.cycles += cycles;
        while self.cycles >= 228 {
            self.cycles -= 228;
            if self.line < self.visible_lines() as u16 {
                self.render_line(self.line as usize);
            }
            self.line += 1;
            if self.line == self.visible_lines() as u16 {
                self.status |= 0x80;
            }
            if self.line
                == if self.msx2 && self.registers[9] & 2 != 0 {
                    313
                } else {
                    262
                }
            {
                self.line = 0;
                self.frames += 1;
            }
            if self.msx2
                && self.line < 256
                && ((self.line + self.registers[23] as u16) & 255) == self.registers[19] as u16
            {
                self.statuses[1] |= 1;
            }
        }
    }
    /// Snapshot rendering for tools/tests; normal emulation renders at each scanline.
    pub fn render(&mut self) {
        for y in 0..self.visible_lines() {
            self.render_line(y);
        }
    }
    fn render_line(&mut self, y: usize) {
        if self.msx2 {
            self.render_v9938_line(y);
            return;
        }
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
    fn sprites(&mut self, y: usize, colors: &mut [u8]) {
        let r = self.registers;
        let base = ((r[5] & if self.msx2 { 0xff } else { 0x7f }) as usize) << 7
            | if self.msx2 {
                ((r[11] & 3) as usize) << 15
            } else {
                0
            };
        let pattern_base = ((r[6] & if self.msx2 { 63 } else { 7 }) as usize) << 11;
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
                let visible = attr & 15 != 0 || (self.msx2 && r[8] & 0x20 != 0);
                if !self.msx2 || visible {
                    if occupied[x as usize] {
                        self.status |= 0x20;
                    }
                    occupied[x as usize] = true;
                }
                if visible && !painted[x as usize] {
                    colors[x as usize] = attr & 15;
                    painted[x as usize] = true;
                }
            }
        }
    }
}

// V9938 power-on palette, expressed as the documented three-bit DAC levels.
fn default_palette() -> [[u8; 3]; 16] {
    let rgb = [
        0x000, 0x000, 0x161, 0x373, 0x117, 0x237, 0x511, 0x267, 0x711, 0x733, 0x661, 0x664, 0x141,
        0x625, 0x555, 0x777,
    ];
    rgb.map(|v| [dac((v >> 8) as u8), dac((v >> 4) as u8), dac(v as u8)])
}
fn dac(v: u8) -> u8 {
    ((v & 7) as u16 * 255 / 7) as u8
}
fn direct_color(v: u8) -> [u8; 3] {
    [
        dac(v >> 2),
        dac(v >> 5),
        dac([0, 2, 4, 7][(v & 3) as usize]),
    ]
}

impl Vdp {
    /// BASIC screen number; 80-column text is represented by 9.
    pub fn mode(&self) -> u8 {
        if !self.msx2 {
            return if self.registers[1] & 0x10 != 0 { 0 } else { 1 };
        }
        match ((self.registers[0] >> 1) & 7, (self.registers[1] >> 3) & 3) {
            (2, 2) => 9,
            (0, 2) => 0,
            (0, 1) => 3,
            (1, 0) => 2,
            (2, 0) => 4,
            (3, 0) => 5,
            (4, 0) => 6,
            (5, 0) => 7,
            (7, 0) => 8,
            _ => 1,
        }
    }
    pub fn width(&self) -> usize {
        if matches!(self.mode(), 6 | 7 | 9) {
            512
        } else {
            256
        }
    }
    fn visible_lines(&self) -> usize {
        if self.msx2 && self.registers[9] & 0x80 != 0 {
            212
        } else {
            192
        }
    }
    pub fn height(&self) -> usize {
        self.visible_lines()
            * if self.msx2 && self.registers[9] & 8 != 0 {
                2
            } else {
                1
            }
    }
    pub fn frame_cycles(&self) -> i32 {
        228 * if self.msx2 && self.registers[9] & 2 != 0 {
            313
        } else {
            262
        }
    }
    pub fn set_register(&mut self, r: usize, value: u8) {
        if r >= 64 || (!self.msx2 && r >= 8) {
            return;
        }
        self.registers[r] = value;
        if r == 14 {
            self.registers[r] &= 7;
            self.address = (self.address & 0x3fff) | ((value as usize & 7) << 14);
        }
        if r == 16 {
            self.palette_latch = None;
        }
        if r == 44 {
            self.command_write(value);
        }
        if r == 46 {
            self.start_command(value);
        }
    }
    pub fn write_palette(&mut self, value: u8) {
        if !self.msx2 {
            return;
        }
        if let Some(rb) = self.palette_latch.take() {
            self.palette[(self.registers[16] & 15) as usize] = [dac(rb >> 4), dac(value), dac(rb)];
            self.registers[16] = (self.registers[16] + 1) & 15;
        } else {
            self.palette_latch = Some(value);
        }
    }
    pub fn write_indirect(&mut self, value: u8) {
        if !self.msx2 {
            return;
        }
        let p = self.registers[17];
        if p & 63 != 17 {
            self.set_register((p & 63) as usize, value);
        }
        if p & 0x80 == 0 {
            self.registers[17] = (p + 1) & 63;
        }
    }
    fn bitmap_layout(&self) -> (usize, usize) {
        match self.mode() {
            5 => (256, 4),
            6 => (512, 2),
            7 => (512, 4),
            _ => (256, 8),
        }
    }
    fn bitmap_address(&self, x: usize, y: usize) -> (usize, u8, u8) {
        let (width, bits) = self.bitmap_layout();
        let ppb = 8 / bits;
        (
            ((y * (width / ppb)) + (x % width) / ppb) & 0x1ffff,
            ((ppb - 1 - x % ppb) * bits) as u8,
            (255 >> (8 - bits)) as u8,
        )
    }
    fn point(&self, x: usize, y: usize) -> u8 {
        let (a, shift, mask) = self.bitmap_address(x, y);
        (self.at(a) >> shift) & mask
    }
    fn pset(&mut self, x: usize, y: usize, value: u8, op: u8) {
        let (a, shift, mask) = self.bitmap_address(x, y);
        let src = value & mask;
        if op & 8 != 0 && src == 0 {
            return;
        }
        let dest = (self.vram[a] >> shift) & mask;
        let v = match op & 7 {
            0 => src,
            1 => src & dest,
            2 => src | dest,
            3 => src ^ dest,
            4 => !src,
            _ => return,
        } & mask;
        self.vram[a] = (self.vram[a] & !(mask << shift)) | (v << shift);
    }
    fn render_v9938_line(&mut self, y: usize) {
        let r = self.registers;
        let mode = self.mode();
        let width = self.width();
        let mut colors = [r[7] & 15; 512];
        let adjust_x = ((r[18] & 15) as i8 ^ 8) - 8;
        let adjust_y = ((r[18] >> 4) as i8 ^ 8) - 8;
        let source_y = y as isize + adjust_y as isize;
        let vertical_display = (0..self.visible_lines() as isize).contains(&source_y);
        let sy = (source_y + r[23] as isize) as usize & 255;
        let field = self.frames as usize & 1;
        let mut page = (r[2] as usize & if mode <= 6 { 0x60 } else { 0x20 })
            << if mode <= 6 { 10 } else { 11 };
        // EO alternates the low page bit; IL weaves consecutive fields into one buffer.
        if matches!(mode, 5..=8)
            && (if r[9] & 4 != 0 {
                field == 0
            } else {
                self.blink_phase()
            })
        {
            page &= !if mode <= 6 { 0x8000 } else { 0x10000 };
        }
        if r[1] & 0x40 != 0 && vertical_display {
            for (x, color) in colors[..width].iter_mut().enumerate() {
                if matches!(mode, 5..=8) {
                    let (w, bits) = self.bitmap_layout();
                    *color = self.point(x, sy + page / (w * bits / 8));
                } else {
                    let text = mode == 0 || mode == 9;
                    let (columns, margin) = if mode == 9 { (80, 16) } else { (40, 8) };
                    if text && !(margin..width - margin).contains(&x) {
                        continue;
                    }
                    let name = (r[2] as usize & if mode == 9 { 0x7c } else { 0x7f }) << 10;
                    let tile = self.at(name
                        + (sy / 8) * if text { columns } else { 32 }
                        + if text { (x - margin) / 6 } else { x / 8 })
                        as usize;
                    let pb = (r[4] as usize & 63) << 11;
                    let cb = ((r[10] as usize & 7) << 14) | ((r[3] as usize) << 6);
                    let (fg, bg, bit) = if text {
                        let index = (sy / 8) * columns + (x - margin) / 6;
                        let blink = mode == 9
                            && self.at(cb + index / 8) & (0x80 >> (index % 8)) != 0
                            && self.blink_phase();
                        let c = if blink { r[12] } else { r[7] };
                        (
                            c >> 4,
                            c & 15,
                            self.at(pb + tile * 8 + sy % 8) & (0x80 >> ((x - margin) % 6)) != 0,
                        )
                    } else if mode == 3 {
                        let c = self.at(pb + tile * 8 + (sy / 4) % 8);
                        (c >> 4, c & 15, x % 8 < 4)
                    } else if mode == 2 || mode == 4 {
                        let offset = (sy / 64) * 2048 + tile * 8 + sy % 8;
                        let pm = (((r[4] & 3) as usize) << 11) | 0x7ff;
                        let cm = (((r[3] & 0x7f) as usize) << 6) | 63;
                        let c = self.at((cb & !0x1fff) + (offset & cm));
                        (
                            c >> 4,
                            c & 15,
                            self.at((pb & !0x1fff) + (offset & pm)) & (0x80 >> (x % 8)) != 0,
                        )
                    } else {
                        let c = self.at(cb + tile / 8);
                        (
                            c >> 4,
                            c & 15,
                            self.at(pb + tile * 8 + sy % 8) & (0x80 >> (x % 8)) != 0,
                        )
                    };
                    *color = if bit { fg } else { bg };
                }
            }
        }
        let mut sprite = [None; 256];
        if r[1] & 0x40 != 0 && vertical_display && mode != 0 && mode != 9 && r[8] & 2 == 0 {
            if mode >= 4 {
                self.sprites2(source_y as usize, &mut sprite);
            } else {
                let mut legacy = [16; 256];
                self.sprites(sy, &mut legacy);
                for (x, c) in legacy.iter().enumerate() {
                    if *c != 16 {
                        sprite[x] = Some(*c);
                    }
                }
            }
        }
        let output_y =
            y * if r[9] & 8 != 0 { 2 } else { 1 } + if r[9] & 8 != 0 { field } else { 0 };
        for x in 0..width {
            let source_x = x as isize + adjust_x as isize * (width / 256) as isize;
            let in_display =
                vertical_display && (0..width as isize).contains(&source_x) && r[1] & 0x40 != 0;
            let mut c = if in_display {
                colors[source_x as usize]
            } else {
                r[7]
            };
            let sp = if in_display {
                sprite[source_x as usize / (width / 256)]
            } else {
                None
            };
            let rgb = if let Some(s) = sp {
                if mode == 8 {
                    // Fixed three-bit RGB values in the V9938 sprite table.
                    let fixed = [
                        0x000, 0x002, 0x300, 0x302, 0x030, 0x032, 0x330, 0x332, 0x742, 0x007,
                        0x700, 0x707, 0x070, 0x077, 0x770, 0x777,
                    ];
                    let value = fixed[s as usize];
                    [
                        dac((value >> 8) as u8),
                        dac((value >> 4) as u8),
                        dac(value as u8),
                    ]
                } else if mode == 6 {
                    self.palette[if x & 1 == 0 { (s >> 2) & 3 } else { s & 3 } as usize]
                } else {
                    self.palette[s as usize]
                }
            } else if mode == 8 {
                direct_color(c)
            } else {
                let border = if mode == 6 {
                    if x & 1 == 0 {
                        (r[7] >> 2) & 3
                    } else {
                        r[7] & 3
                    }
                } else {
                    r[7] & 15
                };
                if mode == 6 {
                    c = if in_display { c & 3 } else { border };
                }
                if c == 0 && r[8] & 0x20 == 0 {
                    c = border;
                }
                self.palette[(c & 15) as usize]
            };
            let offset = (output_y * width + x) * 4;
            self.pixels[offset..offset + 3].copy_from_slice(&rgb);
            self.pixels[offset + 3] = 255;
        }
    }
    fn blink_phase(&self) -> bool {
        let on = (self.registers[13] >> 4) as u64 * 10;
        let off = (self.registers[13] & 15) as u64 * 10;
        on != 0 && (off == 0 || self.frames % (on + off) < on)
    }
    fn sprites2(&mut self, y: usize, colors: &mut [Option<u8>; 256]) {
        let r = self.registers;
        let base = ((r[11] as usize & 3) << 15) | ((r[5] as usize & 0xfc) << 7);
        let pattern = (r[6] as usize & 63) << 11;
        let size = if r[1] & 2 != 0 { 16 } else { 8 };
        let scale = if r[1] & 1 != 0 { 2 } else { 1 };
        let mut count = 0;
        let mut occupied = [false; 256];
        let mut owner = [32; 256];
        let mut group = None;
        for n in 0..32 {
            let sy = self.at(base + n * 4);
            if sy == 216 {
                break;
            }
            let dy = ((y + r[23] as usize).wrapping_sub(sy as usize + 1)) & 255;
            if dy >= size * scale {
                continue;
            }
            count += 1;
            if count == 9 {
                if self.status & 0x40 == 0 {
                    self.status = (self.status & 0xa0) | 0x40 | n as u8;
                }
                break;
            }
            let row = dy / scale;
            let attr = self.at((base & !0x3ff) + n * 16 + row);
            let combine = attr & 0x40 != 0;
            if !combine {
                group = Some(n);
            }
            let left = self.at(base + n * 4 + 1) as isize - if attr & 0x80 != 0 { 32 } else { 0 };
            let tile = self.at(base + n * 4 + 2) as usize & if size == 16 { 0xfc } else { 0xff };
            for dx in 0..size * scale {
                let x = left + dx as isize;
                if !(0..256).contains(&x) {
                    continue;
                }
                let col = dx / scale;
                let offset = if size == 16 {
                    (col / 8) * 16 + (row / 8) * 8 + row % 8
                } else {
                    row
                };
                if self.at(pattern + tile * 8 + offset) & (0x80 >> (col % 8)) == 0 {
                    continue;
                }
                let x = x as usize;
                if !combine && attr & 0x20 == 0 && (attr & 15 != 0 || r[8] & 0x20 != 0) {
                    if occupied[x] && self.status & 0x20 == 0 {
                        self.status |= 0x20;
                        let cx = x + 12;
                        let cy = y + 8;
                        self.statuses[3] = cx as u8;
                        self.statuses[4] = (cx >> 8) as u8;
                        self.statuses[5] = cy as u8;
                        self.statuses[6] = (cy >> 8) as u8;
                    }
                    occupied[x] = true;
                }
                if attr & 15 != 0 || r[8] & 0x20 != 0 {
                    if combine {
                        if let Some(group) = group {
                            if owner[x] == group {
                                colors[x] = Some(colors[x].unwrap_or(0) | (attr & 15));
                            } else if owner[x] == 32 {
                                colors[x] = Some(attr & 15);
                                owner[x] = group;
                            }
                        }
                    } else if owner[x] == 32 {
                        colors[x] = Some(attr & 15);
                        owner[x] = n;
                    }
                }
            }
        }
    }
}
