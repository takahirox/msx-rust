//! Original independent SCC-I bus traces for #49; MIT, no game bytes.
use msx_core::{cpu::Bus, firmware, Firmware, Machine, Model, SoundCartridgeProfile as Profile};
const PROFILES: [Profile; 2] = [Profile::Snatcher, Profile::SdSnatcher];
fn machine(model: Model, profile: Profile) -> Machine {
    let mut m = Machine::default();
    m.set_model(model);
    m.install_sound_cartridge(profile).unwrap();
    m.hardware.secondary[3] = 255;
    m.hardware.slots = 0xd4;
    m
}

#[test]
fn all_ram_banks_aliases_windows_and_protection_on_both_models() {
    for model in [Model::Msx1, Model::Msx2] {
        for profile in PROFILES {
            let mut m = machine(model, profile);
            let h = &mut m.hardware;
            for (bank, snatcher, sd) in [
                (0, true, false),
                (1, true, false),
                (2, true, false),
                (3, true, false),
                (4, true, false),
                (5, true, false),
                (6, true, false),
                (7, true, false),
                (8, false, true),
                (9, false, true),
                (10, false, true),
                (11, false, true),
                (12, false, true),
                (13, false, true),
                (14, false, true),
                (15, false, true),
            ] {
                let populated = if profile == Profile::Snatcher {
                    snatcher
                } else {
                    sd
                };
                for (register, start, end) in [
                    (0x5000, 0x4000, 0x5fff),
                    (0x7000, 0x6000, 0x7fff),
                    (0x9000, 0x8000, 0x9fff),
                    (0xb000, 0xa000, 0xbffd),
                ] {
                    h.write(0xbffe, 0);
                    h.write(register, bank);
                    assert_eq!(h.read(start), if populated { 0 } else { 255 });
                    h.write(start, 0xee); // protected
                    assert_eq!(h.read(start), if populated { 0 } else { 255 });
                    h.write(0xbfff, 0x10);
                    h.write(start, bank + 0x40);
                    h.write(end, bank + 0x60);
                    h.write(0xbffe, 0);
                    for alias in [0x10, 0x40, 0xc0] {
                        h.write(register, bank | alias);
                        assert_eq!(h.read(start), if populated { bank + 0x40 } else { 255 });
                        assert_eq!(h.read(end), if populated { bank + 0x60 } else { 255 });
                    }
                    // Clear this bank before testing a different window.
                    h.write(0xbffe, 0x10);
                    h.write(start, 0);
                    h.write(end, 0);
                }
            }
            h.slots = 0x55; // device selected in every page; no ROM-style mirrors
            for address in [0, 0x3fff, 0xc000, 0xfffe, 0xffff] {
                assert_eq!(h.read(address), 255);
            }
        }
    }
}

#[test]
fn register_boundaries_ram_mode_priority_reserved_bits_and_window_isolation() {
    for model in [Model::Msx1, Model::Msx2] {
        for profile in PROFILES {
            let first = if profile == Profile::Snatcher { 0 } else { 8 };
            let mut m = machine(model, profile);
            let h = &mut m.hardware;
            // Tag two independent banks, then keep all four windows on bank first.
            for bank in [first, first + 1] {
                h.write(0xbffe, 0);
                h.write(0x5000, bank);
                h.write(0xbffe, 1);
                h.write(0x4123, bank + 0x50);
            }
            for (reg, lo, hi) in [
                (0x5000, 0x5000, 0x57ff),
                (0x7000, 0x7000, 0x77ff),
                (0x9000, 0x9000, 0x97ff),
                (0xb000, 0xb000, 0xb7ff),
            ] {
                h.write(0xbffe, 0);
                for r in [0x5000, 0x7000, 0x9000, 0xb000] {
                    h.write(r, first);
                }
                for hole in [lo - 1, hi + 1] {
                    h.write(hole, first + 1);
                    assert_eq!(h.read(reg - 0x1000 + 0x123), first + 0x50);
                }
                for boundary in [lo, hi] {
                    h.write(boundary, first + 1);
                    for other in [0x5000, 0x7000, 0x9000, 0xb000] {
                        assert_eq!(
                            h.read(other - 0x1000 + 0x123),
                            if other == reg {
                                first + 0x51
                            } else {
                                first + 0x50
                            }
                        );
                    }
                }
            }
            for mode in [0, 1, 2, 4, 0x20, 0x24, 0x10, 0x14, 0xff, 0xc8] {
                h.write(0xbffe, 0);
                for (reg, bank) in [
                    (0x5000, first),
                    (0x7000, first + 1),
                    (0x9000, first + 2),
                    (0xb000, first + 3),
                ] {
                    h.write(reg, bank);
                }
                h.write(0xbffe, 0x10);
                for a in [
                    0x4001, 0x6001, 0x8001, 0xa001, 0x5000, 0x7000, 0x9000, 0xb000,
                ] {
                    h.write(a, 0x31);
                }
                h.write(0xbfff, mode);
                for (window, start, reg) in [
                    (0, 0x4000, 0x5000),
                    (1, 0x6000, 0x7000),
                    (2, 0x8000, 0x9000),
                    (3, 0xa000, 0xb000),
                ] {
                    let writable = match mode {
                        1 => window == 0,
                        2 => window == 1,
                        0x24 => window == 2,
                        0x10 | 0x14 | 0xff => true,
                        _ => false,
                    };
                    h.write(start + 1, 0x72);
                    assert_eq!(h.read(start + 1), if writable { 0x72 } else { 0x31 });
                    if writable {
                        h.write(reg, 0x66); // RAM write, not selector
                        assert_eq!(h.read(reg), 0x66);
                        assert_eq!(h.read(start + 1), 0x72);
                    }
                }
            }
            // Both mode addresses beat RAM writes, even in full RAM mode.
            h.write(0xbffe, 0);
            h.write(0xb000, first);
            h.write(0xbffe, 0x10);
            h.write(0xbfff, 0);
            h.write(0xbffd, 0x77);
            assert_eq!(h.read(0xbffd), 0);
            assert_eq!(h.read(0xbffe), 0);
            assert_eq!(h.read(0xbfff), 0);
        }
    }
}

#[test]
fn overlay_enable_full_selectors_mirrors_and_provisional_underlying_ram_writes() {
    for model in [Model::Msx1, Model::Msx2] {
        for profile in PROFILES {
            let mut m = machine(model, profile);
            let h = &mut m.hardware;
            for bank in [0x3f, 0x7f, 0xbf, 0xff] {
                h.write(0x9000, bank);
                h.write(0x9860, 0x23);
                for page in 0x98..=0x9f {
                    assert_eq!(h.read(page * 256 + 0x60), 0x23);
                    assert_eq!(h.read(page * 256 + 0xa0), 0x23);
                }
            }
            h.write(0xbffe, 0x20);
            h.write(0xb000, 0x80);
            h.write(0xb880, 0x91);
            h.write(0xb860, 0x32);
            for page in 0xb8..=0xbf {
                assert_eq!(h.read(page * 256 + 0x80), 0x91);
            }
            h.write(0xb000, 0);
            h.write(0xb880, 0x66); // disabled
            h.write(0xb000, 0x80);
            assert_eq!(h.read(0xb880), 0x91);
            h.write(0xbffe, 0);
            h.write(0x9000, 0x3f);
            assert_eq!(h.read(0x98a0), 0x91); // mode switch retains independent fifth
            h.write(0x9860, 0x55);
            assert_eq!(h.read(0x98a0), 0x55);
            // Provisional policy, not physical evidence: overlay READ wins, but
            // RAM mode WRITE targets RAM and cannot update the sound registers.
            h.write(0xbffe, 0x20);
            h.write(0xb000, 0x8f); // bank 15 populated only on SD
            h.write(0xbffe, 0x30);
            h.write(0xb880, 0x67);
            assert_eq!(h.read(0xb880), 0x55);
            h.write(0xbffe, 0x20);
            h.write(0xb000, 0x0f);
            assert_eq!(
                h.read(0xb880),
                if profile == Profile::SdSnatcher {
                    0x67
                } else {
                    255
                }
            );
        }
    }
}

#[test]
fn reset_insertion_removal_and_each_firmware_transition() {
    for model in [Model::Msx1, Model::Msx2] {
        for profile in PROFILES {
            for transition in 0..12 {
                let mut m = machine(model, profile);
                let main = if model == Model::Msx1 {
                    firmware::CBIOS_MSX1_MAIN
                } else {
                    firmware::CBIOS_MSX2_MAIN
                };
                if model == Model::Msx1 {
                    m.load_bios(main).unwrap();
                } else {
                    m.load_msx2_firmware(main, firmware::CBIOS_MSX2_SUB)
                        .unwrap();
                }
                let mut rom = vec![255; 16384];
                rom[..2].copy_from_slice(b"AB");
                m.install_philips_disk_rom(rom).unwrap();
                m.mount_disk(vec![0x39; 368640], false).unwrap();
                m.hardware.fdc.disk.as_mut().unwrap().dirty = true;
                m.hardware.slots = 0xd4;
                let first = if profile == Profile::Snatcher { 0 } else { 8 };
                m.hardware.write(0x5000, first);
                m.hardware.write(0xbffe, 1);
                m.hardware.write(0x4123, 0x71);
                m.hardware.write(0xbffe, 0);
                m.hardware.write(0x9000, 0x3f);
                m.hardware.write(0x9860, 0x37);
                m.hardware.write(0x9886, 99);
                m.hardware.write(0x988d, 15);
                m.hardware.write(0x988f, 8);
                match transition {
                    0 => m.reset(),
                    1 => m.boot_disk().unwrap(),
                    2 if model == Model::Msx1 => m.load_bios(main).unwrap(),
                    2 => m
                        .load_msx2_firmware(main, firmware::CBIOS_MSX2_SUB)
                        .unwrap(),
                    3 => m.install_sound_cartridge(profile).unwrap(),
                    4 => m.remove_sound_cartridge(),
                    5 => m.use_builtin(),
                    6 => m.use_cbios().unwrap(),
                    7 => m.set_model(model),
                    8 if model == Model::Msx2 => m.use_diagnostic().unwrap(),
                    8 => m.set_model(Model::Msx2),
                    9 => m.load_cartridge(&firmware::test_cartridge()).unwrap(),
                    10 => m.unload_cartridge(),
                    _ => m
                        .install_sound_cartridge(if profile == Profile::Snatcher {
                            Profile::SdSnatcher
                        } else {
                            Profile::Snatcher
                        })
                        .unwrap(),
                }
                assert!(m.hardware.psg.samples.is_empty());
                assert!(m.has_philips_disk_rom());
                let disk = m.hardware.fdc.disk.as_ref().unwrap();
                assert!(disk.dirty);
                assert!(disk.bytes().iter().all(|&b| b == 0x39));
                if transition <= 4 || transition == 10 || transition == 11 {
                    assert_eq!(m.hardware.bios, main);
                    assert_eq!(m.hardware.firmware, Firmware::User);
                }
                if transition <= 3 {
                    assert_eq!(m.sound_cartridge_profile(), Some(profile));
                    m.hardware.secondary[3] = 255;
                    m.hardware.slots = 0xd4;
                    // Reset selectors 0/1/2/3, RAM still protected; SD initial banks empty.
                    assert_eq!(
                        m.hardware.read(0x4123),
                        if profile == Profile::Snatcher && transition != 3 {
                            0x71
                        } else if profile == Profile::Snatcher {
                            0
                        } else {
                            255
                        }
                    );
                    m.hardware.write(0x5000, first);
                    assert_eq!(
                        m.hardware.read(0x4123),
                        if transition == 3 { 0 } else { 0x71 }
                    );
                    m.hardware.write(0x9000, 0x3f);
                    assert_eq!(
                        m.hardware.read(0x9860),
                        if transition == 3 { 0 } else { 0x37 }
                    );
                    // Period/volume survive hardware reset; enable/deformation clear.
                    m.cpu.pc = 0xc100;
                    m.hardware.write(0xc100, 0x76);
                    m.frame();
                    assert!(m.hardware.psg.samples.iter().all(|&v| v == 0.0));
                    m.hardware.write(0x988f, 8);
                    m.frame();
                    if transition != 3 {
                        assert!(m.hardware.psg.samples.iter().any(|&v| v > 0.0));
                    }
                } else if transition != 11 {
                    assert_eq!(m.sound_cartridge_profile(), None);
                }
            }
        }
    }
}

fn fixture(text: &str, size: usize, fill: u8) -> Vec<u8> {
    let mut bytes = vec![fill; size];
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let (offset, data) = line.split_once(':').unwrap();
        let offset = usize::from_str_radix(offset, 16).unwrap();
        for (i, value) in data.split_whitespace().enumerate() {
            bytes[offset + i] = u8::from_str_radix(value, 16).unwrap();
        }
    }
    bytes
}

#[test]
fn cpu_disk_sector_executes_accessory_ram_and_both_sound_modes_with_philips() {
    for model in [Model::Msx1, Model::Msx2] {
        for profile in PROFILES {
            let mut m = machine(model, profile);
            let mut main = fixture(
                include_str!("../../../tests/fixtures/philips-system.hex"),
                32768,
                0,
            );
            main[0x2d] = u8::from(model == Model::Msx2);
            let mut sub = vec![0; 16384];
            sub[..2].copy_from_slice(b"CD");
            sub[0x100] = 0x7b;
            if model == Model::Msx1 {
                m.load_bios(&main).unwrap();
            } else {
                m.load_msx2_firmware(&main, &sub).unwrap();
            }
            let mut disk = fixture(
                include_str!("../../../tests/fixtures/philips-scci-sector.hex"),
                368640,
                0x35,
            );
            disk[510] = if profile == Profile::Snatcher { 0 } else { 8 };
            let mut expected = disk.clone();
            expected[8 * 512..9 * 512].fill(0x6c);
            m.mount_disk(disk, false).unwrap();
            m.install_philips_disk_rom(fixture(
                include_str!("../../../tests/fixtures/philips-disk.hex"),
                16384,
                255,
            ))
            .unwrap();
            m.boot_disk().unwrap();
            for _ in 0..1_000_000 {
                m.step();
                if m.cpu.halted {
                    break;
                }
            }
            assert!(m.cpu.halted, "pc={:04x}", m.cpu.pc);
            assert_eq!(
                (0xc030..0xc034)
                    .map(|a| m.hardware.read(a))
                    .collect::<Vec<_>>(),
                [0x44, 96, 0x6d, 0xa5]
            );
            m.hardware.slots = 0xf4;
            assert_eq!(m.hardware.read(0x4800), 0x3e);
            assert_eq!(m.hardware.read(0x4123), 0x6d);
            assert_eq!(m.hardware.fdc.disk.as_ref().unwrap().bytes(), expected);
            assert_eq!(m.hardware.firmware, Firmware::User);
            assert_eq!(m.hardware.bios, main);
            assert!(m.has_philips_disk_rom());
            if model == Model::Msx2 {
                assert_eq!(m.hardware.subrom, sub);
                assert_eq!(m.hardware.read(0xc011), 0x7b);
                assert_eq!(m.hardware.input(0xfe), 0xf1);
                assert_eq!(m.hardware.input(0xff), 0xf2);
            }
            // Voice 5 remains audible with its slot hidden, unlike wave 4's +64.
            m.frame();
            let pcm = &m.hardware.psg.samples;
            assert!(pcm.iter().any(|&v| v > 0.04));
            assert!(pcm.iter().any(|&v| v < -0.04));
            assert!(pcm.iter().all(|&v| (v.abs() - 0.045).abs() < 1e-6));
            m.hardware.slots = 0xd4;
            m.hardware.write(0xb8af, 8);
            m.frame();
            assert!(m
                .hardware
                .psg
                .samples
                .iter()
                .all(|&v| (v - 0.03).abs() < 1e-6));
        }
    }
}

#[test]
fn plus_pcm_hidden_phase_and_psg_mixing_match_independent_clock_oracle() {
    for model in [Model::Msx1, Model::Msx2] {
        for profile in PROFILES {
            let mut machines = [
                machine(model, profile),
                machine(model, profile),
                machine(model, profile),
            ];
            for (i, m) in machines.iter_mut().enumerate() {
                m.hardware.write(0xbffe, 0x20);
                m.hardware.write(0xb000, 0x80);
                for c in 0..5u16 {
                    for offset in 0..32 {
                        m.hardware.write(
                            0xb800 + c * 32 + offset,
                            (offset as i16 * 5 - 110 + c as i16 * 7) as u8,
                        );
                    }
                    m.hardware.write(0xb8a0 + c * 2, (19 + c * 13) as u8);
                    m.hardware.write(0xb8aa + c, 15 - c as u8 * 2);
                }
                m.hardware.write(0xb8af, if i == 1 { 0 } else { 31 });
                if i != 0 {
                    for (r, v) in [(0, 71), (7, 0x3e), (8, 15)] {
                        m.hardware.psg.select(r);
                        m.hardware.psg.write(v);
                    }
                }
                m.cpu.pc = 0xc100;
                m.hardware.write(0xc100, 0x76);
            }
            // Invalid operations must leave programmed oscillators and slot-1
            // RAM untouched; all subsequent samples still match the oracle.
            assert!(machines[2].load_cartridge(&[0; 32]).is_err());
            assert!(machines[2].load_bios(&[0; 32]).is_err());
            assert!(machines[2].load_msx2_firmware(&[0; 32], &[0; 32]).is_err());
            let mut sample = 0u64;
            for (overlay, slot) in [
                (true, true),
                (false, true),
                (false, false),
                (true, false),
                (true, true),
            ] {
                for m in &mut machines {
                    m.hardware.slots = 0xd4;
                    m.hardware.write(0xb000, if overlay { 0x80 } else { 0 });
                    m.hardware.slots = if slot { 0xd4 } else { 0xf4 };
                    if !overlay || !slot {
                        m.hardware.write(0xb8af, 0);
                        m.hardware.write(0xb880, 0);
                        m.hardware.read(0xb8c0);
                    }
                }
                for steps in [1, 17, 503, 17011] {
                    for m in &mut machines {
                        m.hardware.psg.samples.clear();
                        for _ in 0..steps {
                            m.step();
                        }
                    }
                    for ((&scc, &psg), &both) in machines[0]
                        .hardware
                        .psg
                        .samples
                        .iter()
                        .zip(&machines[1].hardware.psg.samples)
                        .zip(&machines[2].hardware.psg.samples)
                    {
                        sample += 1;
                        let elapsed = (sample * 3_579_545).div_ceil(44100);
                        let mut dac = 0i32;
                        for c in 0..5u64 {
                            let wave =
                                (elapsed / (20 + c * 13) % 32) as i32 * 5 - 110 + c as i32 * 7;
                            dac += (wave * (15 - c as i32 * 2)).div_euclid(16);
                        }
                        assert!((scc - dac as f32 / 2000.0).abs() < 1e-6);
                        assert!((scc + psg - both).abs() < 1e-6);
                    }
                }
            }
        }
    }
}
