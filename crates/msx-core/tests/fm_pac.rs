//! Independent bus sequences and literal expectations for Issue #75.
use msx_core::{
    cpu::Bus, firmware, BootPolicy, Firmware, Machine, Mapper, Model, SoundCartridgeProfile,
};

fn machine(model: Model) -> Machine {
    let mut m = Machine::default();
    m.set_model(model);
    m.install_fm_pac(firmware::fm_pac_diagnostic_firmware())
        .unwrap();
    m
}
fn select(m: &mut Machine) {
    m.hardware.slots = 0xd8;
    m.hardware.secondary[2] = 4;
    m.hardware.secondary[3] = 255;
    // Do not execute BIOS during host-driven sound checks.
    m.cpu.halted = true;
}
fn unlock(m: &mut Machine) {
    m.hardware.write(0x5ffe, 0x4d);
    m.hardware.write(0x5fff, 0x69);
}
fn write(m: &mut Machine, r: u8, v: u8, memory: bool) {
    if memory {
        m.hardware.write(0x7ff4, r);
    } else {
        m.hardware.output(0xab7c, r);
    }
    for _ in 0..4 {
        m.step();
    }
    if memory {
        m.hardware.write(0x7ff5, v);
    } else {
        m.hardware.output(0xcd7d, v);
    }
    for _ in 0..24 {
        m.step();
    }
}
fn tone(m: &mut Machine, memory: bool) {
    for (r, v) in [0x21, 0x21, 0x3f, 0, 0, 0xf0, 0, 0x0f]
        .into_iter()
        .enumerate()
        .map(|(r, v)| (r as u8, v))
        .chain([(0x30, 0), (0x10, 0x22), (0x20, 0x19)])
    {
        write(m, r, v, memory);
    }
}
fn sine(pcm: &[f32]) {
    // Independent pitch formula for Fnum 290 / block 4. Held sample/grid
    // quantization permits two output samples; gain follows the specified DAC scale.
    let crossings: Vec<_> = pcm
        .windows(2)
        .enumerate()
        .filter(|(_, v)| v[0] <= 0.0 && v[1] > 0.0)
        .map(|(i, _)| i)
        .collect();
    assert!(crossings.len() > 40, "insufficient custom sine crossings");
    let period = 44100.0 * 72.0 * 524288.0 / (290.0 * 16.0 * 3579545.0);
    for (n, i) in crossings.iter().enumerate() {
        assert!(((*i - crossings[0]) as f64 - n as f64 * period).abs() < 2.0);
    }
    let peak = pcm.iter().map(|v| v.abs()).fold(0.0, f32::max);
    assert!(peak > 0.015 && peak < 0.016, "peak={peak}");
}

#[test]
fn exact_four_banks_overlays_protection_unlocks_and_ignored_bits() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = machine(model);
        let rom: Vec<u8> = (0..65536)
            .map(|i| (i * 37 + i / 16384 * 19 + i / 256) as u8)
            .collect();
        m.install_fm_pac(rom.clone()).unwrap();
        select(&mut m);
        assert_eq!(m.fm_pac_sram(), [255; 8190]);
        for bank in 0..4 {
            m.hardware.write(0x7ff7, 0xfc | bank);
            assert_eq!(m.hardware.read(0x7ff7), bank);
            for a in 0x4000..=0x7fff {
                if a == 0x7ff6 {
                    assert_eq!(m.hardware.read(a), 0);
                } else if a != 0x7ff7 {
                    assert_eq!(
                        m.hardware.read(a),
                        rom[bank as usize * 16384 + (a - 0x4000) as usize]
                    );
                }
            }
            for a in [0x4000, 0x4018, 0x5ffd, 0x6000, 0x7ff3, 0x7ff8, 0x7fff] {
                let before = m.hardware.read(a);
                m.hardware.write(a, !before);
                assert_eq!(m.hardware.read(a), before);
            }
        }
        m.hardware.write(0x5ffe, 0x4d);
        assert_eq!(m.hardware.read(0x5ffe), rom[3 * 16384 + 0x1ffe]);
        assert_eq!(m.hardware.read(0x4000), rom[3 * 16384]);
        m.hardware.write(0x5fff, 0x68);
        assert_eq!(m.hardware.read(0x5fff), rom[3 * 16384 + 0x1fff]);
        m.hardware.write(0x5fff, 0x69);
        assert_eq!(m.hardware.read(0x5ffe), 0x4d);
        assert_eq!(m.hardware.read(0x5fff), 0x69);
        for a in [0x6000, 0x7ff3, 0x7ff4, 0x7ff5, 0x7ff8, 0x7fff] {
            assert_eq!(m.hardware.read(a), 255);
        }
        for (a, v) in [(0x4000, 0x12), (0x5ffd, 0x34)] {
            m.hardware.write(a, v);
            assert_eq!(m.hardware.read(a), v);
        }
        m.hardware.write(0x5ffe, 0);
        m.hardware.write(0x4000, 0x99);
        assert_eq!(m.fm_pac_sram()[0], 0x12);
        m.hardware.write(0x5ffe, 0x4d); // second latch retained -> unlock again
        assert_eq!(m.hardware.read(0x4000), 0x12);
        for value in [0xff, 0x10, 0x11] {
            m.hardware.write(0x7ff6, value);
            assert_eq!(m.hardware.read(0x7ff6), value & 0x11);
            unlock(&mut m);
            assert_eq!(m.hardware.read(0x4000), rom[3 * 16384]);
        }
        m.hardware.write(0x7ff6, 0xee);
        assert_eq!(m.hardware.read(0x7ff6), 0);
        m.hardware.write(0x5fff, 0x69); // reverse order, partial must still be ROM
        assert_eq!(m.hardware.read(0x4000), rom[3 * 16384]);
        m.hardware.write(0x5ffe, 0x4d);
        assert_eq!(m.hardware.read(0x4000), 0x12);
        m.hardware.write(0x7ff7, 1);
        assert_eq!(m.hardware.read(0x7ff7), 1);
        assert_eq!(m.hardware.read(0x7ff6), 0);
        m.hardware.write(0x5fff, 0);
        assert_eq!(m.hardware.read(0x4000), rom[16384]);
        m.reset();
        select(&mut m);
        assert_eq!(m.hardware.read(0x7ff7), 0);
        assert_eq!(m.hardware.read(0x7ff6), 0);
        assert_eq!(m.hardware.read(0x5ffe), rom[0x1ffe]);
        unlock(&mut m);
        assert_eq!(m.hardware.read(0x4000), 0x12);
        assert_eq!(m.hardware.read(0x5ffd), 0x34);
    }
}

#[test]
fn expanded_slot_two_per_page_latch_precedence_hidden_isolation_and_disk_separation() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = machine(model);
        let mut disk = vec![0xa9; 16384];
        disk[..2].copy_from_slice(b"AB");
        m.install_philips_disk_rom(disk).unwrap();
        m.hardware.slots = 0xaa;
        for sub in 0..4 {
            let selector = sub * 0x55;
            m.hardware.write(0xffff, selector);
            assert_eq!(m.hardware.read(0xffff), !selector);
            for a in [0, 0x3fff, 0x8000, 0xbfff, 0xc000, 0xfffe] {
                m.hardware.write(a, 0);
                assert_eq!(m.hardware.read(a), 255);
            }
            m.hardware.fdc.irq = true;
            m.hardware.fdc.drq = true;
            if sub == 1 {
                assert_eq!(m.hardware.read(0x4000), b'A');
                for a in 0x7ff8..=0x7fff {
                    assert_eq!(m.hardware.read(a), 255);
                    m.hardware.write(a, 0);
                }
                assert!(m.hardware.fdc.irq && m.hardware.fdc.drq);
                m.hardware.write(0x7ff7, 3);
            } else if sub == 0 {
                assert_eq!(m.hardware.read(0x4100), 0xa9);
                m.hardware.write(0x7ff7, 2); // disk ROM write cannot select PAC bank
            } else {
                assert_eq!(m.hardware.read(0x4000), 255);
            }
        }
        // Each page's selector is independent; latch still wins with page-3 subslot 1.
        m.hardware.write(0xffff, 0x44);
        assert_eq!(m.hardware.read(0xffff), 0xbb);
        assert_eq!(m.hardware.read(0x4101), 0x54);
        // Page-1 selection must ignore every other page's two-bit selector.
        for low in 0..4 {
            for page_two in 0..4 {
                for page_three in 0..4 {
                    let selector = low | 4 | (page_two << 4) | (page_three << 6);
                    m.hardware.write(0xffff, selector);
                    assert_eq!(m.hardware.read(0xffff), !selector);
                    assert_eq!(m.hardware.read(0x4101), 0x54);
                }
            }
        }
        m.hardware.slots = 0xd4; // game page 1 hidden writes cannot touch PAC
        m.hardware.secondary[3] = 255;
        m.hardware.write(0x7ff7, 0);
        m.hardware.write(0x7ff6, 1);
        unlock(&mut m);
        m.hardware.slots = 0xd8;
        assert_eq!(m.hardware.read(0x7ff7), 3);
        assert_eq!(m.hardware.read(0x7ff6), 0);
        assert_eq!(m.hardware.read(0x4018), b'W');
        // Independent slot-3 latch / MSX1 physical FFFF RAM survive optional expansion.
        m.hardware.write(0xffff, 0xe7);
        if model == Model::Msx2 {
            assert_eq!(m.hardware.secondary[3], 0xe7);
            assert_eq!(m.hardware.read(0xffff), 0x18);
        } else {
            assert_eq!(m.hardware.read(0xffff), 0xe7);
        }
        m.reset();
        assert_eq!(m.hardware.secondary[2], 0);
        m.remove_fm_pac();
        m.hardware.slots = 0xaa;
        m.hardware.write(0xffff, 0x55);
        assert_eq!(m.hardware.read(0xffff), 255);
        assert_eq!(m.hardware.read(0x4100), 0xa9); // former unexpanded topology
    }
}

#[test]
fn z80_discovery_banked_execution_sram_and_hidden_memory_programmed_sine_both_models() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = machine(model);
        let saved: Vec<u8> = (0..8190).map(|i| (i * 11 + i / 257) as u8).collect();
        m.restore_fm_pac_sram(&saved).unwrap();
        m.load_cartridge_with_mapper(&firmware::fm_pac_test_cartridge(), Mapper::Plain)
            .unwrap();
        while m.cpu.tstates < 40_000_000 && !(m.cpu.halted && (0x8010..0x9000).contains(&m.cpu.pc))
        {
            m.step();
        }
        assert!(m.cpu.halted, "pc={:04x}", m.cpu.pc);
        assert_eq!(
            (0xc100..=0xc109)
                .map(|a| m.hardware.read(a))
                .collect::<Vec<_>>(),
            [1, 0xfb, 0x51, 0x52, 0x53, 0x54, 255, 0x4d, 0x69, 0xa5]
        );
        assert_eq!(m.hardware.vdp.registers[7], 2);
        assert_eq!(m.hardware.slots, 0xd4);
        assert_eq!(m.fm_pac_sram(), saved);
        let mut pcm = Vec::new();
        for _ in 0..8 {
            m.frame();
            pcm.extend_from_slice(&m.hardware.psg.samples);
        }
        sine(&pcm);
    }
}

#[test]
fn io_enable_hidden_audio_and_independent_internal_external_chips() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut a = machine(model);
        let mut b = machine(model);
        select(&mut a);
        select(&mut b);
        tone(&mut a, false);
        tone(&mut b, false);
        a.frame();
        assert!(a.hardware.psg.samples.iter().all(|v| *v == 0.0));
        // Same emulated clock times, memory vs enabled I/O must give exact PCM.
        a.reset();
        b.reset();
        select(&mut a);
        select(&mut b);
        b.hardware.write(0x7ff6, 1);
        tone(&mut a, true);
        tone(&mut b, false);
        a.hardware.write(0x7ff6, 0);
        b.hardware.write(0x7ff6, 0);
        for m in [&mut a, &mut b] {
            m.hardware.slots = 0xd4;
        }
        // Disabled writes cannot change address or data (A ignores both, B gets neither).
        write(&mut a, 0x30, 0xff, false);
        for _ in 0..28 {
            b.step();
        }
        a.frame();
        b.frame();
        assert_eq!(a.hardware.psg.samples, b.hardware.psg.samples);
        let mut pcm = Vec::new();
        for _ in 0..8 {
            a.frame();
            pcm.extend_from_slice(&a.hardware.psg.samples);
        }
        sine(&pcm);
        for p in [0x7c, 0x7d, 0xab7c, 0xcd7d] {
            assert_eq!(a.hardware.input(p), 255);
        }
    }
    // Both chips receive enabled port writes; external memory programming affects
    // only its recipient. Compare exact sums against a separately clocked single.
    let mut both = machine(Model::Msx2);
    let mut single = machine(Model::Msx2);
    both.install_msx_music(firmware::music_diagnostic_firmware())
        .unwrap();
    select(&mut both);
    select(&mut single);
    both.hardware.write(0x7ff6, 1);
    single.hardware.write(0x7ff6, 1);
    tone(&mut both, false);
    tone(&mut single, false);
    both.frame();
    single.frame();
    assert_eq!(
        both.hardware.psg.samples,
        single
            .hardware
            .psg
            .samples
            .iter()
            .map(|v| v * 2.0)
            .collect::<Vec<_>>()
    );
    write(&mut both, 0x30, 15, true);
    write(&mut single, 0x30, 15, true);
    both.frame();
    single.frame();
    assert!(both
        .hardware
        .psg
        .samples
        .iter()
        .zip(&single.hardware.psg.samples)
        .any(|(a, b)| (a - b).abs() > 0.01));
}

#[test]
fn transactional_lifecycle_retains_game_or_scci_disk_tape_and_shared_accessory_save() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = machine(model);
        m.load_cartridge_with_mapper(
            &firmware::mapper_test_cartridge(Mapper::Ascii8Sram2),
            Mapper::Ascii8Sram2,
        )
        .unwrap();
        m.restore_cartridge_sram(&[0x3b; 2048]).unwrap();
        let game = m.hardware.cartridge.clone();
        let saved: Vec<u8> = (0..8190).map(|i| (i * 71) as u8).collect();
        m.restore_fm_pac_sram(&saved).unwrap();
        let mut disk_rom = vec![0x63; 16384];
        disk_rom[..2].copy_from_slice(b"AB");
        m.install_philips_disk_rom(disk_rom).unwrap();
        m.mount_disk(vec![0x42; 368640], false).unwrap();
        m.hardware.fdc.disk.as_mut().unwrap().dirty = true;
        m.hardware
            .cassette
            .mount(firmware::cassette_test_image(), 1200)
            .unwrap();
        m.hardware.cassette.tick(13579, true);
        let position = m.hardware.cassette.position_cycles();
        m.cpu.pc = 0x1234;
        for length in [0, 1, 16384, 65535, 65537] {
            assert!(m.install_fm_pac(vec![0; length]).is_err());
            assert_eq!(m.cpu.pc, 0x1234);
            assert_eq!(m.fm_pac_sram(), saved);
        }
        for length in [0, 8189, 8191, 8192] {
            assert!(m.restore_fm_pac_sram(&vec![0; length]).is_err());
            assert_eq!(m.fm_pac_sram(), saved);
        }
        m.reset();
        assert_eq!(m.fm_pac_sram(), saved);
        assert_eq!(m.hardware.cartridge, game);
        assert_eq!(m.cartridge_sram(), [0x3b; 2048]);
        m.install_fm_pac(firmware::fm_pac_diagnostic_firmware())
            .unwrap();
        assert_eq!(m.hardware.cartridge, game);
        assert_eq!(m.cartridge_sram(), [0x3b; 2048]);
        m.restore_fm_pac_sram(&saved).unwrap();
        assert_eq!(m.hardware.firmware, Firmware::Cbios);
        assert_eq!(m.boot_policy, BootPolicy::Automatic);
        m.install_sound_cartridge(SoundCartridgeProfile::Snatcher)
            .unwrap();
        m.remove_fm_pac();
        assert_eq!(
            m.sound_cartridge_profile(),
            Some(SoundCartridgeProfile::Snatcher)
        );
        m.install_fm_pac(firmware::fm_pac_diagnostic_firmware())
            .unwrap();
        assert_eq!(
            m.sound_cartridge_profile(),
            Some(SoundCartridgeProfile::Snatcher)
        );
        assert!(m.has_philips_disk_rom());
        let disk = m.hardware.fdc.disk.as_ref().unwrap();
        assert!(disk.dirty);
        assert!(!disk.write_protected);
        assert_eq!(disk.bytes(), vec![0x42; 368640]);
        assert_eq!(m.hardware.cassette.position_cycles(), position);
        m.restore_fm_pac_sram(&saved).unwrap();
        m.use_cbios().unwrap();
        assert_eq!(m.fm_pac_sram(), saved);
        if model == Model::Msx1 {
            m.load_bios(firmware::CBIOS_MSX1_MAIN).unwrap();
        } else {
            m.load_msx2_firmware(firmware::CBIOS_MSX2_MAIN, firmware::CBIOS_MSX2_SUB)
                .unwrap();
        }
        assert_eq!(m.fm_pac_sram(), saved);
        m.use_builtin();
        assert_eq!(m.fm_pac_sram(), saved);
        m.set_model(if model == Model::Msx1 {
            Model::Msx2
        } else {
            Model::Msx1
        });
        assert_eq!(m.fm_pac_sram(), saved);
        select(&mut m);
        assert_eq!(m.hardware.read(0x7ff6), 0);
        assert_eq!(m.hardware.read(0x7ff7), 0);
        m.remove_fm_pac();
        assert!(m.fm_pac_sram().is_empty());
        assert!(m.restore_fm_pac_sram(&saved).is_err());
    }
}

fn fixture(text: &str, size: usize, fill: u8) -> Vec<u8> {
    let mut bytes = vec![fill; size];
    for line in text.lines().filter(|s| !s.starts_with('#')) {
        let (offset, data) = line.split_once(':').unwrap();
        let offset = usize::from_str_radix(offset, 16).unwrap();
        for (i, v) in data.split_whitespace().enumerate() {
            bytes[offset + i] = u8::from_str_radix(v, 16).unwrap();
        }
    }
    bytes
}
#[test]
fn cpu_philips_scci_psg_and_external_fm_keep_exact_memory_disk_and_audio() {
    use msx_core::SoundCartridgeProfile;
    for model in [Model::Msx1, Model::Msx2] {
        for profile in [
            SoundCartridgeProfile::Snatcher,
            SoundCartridgeProfile::SdSnatcher,
        ] {
            let mut fm_pcm = Vec::new();
            let mut disabled_pcm = Vec::new();
            for installed in [false, true] {
                let mut m = Machine::default();
                m.set_model(model);
                let mut main = fixture(
                    include_str!("../../../tests/fixtures/fm-pac-system.hex"),
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
                m.install_sound_cartridge(profile).unwrap();
                m.install_philips_disk_rom(fixture(
                    include_str!("../../../tests/fixtures/music-disk.hex"),
                    16384,
                    255,
                ))
                .unwrap();
                if installed {
                    m.install_fm_pac(firmware::fm_pac_diagnostic_firmware())
                        .unwrap();
                }
                let mut image = fixture(
                    include_str!("../../../tests/fixtures/philips-scci-sector.hex"),
                    368640,
                    0x35,
                );
                image[510] = if profile == SoundCartridgeProfile::Snatcher {
                    0
                } else {
                    8
                };
                let mut expected = image.clone();
                expected[8 * 512..9 * 512].fill(0x6c);
                m.mount_disk(image, false).unwrap();
                m.boot_disk().unwrap();
                while m.cpu.tstates < 4_000_000 && !m.cpu.halted {
                    m.step();
                }
                assert!(m.cpu.halted, "pc={:04x}", m.cpu.pc);
                assert_eq!(m.hardware.read(0xc033), 0xa5);
                assert_eq!(m.hardware.read(0xc030), 0x44);
                assert_eq!(m.hardware.read(0xc031), 96);
                assert_eq!(m.hardware.read(0xc032), 0x6d);
                assert_eq!(m.hardware.fdc.disk.as_ref().unwrap().bytes(), expected);
                assert!(m.hardware.fdc.disk.as_ref().unwrap().dirty);
                m.frame();
                assert!(m
                    .hardware
                    .psg
                    .samples
                    .iter()
                    .all(|x| x.is_finite() && x.abs() <= 1.0));
                assert!(m.hardware.psg.samples.iter().any(|x| x.abs() > 0.05));
                if installed {
                    fm_pcm = m.hardware.psg.samples.clone();
                } else {
                    disabled_pcm = m.hardware.psg.samples.clone();
                }
            }
            assert_eq!(fm_pcm.len(), disabled_pcm.len());
            assert!(fm_pcm
                .iter()
                .zip(disabled_pcm)
                .any(|(a, b)| (a - b).abs() > 0.01));
        }
    }
}
