//! Independently written K051649 SCC and SCC-I. See docs/scc.md for hardware
//! references, test modes and the digital/analog accuracy boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Classic,
    Compatible,
    Plus,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scc {
    mode: Mode,
    waves: [[i8; 32]; 5],
    frequency: [u16; 5],
    volume: [u8; 5],
    enabled: u8,
    deformation: u8,
    count: [u32; 5],
    position: [u8; 5],
}

impl Scc {
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    /// SCC-I hardware reset retains programmed waves, periods, volumes and phase.
    pub fn reset_scci(&mut self) {
        self.mode = Mode::Compatible;
        self.enabled = 0;
        self.deformation = 0;
    }

    fn deform_address(&self, r: u8) -> bool {
        if self.mode == Mode::Classic {
            r >= 0xe0
        } else {
            (0xc0..0xe0).contains(&r)
        }
    }

    // Only the low eight address lines reach the sound register decoder.
    pub fn read(&mut self, address: u16) -> u8 {
        let r = address as u8;
        if self.deform_address(r) {
            self.deformation = 0xff;
            return 0xff;
        }
        let wave = match (self.mode, r) {
            (_, 0..=0x7f) | (Mode::Plus, 0x80..=0x9f) => Some((r / 32) as usize),
            (Mode::Compatible, 0xa0..=0xbf) => Some(4),
            _ => None,
        };
        if self.mode != Mode::Classic {
            return wave.map_or(0xff, |bank| {
                // Bit 7 does not disable SCC-I rotation/protection. The pinned
                // compatible read model still uses the raw latch to select the
                // wave-4 clock: voice 5 for 40h, voice 4 for C0h.
                let voice = if bank == 3
                    && self.mode == Mode::Compatible
                    && self.deformation & 0xc0 == 0x40
                {
                    4
                } else {
                    bank
                };
                let offset = if self.deformation & 0x40 != 0 {
                    self.position[voice]
                } else {
                    0
                };
                self.waves[bank][(r.wrapping_add(offset) & 31) as usize] as u8
            });
        }
        match r {
            0x00..=0x7f => {
                let bank = (r / 32) as usize;
                let rotating = if bank == 3 {
                    self.deformation & 0xc0 != 0 && self.deformation & 0xc0 != 0xc0
                } else {
                    self.deformation & 0x40 != 0
                };
                // Bit 6 clocks the shared wave with voice 5; bit 7 uses voice 4.
                let voice = if bank == 3 && self.deformation & 0x40 != 0 {
                    4
                } else {
                    bank
                };
                let offset = if rotating { self.position[voice] } else { 0 };
                self.waves[bank][(r.wrapping_sub(offset) & 31) as usize] as u8
            }
            0xe0..=0xff => {
                // Reading the test register also puts FF on its input latch.
                self.deformation = 0xff;
                0xff
            }
            _ => 0xff, // Frequencies, volumes and enable are write-only.
        }
    }

    pub fn write(&mut self, address: u16, value: u8) {
        let r = address as u8;
        if self.mode != Mode::Classic {
            if self.deform_address(r) {
                self.deformation = value;
            } else if r < if self.mode == Mode::Plus { 0xa0 } else { 0x80 } {
                if self.deformation & 0x40 == 0 {
                    let bank = (r / 32) as usize;
                    self.waves[bank][(r & 31) as usize] = value as i8;
                    if bank == 3 && self.mode == Mode::Compatible {
                        self.waves[4][(r & 31) as usize] = value as i8;
                    }
                }
            } else if (if self.mode == Mode::Plus {
                0xa0..0xc0
            } else {
                0x80..0xa0
            })
            .contains(&r)
            {
                self.write_control(r, value);
            }
            return;
        }
        match r {
            0x00..=0x7f => {
                let bank = (r / 32) as usize;
                if self.deformation & 0x40 == 0 && !(bank == 3 && self.deformation & 0x80 != 0) {
                    self.waves[bank][(r & 31) as usize] = value as i8;
                }
            }
            0x80..=0x9f => self.write_control(r, value),
            0xe0..=0xff => self.deformation = value,
            _ => {}
        }
    }

    fn write_control(&mut self, r: u8, value: u8) {
        match r & 15 {
            r @ 0..=9 => {
                let c = (r / 2) as usize;
                self.frequency[c] = if r & 1 == 0 {
                    (self.frequency[c] & 0xf00) | value as u16
                } else {
                    (self.frequency[c] & 0xff) | ((value as u16 & 15) << 8)
                };
                self.count[c] = 0;
                if self.deformation & 0x20 != 0 {
                    self.position[c] = 0;
                }
            }
            r @ 10..=14 => self.volume[(r - 10) as usize] = value & 15,
            _ => self.enabled = value & 31,
        }
    }

    fn period(&self, c: usize) -> u32 {
        let f = self.frequency[c];
        // Eight-bit mode takes priority when both divider bits are set.
        u32::from(if self.deformation & 2 != 0 {
            f & 255
        } else if self.deformation & 1 != 0 {
            f >> 8
        } else {
            f
        })
    }

    pub fn tick(&mut self, cycles: u32) {
        for c in 0..5 {
            let period = self.period(c);
            // The measured classic chip stops for division ratios 0..=8.
            if period > 8 {
                let elapsed = u64::from(self.count[c]) + u64::from(cycles);
                let divisor = u64::from(period + 1);
                self.position[c] = ((u64::from(self.position[c]) + elapsed / divisor) & 31) as u8;
                self.count[c] = (elapsed % divisor) as u32;
            }
        }
    }

    /// Centered digital DAC sum, normalized with at most 0.3 output gain.
    pub fn output(&self) -> f32 {
        let mut sum = 0i32;
        for c in 0..5 {
            if self.enabled & (1 << c) != 0 && self.period(c) > 8 {
                let bank = if self.mode == Mode::Classic {
                    c.min(3)
                } else {
                    c
                };
                let wave = self.waves[bank][self.position[c] as usize] as i32;
                // Arithmetic shift models the four discarded DAC product bits.
                sum += (wave * self.volume[c] as i32) >> 4;
            }
        }
        sum as f32 * (0.3 / 600.0)
    }
}

#[cfg(test)]
mod tests;
