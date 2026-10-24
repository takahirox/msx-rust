//! Force real allocation errors without changing production allocation behavior.
use msx_core::{cpu::Bus, firmware, BootPolicy, Firmware, Machine, Mapper, Model};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! { static DENY: Cell<bool> = const { Cell::new(false) }; }
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if DENY.try_with(Cell::get).unwrap_or(false)
            || FAIL_SIZE
                .try_with(|s| s.get() == layout.size())
                .unwrap_or(false)
        {
            std::ptr::null_mut()
        } else {
            System.alloc(layout)
        }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        System.dealloc(p, layout);
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
#[test]
fn firmware_commits_reuse_memory_and_cartridge_reservation_failure_is_atomic() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = Machine::default();
        m.set_model(model);
        m.frame();
        m.basic.submit("10 PRINT 42").unwrap();
        let rom = firmware::test_cartridge();
        // Only fail the ROM-sized reservation; error reporting can still allocate.
        FAIL_SIZE.with(|s| s.set(rom.len()));
        let result = m.load_cartridge_with_mapper(&rom, Mapper::Plain);
        FAIL_SIZE.with(|s| s.set(0));
        assert!(result.is_err());
        assert_eq!(m.hardware.firmware, Firmware::WeaveBasic);
        assert!(m.basic.program.contains_key(&10));
        let main = m.hardware.bios.as_ptr();
        let sub = m.hardware.subrom.as_ptr();
        DENY.with(|s| s.set(true));
        m.load_cartridge_owned(rom, Mapper::Plain).unwrap();
        m.reset();
        m.unload_cartridge();
        m.use_cbios().unwrap();
        if model == Model::Msx2 {
            m.use_diagnostic().unwrap();
            m.load_msx2_firmware(firmware::CBIOS_MSX2_MAIN, firmware::CBIOS_MSX2_SUB)
                .unwrap();
        } else {
            m.load_bios(firmware::CBIOS_MSX1_MAIN).unwrap();
        }
        m.use_builtin();
        DENY.with(|s| s.set(false));
        assert_eq!(m.hardware.bios.as_ptr(), main);
        assert_eq!(m.hardware.subrom.as_ptr(), sub);
        assert_eq!(m.boot_policy, BootPolicy::Automatic);
        m.frame();
        assert!(m.basic.console.transcript.contains("READY"));
    }
}
thread_local! { static FAIL_SIZE: Cell<usize> = const { Cell::new(0) }; }

#[test]
fn cartridge_reservation_failure_preserves_active_scc_and_psg() {
    for model in [Model::Msx1, Model::Msx2] {
        let rom = firmware::scc_test_cartridge();
        let mut machines = [Machine::default(), Machine::default()];
        for m in &mut machines {
            m.set_model(model);
            m.load_cartridge_with_mapper(&rom, Mapper::KonamiScc)
                .unwrap();
            for _ in 0..600 {
                m.frame();
                if m.hardware.read(0xc000) == 0xa5 {
                    break;
                }
            }
            assert_eq!(m.hardware.read(0xc000), 0xa5);
            m.hardware.key(8, 5, true);
            m.hardware.key(8, 4, true);
            m.frame();
        }
        FAIL_SIZE.with(|s| s.set(rom.len()));
        let result = machines[0].load_cartridge_with_mapper(&rom, Mapper::KonamiScc);
        FAIL_SIZE.with(|s| s.set(0));
        assert!(result.unwrap_err().contains("Not enough memory"));
        for m in &mut machines {
            m.frame();
        }
        assert_eq!(
            machines[0].hardware.psg.samples,
            machines[1].hardware.psg.samples
        );
        assert!(machines[0]
            .hardware
            .psg
            .samples
            .iter()
            .any(|x| x.abs() > 0.1));
    }
}

#[test]
fn demo_model_display_allocation_failure_preserves_running_basic() {
    for fail_size in [512 * 424 * 4, 131072] {
        let mut m = Machine::default();
        m.frame();
        m.basic.submit("10 PRINT 73").unwrap();
        let rom = firmware::msx2_cartridge();
        let pc = m.cpu.pc;
        let pixels = m.hardware.vdp.pixels.clone();
        FAIL_SIZE.with(|s| s.set(fail_size));
        let result = m.load_cartridge_for_model_owned(rom, Mapper::Plain, Model::Msx2);
        FAIL_SIZE.with(|s| s.set(0));
        assert!(result.unwrap_err().contains("memory"));
        assert_eq!(m.hardware.model, Model::Msx1);
        assert_eq!(m.hardware.firmware, Firmware::WeaveBasic);
        assert_eq!(m.boot_policy, BootPolicy::Automatic);
        assert_eq!(m.cpu.pc, pc);
        assert_eq!(m.hardware.vdp.pixels, pixels);
        assert!(m.basic.program.contains_key(&10));
    }
}

#[test]
fn sram_reservation_failure_preserves_unsaved_cartridge_and_reset_does_not_allocate() {
    for kind in [
        Mapper::Ascii8Sram2,
        Mapper::Ascii8Sram8,
        Mapper::Ascii16Sram2,
        Mapper::Ascii16Sram8,
    ] {
        let mut m = Machine::default();
        m.load_cartridge_owned(
            firmware::mapper_test_cartridge(Mapper::Ascii16Sram8),
            Mapper::Ascii16Sram8,
        )
        .unwrap();
        m.hardware.slots = 0xd4;
        m.hardware.write(0x7000, 0x10);
        m.hardware.write(0x8123, 0x5a);
        m.cpu.pc = 0x4321;
        let incoming = firmware::mapper_test_cartridge(kind);
        FAIL_SIZE.with(|s| s.set(kind.sram_size()));
        let result = m.load_cartridge_for_model_owned(incoming, kind, Model::Msx2);
        FAIL_SIZE.with(|s| s.set(0));
        assert!(result.unwrap_err().contains("memory"));
        assert_eq!(m.hardware.model, Model::Msx1);
        assert_eq!(m.cpu.pc, 0x4321);
        assert_eq!(m.hardware.read(0x8123), 0x5a);
        DENY.with(|s| s.set(true));
        m.reset();
        DENY.with(|s| s.set(false));
        assert_eq!(m.cartridge_sram()[0x123], 0x5a);
        // Invalid headers must be rejected before any SRAM reservation attempt.
        let invalid = vec![0; 128 * 1024];
        FAIL_SIZE.with(|s| s.set(kind.sram_size()));
        let result = m.load_cartridge_owned(invalid, kind);
        FAIL_SIZE.with(|s| s.set(0));
        assert!(result.unwrap_err().contains("AB"));
    }
}

#[test]
fn disk_mount_boot_and_reset_commit_without_allocating() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = Machine::default();
        m.set_model(model);
        let staged = firmware::demo_disk();
        let rom = m.hardware.disk_rom.as_ptr();
        DENY.with(|s| s.set(true));
        m.mount_disk(staged, true).unwrap();
        m.boot_disk().unwrap();
        m.reset();
        m.eject_disk().unwrap();
        DENY.with(|s| s.set(false));
        assert_eq!(m.hardware.disk_rom.as_ptr(), rom);
        assert_eq!(m.hardware.firmware, Firmware::WeaveBasic);
    }
}

#[test]
fn staged_philips_install_reset_remove_retain_media_without_allocating() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = Machine::default();
        m.set_model(model);
        m.use_cbios().unwrap();
        let mut rom = vec![0xff; 16384];
        rom[..2].copy_from_slice(b"AB");
        m.mount_disk(vec![0x35; 368640], false).unwrap();
        DENY.with(|s| s.set(true));
        m.install_philips_disk_rom(rom).unwrap();
        m.boot_disk().unwrap();
        m.reset();
        DENY.with(|s| s.set(false));
        m.hardware.slots = 0xf8;
        assert_eq!(m.hardware.read(0x4000), b'A');
        assert_eq!(m.hardware.firmware, Firmware::Cbios);
        DENY.with(|s| s.set(true));
        m.remove_philips_disk_rom();
        DENY.with(|s| s.set(false));
        assert_eq!(
            m.hardware.fdc.disk.as_ref().unwrap().bytes(),
            vec![0x35; 368640]
        );
    }
}

#[test]
fn scci_allocation_failure_preserves_session_and_reset_remove_do_not_allocate() {
    use msx_core::SoundCartridgeProfile as Profile;
    for model in [Model::Msx1, Model::Msx2] {
        for profile in [Profile::Snatcher, Profile::SdSnatcher] {
            let mut m = Machine::default();
            m.set_model(model);
            m.install_sound_cartridge(profile).unwrap();
            m.hardware.secondary[3] = 255;
            m.hardware.slots = 0xd4;
            m.hardware
                .write(0x5000, if profile == Profile::Snatcher { 0 } else { 8 });
            m.hardware.write(0xbffe, 1);
            m.hardware.write(0x4123, 0x7b);
            m.cpu.pc = 0x1234;
            FAIL_SIZE.with(|s| s.set(65536));
            let result = m.install_sound_cartridge(Profile::Snatcher);
            FAIL_SIZE.with(|s| s.set(0));
            assert!(result.is_err());
            assert_eq!(m.cpu.pc, 0x1234);
            assert_eq!(m.sound_cartridge_profile(), Some(profile));
            assert_eq!(m.hardware.read(0x4123), 0x7b);
            DENY.with(|s| s.set(true));
            m.reset();
            m.remove_sound_cartridge();
            DENY.with(|s| s.set(false));
            assert_eq!(m.sound_cartridge_profile(), None);
        }
    }
}

#[test]
fn cassette_metadata_failure_and_transport_operations_are_transactional() {
    let mut m = Machine::default();
    m.hardware
        .cassette
        .mount(firmware::cassette_test_image(), 2400)
        .unwrap();
    m.hardware.cassette.tick(7_159_263, true);
    let position = m.hardware.cassette.position_cycles();
    m.cpu.pc = 0x4321;
    let incoming = firmware::cassette_test_image();
    // Vec<Block>'s initial four-entry reservation: two usize + three u64 fields.
    FAIL_SIZE.with(|s| s.set(4 * (2 * std::mem::size_of::<usize>() + 3 * 8)));
    let result = m.hardware.cassette.mount(incoming, 1200);
    FAIL_SIZE.with(|s| s.set(0));
    assert!(result.unwrap_err().contains("metadata"));
    assert_eq!(m.hardware.cassette.position_cycles(), position);
    assert_eq!(m.hardware.cassette.rate(), 2400);
    assert_eq!(m.cpu.pc, 0x4321);
    DENY.with(|s| s.set(true));
    m.reset();
    m.hardware.cassette.rewind();
    m.hardware.cassette.eject();
    DENY.with(|s| s.set(false));
}

#[test]
fn staged_music_install_reset_and_removal_do_not_allocate() {
    let mut m = Machine::default();
    m.set_model(Model::Msx2);
    let rom = firmware::music_diagnostic_firmware();
    DENY.with(|s| s.set(true));
    m.install_msx_music(rom).unwrap();
    m.reset();
    m.remove_msx_music();
    DENY.with(|s| s.set(false));
    assert!(!m.has_msx_music());
}
