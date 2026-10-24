//! Independently authored MIT Z80 diagnostic; no vendor firmware or identifiers.
pub fn fm_pac_diagnostic_firmware() -> Vec<u8> {
    let mut rom = vec![0xff; 65536];
    for bank in 0..4 {
        let offset = bank * 16384;
        rom[offset..offset + 4].copy_from_slice(&[b'A', b'B', 0, 0]);
        rom[offset + 0x18..offset + 0x20].copy_from_slice(b"WVFPTEST");
        // Actual executable bank identities, deliberately not a production FM BIOS.
        rom[offset + 0x100..offset + 0x103].copy_from_slice(&[0x3e, 0x51 + bank as u8, 0xc9]);
    }
    rom
}
/// Executes in game slot 1/page 2 under either model's C-BIOS. Discover all four
/// slot-2 subslots from cartridge page 2 while page 3 selects the expansion latch.
/// Literal success bytes C100..C109: 01 FB 51 52 53 54 FF 4D 69 A5.
pub fn fm_pac_test_cartridge() -> Vec<u8> {
    let mut rom = vec![0; 16384];
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x80]);
    let mut code = vec![0xf3, 0x31, 0, 0xf0];
    let mut failures = Vec::new();
    fn out(code: &mut Vec<u8>, port: u8, value: u8) {
        code.extend_from_slice(&[0x3e, value, 0xd3, port]);
    }
    fn store(code: &mut Vec<u8>, address: u16, value: u8) {
        code.extend_from_slice(&[0x3e, value, 0x32]);
        code.extend_from_slice(&address.to_le_bytes());
    }
    fn check(code: &mut Vec<u8>, failures: &mut Vec<usize>, expected: u8) {
        code.extend_from_slice(&[0xfe, expected, 0xc2, 0, 0]);
        failures.push(code.len() - 2);
    }
    // Distinct MSX2 RAM segments; these ports are disconnected on fixed-RAM MSX1.
    for (port, value) in [(0xfc, 3), (0xfd, 2), (0xfe, 1), (0xff, 0)] {
        out(&mut code, port, value);
    }
    // RAM page 3 in subslot 3 on MSX2; ordinary slot 3 on MSX1.
    out(&mut code, 0xa8, 0xd4);
    store(&mut code, 0xffff, 0xff);
    code.extend_from_slice(&[0x06, 0]); // B = candidate subslot
    let probe = 0x8010 + code.len() as u16;
    out(&mut code, 0xa8, 0x98); // slot 2 pages 1/3, cartridge page 2
    code.extend_from_slice(&[0x78, 0x87, 0x87, 0x32, 0xff, 0xff]);
    code.extend_from_slice(&[0x3a, 0x18, 0x40, 0xfe, b'W', 0xca, 0, 0]);
    let found_fixup = code.len() - 2;
    code.extend_from_slice(&[0x04, 0x78, 0xfe, 4, 0xc2]);
    code.extend_from_slice(&probe.to_le_bytes());
    // Missing device must still restore RAM before reporting failure.
    out(&mut code, 0xa8, 0xd8);
    code.extend_from_slice(&[0xc3, 0, 0]);
    failures.push(code.len() - 2);
    let found = 0x8010 + code.len() as u16;
    code[found_fixup..found_fixup + 2].copy_from_slice(&found.to_le_bytes());
    code.extend_from_slice(&[0x3a, 0xff, 0xff, 0x4f]); // C = inverted latch
    out(&mut code, 0xa8, 0xd8); // RAM page 3; FM-PAC page 1
    code.extend_from_slice(&[0x78, 0x32, 0, 0xc1]);
    check(&mut code, &mut failures, 1);
    code.extend_from_slice(&[0x79, 0x32, 1, 0xc1]);
    check(&mut code, &mut failures, 0xfb);
    for bank in 0..4 {
        store(&mut code, 0x7ff7, bank);
        code.extend_from_slice(&[0xcd, 0, 0x41, 0x32, 2 + bank, 0xc1]);
        check(&mut code, &mut failures, 0x51 + bank);
    }
    code.extend_from_slice(&[0xdb, 0x7c, 0x32, 6, 0xc1]);
    check(&mut code, &mut failures, 0xff);
    // Partial unlock must keep ROM; complete unlock exposes SRAM plus latches.
    store(&mut code, 0x5ffe, 0x4d);
    code.extend_from_slice(&[0x3a, 0x18, 0x40]);
    check(&mut code, &mut failures, b'W');
    store(&mut code, 0x5fff, 0x69);
    for (a, expected, result) in [(0x5ffe_u16, 0x4d, 7), (0x5fff, 0x69, 8)] {
        code.push(0x3a);
        code.extend_from_slice(&a.to_le_bytes());
        code.extend_from_slice(&[0x32, result, 0xc1]);
        check(&mut code, &mut failures, expected);
    }
    // Read/write/verify every usable byte without changing an existing save.
    code.extend_from_slice(&[0x21, 0, 0x40, 0x01, 0xfe, 0x1f]); // HL=4000, BC=8190
    let roundtrip = 0x8010 + code.len() as u16;
    code.extend_from_slice(&[0x7e, 0x77, 0xbe, 0xc2, 0, 0]); // A=(HL); (HL)=A; CP (HL)
    failures.push(code.len() - 2);
    code.extend_from_slice(&[0x23, 0x0b, 0x78, 0xb1, 0xc2]);
    code.extend_from_slice(&roundtrip.to_le_bytes());
    // Program custom unmodulated sine via memory with port I/O still disabled.
    for (r, v) in [0x21, 0x21, 0x3f, 0, 0, 0xf0, 0, 0x0f]
        .into_iter()
        .enumerate()
        .map(|(r, v)| (r as u8, v))
        .chain([(0x30, 0), (0x10, 0x11), (0x20, 0x19)])
    {
        store(&mut code, 0x7ff4, r);
        code.extend_from_slice(&[0, 0]);
        store(&mut code, 0x7ff5, v);
        code.extend_from_slice(&[0; 18]);
    }
    // Enabled I/O changes Fnum 273 to 290. Disabled writes try Fnum 324;
    // the final independently measured pitch must still be Fnum 290.
    store(&mut code, 0x7ff6, 1);
    out(&mut code, 0x7c, 0x10);
    code.extend_from_slice(&[0, 0]);
    out(&mut code, 0x7d, 0x22);
    code.extend_from_slice(&[0; 18]);
    store(&mut code, 0x7ff6, 0);
    out(&mut code, 0x7c, 0x10);
    code.extend_from_slice(&[0, 0]);
    out(&mut code, 0x7d, 0x44); // ignored, pitch stays at 290
    code.extend_from_slice(&[0; 18]);
    out(&mut code, 0xa8, 0xd4); // hide device without changing phase/sound
    store(&mut code, 0xc109, 0xa5);
    out(&mut code, 0x99, 0);
    out(&mut code, 0x99, 0x81); // display off: diagnostic backdrop has no BIOS text
    out(&mut code, 0x99, 2);
    out(&mut code, 0x99, 0x87);
    code.extend_from_slice(&[0x76, 0x18, 0xfd]);
    let failure = 0x8010 + code.len() as u16;
    out(&mut code, 0x99, 8);
    out(&mut code, 0x99, 0x87);
    code.extend_from_slice(&[0x76, 0x18, 0xfd]);
    for offset in failures {
        code[offset..offset + 2].copy_from_slice(&failure.to_le_bytes());
    }
    rom[16..16 + code.len()].copy_from_slice(&code);
    rom
}
