//! Independent bus and original Z80 probes for the fixed MSX2 RAM profile.
use msx_core::{cpu::Bus, firmware, Firmware, Machine, Mapper, Model};

fn machine() -> Machine {
    let mut m = Machine::default();
    m.set_model(Model::Msx2);
    m.hardware.output(0xa8, 0xff);
    m.hardware.write(0xffff, 0xff);
    m
}

#[test]
fn all_segments_all_pages_boundaries_and_alias_based_capacity() {
    let mut m = machine();
    let h = &mut m.hardware;
    // No readback or backing-vector length is used to establish capacity.
    // Each segment must retain its own values after every other segment is written.
    for page in 0..4u16 {
        for segment in 0..16u8 {
            h.output(0xfc + page, segment);
            for offset in [0, 1, 0x3ffe] {
                h.write(
                    page * 0x4000 + offset,
                    (segment * 7).wrapping_add(offset as u8),
                );
            }
            if page != 3 {
                h.write(page * 0x4000 + 0x3fff, 0x80 + segment);
            }
        }
        for segment in 0..16u8 {
            h.output(0xfc + page, segment);
            for offset in [0, 1, 0x3ffe] {
                assert_eq!(
                    h.read(page * 0x4000 + offset),
                    (segment * 7).wrapping_add(offset as u8)
                );
            }
            if page != 3 {
                assert_eq!(h.read(page * 0x4000 + 0x3fff), 0x80 + segment);
            }
        }
    }
    // The same segment is shared across pages; changing one selector is independent.
    for page in 0..4 {
        h.output(0xfc + page, 13);
    }
    h.write(0x0123, 0x6b);
    for page in 0..4 {
        assert_eq!(h.read(page * 0x4000 + 0x123), 0x6b);
    }
    h.output(0xfd, 12);
    h.write(0x4123, 0x7c);
    assert_eq!(h.read(0x0123), 0x6b);
    assert_eq!(h.read(0x8123), 0x6b);
    assert_eq!(h.read(0xc123), 0x6b);
    assert_eq!(h.read(0x4123), 0x7c);
    // Adjacent page endpoints translate independently.
    h.output(0xfc, 5);
    h.output(0xfd, 6);
    h.write(0x3fff, 0x31);
    h.write(0x4000, 0x42);
    h.output(0xfe, 5);
    assert_eq!(h.read(0xbfff), 0x31);
    h.output(0xfe, 6);
    assert_eq!(h.read(0x8000), 0x42);
}

#[test]
fn port_masking_hidden_slot_io_rom_and_slot_isolation_ffff_precedence() {
    let mut m = machine();
    m.use_diagnostic().unwrap();
    let h = &mut m.hardware;
    h.output(0xa8, 0); // RAM hidden; all low-byte port aliases still work.
    let rom = h.read(0x123);
    h.write(0x123, rom ^ 0xff);
    assert_eq!(h.read(0x123), rom);
    for page in 0..4u16 {
        for value in 0..=255u8 {
            h.output(0xabfc + page, value);
            assert_eq!(h.input(0x12fc + page), 0xf0 | (value & 15));
        }
    }
    h.output(0xa8, 0xff);
    h.write(0xffff, 0xff);
    h.write(0x0123, 0x51);
    assert_eq!(h.read(0xc123), 0x51); // all four selected segment 15
    h.output(0xff, 7);
    h.write(0xfffe, 0x62);
    // FFFFh is the expanded-slot register even with empty/sub-ROM page 3.
    for secondary in [0xff, 0x3f, 0x7f, 0xbf] {
        h.write(0xffff, secondary);
        assert_eq!(h.read(0xffff), !secondary);
        assert_eq!(h.input(0xff), 0xf7);
    }
    h.write(0xffff, 0xff);
    assert_eq!(h.read(0xfffe), 0x62);
    h.output(0xfc, 7);
    assert_eq!(h.read(0x3fff), 0); // FFFFh never wrote segment storage
    assert_eq!(h.read(0x3ffe), 0x62);
    // Sub-ROM and both empty secondary slots must hide RAM and ignore writes.
    for secondary in [0xfc, 0xfd, 0xfe] {
        h.write(0xffff, secondary);
        let value = h.read(0);
        h.write(0, 0x99);
        assert_eq!(h.read(0), value);
        assert_eq!(value, if secondary == 0xfc { b'C' } else { 0xff });
    }
    h.write(0xffff, 0xff);
    h.output(0xfc, 15);
    assert_eq!(h.read(0x123), 0x51);
    h.output(0xa8, 0xaa); // empty primary slot
    h.write(0x123, 0x77);
    assert_eq!(h.read(0x123), 0xff);
    h.write(0xffff, 0x11);
    assert_eq!(h.read(0xffff), 0xff);
    h.output(0xa8, 0xff);
    assert_eq!(h.read(0xffff), 0);
    assert_eq!(h.read(0x123), 0x51);
}

#[test]
fn msx1_keeps_flat_ram_and_unconnected_mapper_ports() {
    let mut m = Machine::default();
    let h = &mut m.hardware;
    h.output(0xa8, 0xff);
    for page in 0..4 {
        h.write(page * 0x4000, page as u8 + 1);
        h.output(0xfc + page, 15);
        assert_eq!(h.input(0xfc + page), 0xff);
    }
    for page in 0..4 {
        assert_eq!(h.read(page * 0x4000), page as u8 + 1);
    }
    h.write(0xffff, 0x73);
    h.output(0xff, 0);
    assert_eq!(h.read(0xffff), 0x73);
}

#[test]
fn reset_is_zero_before_execution_and_original_loaders_initialize_through_z80() {
    for firmware in [
        Firmware::WeaveBasic,
        Firmware::Diagnostic,
        Firmware::Disk,
        Firmware::Cbios,
        Firmware::User,
    ] {
        let mut m = machine();
        match firmware {
            Firmware::Diagnostic => m.use_diagnostic().unwrap(),
            Firmware::Disk => {
                m.mount_disk(firmware::demo_disk(), true).unwrap();
                m.boot_disk().unwrap();
            }
            Firmware::Cbios => {
                m.use_cbios().unwrap();
            }
            // Authored supplied firmware chooses its own values, not the original-loader defaults.
            Firmware::User => {
                let mut main = vec![0; 32768];
                main[0x2d] = 1;
                main[..18].copy_from_slice(&[
                    0x3e, 9, 0xd3, 0xfc, 0x3e, 8, 0xd3, 0xfd, 0x3e, 7, 0xd3, 0xfe, 0x3e, 6, 0xd3,
                    0xff, 0x76, 0,
                ]);
                m.load_msx2_firmware(&main, &firmware::msx2_subrom())
                    .unwrap();
            }
            _ => {}
        }
        for port in 0xfc..=0xff {
            m.hardware.output(port, 15);
        }
        let ram = m.hardware.ram.as_ptr();
        m.hardware.ram.fill(0x5a);
        m.reset();
        assert_eq!(m.hardware.ram.as_ptr(), ram);
        assert!(m.hardware.ram.iter().all(|&b| b == 0));
        assert_eq!(m.cpu.pc, 0);
        for port in 0xfc..=0xff {
            assert_eq!(m.hardware.input(port), 0xf0);
        }
        let frames = if firmware == Firmware::Cbios {
            300
        } else if firmware == Firmware::Disk {
            80
        } else {
            1
        };
        for _ in 0..frames {
            m.frame();
        }
        let expected = if firmware == Firmware::User {
            [9, 8, 7, 6]
        } else {
            [3, 2, 1, 0]
        };
        for (page, value) in expected.into_iter().enumerate() {
            assert_eq!(
                m.hardware.input(0xfc + page as u16),
                0xf0 | value,
                "{firmware:?}"
            );
        }
        if firmware == Firmware::WeaveBasic {
            assert!(m.basic.console.transcript.contains("READY"));
        }
        if firmware == Firmware::Disk {
            assert_eq!(m.hardware.read(0xf104), 1);
        }
    }
}

#[test]
fn original_cartridge_probes_capacity_through_bundled_cbios_and_reset() {
    let mut m = machine();
    m.load_cartridge(&firmware::ram_mapper_cartridge()).unwrap();
    for _ in 0..2 {
        for _ in 0..600 {
            m.frame();
            if m.hardware.read(0xc210) == 0xa5 {
                break;
            }
        }
        assert_eq!(m.hardware.firmware, Firmware::Cbios);
        assert_eq!(m.hardware.read(0xc210), 0xa5, "pc={:04x}", m.cpu.pc);
        assert_eq!(&m.hardware.vdp.vram[0x1800..0x180f], b"RAM MAPPER PASS");
        m.reset();
    }
    // A missing mapper must produce the failure path rather than a false pass.
    m.set_model(Model::Msx1);
    m.load_cartridge(&firmware::ram_mapper_cartridge()).unwrap();
    for _ in 0..600 {
        m.frame();
        if m.hardware.read(0xc210) == 0xee {
            break;
        }
    }
    assert_eq!(m.hardware.read(0xc210), 0xee);
    assert_eq!(&m.hardware.vdp.vram[0x1800..0x180f], b"RAM MAPPER FAIL");
}

#[test]
fn failed_imports_preserve_nondefault_selectors_ram_firmware_disk_and_unsaved_sram() {
    let mut m = machine();
    m.load_cartridge_with_mapper(
        &firmware::mapper_test_cartridge(Mapper::Ascii8Sram8),
        Mapper::Ascii8Sram8,
    )
    .unwrap();
    m.load_msx2_firmware(&firmware::msx2_bios(), &firmware::msx2_subrom())
        .unwrap();
    m.mount_disk(firmware::demo_disk(), false).unwrap();
    m.hardware.output(0xa8, 0xd4);
    m.hardware.write(0xffff, 0xff);
    m.hardware.write(0x7000, 0x20);
    m.hardware.write(0x8123, 0x9a);
    for (port, segment) in [(0xfc, 15), (0xfd, 12), (0xfe, 8), (0xff, 5)] {
        m.hardware.output(port, segment);
    }
    m.hardware.write(0xc123, 0x6b);
    let ram = m.hardware.ram.clone();
    let bios = m.hardware.bios.clone();
    let sub = m.hardware.subrom.clone();
    let cartridge = m.hardware.cartridge.clone();
    let sram = m.cartridge_sram().to_vec();
    assert!(sram.contains(&0x9a));
    let disk = m.hardware.fdc.disk.as_ref().unwrap().bytes().to_vec();
    let pc = m.cpu.pc;
    assert!(m.load_cartridge(&[0; 17]).is_err());
    assert!(m.load_msx2_firmware(&bios, &[0; 16384]).is_err());
    assert!(m.load_bios(&bios).is_err());
    assert!(m.mount_disk(vec![0; 17], true).is_err());
    assert!(m.restore_cartridge_sram(&[0; 2]).is_err());
    for (port, segment) in [(0xfc, 15), (0xfd, 12), (0xfe, 8), (0xff, 5)] {
        assert_eq!(m.hardware.input(port), 0xf0 | segment);
    }
    assert_eq!(m.hardware.ram, ram);
    assert_eq!(m.hardware.read(0xc123), 0x6b);
    assert_eq!(m.hardware.bios, bios);
    assert_eq!(m.hardware.subrom, sub);
    assert_eq!(m.hardware.cartridge, cartridge);
    assert_eq!(m.cartridge_sram(), sram);
    assert_eq!(m.hardware.firmware, Firmware::User);
    assert_eq!(m.hardware.fdc.disk.as_ref().unwrap().bytes(), disk);
    assert!(!m.hardware.fdc.disk.as_ref().unwrap().write_protected);
    assert_eq!(m.cpu.pc, pc);
}
