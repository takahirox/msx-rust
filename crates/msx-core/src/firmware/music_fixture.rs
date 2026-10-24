//! Original MIT-licensed diagnostics. These contain no vendor FM-BIOS code.
//! The firmware's WVFMTEST marker deliberately identifies only this fixture.
pub fn music_diagnostic_firmware() -> Vec<u8> {
    let mut rom = vec![0xff; 16384];
    rom[..4].copy_from_slice(&[b'A', b'B', 0, 0]);
    rom[0x18..0x20].copy_from_slice(b"WVFMTEST");
    rom[0x100..0x103].copy_from_slice(&[0x3e, 0x65, 0xc9]); // LD A,65h; RET
    rom
}
/// Executes from cartridge page 2 while discovering the independently installed
/// firmware through page 1. Reports exact results at C100h..C105h and green/red
/// backdrop. Programs a sustained custom sine through normal, spaced Z80 OUTs.
pub fn music_test_cartridge() -> Vec<u8> {
    let mut rom = vec![0; 16384];
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x80]);
    let mut code = vec![0xf3, 0x31, 0, 0xf0]; // DI; LD SP,F000h
    let mut failures = Vec::new();
    fn out(c: &mut Vec<u8>, port: u8, value: u8) {
        c.extend_from_slice(&[0x3e, value, 0xd3, port]);
    }
    fn check(c: &mut Vec<u8>, failures: &mut Vec<usize>, expected: u8) {
        c.extend_from_slice(&[0xfe, expected, 0xc2, 0, 0]);
        failures.push(c.len() - 2);
    }
    // Select four distinct mapper segments and expanded primary slot 3/page 1.
    for (port, value) in [(0xfc, 3), (0xfd, 2), (0xfe, 1), (0xff, 0), (0xa8, 0xdc)] {
        out(&mut code, port, value);
    }
    code.extend_from_slice(&[0x06, 0]); // B=secondary slot to probe
    let probe = 0x8010 + code.len() as u16;
    code.extend_from_slice(&[
        0x78, 0x87, 0x87, 0xf6, 0xf3, 0x32, 0xff, 0xff, // secondary=(B<<2)|F3h
        0x3a, 0x18, 0x40, 0xfe, b'W', 0xca, 0, 0, // marker found?
    ]);
    let found_fixup = code.len() - 2;
    code.extend_from_slice(&[0x04, 0x78, 0xfe, 4, 0xc2, probe as u8, (probe >> 8) as u8]);
    code.extend_from_slice(&[0xc3, 0, 0]);
    failures.push(code.len() - 2);
    let found = 0x8010 + code.len() as u16;
    code[found_fixup..found_fixup + 2].copy_from_slice(&found.to_le_bytes());
    code.extend_from_slice(&[0x78, 0x32, 0x00, 0xc1]); // discovered subslot -> C100
    check(&mut code, &mut failures, 1);
    code.extend_from_slice(&[0x3a, 0xff, 0xff, 0x32, 0x01, 0xc1]);
    check(&mut code, &mut failures, 8); // inverted secondary selector F7h
    code.extend_from_slice(&[0x3a, 0x18, 0x40, 0x32, 0x02, 0xc1]);
    check(&mut code, &mut failures, b'W');
    // ROM protection, then actual Z80 CALL through the installed firmware.
    code.extend_from_slice(&[0xaf, 0x32, 0x18, 0x40, 0x3a, 0x18, 0x40]);
    check(&mut code, &mut failures, b'W');
    code.extend_from_slice(&[0xcd, 0, 0x41, 0x32, 0x03, 0xc1]);
    check(&mut code, &mut failures, 0x65);
    code.extend_from_slice(&[0xdb, 0x7d, 0x32, 0x04, 0xc1]);
    check(&mut code, &mut failures, 255);
    for (r, v) in [0x21, 0x21, 0x3f, 0, 0, 0xf0, 0, 0x0f]
        .into_iter()
        .enumerate()
        .map(|(r, v)| (r as u8, v))
        .chain([(0x30, 0), (0x10, 0x22), (0x20, 0x19)])
    {
        out(&mut code, 0x7c, r);
        code.extend_from_slice(&[0, 0]); // >=12 clocks address recovery, including LD A
        out(&mut code, 0x7d, v);
        code.extend_from_slice(&[0; 18]); // >=84 clocks data recovery (18 MSX NOPs=90)
    }
    // Hide firmware; ports and synthesis continue to operate independently.
    out(&mut code, 0xa8, 0xd4);
    code.extend_from_slice(&[0x3e, 255, 0x32, 0xff, 0xff]);
    code.extend_from_slice(&[0x3e, 0xa5, 0x32, 0x05, 0xc1]);
    out(&mut code, 0x99, 2);
    out(&mut code, 0x99, 0x87); // green backdrop
    code.extend_from_slice(&[0x76, 0x18, 0xfd]);
    let failure = 0x8010 + code.len() as u16;
    out(&mut code, 0x99, 8);
    out(&mut code, 0x99, 0x87); // red backdrop
    code.extend_from_slice(&[0x76, 0x18, 0xfd]);
    for offset in failures {
        code[offset..offset + 2].copy_from_slice(&failure.to_le_bytes());
    }
    rom[16..16 + code.len()].copy_from_slice(&code);
    rom
}
