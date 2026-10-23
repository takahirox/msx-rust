//! Original hardware tests and Z80 fixture; no game code or recorded sound.
use msx_core::{cartridge::detect_mapper, cpu::Bus, firmware, Machine, Mapper, Model};

fn image() -> Vec<u8> {
    let mut rom = vec![0x5a; 65536];
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    rom[0x10] = 0x76;
    rom
}
fn machine(model: Model, scc: bool, psg: bool) -> Machine {
    let mut m = Machine::default();
    m.set_model(model);
    m.load_cartridge_with_mapper(&image(), Mapper::KonamiScc)
        .unwrap();
    m.hardware.slots = 0xd4;
    if model == Model::Msx2 {
        m.hardware.secondary[3] = 0xff;
    }
    m.hardware.write(0x9000, 0x3f);
    for i in 0..128 {
        m.hardware
            .write(0x9800 + i, if i & 16 == 0 { 120 } else { 136 });
    }
    for c in 0..5 {
        m.hardware.write(0x9880 + c * 2, 253 - c as u8 * 16);
        m.hardware.write(0x9881 + c * 2, 0);
        m.hardware.write(0x988a + c, 15);
    }
    m.hardware.write(0x988f, if scc { 31 } else { 0 });
    for (r, v) in [(0, 190), (1, 0), (7, 0x3e), (8, if psg { 15 } else { 0 })] {
        m.hardware.output(0xa0, r);
        m.hardware.output(0xa1, v);
    }
    m.cpu.pc = 0xc100;
    m.hardware.write(0xc100, 0x76); // HALT in RAM, no fixture overwriting test registers.
    m
}
fn silent(m: &Machine) -> bool {
    !m.hardware.psg.samples.is_empty() && m.hardware.psg.samples.iter().all(|&x| x == 0.0)
}

#[test]
fn overlay_only_decodes_enabled_selected_slot_and_never_rom_mirrors() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = machine(model, true, false);
        m.hardware.slots = 0xd5; // ROM mirror in page 0, keep the CPU's RAM page.
        m.hardware.write(0x1800, 99);
        m.hardware.write(0x97ff, 0xff);
        assert_eq!(m.hardware.read(0x1800), 0x5a); // Ordinary ROM mirror.
        assert_eq!(m.hardware.read(0x9800), 120);
        m.hardware.write(0x9f00, 0x89);
        assert_eq!(m.hardware.read(0x9800), 0x89);
        for value in [0, 0x3e, 0x40, 0xbe] {
            m.hardware.write(0x9000, value);
            assert_eq!(m.hardware.read(0x9800), 0x5a);
            m.hardware.write(0x9800, 0x33); // Disabled SCC must ignore sound writes.
            m.frame();
            assert!(silent(&m));
        }
        for value in [0x3f, 0x7f, 0xbf, 0xff] {
            m.hardware.write(0x9000, value);
            assert_eq!(m.hardware.read(0x9800), 0x89);
        }
        m.hardware.slots = 0xc4; // Cartridge in page 1, BIOS in SCC page 2.
        m.hardware.write(0x9800, 0x33);
        m.hardware.write(0x9000, 2);
        m.frame();
        assert!(silent(&m));
        m.hardware.slots = 0xd4;
        assert_eq!(m.hardware.read(0x9800), 0x89);
        m.frame();
        assert!(m.hardware.psg.samples.iter().any(|s| s.abs() > 0.05));
        m.hardware.write(0x9000, 2);
        assert_eq!(m.hardware.read(0x9800), 0x5a);
    }
    for mapper in [
        Mapper::Ascii8,
        Mapper::Ascii16,
        Mapper::Konami,
        Mapper::Plain,
    ] {
        let mut m = Machine::default();
        m.load_cartridge_with_mapper(&image()[..32768], mapper)
            .unwrap();
        m.hardware.slots = 0xd4;
        m.hardware.write(0x9000, 0x3f);
        m.hardware.write(0x9800, 0x89);
        assert_eq!(m.hardware.read(0x9800), 0x5a);
    }
}

#[test]
fn mixing_preserves_sample_cadence_psg_waveform_and_headroom() {
    let mut scc = machine(Model::Msx1, true, false);
    let mut psg = machine(Model::Msx1, false, true);
    let mut both = machine(Model::Msx1, true, true);
    let mut total = 0;
    for _ in 0..60 {
        scc.frame();
        psg.frame();
        both.frame();
        let a = &scc.hardware.psg.samples;
        let b = &psg.hardware.psg.samples;
        let mixed = &both.hardware.psg.samples;
        assert_eq!(a.len(), b.len());
        assert_eq!(b.len(), mixed.len());
        total += mixed.len();
        for ((a, b), sum) in a.iter().zip(b).zip(mixed) {
            assert!((a + b - sum).abs() < 0.000001);
            assert!(sum.is_finite() && sum.abs() <= 0.9);
        }
    }
    // NTSC emulated frames, not a rounded 735 samples/frame accumulator.
    let expected = both.hardware.vdp.frame_cycles() as usize * 60 * 44100 / 3_579_545;
    assert!(total.abs_diff(expected) <= 1);
}

#[test]
fn failed_imports_preserve_wave_ram_oscillators_mapper_and_psg_transactionally() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = machine(model, true, true);
        let mut control = machine(model, true, true);
        m.frame();
        control.frame();
        for (rom, mapper) in [
            (vec![], Mapper::KonamiScc),
            (vec![0; 65536], Mapper::Auto),
            (image(), Mapper::Plain),
        ] {
            assert!(m.load_cartridge_owned(rom, mapper).is_err());
        }
        assert!(m.load_bios(&[0; 32]).is_err());
        assert!(m.load_msx2_firmware(&[0; 32], &[0; 32]).is_err());
        assert!(m
            .load_cartridge_for_model_owned(vec![0; 32], Mapper::KonamiScc, Model::Msx2)
            .is_err());
        for address in 0x9800..0x9880 {
            assert_eq!(m.hardware.read(address), control.hardware.read(address));
        }
        for _ in 0..4 {
            m.frame();
            control.frame();
            assert_eq!(m.hardware.psg.samples, control.hardware.psg.samples);
            assert_eq!(m.hardware.psg.registers, control.hardware.psg.registers);
            assert_eq!(m.cpu.pc, control.cpu.pc);
        }
    }
}

#[test]
fn reset_eject_reload_replacement_model_and_firmware_clear_old_sound() {
    for model in [Model::Msx1, Model::Msx2] {
        for transition in 0..9 {
            let mut m = machine(model, true, true);
            m.frame();
            match transition {
                0 => m.reset(),
                1 => m.unload_cartridge(),
                2 => m
                    .load_cartridge_with_mapper(&image(), Mapper::KonamiScc)
                    .unwrap(),
                3 => m.load_cartridge(&firmware::test_cartridge()).unwrap(),
                4 => m.set_model(if model == Model::Msx1 {
                    Model::Msx2
                } else {
                    Model::Msx1
                }),
                5 => m.use_cbios().unwrap(),
                6 => m.use_builtin(),
                7 if model == Model::Msx2 => m.use_diagnostic().unwrap(),
                7 => m.load_bios(firmware::CBIOS_MSX1_MAIN).unwrap(),
                _ if model == Model::Msx2 => m
                    .load_msx2_firmware(firmware::CBIOS_MSX2_MAIN, firmware::CBIOS_MSX2_SUB)
                    .unwrap(),
                _ => m.load_bios(firmware::CBIOS_MSX1_MAIN).unwrap(),
            }
            assert!(m.hardware.psg.samples.is_empty());
            assert_eq!(m.hardware.psg.registers, [0; 16]);
            m.hardware.slots = 0xd4;
            m.hardware.write(0x9000, 0x3f);
            if [0, 2, 8].contains(&transition) || (transition == 7 && model == Model::Msx1) {
                for a in 0x9800..0x9880 {
                    assert_eq!(m.hardware.read(a), 0);
                }
            }
            // Avoid booting/reprogramming a cartridge while checking fresh silence.
            m.cpu.pc = 0xc100;
            if m.hardware.model == Model::Msx2 {
                m.hardware.secondary[3] = 0xff;
            }
            m.hardware.write(0xc100, 0x76);
            m.frame();
            assert!(silent(&m), "transition {transition}, model {model:?}");
        }
    }
}

#[test]
fn original_megarom_executes_real_z80_scc_and_psg_on_both_models_and_auto() {
    let rom = firmware::scc_test_cartridge();
    assert_eq!(detect_mapper(&rom).mapper, Some(Mapper::KonamiScc));
    for model in [Model::Msx1, Model::Msx2] {
        for mapper in [Mapper::KonamiScc, Mapper::Auto] {
            let mut m = Machine::default();
            m.set_model(model);
            m.load_cartridge_with_mapper(&rom, mapper).unwrap();
            for _ in 0..600 {
                m.frame();
                if m.hardware.read(0xc000) == 0xa5 {
                    break;
                }
            }
            assert_eq!(m.hardware.read(0xc000), 0xa5);
            for i in 0..128 {
                assert_eq!(m.hardware.read(0x9800 + i), rom[0x800 + i as usize]);
            }
            for _ in 0..30 {
                m.frame();
            }
            assert!(silent(&m));
            m.hardware.key(8, 0, true); // Space: first SCC only, period 253.
            m.frame();
            let mut pcm = Vec::new();
            for _ in 0..60 {
                m.frame();
                pcm.extend_from_slice(&m.hardware.psg.samples);
            }
            assert!(pcm
                .iter()
                .all(|x| (*x - 0.056).abs() < 0.000001 || (*x + 0.0565).abs() < 0.000001));
            let rising = pcm.windows(2).filter(|w| w[0] < 0.0 && w[1] > 0.0).count();
            let expected = pcm.len() as f64 / 44100.0 * 3_579_545.0 / (32.0 * 254.0);
            assert!((rising as f64 - expected).abs() < 1.0);
            m.hardware.key(8, 0, false);
            m.hardware.key(8, 5, true); // Up: five independent voices.
            m.frame();
            assert!(m.hardware.psg.samples.iter().any(|x| x.abs() > 0.07));
            m.hardware.key(8, 4, true); // Left: concurrent PSG.
            m.frame();
            assert_eq!(m.hardware.psg.registers[8], 15);
            assert!(m.hardware.psg.samples.iter().any(|x| x.abs() > 0.2));
            m.hardware.release_keys();
            for _ in 0..30 {
                m.frame();
            }
            assert!(m.hardware.psg.samples.iter().all(|x| x.abs() < 0.000001));
        }
    }
}
