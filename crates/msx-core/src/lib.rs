//! Platform-independent, original MSX1 implementation. No browser dependencies.
pub mod basic;
pub mod cartridge;
pub mod cpu;
pub use cartridge::Mapper;
use cartridge::Mapping;
pub mod firmware;
pub mod font;
pub mod psg;
pub mod vdp;
use basic::Basic;
use cpu::{Bus, Cpu};
use psg::Psg;
use std::collections::VecDeque;
use vdp::Vdp;

pub struct Hardware {
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
impl Bus for Hardware {
    fn read(&mut self, address: u16) -> u8 {
        let slot = (self.slots >> ((address >> 14) * 2)) & 3;
        let a = address as usize;
        match slot {
            0 => self.bios.get(a).copied().unwrap_or(255),
            1 => self.mapping.read(&self.cartridge, address),
            3 => self.ram[a],
            _ => 255,
        }
    }
    fn write(&mut self, address: u16, value: u8) {
        match (self.slots >> ((address >> 14) * 2)) & 3 {
            1 => self.mapping.write(address, value),
            3 => self.ram[address as usize] = value,
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
            0xa8 => self.slots,
            0xa9 => self
                .keyboard
                .get((self.ppi_c & 15) as usize)
                .copied()
                .unwrap_or(255),
            0xaa => self.ppi_c,
            0xab => self.ppi_control,
            0xf0 if self.builtin => self.keys.pop_front().unwrap_or(0),
            _ => 255,
        }
    }
    fn output(&mut self, port: u16, value: u8) {
        match port as u8 {
            0x98 => self.vdp.write(value),
            0x99 => self.vdp.control(value),
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
            0xf1 if self.builtin => self.services.push_back(value),
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
        if rom.len() != 32768 {
            return Err("Select a 32 KiB MSX1 BIOS/BASIC system ROM.".into());
        }
        self.hardware.bios = rom.to_vec();
        self.hardware.builtin = false;
        self.reset();
        Ok(())
    }
    pub fn use_builtin(&mut self) {
        self.hardware.bios = firmware::bios();
        self.hardware.builtin = true;
        self.hardware.cartridge = Vec::new();
        self.hardware.mapping = Mapping::default();
        self.reset();
    }
    pub fn text_key(&mut self, key: u8) {
        if self.hardware.builtin && self.hardware.keys.len() < 256 {
            self.hardware.keys.push_back(key);
        }
    }
    pub fn step(&mut self) -> u32 {
        let cycles = self.cpu.step(&mut self.hardware);
        self.hardware.vdp.tick(cycles);
        self.hardware.psg.tick(cycles);
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
        self.frame_debt += 228 * 262;
        while self.frame_debt > 0 {
            let c = self.step();
            self.frame_debt -= c as i32;
        }
        if self.hardware.builtin {
            self.basic
                .pump(&mut self.hardware.vdp, &mut self.hardware.psg, 200);
        }
    }
}
