//! ROMless, original 64 KiB SCC-I cartridges. Slot 1 only; see docs/scc.md.
use crate::scc::{Mode, Scc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SoundCartridgeProfile {
    Snatcher = 1,
    SdSnatcher = 2,
}
impl TryFrom<u32> for SoundCartridgeProfile {
    type Error = String;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Snatcher),
            2 => Ok(Self::SdSnatcher),
            _ => Err("Select the Snatcher or SD Snatcher 64 KiB sound cartridge.".into()),
        }
    }
}

pub(crate) struct SoundCartridge {
    pub profile: SoundCartridgeProfile,
    ram: Vec<u8>,
    banks: [u8; 4],
    mode: u8,
}
impl SoundCartridge {
    pub fn new(profile: SoundCartridgeProfile) -> Result<Self, String> {
        let mut ram = Vec::new();
        ram.try_reserve_exact(65536)
            .map_err(|_| "Not enough memory for sound cartridge RAM.".to_string())?;
        // Deterministic insertion policy, not a measurement of power-on RAM.
        ram.resize(65536, 0);
        Ok(Self {
            profile,
            ram,
            banks: [0, 1, 2, 3],
            mode: 0,
        })
    }
    pub fn reset(&mut self) {
        self.banks = [0, 1, 2, 3];
        self.mode = 0;
    }
    fn index(&self, address: u16) -> Option<usize> {
        let bank = self.banks[((address - 0x4000) / 0x2000) as usize] & 15;
        let first = if self.profile == SoundCartridgeProfile::Snatcher {
            0
        } else {
            8
        };
        (first..first + 8)
            .contains(&bank)
            .then(|| usize::from(bank - first) * 8192 + usize::from(address & 0x1fff))
    }
    fn overlay(&self, address: u16) -> bool {
        if self.mode & 0x20 != 0 {
            self.banks[3] & 0x80 != 0 && (0xb800..0xc000).contains(&address)
        } else {
            self.banks[2] & 0x3f == 0x3f && (0x9800..0xa000).contains(&address)
        }
    }
    pub fn read(&self, address: u16, scc: &mut Scc) -> u8 {
        if self.overlay(address) {
            scc.read(address)
        } else if (0x4000..0xc000).contains(&address) {
            self.index(address).map_or(0xff, |i| self.ram[i])
        } else {
            0xff
        }
    }
    pub fn write(&mut self, address: u16, value: u8, scc: &mut Scc) {
        if !(0x4000..0xc000).contains(&address) {
            return;
        }
        if address >= 0xbffe {
            self.mode = value;
            scc.set_mode(if value & 0x20 != 0 {
                Mode::Plus
            } else {
                Mode::Compatible
            });
            return;
        }
        let window = ((address - 0x4000) / 0x2000) as usize;
        let writable = self.mode & 0x10 != 0
            || match window {
                0 => self.mode & 1 != 0,
                1 => self.mode & 2 != 0,
                2 => self.mode & 0x24 == 0x24,
                _ => false,
            };
        if writable {
            // Provisional upstream policy: a readable sound overlay still writes
            // underlying populated RAM in RAM mode, never the sound registers.
            if let Some(i) = self.index(address) {
                self.ram[i] = value;
            }
        } else if address & 0x1800 == 0x1000 {
            self.banks[window] = value;
        } else if self.overlay(address) {
            scc.write(address, value);
        }
    }
}
