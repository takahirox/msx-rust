//! Clocked YM2413 OPLL, adapted from emu2413 a1d5120 (MIT).
//! Copyright (C) 2026 Mitsutaka Okazaki. See third-party/emu2413-LICENSE.txt
//! and docs/msx-music.md for adaptation, timing and accuracy limits.
mod tables;
use tables::{AM_TABLE, EXP_TABLE, FULLSIN_TABLE, INSTRUMENTS};
const MULTIPLIER: [u32; 16] = [1, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 20, 24, 24, 30, 30];
const PM: [[i32; 8]; 8] = [
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 1, 0, 0, 0, -1, 0],
    [0, 1, 2, 1, 0, -1, -2, -1],
    [0, 1, 3, 1, 0, -1, -3, -1],
    [0, 2, 4, 2, 0, -2, -4, -2],
    [0, 2, 5, 2, 0, -2, -5, -2],
    [0, 3, 6, 3, 0, -3, -6, -3],
    [0, 3, 7, 3, 0, -3, -7, -3],
];
const EG_STEPS: [[u32; 8]; 4] = [
    [0, 1, 0, 1, 0, 1, 0, 1],
    [0, 1, 0, 1, 1, 1, 0, 1],
    [0, 1, 1, 1, 0, 1, 1, 1],
    [0, 1, 1, 1, 1, 1, 1, 1],
];
// Twice dB, in eighths; emu2413 truncates to integer before KL shifting.
const KEY_LEVEL: [i32; 16] = [
    0, 144, 192, 222, 240, 258, 270, 282, 288, 300, 306, 312, 318, 324, 330, 336,
];
#[derive(Clone, Copy, PartialEq, Eq)]
enum Envelope {
    Attack,
    Decay,
    Sustain,
    Release,
    Damp,
}
#[derive(Clone, Copy)]
struct Operator {
    phase: u32,
    pg: u32,
    envelope: u32,
    state: Envelope,
    key: bool,
    output: [i32; 2],
}
impl Default for Operator {
    fn default() -> Self {
        Self {
            phase: 0,
            pg: 0,
            envelope: 127,
            state: Envelope::Release,
            key: false,
            output: [0; 2],
        }
    }
}
#[derive(Clone)]
pub struct Ym2413 {
    address: u8,
    registers: [u8; 64],
    operators: [Operator; 18],
    divider: u32,
    clocks: u64,
    envelope_counter: u16,
    pm_phase: u32,
    am_phase: u32,
    noise: u32,
    output: i32,
}
impl Default for Ym2413 {
    fn default() -> Self {
        Self {
            address: 0,
            registers: [0; 64],
            operators: [Operator::default(); 18],
            divider: 0,
            clocks: 0,
            envelope_counter: 0,
            pm_phase: 0,
            am_phase: 0,
            noise: 1,
            output: 0,
        }
    }
}
impl Ym2413 {
    pub fn select(&mut self, address: u8) {
        self.address = address;
    }
    pub fn write(&mut self, value: u8) {
        let mut r = usize::from(self.address);
        if r >= 64 {
            return;
        }
        if matches!(r, 0x19..=0x1f | 0x29..=0x2f | 0x39..=0x3f) {
            r -= 9;
        }
        self.registers[r] = value;
        if r == 0x0e || (0x20..=0x28).contains(&r) {
            for i in 0..18 {
                let channel = i / 2;
                let rhythm_key = if self.rhythm() {
                    match i {
                        12 | 13 => value_for_bit(self.registers[14], 4),
                        14 => value_for_bit(self.registers[14], 0),
                        15 => value_for_bit(self.registers[14], 3),
                        16 => value_for_bit(self.registers[14], 2),
                        17 => value_for_bit(self.registers[14], 1),
                        _ => false,
                    }
                } else {
                    false
                };
                let key = self.registers[0x20 + channel] & 16 != 0 || rhythm_key;
                let single = self.single(i);
                let op = &mut self.operators[i];
                if key != op.key {
                    op.key = key;
                    if key {
                        op.state = Envelope::Damp;
                    } else if i & 1 != 0 || single {
                        op.state = Envelope::Release;
                    }
                }
            }
        }
    }
    fn rhythm(&self) -> bool {
        self.registers[14] & 32 != 0
    }
    fn single(&self, i: usize) -> bool {
        self.rhythm() && i >= 14
    }
    fn patch(&self, i: usize) -> [u8; 8] {
        let ch = i / 2;
        let instrument = if self.rhythm() && ch >= 6 {
            ch + 10
        } else {
            usize::from(self.registers[0x30 + ch] >> 4)
        };
        if instrument == 0 {
            self.registers[..8].try_into().unwrap()
        } else {
            INSTRUMENTS[instrument]
        }
    }
    /// Advance one chip clock per MSX CPU T-state. No CPU waits are inserted.
    pub fn tick(&mut self, cycles: u32) {
        self.clocks += u64::from(cycles);
        let total = u64::from(self.divider) + u64::from(cycles);
        self.divider = (total % 72) as u32;
        for _ in 0..total / 72 {
            self.clock();
        }
    }
    pub fn clocks(&self) -> u64 {
        self.clocks
    }
    pub fn internal_samples(&self) -> u64 {
        self.clocks / 72
    }
    /// Current clock/72 output, held until the next internal sample.
    pub fn output(&self) -> f32 {
        self.output as f32 / 131072.0
    }
    fn clock(&mut self) {
        let test = self.registers[15];
        if test & 2 != 0 {
            self.pm_phase = 0;
            self.am_phase = 0;
        } else {
            self.pm_phase = self
                .pm_phase
                .wrapping_add(if test & 8 != 0 { 1024 } else { 1 });
            // Bounded modulo avoids overflow changing the 210-entry AM pattern.
            self.am_phase = (self.am_phase + if test & 8 != 0 { 64 } else { 1 }) % (210 * 64);
        }
        let am = u32::from(AM_TABLE[(self.am_phase >> 6) as usize]);
        let h = self.operators[14].pg;
        let c = self.operators[17].pg;
        let short = ((h >> 2) ^ (h >> 7) | (h >> 3) ^ (c >> 5) | (c >> 3) ^ (c >> 5)) & 1 != 0;
        self.envelope_counter = self.envelope_counter.wrapping_add(1);
        for i in 0..18 {
            self.update_operator(i, test);
        }
        let mut mixed = 0;
        for ch in 0..if self.rhythm() { 6 } else { 9 } {
            mixed += -self.melody(ch, am) >> 1;
        }
        if self.rhythm() {
            mixed += self.melody(6, am);
        }
        self.noise_tick(14);
        if self.rhythm() {
            let hat = if short {
                if self.noise & 1 != 0 {
                    0x2d0
                } else {
                    0x234
                }
            } else if self.noise & 1 != 0 {
                0x34
            } else {
                0xd0
            };
            let snare = if self.operators[15].pg & 0x100 != 0 {
                if self.noise & 1 != 0 {
                    0x300
                } else {
                    0x200
                }
            } else if self.noise & 1 != 0 {
                0
            } else {
                0x100
            };
            mixed += self.linear(14, hat, 0) + self.linear(15, snare, 0);
        }
        self.noise_tick(2);
        if self.rhythm() {
            mixed += self.linear(16, self.operators[16].pg, 0)
                + self.linear(17, if short { 0x300 } else { 0x100 }, 0);
        }
        self.noise_tick(2);
        self.output = mixed;
    }
    fn noise_tick(&mut self, n: usize) {
        for _ in 0..n {
            if self.noise & 1 != 0 {
                self.noise ^= 0x800200;
            }
            self.noise >>= 1;
        }
    }
    fn update_operator(&mut self, i: usize, test: u8) {
        let p = self.patch(i);
        let side = i & 1;
        let ch = i / 2;
        let single = self.single(i);
        let carrier = side != 0 || single;
        let keep = self.rhythm() && (i == 14 || i == 17);
        let high = self.registers[0x20 + ch];
        let fnum = u32::from(self.registers[0x10 + ch]) | (u32::from(high & 1) << 8);
        let block = u32::from((high >> 1) & 7);
        let rks = if p[side] & 16 != 0 {
            block * 2 + (fnum >> 8)
        } else {
            block >> 1
        };
        let sustain = high & 32 != 0 && carrier;
        let patch_sustain = p[side] & 32 != 0;
        let attack = u32::from(p[4 + side] >> 4);
        let op = &mut self.operators[i];
        let parameter_rate = if !carrier && !op.key {
            0
        } else {
            match op.state {
                Envelope::Attack => attack,
                Envelope::Decay => u32::from(p[4 + side] & 15),
                Envelope::Sustain => {
                    if patch_sustain {
                        0
                    } else {
                        u32::from(p[6 + side] & 15)
                    }
                }
                Envelope::Release => {
                    if sustain {
                        5
                    } else if patch_sustain {
                        u32::from(p[6 + side] & 15)
                    } else {
                        7
                    }
                }
                Envelope::Damp => 12,
            }
        };
        let rate = if parameter_rate == 0 {
            0
        } else {
            (parameter_rate + (rks >> 2)).min(15)
        };
        let shift = if parameter_rate == 0 {
            0
        } else if op.state == Envelope::Attack {
            if rate > 0 && rate < 12 {
                13 - rate
            } else {
                0
            }
        } else {
            13_u32.saturating_sub(rate)
        };
        let counter = u32::from(self.envelope_counter);
        let mask = (1 << shift) - 1;
        let step = |index: u32| EG_STEPS[(rks & 3) as usize][(index & 7) as usize];
        if op.state == Envelope::Attack {
            if op.envelope > 0 && rate > 0 && (counter & mask & !3) == 0 {
                let s = match rate {
                    12..=14 => 16 - rate - step((counter & 12) >> 1),
                    0 | 15 => 0,
                    _ => {
                        if step(counter >> shift) != 0 {
                            4
                        } else {
                            0
                        }
                    }
                };
                if s > 0 {
                    op.envelope = op.envelope.saturating_sub((op.envelope >> s) + 1);
                }
            }
        } else if rate > 0 && counter & mask == 0 {
            let delta = match rate {
                13 => step(((counter & 12) >> 1) | (counter & 1)),
                14 => step((counter & 12) >> 1) + 1,
                15 => 2,
                _ => step(counter >> shift),
            };
            op.envelope = (op.envelope + delta).min(127);
        }
        let mut reset_buddy = false;
        match op.state {
            Envelope::Damp if op.envelope >= 123 && counter & mask == 0 => {
                if (attack + (rks >> 2)).min(15) == 15 {
                    op.state = Envelope::Decay;
                    op.envelope = 0;
                } else {
                    op.state = Envelope::Attack;
                }
                if carrier {
                    if !keep {
                        op.phase = 0;
                    }
                    reset_buddy = !single;
                }
            }
            Envelope::Attack if op.envelope == 0 => op.state = Envelope::Decay,
            Envelope::Decay if op.envelope >> 3 == u32::from(p[6 + side] >> 4) => {
                op.state = Envelope::Sustain
            }
            _ => {}
        }
        if test & 1 != 0 {
            op.envelope = 0;
        }
        if test & 4 != 0 {
            op.phase = 0;
        }
        let pm = if p[side] & 64 != 0 {
            PM[(fnum >> 6) as usize][((self.pm_phase >> 10) & 7) as usize]
        } else {
            0
        };
        let increment =
            ((((fnum * 2) as i32 + pm) as u32 * MULTIPLIER[(p[side] & 15) as usize]) << block) >> 2;
        op.phase = (op.phase + increment) & ((1 << 19) - 1);
        op.pg = op.phase >> 9;
        if reset_buddy {
            self.operators[i - 1].phase = 0;
        }
    }
    fn linear(&self, i: usize, phase: u32, am: u32) -> i32 {
        let op = &self.operators[i];
        if op.envelope > 123 {
            return 0;
        }
        let p = self.patch(i);
        let side = i & 1;
        let high = self.registers[0x20 + i / 2];
        let block = i32::from((high >> 1) & 7);
        let fnum_high = usize::from((self.registers[0x10 + i / 2] >> 5) | ((high & 1) << 3));
        let kl = if side == 0 { p[2] >> 6 } else { p[3] >> 6 };
        let key_level = if kl == 0 {
            0
        } else {
            let tmp = (KEY_LEVEL[fnum_high] / 8 - 6 * (7 - block)).max(0);
            ((tmp >> (3 - kl)) * 8 / 3) as u32
        };
        let volume = if side != 0 {
            u32::from(self.registers[0x30 + i / 2] & 15) * 4
        } else if self.single(i) {
            u32::from(self.registers[0x30 + i / 2] >> 4) * 4
        } else {
            u32::from(p[2] & 63)
        };
        let att = (op.envelope + key_level + volume * 2 + am).min(127) << 4;
        let phase = phase & 1023;
        let half = p[3] & (if side == 0 { 8 } else { 16 }) != 0;
        let sine = if half && phase >= 512 {
            0xfff
        } else {
            let q = phase & 511;
            FULLSIN_TABLE[if q >= 256 { 511 - q } else { q } as usize]
                | if phase >= 512 { 0x8000 } else { 0 }
        };
        let log = u32::from(sine) + att;
        let value =
            (i32::from(EXP_TABLE[((log & 255) ^ 255) as usize]) + 1024) >> ((log & 0x7f00) >> 8);
        (if log & 0x8000 != 0 { !value } else { value }) * 2
    }
    fn melody(&mut self, ch: usize, am: u32) -> i32 {
        let i = ch * 2;
        let p = self.patch(i);
        let fb = p[3] & 7;
        let op = &self.operators[i];
        let feedback = if fb == 0 {
            0
        } else {
            (op.output[0] + op.output[1]) >> (9 - fb)
        };
        let modulation = self.linear(
            i,
            op.pg.wrapping_add_signed(feedback),
            if p[0] & 128 != 0 { am } else { 0 },
        );
        self.operators[i].output = [modulation, self.operators[i].output[0]];
        let value = self.linear(
            i + 1,
            self.operators[i + 1]
                .pg
                .wrapping_add_signed(2 * (modulation >> 1)),
            if p[1] & 128 != 0 { am } else { 0 },
        );
        self.operators[i + 1].output = [value, self.operators[i + 1].output[0]];
        value
    }
}
fn value_for_bit(value: u8, bit: u8) -> bool {
    value & (1 << bit) != 0
}

#[cfg(test)]
mod tests;
