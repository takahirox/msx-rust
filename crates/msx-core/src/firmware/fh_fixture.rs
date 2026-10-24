//! Original MIT-licensed CPU/port/IM1 probe for V9938 FH, without BIOS services.
use crate::font;

fn out(code: &mut Vec<u8>, port: u8, value: u8) {
    code.extend_from_slice(&[0x3e, value, 0xd3, port]);
}
fn reg(code: &mut Vec<u8>, register: u8, value: u8) {
    out(code, 0x99, value);
    out(code, 0x99, 0x80 | register);
}
fn jump(code: &mut Vec<u8>, opcode: u8, address: u16) {
    code.push(opcode);
    code.extend_from_slice(&address.to_le_bytes());
}
fn here(code: &[u8]) -> u16 {
    0x4010 + code.len() as u16
}
fn delay(code: &mut Vec<u8>) {
    // More than one NTSC frame; no status reads that could acknowledge FH.
    code.extend_from_slice(&[0x21, 0, 0x10]); // LD HL,1000h
    let loop_at = here(code);
    code.extend_from_slice(&[0x2b, 0x7c, 0xb5]); // DEC HL; LD A,H; OR L
    jump(code, 0xc2, loop_at);
}
fn check(code: &mut Vec<u8>, expected: u8, stage: u8) {
    code.extend_from_slice(&[0xfe, expected]);
    jump(code, 0xc2, 0x5800); // JP NZ, failure display
    code.extend_from_slice(&[0x3e, 0xa5, 0x32, stage, 0xc2]);
}
fn pair(code: &mut Vec<u8>) {
    // Two IN instructions 14 MSX T-states apart (two M1 fetch waits). Both must read FH=1 without
    // acknowledgement. Bounded search avoids a hang on the old latched model.
    code.extend_from_slice(&[0x01, 0x99, 0, 0x21, 0xff, 0xff]);
    let loop_at = here(code);
    code.extend_from_slice(&[0xed, 0x50, 0xed, 0x58, 0x7a, 0xa3, 0xe6, 1]);
    let success = code.len();
    jump(code, 0xc2, 0);
    code.extend_from_slice(&[0x2b, 0x7c, 0xb5]);
    jump(code, 0xc2, loop_at);
    jump(code, 0xc3, 0x5800);
    let address = here(code).to_le_bytes();
    code[success + 1..success + 3].copy_from_slice(&address);
}
fn display(code: &mut Vec<u8>, pass: bool) {
    fn address(code: &mut Vec<u8>, address: u16) {
        out(code, 0x99, address as u8);
        out(code, 0x99, 0x40 | (address >> 8) as u8);
    }
    reg(code, 0, 0);
    for (r, v) in [(1, 0x40), (2, 6), (3, 0x80), (4, 0), (5, 0x36), (7, 0xf1)] {
        reg(code, r, v);
    }
    address(code, 0x1800);
    code.extend_from_slice(&[
        0x01, 0, 3, 0x3e, 32, 0xd3, 0x98, 0x0b, 0x78, 0xb1, 0x20, 0xf7,
    ]);
    address(code, 0x1b00);
    out(code, 0x98, 208);
    let text = if pass { b"FH PASS" } else { b"FH FAIL" };
    for ch in text {
        address(code, u16::from(*ch) * 8);
        for byte in font::glyph(*ch) {
            out(code, 0x98, byte);
        }
    }
    address(code, 0x400);
    for _ in 0..8 {
        out(code, 0x98, 255);
    }
    address(code, 0x2000);
    for _ in 0..32 {
        out(code, 0x98, 0xf1);
    }
    address(code, 0x2010);
    out(code, 0x98, if pass { 0x21 } else { 0x61 });
    address(code, 0x1800);
    for ch in text {
        out(code, 0x98, *ch);
    }
    address(code, 0x1840);
    for _ in 0..8 {
        out(code, 0x98, 128);
    }
    code.extend_from_slice(&[
        0x3e,
        if pass { 0xa5 } else { 0xee },
        0x32,
        0x10,
        0xc2,
        0x76,
        0x18,
        0xfd,
    ]);
}

pub fn fh_test_cartridge() -> Vec<u8> {
    let mut rom = vec![0; 16384];
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    let mut code = vec![0xf3, 0x31, 0, 0xf0]; // DI; private RAM stack
    out(&mut code, 0xa8, 0xf7); // RAM in page zero, cartridge in page one
    code.extend_from_slice(&[0x3e, 0xff, 0x32, 0xff, 0xff]);
    out(&mut code, 0xfc, 3);
    // Install an original IM1 handler in page-zero RAM. Count and acknowledge
    // only horizontal interrupts; restore AF, reenable, RETI.
    for (i, byte) in [
        0xf5, 0x3a, 0, 0xc2, 0x3c, 0x32, 0, 0xc2, 0xdb, 0x99, 0xf1, 0xfb, 0xed, 0x4d,
    ]
    .into_iter()
    .enumerate()
    {
        code.extend_from_slice(&[0x3e, byte, 0x32, 0x38 + i as u8, 0]);
    }
    for offset in 0..=0x10 {
        code.extend_from_slice(&[0xaf, 0x32, offset, 0xc2]);
    }
    for (r, value) in [
        (0, 6),
        (1, 0),
        (8, 2),
        (9, 0),
        (18, 0),
        (19, 32),
        (23, 0),
        (15, 1),
    ] {
        reg(&mut code, r, value);
    }
    pair(&mut code);
    code.extend_from_slice(&[0x3e, 1]);
    check(&mut code, 1, 1); // Disabled repeated reads.
    code.extend_from_slice(&[0; 30]); // 150 CPU cycles: pulse must expire.
    code.extend_from_slice(&[0xdb, 0x99, 0xe6, 1]);
    check(&mut code, 0, 2);
    reg(&mut code, 0, 0x16); // Enable after a polled match; no stale FH.
    code.extend_from_slice(&[0xdb, 0x99, 0xe6, 1]);
    check(&mut code, 0, 3);
    delay(&mut code); // Enabled latch persists beyond the border pulse.
    code.extend_from_slice(&[0xdb, 0x99, 0xe6, 1]);
    check(&mut code, 1, 4);
    delay(&mut code);
    // Direct disable, indirect reenable: a disabled latch cannot resurrect.
    reg(&mut code, 0, 6);
    reg(&mut code, 17, 0x80);
    out(&mut code, 0x9b, 0x16);
    code.extend_from_slice(&[0xdb, 0x99, 0xe6, 1]);
    check(&mut code, 0, 5);
    code.extend_from_slice(&[0xed, 0x56, 0xfb]); // IM1; EI
    delay(&mut code);
    code.push(0xf3);
    // At least one real interrupt, independent of exact instruction alignment.
    code.extend_from_slice(&[0x3a, 0, 0xc2, 0xb7]);
    jump(&mut code, 0xca, 0x5800);
    code.extend_from_slice(&[0x3e, 1]);
    check(&mut code, 1, 6);
    display(&mut code, true);
    assert!(code.len() < 0x17f0);
    rom[0x10..0x10 + code.len()].copy_from_slice(&code);
    code.clear();
    display(&mut code, false);
    rom[0x1800..0x1800 + code.len()].copy_from_slice(&code);
    rom
}
