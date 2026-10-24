use msx_core::{cpu::Bus, firmware, BootPolicy, Firmware, Machine, Mapper, Model};

fn byte_ready(m: &mut Machine) {
    for _ in 0..45_100 {
        m.hardware.fdc.tick(16);
        if m.hardware.fdc.drq {
            return;
        }
    }
    panic!("Philips sector did not reach DRQ");
}
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
                byte_ready(&mut m);
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
        byte_ready(&mut m);
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
        assert_eq!(m.hardware.read(0x7ff8), 1);
        m.hardware.fdc.tick(5 * 715_909);
        assert_eq!(m.hardware.read(0x7ff8), 0x10); // absent side, fifth index
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
                if !protected && (mode == 0 || mode == 2) {
                    let sector = if size == 737280 { 17 } else { 8 };
                    expected[sector * 512..(sector + 1) * 512].fill(if mode == 2 {
                        0
                    } else {
                        0xa6
                    });
                    if mode == 2 {
                        expected[sector * 512] = 0xa6;
                    }
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
fn bus_write_underrun_commits_but_explicit_cancellation_discards_partial_sectors() {
    for model in [Model::Msx1, Model::Msx2] {
        for change in 0..6 {
            let mut m = machine(model);
            m.mount_disk(vec![0x29; 737280], false).unwrap();
            m.hardware.write(0x7ffd, 0x80);
            m.hardware.write(0x7ff8, 0xa0);
            for _ in 0..511 {
                byte_ready(&mut m);
                m.hardware.write(0x7ffb, 0x5a);
            }
            match change {
                0 => m.hardware.write(0x7ff8, 0xd8),
                1 => {
                    byte_ready(&mut m);
                    m.hardware.fdc.tick(1_000);
                }
                2 => m.hardware.write(0x7ffc, 1),
                3 => m.hardware.write(0x7ffd, 0),
                4 => m.reset(),
                _ => m.remove_philips_disk_rom(),
            }
            let disk = m.hardware.fdc.disk.as_ref().unwrap();
            let mut expected = vec![0x29; 737280];
            if change == 1 {
                expected[..511].fill(0x5a);
                expected[511] = 0;
            }
            assert_eq!(disk.bytes(), expected);
            assert_eq!(disk.dirty, change == 1);
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
fn profile_removal_and_reset_cancel_every_type_one_phase() {
    for model in [Model::Msx1, Model::Msx2] {
        for remove in [false, true] {
            for elapsed in [30_000, 80_000, 180_000, 2_000_000] {
                let mut m = machine(model);
                m.mount_disk(vec![0; 368640], false).unwrap();
                m.hardware.write(0x7ffd, 0x80);
                m.hardware.write(0x7ffc, 1); // Missing side keeps verification searching.
                m.hardware.write(0x7ffb, 3);
                m.hardware.write(0x7ff8, 0x14);
                m.hardware.fdc.tick(elapsed);
                assert_eq!(m.hardware.fdc.track, if elapsed == 30_000 { 2 } else { 3 });
                if remove {
                    m.remove_philips_disk_rom();
                } else {
                    m.reset();
                }
                m.hardware.fdc.tick(u32::MAX);
                assert_eq!(m.hardware.fdc.track, 0);
                assert_eq!(m.hardware.fdc.control, u8::from(!remove));
                assert!(!m.hardware.fdc.irq);
                assert!(!m.hardware.fdc.drq);
                assert!(m.hardware.fdc.disk.is_some());
            }
        }
    }
}

#[test]
fn cpu_positions_diskless_then_polls_delayed_verification_and_exports_retained_track() {
    for model in [Model::Msx1, Model::Msx2] {
        for insertion_delay in [
            0, 40_000, 80_000, 120_000, 160_000, 200_000, 400_000, 600_000,
        ] {
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
            m.install_philips_disk_rom(fixture(
                include_str!("../../../tests/fixtures/philips-readiness.hex"),
                16384,
                255,
            ))
            .unwrap();
            for _ in 0..1_000_000 {
                m.step();
                if m.hardware.vdp.registers[7] == 0xff || m.cpu.halted {
                    break;
                }
            }
            assert!(!m.cpu.halted, "diskless pc={:04x}", m.cpu.pc);
            assert_eq!(m.hardware.vdp.registers[7], 0xff);
            assert_eq!(m.hardware.fdc.track, 2);
            let mut image = vec![0x35; 368640];
            image[2 * 9 * 512..3 * 9 * 512].fill(0x62);
            let mut expected = image.clone();
            let trace = [0x84, 2, 0x80, 0x81, 0x81, 0xbf, 0x80, 0];
            expected[(2 * 9 + 8) * 512..(2 * 9 + 8) * 512 + 8].copy_from_slice(&trace);
            // Vary insertion phase by executing the real wait loop, keeping the
            // polling bounds independent of frame scheduling and host import time.
            let start = m.cpu.tstates;
            while m.cpu.tstates - start < insertion_delay {
                m.step();
            }
            m.mount_disk(image, false).unwrap();
            for _ in 0..1_000_000 {
                m.step();
                if m.cpu.halted {
                    break;
                }
            }
            assert!(m.cpu.halted);
            assert_eq!(m.hardware.read(0xc040), 0xa5, "pc={:04x}", m.cpu.pc);
            assert_eq!(
                (0x9000..0x9008)
                    .map(|a| m.hardware.read(a))
                    .collect::<Vec<_>>(),
                trace
            );
            assert_eq!(m.hardware.fdc.disk.as_ref().unwrap().bytes(), expected);
        }
    }
}

#[test]
fn cpu_polls_type_two_e_settling_rotational_reads_and_missing_sector_then_exports_trace() {
    for model in [Model::Msx1, Model::Msx2] {
        for size in [368_640, 737_280] {
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
            let mut rom = fixture(
                include_str!("../../../tests/fixtures/philips-search.hex"),
                16384,
                255,
            );
            rom[0x3ff] = u8::from(size == 737_280);
            let mut image = vec![0x35; size];
            image[2 * 512..3 * 512].fill(0x62);
            let mut expected = image.clone();
            let sector = if size == 737_280 { 17 } else { 8 };
            expected[sector * 512..(sector + 1) * 512].fill(0x62);
            let trace = [1, 0xff, 0x7f, 0, 1, 0xff, 0xbf, 0x10].repeat(2);
            expected[sector * 512..sector * 512 + 16].copy_from_slice(&trace);
            m.mount_disk(image, false).unwrap();
            m.install_philips_disk_rom(rom).unwrap();
            // Two failed searches, three transfers, and setup fit below 18M
            // cycles: 2*(settle+5 rev) + 3*(settle+rev+559*114.54544).
            while m.cpu.tstates < 18_000_000 && !m.cpu.halted {
                m.step();
            }
            assert!(m.cpu.halted, "pc={:04x}", m.cpu.pc);
            assert_eq!(m.hardware.read(0xc050), 0xa5, "pc={:04x}", m.cpu.pc);
            assert_eq!(
                (0x9000..0x9010)
                    .map(|a| m.hardware.read(a))
                    .collect::<Vec<_>>(),
                trace
            );
            assert_eq!(m.hardware.fdc.disk.as_ref().unwrap().bytes(), expected);
        }
    }
}

#[test]
fn profile_removal_and_reset_cancel_every_type_two_phase() {
    for model in [Model::Msx1, Model::Msx2] {
        for remove in [false, true] {
            for write in [false, true] {
                // Sector 3 ReadTail spans ceil(159091 + 557 * 114.54544)
                // through ceil(159091 + 558 * 114.54544) - 1: 222893..=223007.
                for elapsed in [
                    107_386, 107_387, 159_091, 160_122, 161_038, 161_039, 164_131, 164_361,
                    222_777, 222_893, 223_007, 223_008, 223_121,
                ] {
                    let mut m = machine(model);
                    m.mount_disk(vec![0x35; 368640], false).unwrap();
                    m.hardware.write(0x7ffd, 0x80);
                    m.hardware.write(0x7ffa, 3);
                    m.hardware.write(0x7ff8, if write { 0xa4 } else { 0x84 });
                    if write && elapsed >= 161_039 {
                        m.hardware.fdc.tick(160_122);
                        m.hardware.write(0x7ffb, 0x17);
                        m.hardware.fdc.tick(elapsed - 160_122);
                    } else {
                        m.hardware.fdc.tick(elapsed);
                    }
                    if elapsed >= 160_122 {
                        if write {
                            m.hardware.write(0x7ffb, 0x17);
                        } else {
                            m.hardware.read(0x7ffb);
                        }
                    }
                    if remove {
                        m.remove_philips_disk_rom();
                    } else {
                        m.reset();
                    }
                    m.hardware.fdc.tick(u32::MAX);
                    assert!(!m.hardware.fdc.irq);
                    assert!(!m.hardware.fdc.drq);
                    let disk = m.hardware.fdc.disk.as_ref().unwrap();
                    assert!(!disk.dirty);
                    assert!(disk.bytes().iter().all(|&byte| byte == 0x35));
                }
            }
        }
    }
}

#[test]
fn cpu_observes_device_clock_overrun_and_underrun_on_both_interfaces_and_geometries() {
    for model in [Model::Msx1, Model::Msx2] {
        for size in [368_640, 737_280] {
            for philips in [false, true] {
                let mut m = Machine::default();
                m.set_model(model);
                let mut image = if philips {
                    vec![0x35; size]
                } else {
                    fixture(
                        include_str!("../../../tests/fixtures/philips-clock-weave.hex"),
                        size,
                        0x35,
                    )
                };
                if philips {
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
                } else {
                    image[2] = if size == 737_280 { 0xf9 } else { 0xf8 };
                }
                // Sector 3 is payload for Philips, but the Weave loader also
                // loads it. Keep diagnostic code below the first loaded sector.
                for i in 0..512 {
                    image[1024 + i] = (i * 73 + i / 256) as u8;
                }
                let mut expected = image.clone();
                expected[5 * 512..6 * 512].fill(0);
                expected[5 * 512] = 0x61;
                expected[8 * 512..9 * 512].copy_from_slice(&image[1024..1536]);
                let trace = [if philips { 0xbf } else { 0x80 }, 4, 0xb8, 0, 4, 4];
                expected[8 * 512..8 * 512 + 6].copy_from_slice(&trace);
                m.mount_disk(image, false).unwrap();
                if philips {
                    m.install_philips_disk_rom(fixture(
                        include_str!("../../../tests/fixtures/philips-clock.hex"),
                        16384,
                        255,
                    ))
                    .unwrap();
                }
                m.boot_disk().unwrap();
                // Boot reads plus five transfers and setup: <8*(rev+559B),
                // with margin for CPU verification. This is an emulated bound.
                while m.cpu.tstates < 8_000_000 && !m.cpu.halted {
                    m.step();
                }
                assert!(
                    m.cpu.halted,
                    "{model:?} philips={philips} pc={:04x}",
                    m.cpu.pc
                );
                assert_eq!(
                    m.hardware.read(0xc060),
                    0xa5,
                    "{model:?} philips={philips} pc={:04x}",
                    m.cpu.pc
                );
                assert_eq!(m.hardware.fdc.disk.as_ref().unwrap().bytes(), expected);
            }
        }
    }
}
