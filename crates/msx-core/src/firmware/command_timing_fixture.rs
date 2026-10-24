//! Original MIT Z80 probe for Issue #77; no BIOS calls or production timing helpers.
use crate::font;

const ORIGIN: u16 = 0x4010;
const FAILURE: u16 = 0x7000;
fn out(code: &mut Vec<u8>, port: u8, value: u8) {
    code.extend([0x3e, value, 0xd3, port]);
}
fn reg(code: &mut Vec<u8>, r: u8, value: u8) {
    out(code, 0x99, value);
    out(code, 0x99, 0x80 | r);
}
fn word(code: &mut Vec<u8>, r: u8, value: u16) {
    reg(code, r, value as u8);
    reg(code, r + 1, (value >> 8) as u8);
}
fn address(code: &mut Vec<u8>, a: u16, write: bool) {
    reg(code, 14, (a >> 14) as u8);
    out(code, 0x99, a as u8);
    out(
        code,
        0x99,
        ((a >> 8) as u8 & 63) | if write { 64 } else { 0 },
    );
}
fn jump(code: &mut Vec<u8>, op: u8, target: u16) {
    code.push(op);
    code.extend(target.to_le_bytes());
}
fn check(code: &mut Vec<u8>, value: u8) {
    code.extend([0xfe, value]);
    jump(code, 0xc2, FAILURE);
}
fn sample(code: &mut Vec<u8>, ram: u8) {
    code.extend([0xdb, 0x98, 0x32, ram, 0xc2]);
}
fn display(code: &mut Vec<u8>, pass: bool) {
    // Original SCREEN 1 text and a solid status bar, visible through Canvas.
    for (r, value) in [
        (0, 0),
        (1, 0x40),
        (2, 6),
        (3, 0x80),
        (4, 0),
        (5, 0x36),
        (7, 0xf1),
        (8, 2),
    ] {
        reg(code, r, value);
    }
    address(code, 0x1800, true);
    // Clear 768 names to spaces with a CPU loop.
    code.extend([
        0x01, 0, 3, 0x3e, 32, 0xd3, 0x98, 0x0b, 0x78, 0xb1, 0x20, 0xf7,
    ]);
    address(code, 0x1b00, true);
    out(code, 0x98, 208);
    let text = if pass { b"CMD PASS" } else { b"CMD FAIL" };
    for ch in text {
        address(code, u16::from(*ch) * 8, true);
        for byte in font::glyph(*ch) {
            out(code, 0x98, byte);
        }
    }
    address(code, 0x400, true);
    for _ in 0..8 {
        out(code, 0x98, 255);
    }
    address(code, 0x2000, true);
    for _ in 0..32 {
        out(code, 0x98, 0xf1);
    }
    address(code, 0x2010, true);
    out(code, 0x98, if pass { 0x21 } else { 0x61 });
    address(code, 0x1800, true);
    for ch in text {
        out(code, 0x98, *ch);
    }
    address(code, 0x1840, true);
    for _ in 0..8 {
        out(code, 0x98, 128);
    }
    code.extend([
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

pub fn command_timing_cartridge() -> Vec<u8> {
    let mut rom = vec![0; 16384];
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    let mut code = vec![0xf3, 0x31, 0, 0xf0];
    out(&mut code, 0xa8, 0xf7);
    code.extend([0x3e, 0xff, 0x32, 0xff, 0xff]);
    out(&mut code, 0xfc, 3);
    for offset in 0..=0x45 {
        code.extend([0xaf, 0x32, offset, 0xc2]);
    }
    reg(&mut code, 0, 6); // SCREEN 5
    reg(&mut code, 9, 0);
    reg(&mut code, 45, 0);
    for (stage, (command, grid)) in [
        (0x80, 0),
        (0x80, 1),
        (0x80, 2),
        (0x90, 0),
        (0x90, 1),
        (0x90, 2),
    ]
    .into_iter()
    .enumerate()
    {
        code.extend([0x3e, stage as u8 + 1, 0x32, 0x11, 0xc2]);
        reg(&mut code, 1, if grid == 0 { 0 } else { 0x40 });
        reg(&mut code, 8, if grid == 2 { 0 } else { 2 });
        address(&mut code, 0x4000, true);
        for _ in 0..32 {
            out(&mut code, 0x98, 0x22);
        }
        address(&mut code, 0x2000, true);
        for _ in 0..32 {
            out(&mut code, 0x98, 0);
        }
        for (r, value) in [(32, 0), (34, 128), (36, 0), (38, 64), (40, 64), (42, 1)] {
            word(&mut code, r, value);
        }
        reg(&mut code, 44, 3);
        reg(&mut code, 15, 2);
        reg(&mut code, 46, command);
        // R46 mutates at the beginning of its final OUT's device interval.
        // OUT = 12 T, then 130 NOPs = 650 T with MSX fetch waits. The IN
        // therefore samples CE at +662 T. Baseline finishes 64 pixels at
        // +512 T. Even the fastest corrected grid needs >1000 T.
        code.extend([0; 130]);
        code.extend([0xdb, 0x99, 0xe6, 1, 0x32, 0x20 + stage as u8, 0xc2]);
        check(&mut code, 1);
        // Read prefetch is part of address setup, then IN consumes the buffer.
        // At this bounded elapsed interval the first byte is complete and the
        // last is untouched for every start phase in each selected grid.
        address(&mut code, 0x2000, false);
        sample(&mut code, 0x40 + stage as u8);
        check(&mut code, if command == 0x80 { 0x33 } else { 0x22 });
        address(&mut code, 0x201f, false);
        sample(&mut code, 0x30 + stage as u8);
        check(&mut code, 0);
        // Bounded completion poll; no hang even if CE never clears.
        code.extend([0x21, 0, 0x40]);
        let poll = ORIGIN + code.len() as u16;
        code.extend([0xdb, 0x99, 0xe6, 1, 0x28, 9, 0x2b, 0x7c, 0xb5]);
        jump(&mut code, 0xc2, poll);
        jump(&mut code, 0xc3, FAILURE);
        address(&mut code, 0x2000, false);
        for _ in 0..32 {
            code.extend([0xdb, 0x98]);
            check(&mut code, if command == 0x80 { 0x33 } else { 0x22 });
        }
        code.extend([0x3e, 0xa5, 0x32, stage as u8, 0xc2]);
    }
    display(&mut code, true);
    assert!(code.len() < usize::from(FAILURE - ORIGIN));
    rom[0x10..0x10 + code.len()].copy_from_slice(&code);
    code.clear();
    display(&mut code, false);
    rom[0x3000..0x3000 + code.len()].copy_from_slice(&code);
    rom
}
