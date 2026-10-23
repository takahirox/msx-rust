use msx_core::{
    cpu::Bus,
    disk::{BYTE_CYCLES, SERVICE_TIMEOUT},
    firmware, BootPolicy, Firmware, Machine, Mapper, Model,
};

fn fixture(text: &str, size: usize, fill: u8) -> Vec<u8> {
    let mut bytes = vec![fill; size];
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let (offset, data) = line.split_once(':').unwrap();
        let offset = usize::from_str_radix(offset, 16).unwrap();
        for (i, value) in data.split_whitespace().enumerate() {
            bytes[offset + i] = u8::from_str_radix(value, 16).unwrap();
        }
    }
    bytes
}
fn rom() -> Vec<u8> {
    fixture(
        include_str!("../../../tests/fixtures/philips-disk.hex"),
        16384,
        0xff,
    )
}
fn machine(model: Model) -> Machine {
    let mut m = Machine::default();
    m.set_model(model);
    m.install_philips_disk_rom(rom()).unwrap();
    m.hardware.slots = 0xf8;
    m.hardware.secondary[3] = 0xff;
    m
}

#[test]
fn page_one_slot_two_only_rom_protection_and_no_hidden_acknowledgement() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = machine(model);
        assert_eq!(m.hardware.firmware, Firmware::WeaveBasic);
        for address in [0x4000, 0x4001, 0x4100, 0x7ff7] {
            let before = m.hardware.read(address);
            m.hardware.write(address, before ^ 255);
            assert_eq!(m.hardware.read(address), before);
        }
        m.mount_disk(vec![0; 368640], true).unwrap();
        m.hardware.fdc.irq = true;
        m.hardware.fdc.drq = true;
        // Select slot 2 on all pages: this external device maps only page 1.
        m.hardware.slots = 0xaa;
        for address in [0, 0x3ff8, 0x3fff, 0x8000, 0xbff8, 0xbfff, 0xfff8, 0xffff] {
            assert_eq!(m.hardware.read(address), 255);
            m.hardware.write(address, 0xd0);
            assert!(m.hardware.fdc.irq);
            assert!(m.hardware.fdc.drq);
        }
        for slot in [0, 1, 3] {
            m.hardware.slots = (0xaa & !12) | slot << 2;
            for address in 0x7ff8..=0x7fff {
                m.hardware.read(address);
                m.hardware.write(address, 0);
            }
            assert!(m.hardware.fdc.irq);
            assert!(m.hardware.fdc.drq);
        }
        m.hardware.slots = 0xf8;
        assert_eq!(m.hardware.read(0x7ffd) & 4, 0); // hidden reads did not acknowledge change
        assert_eq!(m.hardware.read(0x7fff), 0x3f);
        assert_eq!(m.hardware.read(0x7fff), 0x3f);
        assert!(!m.hardware.irq());
        m.hardware.read(0x7ff8);
        assert!(!m.hardware.fdc.irq);
        assert_eq!(m.hardware.read(0x7fff), 0x7f);
        if model == Model::Msx2 {
            m.hardware.write(0xffff, 0x93);
            assert_eq!(m.hardware.read(0xffff), !0x93);
            assert!(m.hardware.fdc.drq);
            m.hardware.secondary[3] = 0xfc;
            m.hardware.slots = 0xfb; // page 0 sub-ROM; page 1 remains disk
            m.hardware.subrom.resize(16384, 0x6b);
            assert_eq!(m.hardware.read(0x100), 0x6b);
            m.hardware.write(0x100, 0);
            assert_eq!(m.hardware.read(0x100), 0x6b);
        }
    }
}

#[test]
fn drive_select_side_motor_latches_change_and_signal_polarity() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = machine(model);
        let mut image = vec![0x5b; 737280];
        image[9 * 512..18 * 512].fill(0xa9);
        m.mount_disk(image, false).unwrap();
        // No Weave geometry extension or acknowledgement at 7FFE.
        assert_eq!(m.hardware.read(0x7ffe), 255);
        m.hardware.write(0x7ffe, 0);
        // Reads for absent B/no drive must not consume A's change indication.
        for select in [0x81, 0x83] {
            m.hardware.write(0x7ffd, select);
            assert_eq!(m.hardware.read(0x7ffd), select | 4);
        }
        m.hardware.write(0x7ffd, 0xc2);
        assert_eq!(m.hardware.read(0x7ffd), 0xc2);
        assert_eq!(m.hardware.read(0x7ffd), 0xc6);
        // All latch bits read back; only bit zero of side is electrically used.
        for side in [0xfe, 0xff] {
            m.hardware.write(0x7ffc, side);
            assert_eq!(m.hardware.read(0x7ffc), side);
            assert_eq!(m.hardware.fdc.side, side & 1);
        }
        for control in [0, 1, 2, 3, 0x80, 0x81, 0x82, 0x83] {
            m.hardware.write(0x7ffd, control);
            m.hardware.write(0x7ff8, 0x80);
            let ready = control == 0x80 || control == 0x82;
            if ready {
                assert_eq!(m.hardware.read(0x7fff), 255);
                m.hardware.fdc.tick(BYTE_CYCLES);
                assert_eq!(m.hardware.read(0x7fff), 0x7f);
                assert_eq!(m.hardware.read(0x7fff), 0x7f);
                assert_eq!(m.hardware.read(0x7ffb), 0xa9);
                m.hardware.write(0x7ff8, 0xd8);
            } else {
                assert_eq!(m.hardware.read(0x7fff), 0xbf);
                assert_eq!(m.hardware.read(0x7ff8) & 0x80, 0x80);
                assert_eq!(m.hardware.read(0x7fff), 255);
            }
        }
        // Ignored latch bits and equivalent A selectors do not abort a transfer.
        m.hardware.write(0x7ffd, 0x80);
        m.hardware.write(0x7ff8, 0x80);
        m.hardware.write(0x7ffd, 0xfe);
        m.hardware.write(0x7ffc, 0x7f);
        m.hardware.fdc.tick(BYTE_CYCLES);
        assert_eq!(m.hardware.read(0x7fff), 0x7f);
        m.reset();
        m.hardware.slots = 0xf8;
        assert_eq!(m.hardware.read(0x7ffc), 0);
        assert_eq!(m.hardware.read(0x7ffd), 4);
        assert_eq!(m.hardware.read(0x7fff), 255);
        assert!(m.has_philips_disk_rom());
        m.mount_disk(vec![0; 368640], false).unwrap();
        m.hardware.write(0x7ffd, 0x80);
        m.hardware.write(0x7ffc, 1);
        m.hardware.write(0x7ff8, 0x80);
        assert_eq!(m.hardware.read(0x7ff8) & 0x10, 0x10); // absent side
        m.eject_disk().unwrap();
        assert!(m.has_philips_disk_rom());
        assert_eq!(m.hardware.read(0x7ffd), 0x80);
        assert_eq!(m.hardware.read(0x7ffd), 0x84);
        m.hardware.write(0x7ffd, 0x80);
        m.hardware.write(0x7ff8, 0x80);
        assert_eq!(m.hardware.read(0x7ff8) & 0x80, 0x80);
    }
}

#[test]
fn transactional_installation_lifecycle_and_firmware_coexistence() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = machine(model);
        m.load_cartridge_with_mapper(
            &firmware::mapper_test_cartridge(Mapper::Ascii8Sram2),
            Mapper::Ascii8Sram2,
        )
        .unwrap();
        m.restore_cartridge_sram(&vec![0x69; 2048]).unwrap();
        if model == Model::Msx2 {
            m.load_msx2_firmware(firmware::CBIOS_MSX2_MAIN, firmware::CBIOS_MSX2_SUB)
                .unwrap();
        } else {
            m.load_bios(firmware::CBIOS_MSX1_MAIN).unwrap();
        }
        m.mount_disk(vec![0x4c; 368640], false).unwrap();
        m.hardware.fdc.disk.as_mut().unwrap().dirty = true;
        let bios = m.hardware.bios.clone();
        let sub = m.hardware.subrom.clone();
        let cart = m.hardware.cartridge.clone();
        for invalid in [vec![], vec![0; 16384], vec![0; 16385]] {
            m.cpu.pc = 0x1234;
            assert!(m.install_philips_disk_rom(invalid).is_err());
            assert_eq!(m.cpu.pc, 0x1234);
            assert!(m.has_philips_disk_rom());
        }
        m.install_philips_disk_rom(rom()).unwrap();
        m.boot_disk().unwrap();
        assert_eq!(m.hardware.bios, bios);
        assert_eq!(m.hardware.subrom, sub);
        assert_eq!(m.hardware.cartridge, cart);
        assert_eq!(m.cartridge_sram(), vec![0x69; 2048]);
        assert_eq!(m.boot_policy, BootPolicy::Explicit);
        assert_eq!(m.hardware.firmware, Firmware::User);
        assert!(m.eject_disk().is_err());
        assert!(m.mount_disk(vec![0; 368640], false).is_err());
        m.remove_philips_disk_rom();
        assert!(!m.has_philips_disk_rom());
        assert!(m.hardware.fdc.disk.as_ref().unwrap().dirty);
        assert_eq!(m.hardware.bios, bios);
        assert_eq!(m.cartridge_sram(), vec![0x69; 2048]);
        assert!(m.boot_disk().is_err()); // explicit system firmware still protected
        m.install_philips_disk_rom(rom()).unwrap();
        m.hardware.fdc.disk.as_mut().unwrap().dirty = false;
        m.eject_disk().unwrap();
        assert_eq!(m.hardware.firmware, Firmware::User);
        m.unload_cartridge();
        assert!(m.has_philips_disk_rom());
        m.use_cbios().unwrap();
        assert!(m.has_philips_disk_rom());
        m.use_builtin();
        assert!(m.has_philips_disk_rom());
        m.set_model(if model == Model::Msx1 {
            Model::Msx2
        } else {
            Model::Msx1
        });
        assert!(m.has_philips_disk_rom());
        m.remove_philips_disk_rom();
        m.mount_disk(firmware::demo_disk(), true).unwrap();
        m.boot_disk().unwrap();
        assert!(m.install_philips_disk_rom(rom()).is_err());
        assert_eq!(m.hardware.firmware, Firmware::Disk);
    }
}

#[test]
fn real_system_discovers_ab_rom_and_executes_non_wv_sector_with_exact_writes() {
    for model in [Model::Msx1, Model::Msx2] {
        for size in [368640, 737280] {
            for (protected, mode, status) in
                [(false, 0, 0), (true, 0, 0x40), (false, 1, 0), (false, 2, 4)]
            {
                let mut m = Machine::default();
                m.set_model(model);
                m.load_cartridge_with_mapper(
                    &firmware::mapper_test_cartridge(Mapper::Ascii8Sram2),
                    Mapper::Ascii8Sram2,
                )
                .unwrap();
                let mut main = fixture(
                    include_str!("../../../tests/fixtures/philips-system.hex"),
                    32768,
                    0,
                );
                main[0x2d] = u8::from(model == Model::Msx2);
                let mut sub = vec![0; 16384];
                sub[..2].copy_from_slice(b"CD");
                sub[0x100] = 0x7b;
                if model == Model::Msx2 {
                    m.load_msx2_firmware(&main, &sub).unwrap();
                } else {
                    m.load_bios(&main).unwrap();
                }
                let mut disk = fixture(
                    include_str!("../../../tests/fixtures/philips-sector.hex"),
                    size,
                    0x35,
                );
                disk[0x1f0] = mode;
                disk[0x1f1] = u8::from(size == 737280);
                assert_ne!(&disk[510..512], b"WV");
                let mut expected = disk.clone();
                if !protected && mode == 0 {
                    let sector = if size == 737280 { 17 } else { 8 };
                    expected[sector * 512..(sector + 1) * 512].fill(0xa6);
                }
                m.mount_disk(disk, protected).unwrap();
                m.install_philips_disk_rom(rom()).unwrap();
                m.boot_disk().unwrap();
                for _ in 0..1_000_000 {
                    m.step();
                    if m.cpu.halted {
                        break;
                    }
                }
                assert_eq!(
                    m.hardware.read(0xc000),
                    0xa5,
                    "read/execute pc={:04x}",
                    m.cpu.pc
                );
                assert_eq!(m.hardware.read(0xc002), 0x5a, "write pc={:04x}", m.cpu.pc);
                assert_eq!(m.hardware.read(0xc001), status);
                assert_eq!(m.hardware.fdc.disk.as_ref().unwrap().bytes(), expected);
                assert_eq!(m.hardware.firmware, Firmware::User);
                assert_eq!(m.hardware.bios, main);
                assert_eq!(m.hardware.read(0xc010), b'A');
                assert_eq!(m.cartridge_sram()[0x123], 0x6d);
                if model == Model::Msx2 {
                    assert_eq!(m.hardware.subrom, sub);
                    assert_eq!(m.hardware.read(0xc011), 0x7b);
                    assert_eq!(m.hardware.input(0xfe), 0xf1);
                    assert_eq!(m.hardware.input(0xff), 0xf2);
                    // The executed sector lives in mapper segment 1; segment 0 is separate.
                    m.hardware.output(0xfe, 0);
                    assert_eq!(m.hardware.read(0x8000), 0);
                    m.hardware.output(0xfe, 1);
                    assert_eq!(m.hardware.read(0x8000), 0x3e);
                }
            }
        }
    }
}

#[test]
fn bus_write_timeout_abort_side_motor_and_media_changes_never_commit_partial_sectors() {
    for model in [Model::Msx1, Model::Msx2] {
        for change in 0..6 {
            let mut m = machine(model);
            m.mount_disk(vec![0x29; 737280], false).unwrap();
            m.hardware.write(0x7ffd, 0x80);
            m.hardware.write(0x7ff8, 0xa0);
            for _ in 0..511 {
                m.hardware.fdc.tick(BYTE_CYCLES);
                m.hardware.write(0x7ffb, 0x5a);
            }
            match change {
                0 => m.hardware.write(0x7ff8, 0xd8),
                1 => {
                    m.hardware.fdc.tick(BYTE_CYCLES);
                    m.hardware.fdc.tick(SERVICE_TIMEOUT);
                }
                2 => m.hardware.write(0x7ffc, 1),
                3 => m.hardware.write(0x7ffd, 0),
                4 => m.reset(),
                _ => m.remove_philips_disk_rom(),
            }
            let disk = m.hardware.fdc.disk.as_ref().unwrap();
            assert!(disk.bytes().iter().all(|b| *b == 0x29));
            assert!(!disk.dirty);
        }
    }
}

#[test]
fn cpu_observes_incremental_seek_interrupts_and_addresses_retained_position() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = Machine::default();
        m.set_model(model);
        let mut main = fixture(
            include_str!("../../../tests/fixtures/philips-system.hex"),
            32768,
            0,
        );
        main[0x2d] = u8::from(model == Model::Msx2);
        if model == Model::Msx2 {
            let mut sub = vec![0; 16384];
            sub[..2].copy_from_slice(b"CD");
            m.load_msx2_firmware(&main, &sub).unwrap();
        } else {
            m.load_bios(&main).unwrap();
        }
        let mut image = fixture(
            include_str!("../../../tests/fixtures/philips-movement.hex"),
            368640,
            0x35,
        );
        image[2 * 9 * 512..3 * 9 * 512].fill(0x62);
        let mut expected = image.clone();
        let trace = [2, 1, 0xff, 2, 0xbf, 0, 0xff, 2];
        expected[(2 * 9 + 8) * 512..(2 * 9 + 8) * 512 + 8].copy_from_slice(&trace);
        m.mount_disk(image, false).unwrap();
        m.install_philips_disk_rom(rom()).unwrap();
        m.boot_disk().unwrap();
        for _ in 0..1_000_000 {
            m.step();
            if m.cpu.halted {
                break;
            }
        }
        assert!(m.cpu.halted);
        assert_eq!(m.hardware.read(0xc020), 0xa5, "pc={:04x}", m.cpu.pc);
        assert_eq!(
            (0x9000..0x9008)
                .map(|a| m.hardware.read(a))
                .collect::<Vec<_>>(),
            trace
        );
        assert_eq!(m.hardware.fdc.disk.as_ref().unwrap().bytes(), expected);
        assert_eq!(m.hardware.fdc.track, 2);
        assert!(!m.hardware.irq());
    }
}

#[test]
fn profile_removal_and_reset_cancel_pending_movement() {
    for model in [Model::Msx1, Model::Msx2] {
        for remove in [false, true] {
            let mut m = machine(model);
            m.mount_disk(vec![0; 368640], false).unwrap();
            m.hardware.write(0x7ffd, 0x80);
            m.hardware.write(0x7ffb, 79);
            m.hardware.write(0x7ff8, 0x10);
            m.hardware.fdc.tick(30_000);
            assert_eq!(m.hardware.fdc.track, 2);
            if remove {
                m.remove_philips_disk_rom();
            } else {
                m.reset();
            }
            m.hardware.fdc.tick(u32::MAX);
            assert_eq!(m.hardware.fdc.track, 0);
            assert_eq!(m.hardware.fdc.control, 0);
            assert!(!m.hardware.fdc.irq);
            assert!(!m.hardware.fdc.drq);
            assert!(m.hardware.fdc.disk.is_some());
        }
    }
}
