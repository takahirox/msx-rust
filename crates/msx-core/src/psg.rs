//! AY-3-8910: three tones, 17-bit noise, mixer, logarithmic levels and envelopes.
pub const SAMPLE_RATE: u32 = 44100;
pub const CPU_HZ: u32 = 3_579_545;
const LEVEL: [f32; 16] = [
    0.0, 0.009, 0.013, 0.019, 0.027, 0.040, 0.058, 0.085, 0.123, 0.178, 0.258, 0.375, 0.544, 0.689,
    0.838, 1.0,
];
pub struct Psg {
    pub registers: [u8; 16],
    pub selected: u8,
    tone_count: [u32; 3],
    tone: [bool; 3],
    noise_count: u32,
    noise: u32,
    envelope_count: u32,
    envelope_step: i8,
    envelope_attack: u8,
    envelope_hold: bool,
    divider: u32,
    sample_clock: u32,
    pub samples: Vec<f32>,
    dc: f32,
}
impl Default for Psg {
    fn default() -> Self {
        Self {
            registers: [0; 16],
            selected: 0,
            tone_count: [0; 3],
            tone: [false; 3],
            noise_count: 0,
            noise: 0x1ffff,
            envelope_count: 0,
            envelope_step: 15,
            envelope_attack: 0,
            envelope_hold: false,
            divider: 0,
            sample_clock: 0,
            samples: Vec::new(),
            dc: 0.0,
        }
    }
}
impl Psg {
    pub(crate) fn reset(&mut self) {
        let mut samples = std::mem::take(&mut self.samples);
        samples.clear();
        *self = Self::default();
        self.samples = samples;
    }
    pub fn select(&mut self, value: u8) {
        self.selected = value & 15;
    }
    pub fn write(&mut self, value: u8) {
        let r = self.selected as usize;
        let mask = match r {
            1 | 3 | 5 | 13 => 15,
            6 | 8 | 9 | 10 => 31,
            _ => 255,
        };
        self.registers[r] = value & mask;
        if r == 13 {
            self.envelope_step = 15;
            self.envelope_attack = if value & 4 != 0 { 15 } else { 0 };
            self.envelope_hold = false;
            self.envelope_count = 0;
        }
    }
    pub fn read(&self) -> u8 {
        self.registers[self.selected as usize]
    }
    pub fn envelope_level(&self) -> u8 {
        (self.envelope_step.max(0) as u8) ^ self.envelope_attack
    }
    fn clock(&mut self) {
        for c in 0..3 {
            let period =
                (self.registers[c * 2] as u32 | ((self.registers[c * 2 + 1] as u32) << 8)).max(1);
            self.tone_count[c] += 1;
            if self.tone_count[c] >= period {
                self.tone_count[c] = 0;
                self.tone[c] = !self.tone[c];
            }
        }
        self.noise_count += 1;
        if self.noise_count >= 2 * (self.registers[6] as u32).max(1) {
            self.noise_count = 0;
            let feedback = (self.noise ^ (self.noise >> 3)) & 1;
            self.noise = (self.noise >> 1) | (feedback << 16);
        }
        self.envelope_count += 1;
        let period = u16::from_le_bytes([self.registers[11], self.registers[12]]) as u32;
        if self.envelope_count >= 32 * period.max(1) {
            self.envelope_count = 0;
            if !self.envelope_hold {
                self.envelope_step -= 1;
                if self.envelope_step < 0 {
                    let shape = self.registers[13];
                    if shape & 8 == 0 {
                        self.envelope_step = 0;
                        self.envelope_attack = 0;
                        self.envelope_hold = true;
                    } else if shape & 1 != 0 {
                        if shape & 2 != 0 {
                            self.envelope_attack ^= 15;
                        }
                        self.envelope_step = 0;
                        self.envelope_hold = true;
                    } else {
                        if shape & 2 != 0 {
                            self.envelope_attack ^= 15;
                        }
                        self.envelope_step = 15;
                    }
                }
            }
        }
    }
    pub fn tick(&mut self, cycles: u32) {
        self.tick_with_scc(cycles, None);
    }
    pub(crate) fn tick_with_scc(
        &mut self,
        cycles: u32,
        mut scc: Option<(&mut crate::scc::Scc, bool)>,
    ) {
        let mut scc_cycles = 0;
        for _ in 0..cycles {
            scc_cycles += 1;
            self.divider += 1;
            if self.divider == 16 {
                self.divider = 0;
                self.clock();
            }
            self.sample_clock += SAMPLE_RATE;
            if self.sample_clock >= CPU_HZ {
                self.sample_clock -= CPU_HZ;
                let mut sample = 0.0;
                for c in 0..3 {
                    let mixer = self.registers[7];
                    let enabled = (mixer & (1 << c) != 0 || self.tone[c])
                        && (mixer & (8 << c) != 0 || self.noise & 1 != 0);
                    let volume = self.registers[8 + c];
                    let level = if volume & 16 != 0 {
                        self.envelope_level()
                    } else {
                        volume & 15
                    };
                    if enabled {
                        sample += LEVEL[level as usize] / 3.0;
                    }
                }
                // A simple DC blocker models AC-coupled output and avoids startup clicks.
                self.dc += 0.001 * (sample - self.dc);
                let mut mixed = (sample - self.dc) * 0.6;
                if let Some((chip, audible)) = scc.as_mut() {
                    chip.tick(scc_cycles);
                    if *audible {
                        mixed += chip.output();
                    }
                }
                scc_cycles = 0;
                self.samples.push(mixed);
            }
        }
        if let Some((chip, _)) = scc {
            chip.tick(scc_cycles);
        }
    }
}
