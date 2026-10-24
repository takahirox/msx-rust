//! Platform-independent, original MSX1/MSX2 implementation. No browser dependencies.
pub mod basic;
pub mod cartridge;
pub mod cassette;
pub mod cpu;
pub mod disk;
pub use cartridge::Mapper;
use cartridge::Mapping;
pub mod firmware;
mod fm_pac;
pub mod font;
pub mod psg;
pub mod rtc;
pub mod scc;
mod sound_cartridge;
pub mod ym2413;
use sound_cartridge::SoundCartridge;
pub use sound_cartridge::SoundCartridgeProfile;
pub mod vdp;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Model {
    #[default]
    Msx1,
    Msx2,
}
/// Automatic transitions apply only until an explicit firmware selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BootPolicy {
    #[default]
    Automatic,
    Explicit,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Firmware {
    #[default]
    WeaveBasic,
    Cbios,
    Diagnostic,
    User,
    Disk,
}
use basic::Basic;
use cpu::{Bus, Cpu, FetchWait};
use psg::Psg;
use std::collections::VecDeque;
use vdp::Vdp;

pub struct Hardware {
    pub model: Model,
    pub subrom: Vec<u8>,
    pub secondary: [u8; 4],
    pub rtc: rtc::Rtc,
    pub fdc: disk::Controller,
    pub cassette: cassette::Cassette,
    pub disk_rom: Vec<u8>,
    philips_rom: Option<Vec<u8>>,
    music_rom: Option<Vec<u8>>,
    fm: ym2413::Ym2413,
    fm_pac: Option<fm_pac::FmPac>,
    /// Physical RAM storage. CPU-address observations must use the Bus mapping.
    pub ram: Vec<u8>,
    ram_segments: [u8; 4],
    pub bios: Vec<u8>,
    pub cartridge: Vec<u8>,
    mapping: Mapping,
    sound_cartridge: Option<SoundCartridge>,
    pub slots: u8,
    pub ppi_c: u8,
    pub ppi_control: u8,
    pub keyboard: [u8; 11],
    /// Connector one's active-low inputs; retained for existing core callers.
    pub joystick: u8,
    joystick2: u8,
    pub vdp: Vdp,
    pub psg: Psg,
    scc: scc::Scc,
    pub firmware: Firmware,
    keys: VecDeque<u8>,
    services: VecDeque<u8>,
}
impl Default for Hardware {
    fn default() -> Self {
        Self {
            model: Model::Msx1,
            // Reserve firmware capacity once: all later firmware switches and
            // cartridge commits reuse buffers and cannot fail allocation.
            subrom: Vec::with_capacity(16384),
            secondary: [0; 4],
            rtc: rtc::Rtc::default(),
            fdc: disk::Controller::default(),
            cassette: cassette::Cassette::default(),
            disk_rom: firmware::disk_rom(),
            philips_rom: None,
            music_rom: None,
            fm: ym2413::Ym2413::default(),
            fm_pac: None,
            // Reserve the largest profile up front; model changes and reset
            // must not allocate during a transactional cartridge commit.
            ram: vec![0; 256 * 1024],
            ram_segments: [0; 4],
            bios: firmware::bios(),
            cartridge: Vec::new(),
            mapping: Mapping::default(),
            sound_cartridge: None,
            slots: 0xf0,
            ppi_c: 0,
            ppi_control: 0x82,
            keyboard: [255; 11],
            joystick: 0x3f,
            joystick2: 0x3f,
            vdp: Vdp::default(),
            psg: Psg::default(),
            scc: scc::Scc::default(),
            firmware: Firmware::WeaveBasic,
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
        self.ram_segments = [0; 4];
        self.fdc.reset();
        if self.philips_rom.is_some() {
            self.fdc.write_philips(5, 0); // Reset latch selects installed A, motor off.
        }
        self.mapping.reset();
        self.slots = 0xf0;
        self.secondary = [0; 4];
        if self.model == Model::Msx2
            && matches!(self.firmware, Firmware::WeaveBasic | Firmware::Diagnostic)
        {
            self.secondary[3] = 0xff;
        }
        self.rtc.selected = 0;
        self.ppi_c = 0;
        self.ppi_control = 0x82;
        self.release_keys();
        self.vdp.reset();
        self.psg.reset();
        self.fm = ym2413::Ym2413::default();
        if let Some(pac) = &mut self.fm_pac {
            pac.reset();
        }
        if let Some(cart) = &mut self.sound_cartridge {
            cart.reset();
            self.scc.reset_scci();
        } else {
            self.scc = scc::Scc::default();
        }
        self.keys.clear();
        self.services.clear();
        // A real BIOS probes slots itself. Our firmware boots AB cartridges in their initial banks.
        if self.firmware == Firmware::Diagnostic && !self.cartridge.is_empty() {
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
        self.joystick2 = 0x3f;
    }
    /// Set six active-low inputs on connector 0 or 1. Invalid indices leave both
    /// connectors unchanged and return false. Pin 8 gates reads, not held state.
    pub fn set_joystick(&mut self, connector: usize, value: u8) -> bool {
        match connector {
            0 => self.joystick = value & 0x3f,
            1 => self.joystick2 = value & 0x3f,
            _ => return false,
        }
        true
    }
}
impl Hardware {
    fn ram_address(&self, address: u16) -> usize {
        if self.model == Model::Msx2 {
            usize::from(self.ram_segments[usize::from(address >> 14)]) * 0x4000
                + usize::from(address & 0x3fff)
        } else {
            usize::from(address)
        }
    }
    fn expanded(&self, primary: u8) -> bool {
        (primary == 3 && self.model == Model::Msx2) || (primary == 2 && self.fm_pac.is_some())
    }
    fn slot(&self, address: u16) -> (u8, u8) {
        let page = (address >> 14) * 2;
        let primary = (self.slots >> page) & 3;
        let secondary = if self.expanded(primary) {
            (self.secondary[usize::from(primary)] >> page) & 3
        } else {
            0
        };
        (primary, secondary)
    }
}
impl Bus for Hardware {
    fn read(&mut self, address: u16) -> u8 {
        if address == 0xffff && self.expanded(self.slots >> 6) {
            return !self.secondary[usize::from(self.slots >> 6)];
        }
        let a = address as usize;
        match self.slot(address) {
            (2, 1) if self.fm_pac.is_some() => self.fm_pac.as_ref().unwrap().read(address),
            (0, 0) => self.bios.get(a).copied().unwrap_or(255),
            (2, 0)
                if (self.philips_rom.is_some() || self.firmware == Firmware::Disk)
                    && (0x7ff8..=0x7fff).contains(&address) =>
            {
                if self.philips_rom.is_some() {
                    self.fdc.read_philips((address - 0x7ff8) as u8)
                } else {
                    self.fdc.read((address - 0x7ff8) as u8)
                }
            }
            (2, 0)
                if (self.philips_rom.is_some() || self.firmware == Firmware::Disk)
                    && (0x4000..0x8000).contains(&address) =>
            {
                self.philips_rom.as_ref().unwrap_or(&self.disk_rom)[(address - 0x4000) as usize]
            }
            (1, 0) if self.sound_cartridge.is_some() => self
                .sound_cartridge
                .as_ref()
                .unwrap()
                .read(address, &mut self.scc),
            (1, 0) if self.mapping.scc_access(address) => self.scc.read(address),
            (1, 0) => self.mapping.read(&self.cartridge, address),
            (3, 1) if self.model == Model::Msx2 && (0x4000..0x8000).contains(&address) => {
                self.music_rom.as_ref().map_or(255, |rom| rom[a - 0x4000])
            }
            (3, 0) if self.model == Model::Msx2 => self.subrom.get(a).copied().unwrap_or(255),
            (3, sub) if self.model == Model::Msx1 || sub == 3 => {
                self.ram[self.ram_address(address)]
            }
            _ => 255,
        }
    }
    fn write(&mut self, address: u16, value: u8) {
        if address == 0xffff && self.expanded(self.slots >> 6) {
            self.secondary[usize::from(self.slots >> 6)] = value;
            return;
        }
        match self.slot(address) {
            (2, 1) if self.fm_pac.is_some() => self.fm_pac.as_mut().unwrap().write(address, value),
            (2, 0)
                if (self.philips_rom.is_some() || self.firmware == Firmware::Disk)
                    && (0x7ff8..=0x7fff).contains(&address) =>
            {
                if self.philips_rom.is_some() {
                    self.fdc.write_philips((address - 0x7ff8) as u8, value);
                } else {
                    self.fdc.write((address - 0x7ff8) as u8, value);
                }
            }
            (1, 0) if self.sound_cartridge.is_some() => self
                .sound_cartridge
                .as_mut()
                .unwrap()
                .write(address, value, &mut self.scc),
            (1, 0) if self.mapping.scc_access(address) => self.scc.write(address, value),
            (1, 0) => self.mapping.write(address, value),
            (3, sub) if self.model == Model::Msx1 || sub == 3 => {
                let a = self.ram_address(address);
                self.ram[a] = value;
            }
            _ => {}
        }
    }
    fn input(&mut self, port: u16) -> u8 {
        match port as u8 {
            0x98 => self.vdp.read(),
            0x99 => self.vdp.read_status(),
            0xa2 => {
                if self.psg.selected == 14 {
                    // MSX motherboard compatibility policy: fixed R14 input / R15
                    // output routing regardless of R7 direction bits. R7 itself
                    // retains normal readback and all six audio mixer controls.
                    let control = self.psg.registers[15];
                    let (inputs, common) = if control & 0x40 == 0 {
                        (self.joystick, 0x10)
                    } else {
                        (self.joystick2, 0x20)
                    };
                    (u8::from(self.cassette.input(self.ppi_c & 0x10 == 0)) << 7)
                        | 0x40
                        | if control & common == 0 {
                            inputs & 0x3f
                        } else {
                            0x3f
                        }
                } else {
                    self.psg.read()
                }
            }
            0xb5 if self.model == Model::Msx2 => self.rtc.read(),
            0xfc..=0xff if self.model == Model::Msx2 => {
                0xf0 | self.ram_segments[usize::from(port as u8 - 0xfc)]
            }
            0xa8 => self.slots,
            0xa9 => self
                .keyboard
                .get((self.ppi_c & 15) as usize)
                .copied()
                .unwrap_or(255),
            0xaa => self.ppi_c,
            0xab => self.ppi_control,
            0xf0 if self.firmware == Firmware::WeaveBasic => self.keys.pop_front().unwrap_or(0),
            _ => 255,
        }
    }
    fn output(&mut self, port: u16, value: u8) {
        match port as u8 {
            p @ (0x7c | 0x7d) => {
                if self.model == Model::Msx2 && self.music_rom.is_some() {
                    if p == 0x7c {
                        self.fm.select(value);
                    } else {
                        self.fm.write(value);
                    }
                }
                if let Some(pac) = &mut self.fm_pac {
                    pac.output(p, value);
                }
            }
            0x98 => self.vdp.write(value),
            0x99 => self.vdp.control(value),
            0x9a => self.vdp.write_palette(value),
            0x9b => self.vdp.write_indirect(value),
            0xfc..=0xff if self.model == Model::Msx2 => {
                self.ram_segments[usize::from(port as u8 - 0xfc)] = value & 15;
            }
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
            0xf1 if self.firmware == Firmware::WeaveBasic => self.services.push_back(value),
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
    pub boot_policy: BootPolicy,
    // Original diagnostic images, prepared before any running state exists.
    diagnostic_main: Vec<u8>,
    diagnostic_sub: Vec<u8>,
    frame_debt: i32,
}
impl Default for Machine {
    fn default() -> Self {
        Self {
            cpu: Cpu::with_fetch_wait(FetchWait::Msx),
            hardware: Hardware::default(),
            basic: Basic::default(),
            boot_policy: BootPolicy::Automatic,
            diagnostic_main: firmware::msx2_bios(),
            diagnostic_sub: firmware::msx2_subrom(),
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
    /// Restore an explicit bundled cartridge BIOS selection, releasing cartridges.
    pub fn use_cbios(&mut self) -> Result<(), String> {
        self.install_firmware(Firmware::Cbios);
        self.boot_policy = BootPolicy::Explicit;
        self.hardware.cartridge.clear();
        self.hardware.mapping = Mapping::default();
        self.hardware.sound_cartridge = None;
        self.reset();
        Ok(())
    }
    /// Retain the original MSX2 diagnostic loader as an explicit override.
    pub fn use_diagnostic(&mut self) -> Result<(), String> {
        if self.hardware.model != Model::Msx2 {
            return Err("Select MSX2 before selecting its diagnostic loader.".into());
        }
        self.install_firmware(Firmware::Diagnostic);
        self.boot_policy = BootPolicy::Explicit;
        self.hardware.cartridge.clear();
        self.hardware.mapping = Mapping::default();
        self.hardware.sound_cartridge = None;
        self.reset();
        Ok(())
    }
    // BIOS is always 32 KiB; sub-ROM capacity is retained even on MSX1. No
    // allocation occurs here, so an already validated cartridge commits atomically.
    fn install_firmware(&mut self, firmware: Firmware) {
        let h = &mut self.hardware;
        h.subrom.clear();
        match firmware {
            Firmware::WeaveBasic => firmware::write_bios(&mut h.bios, h.model == Model::Msx2),
            Firmware::Cbios => {
                h.bios.copy_from_slice(if h.model == Model::Msx2 {
                    firmware::CBIOS_MSX2_MAIN
                } else {
                    firmware::CBIOS_MSX1_MAIN
                });
                if h.model == Model::Msx2 {
                    h.subrom.extend_from_slice(firmware::CBIOS_MSX2_SUB);
                }
            }
            Firmware::Disk => firmware::write_disk_boot(&mut h.bios, h.model == Model::Msx2),
            Firmware::Diagnostic => {
                h.bios.copy_from_slice(&self.diagnostic_main);
                h.subrom.extend_from_slice(&self.diagnostic_sub);
            }
            Firmware::User => unreachable!("local firmware uses validated buffers"),
        }
        h.firmware = firmware;
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
        self.hardware.bios.copy_from_slice(main);
        self.hardware.subrom.clear();
        self.hardware.subrom.extend_from_slice(sub);
        self.hardware.firmware = Firmware::User;
        self.boot_policy = BootPolicy::Explicit;
        self.reset();
        Ok(())
    }
    pub fn reset(&mut self) {
        self.hardware.reset();
        self.cpu = Cpu::with_fetch_wait(FetchWait::Msx);
        self.basic = Basic::default();
        self.frame_debt = 0;
    }
    pub fn load_cartridge(&mut self, rom: &[u8]) -> Result<(), String> {
        self.load_cartridge_with_mapper(rom, Mapper::Auto)
    }
    pub fn load_cartridge_with_mapper(&mut self, rom: &[u8], mapper: Mapper) -> Result<(), String> {
        let mapper = Mapping::validate(rom, mapper)?.kind;
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
        self.load_cartridge_for_model_owned(rom, mapper, self.hardware.model)
    }
    /// Validate and reserve a curated cartridge and its model before committing.
    /// Explicit local/diagnostic firmware cannot be moved to an incompatible model.
    pub fn load_cartridge_for_model_owned(
        &mut self,
        rom: Vec<u8>,
        mapper: Mapper,
        model: Model,
    ) -> Result<(), String> {
        let mut mapping = Mapping::validate(&rom, mapper)?;
        mapping.reserve_sram()?;
        if model != self.hardware.model {
            if self.boot_policy == BootPolicy::Explicit && self.hardware.firmware != Firmware::Cbios
            {
                return Err("This demo needs a different machine. Choose Use Weave BASIC to clear the explicit firmware override first.".into());
            }
            let vdp = Vdp::try_new(model == Model::Msx2)?;
            self.hardware.model = model;
            self.hardware.vdp = vdp;
            self.hardware.rtc = rtc::Rtc::default();
            // An explicit C-BIOS policy survives, using the matching model's BIOS.
            self.install_firmware(Firmware::Cbios);
        }
        if self.boot_policy == BootPolicy::Automatic {
            self.install_firmware(Firmware::Cbios);
        }
        self.hardware.cartridge = rom;
        self.hardware.mapping = mapping;
        self.hardware.sound_cartridge = None;
        self.reset();
        Ok(())
    }
    /// Explicit ROMless slot-1 replacement. Allocate before changing any session state;
    /// retain the selected firmware, independently installed disk interface and media.
    pub fn install_sound_cartridge(
        &mut self,
        profile: SoundCartridgeProfile,
    ) -> Result<(), String> {
        let cart = SoundCartridge::new(profile)?;
        self.hardware.cartridge.clear();
        self.hardware.mapping = Mapping::default();
        self.hardware.sound_cartridge = Some(cart);
        self.hardware.scc = scc::Scc::default();
        self.reset();
        Ok(())
    }
    pub fn sound_cartridge_profile(&self) -> Option<SoundCartridgeProfile> {
        self.hardware.sound_cartridge.as_ref().map(|c| c.profile)
    }
    /// Accessory removal restarts the selected firmware, including automatic policy.
    pub fn remove_sound_cartridge(&mut self) {
        if self.hardware.sound_cartridge.take().is_some() {
            self.reset();
        }
    }
    /// Battery-backed bytes for the current cartridge only (empty for ROM-only mappers).
    pub fn cartridge_sram(&self) -> &[u8] {
        &self.hardware.mapping.sram
    }
    /// Restore a complete save atomically. Container identity/version is checked by the caller.
    pub fn restore_cartridge_sram(&mut self, bytes: &[u8]) -> Result<(), String> {
        let sram = &mut self.hardware.mapping.sram;
        if sram.is_empty() || bytes.len() != sram.len() {
            return Err("Save size does not match this cartridge's SRAM.".into());
        }
        sram.copy_from_slice(bytes);
        Ok(())
    }
    pub fn unload_cartridge(&mut self) {
        if self.hardware.sound_cartridge.is_some() {
            self.remove_sound_cartridge();
            return;
        }
        if self.boot_policy == BootPolicy::Automatic {
            self.install_firmware(Firmware::WeaveBasic);
        }
        self.hardware.cartridge = Vec::new();
        self.hardware.mapping = Mapping::default();
        self.hardware.sound_cartridge = None;
        self.reset();
    }
    pub fn load_bios(&mut self, rom: &[u8]) -> Result<(), String> {
        if self.hardware.model == Model::Msx2 {
            return Err("MSX2 requires both the main BIOS and sub-ROM. Select both files.".into());
        }
        if rom.len() != 32768 {
            return Err("Select a 32 KiB MSX1 BIOS/BASIC system ROM.".into());
        }
        if rom[0x2d] != 0 {
            return Err("Select an MSX1 system ROM (generation byte 002Dh = 0).".into());
        }
        self.hardware.bios.copy_from_slice(rom);
        self.hardware.firmware = Firmware::User;
        self.boot_policy = BootPolicy::Explicit;
        self.reset();
        Ok(())
    }
    /// Clear the override and cartridge, returning to automatic Weave BASIC.
    pub fn use_builtin(&mut self) {
        self.install_firmware(Firmware::WeaveBasic);
        self.boot_policy = BootPolicy::Automatic;
        self.hardware.cartridge.clear();
        self.hardware.mapping = Mapping::default();
        self.hardware.sound_cartridge = None;
        self.reset();
    }
    /// Install a separately supplied, already reserved Philips disk ROM. A failed
    /// validation leaves CPU, firmware, cartridge/SRAM and media untouched.
    pub fn install_philips_disk_rom(&mut self, rom: Vec<u8>) -> Result<(), String> {
        if rom.len() != 16384 || &rom[..2] != b"AB" {
            return Err("Philips NMS1200-style requires a 16 KiB AB-header disk ROM.".into());
        }
        if self.hardware.firmware == Firmware::Disk {
            return Err("Choose Use Weave BASIC or Open ROM to leave Weave disk firmware before installing the Philips interface.".into());
        }
        self.hardware.philips_rom = Some(rom);
        self.reset();
        Ok(())
    }
    /// Hardware installation survives all firmware, cartridge and model switches.
    pub fn has_philips_disk_rom(&self) -> bool {
        self.hardware.philips_rom.is_some()
    }
    /// Return to the default Weave boot option without discarding mounted media.
    pub fn remove_philips_disk_rom(&mut self) {
        if self.hardware.philips_rom.take().is_some() {
            self.reset();
        }
    }
    /// Explicit internal-style MSX-MUSIC. Exact bytes are mapped independently
    /// of the system firmware. MSX1 retains the installation inactive.
    pub fn install_msx_music(&mut self, rom: Vec<u8>) -> Result<(), String> {
        if rom.len() != 16384 {
            return Err(
                "MSX-MUSIC internal profile requires a separate 16 KiB firmware image.".into(),
            );
        }
        self.hardware.music_rom = Some(rom);
        self.reset();
        Ok(())
    }
    pub fn has_msx_music(&self) -> bool {
        self.hardware.music_rom.is_some()
    }
    pub fn remove_msx_music(&mut self) {
        self.hardware.music_rom = None;
        self.reset();
    }
    /// Independently install the original external FM-PAC profile in slot 2-1.
    /// Reserve SRAM before committing; invalid/failed imports leave the session intact.
    pub fn install_fm_pac(&mut self, rom: Vec<u8>) -> Result<(), String> {
        let pac = fm_pac::FmPac::new(rom)?;
        self.hardware.fm_pac = Some(pac);
        self.reset();
        Ok(())
    }
    pub fn has_fm_pac(&self) -> bool {
        self.hardware.fm_pac.is_some()
    }
    pub fn remove_fm_pac(&mut self) {
        if self.hardware.fm_pac.take().is_some() {
            self.reset();
        }
    }
    /// Only the 8190 battery-backed bytes; unlock latches are never save data.
    pub fn fm_pac_sram(&self) -> &[u8] {
        self.hardware.fm_pac.as_ref().map_or(&[], |pac| &pac.sram)
    }
    pub fn restore_fm_pac_sram(&mut self, bytes: &[u8]) -> Result<(), String> {
        match &mut self.hardware.fm_pac {
            Some(pac) if bytes.len() == 8190 => {
                pac.sram.copy_from_slice(bytes);
                Ok(())
            }
            _ => Err("Save size does not match installed FM-PAC SRAM (8190 bytes).".into()),
        }
    }
    /// Mount a private staged image without changing execution, slots or firmware.
    pub fn mount_disk(&mut self, bytes: Vec<u8>, protected: bool) -> Result<(), String> {
        let disk = disk::Disk::new(bytes, protected)?;
        self.hardware.fdc.mount(disk)
    }
    /// Original firmware owns slot 2 and takes boot priority; cartridge/SRAM stay parked.
    pub fn boot_disk(&mut self) -> Result<(), String> {
        if self.hardware.fdc.disk.is_none() {
            return Err("Mount a DSK in drive A: first.".into());
        }
        if self.has_philips_disk_rom() {
            self.reset();
            return Ok(());
        }
        if self.boot_policy == BootPolicy::Explicit {
            return Err("Disk boot needs the original disk firmware. Choose Use Weave BASIC to clear the explicit firmware override first.".into());
        }
        self.install_firmware(Firmware::Disk);
        self.reset();
        Ok(())
    }
    pub fn eject_disk(&mut self) -> Result<(), String> {
        self.hardware.fdc.eject()?;
        if self.hardware.firmware == Firmware::Disk {
            self.install_firmware(if self.hardware.cartridge.is_empty() {
                Firmware::WeaveBasic
            } else {
                Firmware::Cbios
            });
            self.reset();
        }
        Ok(())
    }
    pub fn text_key(&mut self, key: u8) {
        if self.hardware.firmware == Firmware::WeaveBasic && self.hardware.keys.len() < 256 {
            self.hardware.keys.push_back(key);
        }
    }
    pub fn step(&mut self) -> u32 {
        let cycles = self.cpu.step(&mut self.hardware);
        self.hardware.vdp.tick(cycles);
        self.hardware.fdc.tick(cycles);
        self.hardware
            .cassette
            .tick(cycles, self.hardware.ppi_c & 0x10 == 0);
        let h = &mut self.hardware;
        // Register visibility controls CPU access, not the installed chip's audio.
        let scc = (h.mapping.kind == Mapper::KonamiScc || h.sound_cartridge.is_some())
            .then_some(&mut h.scc);
        let fm = (h.model == Model::Msx2 && h.music_rom.is_some()).then_some(&mut h.fm);
        let pac = h.fm_pac.as_mut().map(|pac| &mut pac.chip);
        h.psg.tick_with_fm_pac(cycles, scc, fm, pac);
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
        // Snapshot the active field budget before CPU writes can queue a new
        // R9 timing. Carry instruction overshoot across timing changes as well.
        self.frame_debt += self.hardware.vdp.frame_cycles();
        while self.frame_debt > 0 {
            let c = self.step();
            self.frame_debt -= c as i32;
        }
        if self.hardware.firmware == Firmware::WeaveBasic {
            self.basic
                .pump(&mut self.hardware.vdp, &mut self.hardware.psg, 200);
        }
    }
}
