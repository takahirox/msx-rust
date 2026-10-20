//! Independently written classic K051649 SCC. See docs/scc.md for hardware
//! references, test modes and the digital/analog accuracy boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scc {
    waves: [[i8; 32]; 4],
    frequency: [u16; 5],
    volume: [u8; 5],
    enabled: u8,
    deformation: u8,
    count: [u32; 5],
    position: [u8; 5],
}

impl Scc {
    // Only the low eight address lines reach the sound register decoder.
    pub fn read(&mut self, address: u16) -> u8 {
        let r = address as u8;
        match r {
            0x00..=0x7f => {
                let bank = (r / 32) as usize;
                let rotating = if bank == 3 {
                    self.deformation & 0xc0 != 0 && self.deformation & 0xc0 != 0xc0
                } else {
                    self.deformation & 0x40 != 0
                };
                let offset = if rotating { self.position[bank] } else { 0 };
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
        match r {
            0x00..=0x7f => {
                let bank = (r / 32) as usize;
                if self.deformation & 0x40 == 0 && !(bank == 3 && self.deformation & 0x80 != 0) {
                    self.waves[bank][(r & 31) as usize] = value as i8;
                }
            }
            0x80..=0x9f => match r & 15 {
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
            },
            0xe0..=0xff => self.deformation = value,
            _ => {} // Reserved; no fifth waveform or SCC+ mode registers.
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
                let wave = self.waves[c.min(3)][self.position[c] as usize] as i32;
                // Arithmetic shift models the four discarded DAC product bits.
                sum += (wave * self.volume[c] as i32) >> 4;
            }
        }
        sum as f32 * (0.3 / 600.0)
    }
}

#[cfg(test)]
mod tests;
