//! Platform-independent, original MSX1/MSX2 implementation. No browser dependencies.
pub mod basic;
pub mod cartridge;
pub mod cpu;
pub use cartridge::Mapper;
use cartridge::Mapping;
pub mod firmware;
pub mod font;
pub mod psg;
pub mod rtc;
pub mod vdp;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Model {
    #[default]
    Msx1,
    Msx2,
}
use basic::Basic;
use cpu::{Bus, Cpu};
use psg::Psg;
use std::collections::VecDeque;
use vdp::Vdp;

pub struct Hardware {
    pub model: Model,
    pub subrom: Vec<u8>,
    pub secondary: [u8; 4],
    pub rtc: rtc::Rtc,
    pub ram: Vec<u8>,
    pub bios: Vec<u8>,
    pub cartridge: Vec<u8>,
    mapping: Mapping,
    pub slots: u8,
    pub ppi_c: u8,
    pub ppi_control: u8,
    pub keyboard: [u8; 11],
    pub joystick: u8,
    pub vdp: Vdp,
    pub psg: Psg,
    pub builtin: bool,
    keys: VecDeque<u8>,
    services: VecDeque<u8>,
}
impl Default for Hardware {
    fn default() -> Self {
        Self {
            model: Model::Msx1,
            subrom: Vec::new(),
            secondary: [0; 4],
            rtc: rtc::Rtc::default(),
            ram: vec![0; 65536],
            bios: firmware::bios(),
            cartridge: Vec::new(),
            mapping: Mapping::default(),
            slots: 0xf0,
            ppi_c: 0,
            ppi_control: 0x82,
            keyboard: [255; 11],
            joystick: 0x3f,
            vdp: Vdp::default(),
            psg: Psg::default(),
            builtin: true,
            keys: VecDeque::new(),
            services: VecDeque::new(),
        }
    }
}
impl Hardware {
    /// Reset in place so installing an already reserved cartridge cannot run out
    /// of memory while allocating replacement RAM, firmware or video buffers.
    fn reset(&mut self) {
        self.ram.fill(0);
        self.mapping = Mapping::new(self.mapping.kind);
        self.slots = 0xf0;
        self.secondary = [0; 4];
        if self.model == Model::Msx2 && self.builtin {
            self.secondary[3] = 0xff;
        }
        self.rtc.selected = 0;
        self.ppi_c = 0;
        self.ppi_control = 0x82;
        self.release_keys();
        self.vdp.reset();
        self.psg.reset();
        self.keys.clear();
        self.services.clear();
        // A real BIOS probes slots itself. Our firmware boots AB cartridges in their initial banks.
        if self.builtin && !self.cartridge.is_empty() {
            self.slots = 0xd4;
        }
    }
    pub fn key(&mut self, row: usize, bit: u8, down: bool) {
        if row < 11 && bit < 8 {
            if down {
                self.keyboard[row] &= !(1 << bit);
            } else {
                self.keyboard[row] |= 1 << bit;
            }
        }
    }
    pub fn release_keys(&mut self) {
        self.keyboard.fill(255);
        self.joystick = 0x3f;
    }
}
impl Hardware {
    fn slot(&self, address: u16) -> (u8, u8) {
        let page = (address >> 14) * 2;
        let primary = (self.slots >> page) & 3;
        let secondary = if self.model == Model::Msx2 && primary == 3 {
            (self.secondary[3] >> page) & 3
        } else {
            0
        };
        (primary, secondary)
    }
}
impl Bus for Hardware {
    fn read(&mut self, address: u16) -> u8 {
        if address == 0xffff && self.model == Model::Msx2 && self.slots >> 6 == 3 {
            return !self.secondary[3];
        }
        let a = address as usize;
        match self.slot(address) {
            (0, 0) => self.bios.get(a).copied().unwrap_or(255),
            (1, 0) => self.mapping.read(&self.cartridge, address),
            (3, 0) if self.model == Model::Msx2 => self.subrom.get(a).copied().unwrap_or(255),
            (3, sub) if self.model == Model::Msx1 || sub == 3 => self.ram[a],
            _ => 255,
        }
    }
    fn write(&mut self, address: u16, value: u8) {
        if address == 0xffff && self.model == Model::Msx2 && self.slots >> 6 == 3 {
            self.secondary[3] = value;
            return;
        }
        match self.slot(address) {
            (1, 0) => self.mapping.write(address, value),
            (3, sub) if self.model == Model::Msx1 || sub == 3 => self.ram[address as usize] = value,
            _ => {}
        }
    }
    fn input(&mut self, port: u16) -> u8 {
        match port as u8 {
            0x98 => self.vdp.read(),
            0x99 => self.vdp.read_status(),
            0xa2 => {
                if self.psg.selected == 14 {
                    if self.psg.registers[15] & 0x40 == 0 {
                        self.joystick | 0x40
                    } else {
                        0x7f
                    }
                } else {
                    self.psg.read()
                }
            }
            0xb5 if self.model == Model::Msx2 => self.rtc.read(),
            0xa8 => self.slots,
            0xa9 => self
                .keyboard
                .get((self.ppi_c & 15) as usize)
                .copied()
                .unwrap_or(255),
            0xaa => self.ppi_c,
            0xab => self.ppi_control,
            0xf0 if self.builtin && self.model == Model::Msx1 => self.keys.pop_front().unwrap_or(0),
            _ => 255,
        }
    }
    fn output(&mut self, port: u16, value: u8) {
        match port as u8 {
            0x98 => self.vdp.write(value),
            0x99 => self.vdp.control(value),
            0x9a => self.vdp.write_palette(value),
            0x9b => self.vdp.write_indirect(value),
            0xb4 if self.model == Model::Msx2 => self.rtc.selected = value & 15,
            0xb5 if self.model == Model::Msx2 => self.rtc.write(value),
            0xa0 => self.psg.select(value),
            0xa1 => self.psg.write(value),
            0xa8 => self.slots = value,
            0xaa => self.ppi_c = value,
            0xab => {
                if value & 0x80 != 0 {
                    self.ppi_control = value;
                } else {
                    let bit = 1 << ((value >> 1) & 7);
                    if value & 1 != 0 {
                        self.ppi_c |= bit;
                    } else {
                        self.ppi_c &= !bit;
                    }
                }
            }
            0xf1 if self.builtin && self.model == Model::Msx1 => self.services.push_back(value),
            _ => {}
        }
    }
    fn irq(&self) -> bool {
        self.vdp.irq()
    }
}
pub struct Machine {
    pub cpu: Cpu,
    pub hardware: Hardware,
    pub basic: Basic,
    frame_debt: i32,
}
impl Default for Machine {
    fn default() -> Self {
        Self {
            cpu: Cpu::new(),
            hardware: Hardware::default(),
            basic: Basic::default(),
            frame_debt: 0,
        }
    }
}
impl Machine {
    /// Switching profiles releases incompatible firmware and cartridge images.
    pub fn set_model(&mut self, model: Model) {
        self.hardware.model = model;
        self.hardware.vdp = Vdp::new(model == Model::Msx2);
        self.hardware.rtc = rtc::Rtc::default();
        self.use_builtin();
    }
    /// Install a complete MSX2 firmware set atomically. No component is committed
    /// to the running machine until both lengths have been validated.
    pub fn load_msx2_firmware(&mut self, main: &[u8], sub: &[u8]) -> Result<(), String> {
        if self.hardware.model != Model::Msx2 {
            return Err("Select MSX2 before loading its main BIOS and sub-ROM.".into());
        }
        if main.len() != 32768 || sub.len() != 16384 {
            return Err(
                "MSX2 requires a 32 KiB main BIOS/BASIC and a separate 16 KiB sub-ROM.".into(),
            );
        }
        if main[0x2d] != 1 || &sub[..2] != b"CD" {
            return Err("Expected an MSX2 main ROM (generation byte 002Dh = 1) and a CD-header MSX2 sub-ROM. Select a matching firmware pair.".into());
        }
        self.hardware.bios = main.to_vec();
        self.hardware.subrom = sub.to_vec();
        self.hardware.builtin = false;
        self.reset();
        Ok(())
    }
    pub fn reset(&mut self) {
        self.hardware.reset();
        self.cpu = Cpu::new();
        self.basic = Basic::default();
        self.frame_debt = 0;
    }
    pub fn load_cartridge(&mut self, rom: &[u8]) -> Result<(), String> {
        self.load_cartridge_with_mapper(rom, Mapper::Auto)
    }
    pub fn load_cartridge_with_mapper(&mut self, rom: &[u8], mapper: Mapper) -> Result<(), String> {
        Mapping::validate(rom, mapper)?;
        let mut owned = Vec::new();
        owned.try_reserve_exact(rom.len()).map_err(|_| {
            "Not enough memory for this cartridge. Close other tabs or try a smaller image."
                .to_string()
        })?;
        owned.extend_from_slice(rom);
        self.load_cartridge_owned(owned, mapper)
    }
    /// Takes ownership of a staged upload, avoiding another full MegaROM copy.
    /// Validation finishes before the running machine is changed.
    pub fn load_cartridge_owned(&mut self, rom: Vec<u8>, mapper: Mapper) -> Result<(), String> {
        let mapping = Mapping::validate(&rom, mapper)?;
        self.hardware.cartridge = rom;
        self.hardware.mapping = mapping;
        self.reset();
        Ok(())
    }
    pub fn unload_cartridge(&mut self) {
        self.hardware.cartridge = Vec::new();
        self.hardware.mapping = Mapping::default();
        self.reset();
    }
    pub fn load_bios(&mut self, rom: &[u8]) -> Result<(), String> {
        if self.hardware.model == Model::Msx2 {
            return Err("MSX2 requires both the main BIOS and sub-ROM. Select both files.".into());
        }
        if rom.len() != 32768 {
            return Err("Select a 32 KiB MSX1 BIOS/BASIC system ROM.".into());
        }
        self.hardware.bios = rom.to_vec();
        self.hardware.builtin = false;
        self.reset();
        Ok(())
    }
    pub fn use_builtin(&mut self) {
        self.hardware.bios = if self.hardware.model == Model::Msx2 {
            firmware::msx2_bios()
        } else {
            firmware::bios()
        };
        self.hardware.subrom = if self.hardware.model == Model::Msx2 {
            firmware::msx2_subrom()
        } else {
            Vec::new()
        };
        self.hardware.builtin = true;
        self.hardware.cartridge = Vec::new();
        self.hardware.mapping = Mapping::default();
        self.reset();
    }
    pub fn text_key(&mut self, key: u8) {
        if self.hardware.builtin
            && self.hardware.model == Model::Msx1
            && self.hardware.keys.len() < 256
        {
            self.hardware.keys.push_back(key);
        }
    }
    pub fn step(&mut self) -> u32 {
        let cycles = self.cpu.step(&mut self.hardware);
        self.hardware.vdp.tick(cycles);
        self.hardware.psg.tick(cycles);
        if self.hardware.model == Model::Msx2 {
            self.hardware.rtc.tick(cycles);
        }
        while let Some(key) = self.hardware.services.pop_front() {
            if key == 0 {
                self.basic.boot(&mut self.hardware.vdp);
            } else {
                self.basic
                    .key(key, &mut self.hardware.vdp, &mut self.hardware.psg);
            }
        }
        cycles
    }
    pub fn frame(&mut self) {
        self.hardware.psg.samples.clear();
        self.frame_debt += self.hardware.vdp.frame_cycles();
        while self.frame_debt > 0 {
            let c = self.step();
            self.frame_debt -= c as i32;
        }
        if self.hardware.builtin && self.hardware.model == Model::Msx1 {
            self.basic
                .pump(&mut self.hardware.vdp, &mut self.hardware.psg, 200);
        }
    }
}
