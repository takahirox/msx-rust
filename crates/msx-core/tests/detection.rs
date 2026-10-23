//! Independently authored MIT-licensed code/data; no ROM samples or identifiers.
use msx_core::{
    cartridge::{detect_mapper, Confidence},
    cpu::Bus,
    firmware, Machine, Mapper, Model,
};

fn image(size: usize, code: &[u8]) -> Vec<u8> {
    let mut rom = vec![0; size];
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    rom[0x10..0x10 + code.len()].copy_from_slice(code);
    rom
}

fn stores(addresses: &[u16]) -> Vec<u8> {
    let mut code = Vec::new();
    for address in addresses {
        code.extend([0x3e, 0, 0x32, *address as u8, (address >> 8) as u8]);
    }
    code.push(0x76);
    code
}

fn banked(mapper: Mapper, size: usize) -> Vec<u8> {
    let bank_size = if mapper == Mapper::Ascii16 {
        16384
    } else {
        8192
    };
    let last = size / bank_size - 1;
    let register: u16 = match mapper {
        Mapper::Ascii8 | Mapper::Ascii16 => 0x7000,
        Mapper::Konami => 0x8000,
        _ => 0x9000,
    };
    // Map both cartridge pages explicitly; a real BIOS maps the AB-init page.
    let mut code = vec![0xf3, 0x3e, 0xd4, 0xd3, 0xa8];
    if mapper == Mapper::KonamiScc {
        code.extend([0x3e, 0, 0x32, 0, 0x50]);
    }
    code.extend([
        0x3e,
        last as u8,
        0x32,
        register as u8,
        (register >> 8) as u8,
        0xc3,
        0,
        0x80,
    ]);
    let mut rom = image(size, &code);
    // At least four actual instructions, then HALT; only the intended stride reaches this code.
    rom[last * bank_size..last * bank_size + 8]
        .copy_from_slice(&[0x3e, 0x5a, 0x32, 0, 0xc0, 0xf3, 0x76, 0]);
    rom
}

#[test]
fn recognizes_plain_formats_and_header_positions() {
    for size in [8192, 16384, 32768, 49152] {
        let mut rom = image(size, &[0x76]);
        if size == 49152 {
            rom.copy_within(..32, 16384);
            rom[..32].fill(0);
        }
        let d = detect_mapper(&rom);
        assert_eq!(
            (d.mapper, d.confidence),
            (Some(Mapper::Plain), Confidence::PlainFormat)
        );
    }
    let mut rom = vec![0; 32768];
    rom[16384..16388].copy_from_slice(&[b'A', b'B', 0x10, 0x80]);
    rom[16400] = 0x76;
    assert_eq!(detect_mapper(&rom).mapper, Some(Mapper::Plain));
    assert_eq!(
        detect_mapper(&firmware::test_cartridge()).mapper,
        Some(Mapper::Plain)
    );
}

#[test]
fn infers_each_mapper_and_executes_in_both_machine_models() {
    for mapper in [
        Mapper::Ascii8,
        Mapper::Ascii16,
        Mapper::Konami,
        Mapper::KonamiScc,
    ] {
        let rom = banked(mapper, 128 * 1024);
        let d = detect_mapper(&rom);
        assert_eq!(d.mapper, Some(mapper), "{d:?}");
        assert_eq!(d.confidence, Confidence::Likely);
        assert!(d.candidates[0].banked_transfer);
        assert!(d.explanation.contains("estimate"));
        assert_eq!(d, detect_mapper(&rom));
        for model in [Model::Msx1, Model::Msx2] {
            let mut m = Machine::default();
            m.set_model(model);
            m.load_cartridge(&rom).unwrap();
            assert_eq!(m.hardware.model, model);
            for _ in 0..600 {
                m.frame();
                if m.hardware.read(0xc000) == 0x5a {
                    break;
                }
            }
            assert!(m.cpu.halted);
            assert_eq!(m.hardware.read(0xc000), 0x5a);
        }
    }
    // A small bank-switched image must not be classified solely by its plain-compatible size.
    assert_eq!(
        detect_mapper(&banked(Mapper::Ascii8, 32768)).mapper,
        Some(Mapper::Ascii8)
    );
    // Full supported ASCII bank ranges are content-detected too.
    for (mapper, size) in [
        (Mapper::Ascii8, 2 * 1024 * 1024),
        (Mapper::Ascii16, 4 * 1024 * 1024),
    ] {
        assert_eq!(detect_mapper(&banked(mapper, size)).mapper, Some(mapper));
    }
}

#[test]
fn competing_register_windows_and_noncanonical_addresses() {
    for (mapper, addresses) in [
        (Mapper::Ascii8, vec![0x6800, 0x7800]),
        (Mapper::Konami, vec![0x6000, 0x8000, 0xa000]),
        (Mapper::KonamiScc, vec![0x5000, 0x9000]),
        (Mapper::KonamiScc, vec![0x57ff, 0x77ff, 0x97ff, 0xb7ff]),
    ] {
        let d = detect_mapper(&image(131072, &stores(&addresses)));
        assert_eq!(d.mapper, Some(mapper), "{d:?}");
        assert!(d.candidates[0].register_windows >= 2);
    }
    // Both ASCII variants decode these addresses. Absence of 6800/7800 is not proof of ASCII16.
    let d = detect_mapper(&image(131072, &stores(&[0x6000, 0x7000])));
    assert_eq!((d.mapper, d.confidence), (None, Confidence::Ambiguous));
    assert_eq!(d.candidates[0].score, d.candidates[1].score);
    // Stable hint ordering, never a tie-breaking guess.
    assert_eq!(d.candidates[0].mapper, Mapper::Ascii8);
    // Incompatible families compete; the first distinctive store cannot win unconditionally.
    let d = detect_mapper(&image(131072, &stores(&[0x6800, 0x7800, 0x5000, 0x9000])));
    assert_eq!(d.mapper, None, "{d:?}");
}

#[test]
fn incidental_data_operands_prefixes_and_generic_writes_are_not_identity() {
    let patterns = stores(&[0x6800, 0x7800, 0x5000, 0x9000, 0x8000, 0xa000]);
    let mut rom = image(131072, &[0xc3, 0x00, 0x41]); // skip a data table
    rom[0x20..0x20 + patterns.len()].copy_from_slice(&patterns);
    rom[0x100] = 0x76;
    assert_eq!(detect_mapper(&rom).confidence, Confidence::Insufficient);
    for code in [
        vec![0x21, 0x32, 0x68, 0x01, 0x32, 0x78, 0x76], // 32 is an immediate operand
        vec![0xdd, 0x21, 0x32, 0x68, 0xfd, 0x21, 0x32, 0x78, 0x76],
        vec![0xdd, 0x36, 0x32, 0x68, 0xfd, 0xcb, 0x32, 0x78, 0x76],
        vec![0xed, 0x43, 0x32, 0x68, 0xed, 0x53, 0x32, 0x78, 0x76],
        vec![0x76], // a size and header alone cannot identify any MegaROM
        stores(&[0x6000]),
        stores(&[0x7000; 20]), // repeated generic writes do not accumulate points
        stores(&[0x9800, 0xc000]), // SCC audio and RAM writes are not mapper evidence
    ] {
        let d = detect_mapper(&image(131072, &code));
        assert_eq!(d.mapper, None, "{d:?}");
    }
    // Do not force Plain either if a small image contains unresolved banking evidence.
    assert_eq!(
        detect_mapper(&image(32768, &stores(&[0x7000]))).mapper,
        None
    );
    // Mirrored bank values do not establish a physical transfer in a one-bank image.
    let mut mirrored = image(8192, &[0x3e, 1, 0x32, 0, 0x70, 0xc3, 0x20, 0x80]);
    mirrored[0x20..0x28].copy_from_slice(&[0x3e, 1, 0xd3, 0x99, 0xf3, 0x76, 0, 0]);
    assert_eq!(detect_mapper(&mirrored).mapper, None);
}

#[test]
fn index_prefixed_control_flow_skips_unreachable_mapper_writes() {
    for prefix in [0xdd, 0xfd] {
        for transfer in [
            vec![prefix, 0xc3, 0x00, 0x81], // JP 8100h (index prefix ignored)
            vec![prefix, 0x18, 0x0d],       // JR 4020h, relative to the prefixed length
            vec![prefix, 0xcd, 0x00, 0x81], // CALL: do not invent a return state
            vec![prefix, 0xc9],             // RET
            vec![prefix, 0x76],             // HALT
            vec![prefix, 0xe9],             // JP (IX/IY): unknown destination
            vec![prefix, 0xc7],             // RST 0
        ] {
            let mut code = transfer.clone();
            code.extend(stores(&[0x6800, 0x7800]));
            let mut rom = image(32768, &code);
            // Preserve the exact indexed-branch locations while preparing both
            // cartridge pages under the default MSX2 C-BIOS as well as MSX1.
            rom[2] = 0x80;
            rom[0x80..0x88].copy_from_slice(&[0xf3, 0x3e, 0xd4, 0xd3, 0xa8, 0xc3, 0x10, 0x40]);
            let destination = [0x3e, 0x5a, 0x32, 0x00, 0xc0, 0x76];
            rom[0x4100..0x4106].copy_from_slice(&destination);
            rom[0x20..0x26].copy_from_slice(&destination);
            let d = detect_mapper(&rom);
            assert_eq!(
                (d.mapper, d.confidence),
                (Some(Mapper::Plain), Confidence::PlainFormat),
                "{transfer:02x?}: {d:?}"
            );
            assert!(d.candidates.iter().all(|c| c.register_windows == 0));
            if transfer[1] == 0xc3 || transfer[1] == 0x18 {
                for model in [Model::Msx1, Model::Msx2] {
                    let mut m = Machine::default();
                    m.set_model(model);
                    m.load_cartridge(&rom).unwrap();
                    for _ in 0..600 {
                        m.frame();
                        if m.hardware.read(0xc000) == 0x5a {
                            break;
                        }
                    }
                    assert!(m.cpu.halted);
                    assert_eq!(m.hardware.read(0xc000), 0x5a);
                }
            }
        }
    }
}

#[test]
fn index_prefixed_branches_follow_the_effective_target() {
    for prefix in [0xdd, 0xfd] {
        for branch in [
            vec![prefix, 0xc3, 0x00, 0x41], // JP
            vec![prefix, 0xcd, 0x00, 0x41], // CALL
            vec![prefix, 0xc2, 0x00, 0x41], // JP NZ
            vec![prefix, 0xc4, 0x00, 0x41], // CALL NZ
            vec![prefix, 0x18, 0x0d],       // JR
            vec![prefix, 0x20, 0x0d],       // JR NZ
            vec![prefix, 0x10, 0x0d],       // DJNZ
        ] {
            let mut code = branch.clone();
            code.push(0x76);
            let mut rom = image(131072, &code);
            let target = if branch.len() == 4 { 0x100 } else { 0x20 };
            let writes = stores(&[0x6800, 0x7800]);
            rom[target..target + writes.len()].copy_from_slice(&writes);
            let d = detect_mapper(&rom);
            assert_eq!(d.mapper, Some(Mapper::Ascii8), "{branch:02x?}: {d:?}");
            assert_eq!(d.confidence, Confidence::Likely);
        }
    }
}

#[test]
fn calls_branches_unknown_values_and_truncated_instructions_are_bounded() {
    let mut rom = image(131072, &[0xcd, 0x00, 0x41]);
    let code = stores(&[0x6800, 0x7800]);
    rom[0x100..0x100 + code.len()].copy_from_slice(&code);
    assert_eq!(detect_mapper(&rom).mapper, Some(Mapper::Ascii8));
    let mut rom = image(131072, &[0x20, 0x0e, 0x76]);
    rom[0x20..0x20 + code.len()].copy_from_slice(&code);
    assert_eq!(detect_mapper(&rom).mapper, Some(Mapper::Ascii8));
    for code in [
        vec![0xc3, 0x10, 0x40],                            // infinite code loop
        vec![0x3a, 0, 0xc0, 0x32, 0, 0x70, 0xc3, 0, 0x80], // unknown bank
        vec![0xdd, 0xdd, 0xdd],                            // repeated index prefixes
        vec![0xc3, 0xff, 0xbf], // truncated three-byte instruction at end of window
    ] {
        let mut rom = image(131072, &code);
        rom[0x7fff] = 0x32;
        assert_eq!(detect_mapper(&rom).mapper, None);
    }
}

#[test]
fn analysis_limits_do_not_turn_partial_scores_into_a_guess() {
    let mut rom = image(131072, &stores(&[0x6800, 0x7800]));
    // INC A code continues through initial banks and exceeds the bounded instruction budget.
    rom[0x1a..0x2000].fill(0x3c);
    let d = detect_mapper(&rom);
    assert_eq!(d.mapper, None, "{d:?}");
    assert_eq!(d.confidence, Confidence::Insufficient);
    assert!(d.candidates.iter().any(|c| c.truncated));
    assert!(d.explanation.contains("analysis limit"));
}

#[test]
fn malformed_unsupported_and_ambiguous_loads_preserve_the_active_machine() {
    let mut m = Machine::default();
    m.load_cartridge_owned(banked(Mapper::Ascii8, 131072), Mapper::Auto)
        .unwrap();
    for _ in 0..600 {
        m.frame();
        if m.hardware.read(0xc000) == 0x5a {
            break;
        }
    }
    assert_eq!(m.hardware.read(0xc000), 0x5a);
    m.cpu.pc = 0x8123;
    m.hardware.write(0xc001, 0x33);
    let before = m.hardware.cartridge.clone();
    let pixels = m.hardware.vdp.pixels.clone();
    let bios = m.hardware.bios.clone();
    for rom in [
        vec![],
        vec![b'A'],
        vec![0; 131072],
        vec![0; 17],
        image(65535, &[0x76]),
        image(8 * 1024 * 1024, &[0x76]),
        image(131072, &stores(&[0x6000, 0x7000])),
        image(131072, &stores(&[0x6800, 0x7800, 0x5000, 0x9000])),
    ] {
        let d = detect_mapper(&rom);
        assert_eq!(d.mapper, None);
        assert!(d.explanation.contains("same file again"));
        assert!(m.load_cartridge_owned(rom, Mapper::Auto).is_err());
        assert_eq!(m.cpu.pc, 0x8123);
        assert_eq!(m.hardware.read(0xc001), 0x33);
        assert_eq!(m.hardware.read(0x8000), 0x3e); // switched bank retained
        assert_eq!(m.hardware.cartridge, before);
        assert_eq!(m.hardware.vdp.pixels, pixels);
        assert_eq!(m.hardware.bios, bios);
    }
    for mapper in [
        Mapper::Plain,
        Mapper::Ascii8,
        Mapper::Ascii16,
        Mapper::Konami,
        Mapper::KonamiScc,
    ] {
        // Manual overrides bypass choice even when the content has conflicting evidence.
        m.load_cartridge_owned(image(32768, &stores(&[0x6000, 0x7000])), mapper)
            .unwrap();
        assert!(m.load_cartridge_owned(vec![], mapper).is_err());
    }
}
