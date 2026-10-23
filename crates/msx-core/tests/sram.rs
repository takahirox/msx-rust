//! Original register probes and executable fixtures derived from hardware docs.
use msx_core::{
    cartridge::detect_mapper, cpu::Bus, firmware, BootPolicy, Firmware, Machine, Mapper, Model,
};
const SRAM: [Mapper; 4] = [
    Mapper::Ascii8Sram2,
    Mapper::Ascii8Sram8,
    Mapper::Ascii16Sram2,
    Mapper::Ascii16Sram8,
];
fn ascii8(kind: Mapper) -> bool {
    matches!(kind, Mapper::Ascii8Sram2 | Mapper::Ascii8Sram8)
}
fn image(kind: Mapper, size: usize) -> Vec<u8> {
    let stride = if ascii8(kind) { 8192 } else { 16384 };
    let mut rom = vec![0; size];
    for (i, bank) in rom.chunks_mut(stride).enumerate() {
        bank.fill(i as u8);
    }
    let header = if kind == Mapper::RType {
        15 * stride
    } else {
        0
    };
    rom[header..header + 4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    rom
}
fn loaded(kind: Mapper, size: usize) -> Machine {
    let mut m = Machine::default();
    m.load_cartridge_owned(image(kind, size), kind).unwrap();
    m.hardware.slots = 0x55;
    m
}
#[test]
fn stable_abi_and_exact_register_decoding_for_all_sram_variants() {
    for (id, kind) in SRAM.into_iter().enumerate() {
        assert_eq!(Mapper::try_from(id as u32 + 6).unwrap(), kind);
        let stride = if ascii8(kind) { 8192 } else { 16384 };
        let mut m = loaded(kind, 128 * 1024);
        let registers = if ascii8(kind) {
            vec![
                (0x6000, 0x67ff, 0x4100),
                (0x6800, 0x6fff, 0x6100),
                (0x7000, 0x77ff, 0x8100),
                (0x7800, 0x7fff, 0xa100),
            ]
        } else {
            vec![(0x6000, 0x67ff, 0x4100), (0x7000, 0x77ff, 0x8100)]
        };
        for &(start, end, read) in &registers {
            for a in start..=end {
                m.hardware.write(a, 3);
                assert_eq!(m.hardware.read(read), 3);
                m.hardware.write(a, 0);
            }
        }
        for a in [0, 0x3fff, 0x5fff, 0x8000, 0xbfff, 0xc000, 0xffff] {
            m.hardware.write(a, 3);
        }
        if !ascii8(kind) {
            for a in [0x6800, 0x6fff, 0x7800, 0x7fff] {
                m.hardware.write(a, 3);
            }
        }
        assert_eq!(m.hardware.read(0x4100), 0);
        assert_eq!(m.hardware.read(0x8100), 0);
        assert_eq!(m.hardware.read(0x0100), 0xff);
        assert_eq!(m.hardware.read(0xc100), 0xff);
        // Upper unused bits mirror ROM; bit 4 is reserved for ASCII16 SRAM.
        m.hardware.write(0x7000, 0xe7);
        assert_eq!(m.hardware.read(0x8100), (7 % (128 * 1024 / stride)) as u8);
        assert_eq!(m.cartridge_sram().len(), kind.sram_size());
        assert!(m.cartridge_sram().iter().all(|b| *b == 0xff));
    }
    for id in 0..=5 {
        assert_eq!(Mapper::try_from(id).unwrap() as u32, id);
    }
    assert_eq!(Mapper::try_from(10).unwrap(), Mapper::RType);
}
#[test]
fn selection_bits_mirroring_protection_and_slot_visibility() {
    for kind in SRAM {
        for size in [32, 128, 256] {
            let mut m = loaded(kind, size * 1024);
            let select = if ascii8(kind) {
                (size * 1024 / 8192) as u8
            } else {
                0x10
            };
            m.hardware.write(0x7000, select | 3);
            if ascii8(kind) {
                m.hardware.write(0x7800, select);
            }
            assert_eq!(m.hardware.read(0x8123), 0xff);
            m.hardware.write(0x8123, 0x6a);
            let mirror = 0x8123 + kind.sram_size() as u16;
            assert_eq!(m.hardware.read(mirror), 0x6a);
            m.hardware.write(mirror, 0x7b);
            assert_eq!(m.hardware.read(0x8123), 0x7b);
            m.hardware.write(0x6000, select); // read-only SRAM in page 1
            assert_eq!(m.hardware.read(0x4123), 0x7b);
            m.hardware.write(0x4123, 0x91);
            assert_eq!(m.hardware.read(0x8123), 0x7b);
            // Writes to RAM/unmapped slots affect neither mapper nor SRAM.
            m.hardware.slots = 0xaa;
            m.hardware.write(0x7000, 0);
            m.hardware.write(0x8123, 0x92);
            m.hardware.slots = 0x55;
            assert_eq!(m.hardware.read(0x8123), 0x7b);
            m.hardware.write(0x7000, 0);
            assert_eq!(m.hardware.read(0x8123), 0);
            m.hardware.write(0x8123, 0x93); // ROM protection
            assert_eq!(m.hardware.read(0x8123), 0);
            m.hardware.write(0x7000, select);
            assert_eq!(m.hardware.read(0x8123), 0x7b);
        }
    }
    // The ASCII8 select line follows physical ROM size, including bit 7 at 1 MiB.
    for kind in [Mapper::Ascii8Sram2, Mapper::Ascii8Sram8] {
        let mut m = loaded(kind, 1024 * 1024);
        m.hardware.write(0x7000, 0x20);
        assert_eq!(m.hardware.read(0x8100), 32);
        m.hardware.write(0x7000, 0x80);
        assert_eq!(m.hardware.read(0x8100), 255);
    }
}
#[test]
fn reset_retains_sram_and_replacement_firmware_model_and_eject_isolate_it() {
    for model in [Model::Msx1, Model::Msx2] {
        for kind in SRAM {
            let mut m = Machine::default();
            m.set_model(model);
            m.load_cartridge_owned(image(kind, 128 * 1024), kind)
                .unwrap();
            let save = vec![0x6c; kind.sram_size()];
            m.restore_cartridge_sram(&save).unwrap();
            for bad in [
                vec![],
                vec![0; kind.sram_size() - 1],
                vec![0; kind.sram_size() + 1],
            ] {
                assert!(m.restore_cartridge_sram(&bad).is_err());
                assert_eq!(m.cartridge_sram(), save);
            }
            m.hardware.slots = 0xd4;
            m.hardware.write(0x7000, 3);
            m.reset();
            m.hardware.slots = 0xd4;
            assert_eq!(m.hardware.read(0x8100), 0);
            assert_eq!(m.cartridge_sram(), save);
            if model == Model::Msx1 {
                m.load_bios(firmware::CBIOS_MSX1_MAIN).unwrap();
            } else {
                m.load_msx2_firmware(firmware::CBIOS_MSX2_MAIN, firmware::CBIOS_MSX2_SUB)
                    .unwrap();
            }
            assert_eq!(m.cartridge_sram(), save); // local firmware resets, retains cartridge
            m.load_cartridge_owned(image(kind, 128 * 1024), kind)
                .unwrap();
            assert!(m.cartridge_sram().iter().all(|b| *b == 0xff));
            m.restore_cartridge_sram(&save).unwrap();
            m.unload_cartridge();
            assert!(m.cartridge_sram().is_empty());
            for action in 0..4 {
                m.load_cartridge_owned(image(kind, 128 * 1024), kind)
                    .unwrap();
                m.restore_cartridge_sram(&save).unwrap();
                match action {
                    0 => m.use_cbios().unwrap(),
                    1 => m.use_builtin(),
                    2 => m.set_model(model),
                    _ => {
                        m.set_model(Model::Msx2);
                        m.use_diagnostic().unwrap();
                    }
                }
                assert!(m.cartridge_sram().is_empty());
            }
        }
    }
}
#[test]
fn malformed_and_ambiguous_imports_keep_unsaved_sram_model_firmware_and_banks() {
    let mut m = loaded(Mapper::Ascii16Sram8, 128 * 1024);
    m.hardware.write(0x7000, 0x10);
    m.hardware.write(0x8123, 0xa4);
    m.cpu.pc = 0x4567;
    let rom = m.hardware.cartridge.clone();
    let bios = m.hardware.bios.clone();
    for (bytes, kind) in [
        (vec![0; 128 * 1024], Mapper::Ascii16Sram8),
        (
            image(Mapper::Ascii16Sram8, 384 * 1024),
            Mapper::Ascii16Sram8,
        ),
        (
            image(Mapper::Ascii8Sram8, 2 * 1024 * 1024),
            Mapper::Ascii8Sram8,
        ),
        (image(Mapper::RType, 512 * 1024), Mapper::RType),
        (image(Mapper::Ascii8Sram8, 64 * 1024), Mapper::Auto),
    ] {
        assert!(m
            .load_cartridge_for_model_owned(bytes, kind, Model::Msx2)
            .is_err());
        assert_eq!(m.hardware.model, Model::Msx1);
        assert_eq!(m.hardware.firmware, Firmware::Cbios);
        assert_eq!(m.boot_policy, BootPolicy::Automatic);
        assert_eq!(m.cpu.pc, 0x4567);
        assert_eq!(m.hardware.read(0x8123), 0xa4);
        assert_eq!(m.hardware.cartridge, rom);
        assert_eq!(m.hardware.bios, bios);
    }
    for kind in SRAM {
        assert!(detect_mapper(&firmware::mapper_test_cartridge(kind))
            .mapper
            .is_none());
    }
}
#[test]
fn auto_sram_select_evidence_blocks_a_strong_rom_only_guess() {
    let mut rom = image(Mapper::Ascii8Sram8, 256 * 1024);
    rom[0x10..0x20].copy_from_slice(&[
        0x3e, 0, 0x32, 0, 0x60, // bank 0 remains executable
        0x3e, 0, 0x32, 0, 0x68, 0x3e, 0x20, 0x32, 0,
        0x70, // documented SRAM line, also a ROM-only mirror
        0x76,
    ]);
    let detection = detect_mapper(&rom);
    assert!(detection.mapper.is_none());
    assert!(detection
        .candidates
        .iter()
        .any(|c| c.mapper == Mapper::Ascii8 && c.score >= 9));
    assert!(detection.explanation.contains("Possible ASCII SRAM"));
}

#[test]
fn rtype_fixed_bank_chip_selection_full_decoder_and_physical_size() {
    let mut m = loaded(Mapper::RType, 384 * 1024);
    assert_eq!(m.hardware.read(0x4100), 15);
    assert_eq!(m.hardware.read(0x8100), 0);
    for a in [0x6000, 0x67ff, 0x6800, 0x6fff, 0x8000, 0xbfff] {
        m.hardware.write(a, 8);
    }
    assert_eq!(m.hardware.read(0x4100), 15);
    assert_eq!(m.hardware.read(0x8100), 0);
    for a in [0x7000, 0x77ff, 0x7800, 0x7fff] {
        for value in 0..=255 {
            m.hardware.write(a, value);
            let expected = if value & 16 == 0 {
                value & 15
            } else {
                16 | (value & 7)
            };
            assert_eq!(m.hardware.read(0x8100), expected);
            assert_eq!(m.hardware.read(0x4100), 15);
        }
    }
    m.reset();
    m.hardware.slots = 0xd4;
    assert_eq!(m.hardware.read(0x8100), 0);
    for size in [
        0,
        16384,
        256 * 1024,
        384 * 1024 - 1,
        384 * 1024 + 1,
        512 * 1024,
    ] {
        assert!(m
            .load_cartridge_owned(vec![0; size], Mapper::RType)
            .unwrap_err()
            .contains("384 KiB"));
    }
    assert!(m
        .load_cartridge_owned(vec![0; 384 * 1024], Mapper::RType)
        .unwrap_err()
        .contains("AB"));
    assert!(m
        .load_cartridge_owned(image(Mapper::RType, 384 * 1024), Mapper::Ascii16)
        .unwrap_err()
        .contains("power-of-two"));
}
fn boot(m: &mut Machine) {
    for _ in 0..600 {
        m.frame();
        if m.hardware.read(0xc000) == 0xa5 {
            return;
        }
    }
    panic!("fixture did not execute at {:04x}", m.cpu.pc);
}
#[test]
fn original_z80_fixtures_execute_banked_code_restore_saves_and_show_video_on_both_models() {
    for model in [Model::Msx1, Model::Msx2] {
        for kind in SRAM.into_iter().chain([Mapper::RType]) {
            let mut m = Machine::default();
            m.set_model(model);
            m.load_cartridge_owned(firmware::mapper_test_cartridge(kind), kind)
                .unwrap();
            boot(&mut m);
            assert_eq!(
                m.hardware.read(0xc002),
                if kind == Mapper::RType { 23 } else { 7 }
            );
            assert_eq!(
                m.hardware.vdp.registers[7],
                if kind == Mapper::RType { 10 } else { 4 }
            );
            if kind == Mapper::RType {
                continue;
            }
            assert_eq!(m.hardware.read(0xc001), 4);
            m.hardware.key(8, 0, true);
            m.frame();
            m.hardware.key(8, 0, false);
            m.frame();
            assert_eq!(m.hardware.read(0xc001), 5);
            assert_eq!(m.cartridge_sram()[0], 5);
            let save = m.cartridge_sram().to_vec();
            m.reset();
            boot(&mut m);
            assert_eq!(m.hardware.read(0xc001), 5);
            m.load_cartridge_owned(firmware::mapper_test_cartridge(kind), kind)
                .unwrap();
            m.restore_cartridge_sram(&save).unwrap();
            boot(&mut m);
            assert_eq!(m.hardware.read(0xc001), 5);
            assert_eq!(m.hardware.vdp.registers[7], 5);
        }
    }
}
