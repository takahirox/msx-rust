//! Original FM-PAC decoding authored from the behavioral references in
//! docs/fm-pac.md. The separately supplied firmware is never modified.
use crate::ym2413::Ym2413;

pub(crate) struct FmPac {
    rom: Vec<u8>,
    pub sram: Vec<u8>,
    pub chip: Ym2413,
    control: u8,
    bank: u8,
    unlock: [u8; 2],
}

impl FmPac {
    pub fn new(rom: Vec<u8>) -> Result<Self, String> {
        if rom.len() != 65536 {
            return Err("Original FM-PAC requires a separate 64 KiB firmware image.".into());
        }
        let mut sram = Vec::new();
        sram.try_reserve_exact(8190)
            .map_err(|_| "Not enough memory for FM-PAC SRAM.".to_string())?;
        sram.resize(8190, 0xff);
        Ok(Self {
            rom,
            sram,
            chip: Ym2413::default(),
            control: 0,
            bank: 0,
            unlock: [0; 2],
        })
    }
    pub fn reset(&mut self) {
        self.chip = Ym2413::default();
        self.control = 0;
        self.bank = 0;
        self.unlock = [0; 2];
    }
    fn unlocked(&self) -> bool {
        self.unlock == [0x4d, 0x69]
    }
    pub fn read(&self, address: u16) -> u8 {
        match address {
            0x7ff6 => self.control,
            0x7ff7 => self.bank,
            0x4000..=0x5ffd if self.unlocked() => self.sram[usize::from(address - 0x4000)],
            0x5ffe..=0x5fff if self.unlocked() => self.unlock[usize::from(address - 0x5ffe)],
            0x4000..=0x7fff if self.unlocked() => 0xff,
            0x4000..=0x7fff => {
                self.rom[usize::from(self.bank) * 16384 + usize::from(address - 0x4000)]
            }
            _ => 0xff,
        }
    }
    pub fn write(&mut self, address: u16, value: u8) {
        match address {
            0x5ffe..=0x5fff if self.control & 0x10 == 0 => {
                self.unlock[usize::from(address - 0x5ffe)] = value;
            }
            // Memory writes are independent of the I/O enable latch.
            0x7ff4 => self.chip.select(value),
            0x7ff5 => self.chip.write(value),
            0x7ff6 => {
                self.control = value & 0x11;
                if self.control & 0x10 != 0 {
                    self.unlock = [0; 2];
                }
            }
            0x7ff7 => self.bank = value & 3,
            0x4000..=0x5ffd if self.unlocked() => {
                self.sram[usize::from(address - 0x4000)] = value;
            }
            _ => {}
        }
    }
    pub fn output(&mut self, port: u8, value: u8) {
        if self.control & 1 != 0 {
            match port {
                0x7c => self.chip.select(value),
                0x7d => self.chip.write(value),
                _ => {}
            }
        }
    }
}
