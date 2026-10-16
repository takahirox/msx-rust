//! Original, generated ROM fixtures; no game or system ROM data.
use msx_core::{cpu::Bus, firmware, Machine, Mapper};

fn image(size: usize) -> Vec<u8> {
    let mut rom = vec![0; size];
    for (bank, bytes) in rom.chunks_mut(8192).enumerate() {
        bytes.fill(bank as u8);
    }
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    rom
}
fn machine(mapper: Mapper) -> Machine {
    let mut m = Machine::default();
    m.load_cartridge_owned(image(128 * 1024), mapper).unwrap();
    m
}
fn windows(m: &mut Machine) -> [u8; 4] {
    [0x4100, 0x6100, 0x8100, 0xa100].map(|a| m.hardware.read(a))
}
#[test]
fn ascii8_all_register_ranges_and_holes() {
    let mut m = machine(Mapper::Ascii8);
    assert_eq!(windows(&mut m), [0, 0, 0, 0]);
    for (i, (start, end)) in [
        (0x6000, 0x67ff),
        (0x6800, 0x6fff),
        (0x7000, 0x77ff),
        (0x7800, 0x7fff),
    ]
    .into_iter()
    .enumerate()
    {
        m.hardware.write(start, (i + 4) as u8);
        assert_eq!(windows(&mut m)[i], (i + 4) as u8);
        m.hardware.write(end, (i + 8) as u8);
        assert_eq!(windows(&mut m)[i], (i + 8) as u8);
    }
    for a in [0x5fff, 0x8000, 0x9000, 0xbfff] {
        m.hardware.write(a, 15);
    }
    assert_eq!(windows(&mut m), [8, 9, 10, 11]);
    m.hardware.write(0x7800, 0xff);
    assert_eq!(m.hardware.read(0xbfff), 15); // unconnected bank address lines
}
#[test]
fn ascii16_register_ranges_and_holes() {
    let mut m = machine(Mapper::Ascii16);
    assert_eq!(windows(&mut m), [0, 1, 0, 1]);
    for a in [0x6000, 0x67ff] {
        m.hardware.write(a, 3);
        assert_eq!(windows(&mut m), [6, 7, 0, 1]);
    }
    for a in [0x7000, 0x77ff] {
        m.hardware.write(a, 5);
        assert_eq!(windows(&mut m), [6, 7, 10, 11]);
    }
    for a in [0x5fff, 0x6800, 0x6fff, 0x7800, 0x7fff, 0x8000] {
        m.hardware.write(a, 0);
    }
    assert_eq!(windows(&mut m), [6, 7, 10, 11]);
    m.hardware.write(0x7000, 0xff);
    assert_eq!(m.hardware.read(0xbfff), 15);
}
#[test]
fn konami_fixed_first_bank_full_decoding_and_mirrors() {
    let mut m = machine(Mapper::Konami);
    assert_eq!(windows(&mut m), [0, 1, 2, 3]);
    m.hardware.slots = 0x55;
    for a in [0x4000, 0x5fff, 0x0000, 0xc000] {
        m.hardware.write(a, 7);
    }
    assert_eq!(windows(&mut m), [0, 1, 2, 3]);
    for (i, (start, end)) in [(0x6000, 0x7fff), (0x8000, 0x9fff), (0xa000, 0xbfff)]
        .into_iter()
        .enumerate()
    {
        for a in [start, end] {
            m.hardware.write(a, (i + 8) as u8);
            assert_eq!(windows(&mut m)[i + 1], (i + 8) as u8);
        }
    }
    assert_eq!(m.hardware.read(0x0100), 0);
    assert_eq!(m.hardware.read(0x2100), 8);
    assert_eq!(m.hardware.read(0xc100), 9);
    assert_eq!(m.hardware.read(0xe100), 10);
}
#[test]
fn scc_exact_register_decoding_overlay_and_different_mirrors() {
    let mut m = machine(Mapper::KonamiScc);
    m.hardware.slots = 0x55;
    assert_eq!(windows(&mut m), [0, 1, 2, 3]);
    for (i, (start, end)) in [
        (0x5000, 0x57ff),
        (0x7000, 0x77ff),
        (0x9000, 0x97ff),
        (0xb000, 0xb7ff),
    ]
    .into_iter()
    .enumerate()
    {
        for a in [start, end] {
            m.hardware.write(a, (i + 8) as u8);
            assert_eq!(windows(&mut m)[i], (i + 8) as u8);
        }
    }
    for a in [
        0x4fff, 0x5800, 0x6000, 0x6fff, 0x7800, 0x8000, 0x8fff, 0x9800, 0xa000, 0xafff, 0xb800,
    ] {
        m.hardware.write(a, 0);
    }
    assert_eq!(windows(&mut m), [8, 9, 10, 11]);
    assert_eq!(m.hardware.read(0x0100), 10);
    assert_eq!(m.hardware.read(0x2100), 11);
    assert_eq!(m.hardware.read(0xc100), 8);
    assert_eq!(m.hardware.read(0xe100), 9);
    m.hardware.write(0x97ff, 0xff); // low six bits enable SCC
    assert_eq!(m.hardware.read(0x97ff), 15);
    assert_eq!(m.hardware.read(0x9800), 0xff);
    assert_eq!(m.hardware.read(0x9fff), 0xff);
    assert_eq!(m.hardware.read(0x1800), 15); // ROM mirror has no SCC overlay
    m.hardware.write(0x9800, 0);
    assert_eq!(m.hardware.read(0x9800), 0xff);
    m.hardware.write(0x9000, 14);
    assert_eq!(m.hardware.read(0x9800), 14);
    m.reset();
    assert_eq!(m.hardware.read(0x9800), 2);
}
#[test]
fn reset_slots_replacement_ejection_and_system_rom() {
    for kind in [
        Mapper::Ascii8,
        Mapper::Ascii16,
        Mapper::Konami,
        Mapper::KonamiScc,
    ] {
        let mut m = machine(kind);
        let initial = windows(&mut m);
        m.hardware.slots = 0xff; // writes now go to RAM, not the cartridge
        m.hardware.write(0x6000, 9);
        m.hardware.write(0x7000, 9);
        m.hardware.write(0x9000, 9);
        m.hardware.slots = 0xd4;
        assert_eq!(windows(&mut m), initial);
        m.hardware.write(0x6000, 9);
        m.hardware.write(0x7000, 9);
        m.hardware.write(0x9000, 9);
        m.reset();
        assert_eq!(windows(&mut m), initial);
        // Hidden ASCII pages read open bus; Konami mirror behavior is tested separately.
        if matches!(kind, Mapper::Ascii8 | Mapper::Ascii16) {
            m.hardware.slots = 0x55;
            assert_eq!(m.hardware.read(0), 0xff);
            assert_eq!(m.hardware.read(0xffff), 0xff);
        }
        m.load_bios(&vec![0; 32768]).unwrap();
        assert_eq!(m.hardware.slots, 0xf0);
        m.hardware.slots = 0xd4;
        assert_eq!(windows(&mut m), initial);
        m.load_cartridge(&firmware::test_cartridge()).unwrap();
        assert_eq!(m.hardware.read(0x4000), 0); // system BIOS still controls slots
        m.hardware.slots = 0xd4;
        m.hardware.write(0x6000, 9);
        assert_eq!(m.hardware.read(0x4000), b'A');
        m.unload_cartridge();
        m.hardware.slots = 0x55;
        assert_eq!(m.hardware.read(0x4000), 0xff);
        m.use_builtin();
        m.frame();
        assert!(m.basic.console.transcript.contains("READY"));
    }
}
#[test]
fn cartridge_installation_clears_previous_hardware_state() {
    let mut m = Machine::default();
    m.frame(); // Populate video, audio and BASIC state before replacing it.
    m.hardware.ram.fill(0x5a);
    m.hardware.slots = 0xff;
    m.hardware.ppi_c = 8;
    m.hardware.ppi_control = 0x80;
    m.hardware.key(8, 0, true);
    m.hardware.joystick = 0;
    m.hardware.psg.select(8);
    m.hardware.psg.write(15);
    m.hardware.psg.tick(10000);
    m.hardware.vdp.control(0x55); // Leave a pending control-port latch.
    m.text_key(b'X');
    m.hardware.output(0xf1, b'Y');

    m.load_cartridge_owned(image(2 * 1024 * 1024), Mapper::Ascii8)
        .unwrap();
    assert!(m.hardware.ram.iter().all(|&b| b == 0));
    assert_eq!(m.hardware.slots, 0xd4);
    assert_eq!(m.hardware.ppi_c, 0);
    assert_eq!(m.hardware.ppi_control, 0x82);
    assert_eq!(m.hardware.keyboard, [255; 11]);
    assert_eq!(m.hardware.joystick, 0x3f);
    assert_eq!(m.hardware.input(0xf0), 0);
    assert_eq!(m.hardware.vdp.vram, [0; 16384]);
    assert_eq!(m.hardware.vdp.registers, [0; 8]);
    assert!(m.hardware.vdp.pixels.iter().all(|&b| b == 0));
    assert_eq!(m.hardware.vdp.line, 0);
    assert_eq!(m.hardware.vdp.frames, 0);
    m.hardware.vdp.control(7);
    m.hardware.vdp.control(0x87);
    assert_eq!(m.hardware.vdp.registers[7], 7);
    assert_eq!(m.hardware.vdp.read_status(), 0x1f);
    assert_eq!(m.hardware.psg.registers, [0; 16]);
    assert_eq!(m.hardware.psg.selected, 0);
    assert!(m.hardware.psg.samples.is_empty());
    let mut fresh_psg = msx_core::psg::Psg::default();
    fresh_psg.tick(10000);
    m.hardware.psg.tick(10000);
    assert_eq!(m.hardware.psg.samples, fresh_psg.samples);
    assert!(m.basic.console.transcript.is_empty());
    m.step(); // No stale firmware service may execute after installation.
    assert!(m.basic.console.transcript.is_empty());
}
#[test]
fn large_images_reach_last_bank_without_truncation() {
    for size in [64, 128, 256, 512, 1024, 2048, 4096] {
        let kind = if size <= 2048 {
            Mapper::Ascii8
        } else {
            Mapper::Ascii16
        };
        let mut rom = image(size * 1024);
        *rom.last_mut().unwrap() = 0x5a;
        let mut m = Machine::default();
        m.load_cartridge_owned(rom, kind).unwrap();
        m.hardware.write(
            if kind == Mapper::Ascii8 {
                0x7800
            } else {
                0x7000
            },
            0xff,
        );
        assert_eq!(m.hardware.read(0xbfff), 0x5a, "{size} KiB");
        assert_eq!(m.hardware.cartridge.len(), size * 1024);
    }
}
#[test]
fn validation_and_auto_are_conservative_and_transactional() {
    let mut m = machine(Mapper::Ascii8);
    m.hardware.write(0x7800, 7);
    m.hardware.ram[0xc000] = 99;
    m.cpu.pc = 0x4321;
    let old_bios = m.hardware.bios.clone();
    for (rom, kind) in [
        (vec![], Mapper::Ascii8),
        (image(65536), Mapper::Auto),
        (image(65536), Mapper::Plain),
        (vec![0; 65536], Mapper::Ascii8),
        (image(49152), Mapper::Ascii8),
        (image(512 * 1024), Mapper::Konami),
        (image(1024 * 1024), Mapper::KonamiScc),
        (image(4 * 1024 * 1024), Mapper::Ascii8),
        (image(8 * 1024 * 1024), Mapper::Ascii16),
    ] {
        assert!(m.load_cartridge_owned(rom, kind).is_err());
        assert_eq!(m.hardware.read(0xbfff), 7);
        assert_eq!(m.hardware.ram[0xc000], 99);
        assert_eq!(m.cpu.pc, 0x4321);
        assert_eq!(m.hardware.bios, old_bios);
    }
    assert!(Mapper::try_from(6).is_err());
    assert!(m.load_bios(&vec![0; 65536]).is_err());
    assert_eq!(m.cpu.pc, 0x4321);
    // Explicitly supplied hardware can also be used with small images.
    m.load_cartridge_owned(image(32768), Mapper::Ascii8)
        .unwrap();
}
#[test]
fn cpu_executes_from_a_switched_bank() {
    for kind in [
        Mapper::Ascii8,
        Mapper::Ascii16,
        Mapper::Konami,
        Mapper::KonamiScc,
    ] {
        let mut rom = image(128 * 1024);
        let register: u16 = match kind {
            Mapper::Ascii8 => 0x7000,
            Mapper::Ascii16 => 0x7000,
            Mapper::Konami => 0x8000,
            _ => 0x9000,
        };
        rom[0x10..0x18].copy_from_slice(&[
            0x3e,
            7,
            0x32,
            register as u8,
            (register >> 8) as u8,
            0xc3,
            0x00,
            0x80,
        ]);
        let bank = if kind == Mapper::Ascii16 {
            0x4000
        } else {
            0x2000
        };
        rom[7 * bank..7 * bank + 6].copy_from_slice(&[0x3e, 0x5a, 0x32, 0x00, 0xc0, 0x76]);
        let mut m = Machine::default();
        m.load_cartridge_owned(rom, kind).unwrap();
        m.frame();
        assert!(m.cpu.halted);
        assert_eq!(m.hardware.ram[0xc000], 0x5a);
    }
}
