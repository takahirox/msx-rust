//! Cartridge ROM mappings, including classic SCC activation. SRAM is unsupported.

mod detect;
pub use detect::{detect_mapper, Candidate, Confidence, Detection};

/// Values are also used by the raw Wasm ABI and the browser mapper picker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum Mapper {
    #[default]
    Auto = 0,
    Plain = 1,
    Ascii8 = 2,
    Ascii16 = 3,
    Konami = 4,
    KonamiScc = 5,
}
impl Mapper {
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Plain => "Plain",
            Self::Ascii8 => "ASCII8",
            Self::Ascii16 => "ASCII16",
            Self::Konami => "Konami",
            Self::KonamiScc => "Konami SCC",
        }
    }
}
impl TryFrom<u32> for Mapper {
    type Error = String;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Auto),
            1 => Ok(Self::Plain),
            2 => Ok(Self::Ascii8),
            3 => Ok(Self::Ascii16),
            4 => Ok(Self::Konami),
            5 => Ok(Self::KonamiScc),
            _ => Err("Unsupported cartridge mapper. Select Plain, ASCII8, ASCII16, Konami or Konami SCC.".into()),
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct Mapping {
    pub kind: Mapper,
    banks: [u8; 4],
    scc_enabled: bool,
}
impl Mapping {
    pub fn new(kind: Mapper) -> Self {
        Self {
            kind,
            banks: match kind {
                Mapper::Konami | Mapper::KonamiScc => [0, 1, 2, 3],
                _ => [0; 4],
            },
            scc_enabled: false,
        }
    }
    pub fn validate(rom: &[u8], requested: Mapper) -> Result<Self, String> {
        let kind = if requested == Mapper::Auto {
            let detection = detect_mapper(rom);
            detection.mapper.ok_or(detection.explanation)?
        } else {
            requested
        };
        Self::validate_explicit(rom, kind)
    }
    fn validate_explicit(rom: &[u8], kind: Mapper) -> Result<Self, String> {
        let plain_size = [8192, 16384, 32768, 49152].contains(&rom.len());
        if kind == Mapper::Plain {
            if !plain_size {
                return Err("Plain mapping needs an 8, 16, 32 or 48 KiB image. Select the documented MegaROM mapper for bank-switched cartridges.".into());
            }
        } else {
            let (bank, max) = match kind {
                Mapper::Ascii16 => (16384, 4 * 1024 * 1024),
                Mapper::Ascii8 => (8192, 2 * 1024 * 1024),
                Mapper::Konami => (8192, 256 * 1024),
                Mapper::KonamiScc => (8192, 512 * 1024),
                _ => unreachable!(),
            };
            if rom.len() < bank || !rom.len().is_power_of_two() {
                return Err(format!("Invalid MegaROM image: this mapper needs a power-of-two number of bytes, at least {} KiB. Check the dump and mapper selection.", bank / 1024));
            }
            if rom.len() > max {
                return Err(format!("This image exceeds the selected mapper's {} KiB hardware address range. Choose the correct mapper; extended mapper variants are unsupported.", max / 1024));
            }
        }
        let mapping = Self::new(kind);
        if ![0x4000, 0x8000]
            .into_iter()
            .any(|a| mapping.read(rom, a) == b'A' && mapping.read(rom, a + 1) == b'B')
        {
            return Err("No AB cartridge header at CPU address 4000h or 8000h with this mapper's initial banks. Check the ROM dump and mapper selection.".into());
        }
        Ok(mapping)
    }
    pub fn read(&self, rom: &[u8], address: u16) -> u8 {
        if rom.is_empty() {
            return 0xff;
        }
        let a = address as usize;
        if self.kind == Mapper::Plain {
            return if rom.len() == 49152 {
                rom.get(a).copied().unwrap_or(0xff)
            } else if (0x4000..0xc000).contains(&a) {
                rom[(a - 0x4000) % rom.len()]
            } else {
                0xff
            };
        }
        // Konami families expose mirrors when slot 1 is selected in pages 0/3.
        let a = match self.kind {
            Mapper::Konami => match a {
                0..0x4000 => a + 0x4000,
                0xc000.. => a - 0x4000,
                _ => a,
            },
            Mapper::KonamiScc => match a {
                0..0x4000 => a + 0x8000,
                0xc000.. => a - 0x8000,
                _ => a,
            },
            _ if !(0x4000..0xc000).contains(&a) => return 0xff,
            _ => a,
        };
        // Hardware routes live SCC accesses to the chip. Static mapper inference
        // has no wave RAM: keep the overlay opaque rather than tracing hidden ROM.
        if self.scc_access(address) {
            return 0xff;
        }
        let (bank, offset) = if self.kind == Mapper::Ascii16 {
            (
                self.banks[(a - 0x4000) / 0x4000] as usize * 0x4000,
                a & 0x3fff,
            )
        } else {
            (
                self.banks[(a - 0x4000) / 0x2000] as usize * 0x2000,
                a & 0x1fff,
            )
        };
        rom[(bank + offset) & (rom.len() - 1)]
    }
    pub fn scc_access(&self, address: u16) -> bool {
        self.kind == Mapper::KonamiScc && self.scc_enabled && (0x9800..0xa000).contains(&address)
    }
    pub fn write(&mut self, address: u16, value: u8) {
        let index = match self.kind {
            Mapper::Ascii8 if (0x6000..0x8000).contains(&address) => {
                ((address - 0x6000) / 0x800) as usize
            }
            Mapper::Ascii16 if (0x6000..0x6800).contains(&address) => 0,
            Mapper::Ascii16 if (0x7000..0x7800).contains(&address) => 1,
            Mapper::Konami if (0x6000..0xc000).contains(&address) => {
                ((address - 0x4000) / 0x2000) as usize
            }
            Mapper::KonamiScc
                if (0x5000..0xb800).contains(&address) && address & 0x1800 == 0x1000 =>
            {
                let index = ((address - 0x4000) / 0x2000) as usize;
                if index == 2 {
                    self.scc_enabled = value & 0x3f == 0x3f;
                }
                index
            }
            _ => return,
        };
        self.banks[index] = match self.kind {
            Mapper::Konami => value & 0x1f,
            Mapper::KonamiScc => value & 0x3f,
            _ => value,
        };
    }
}
