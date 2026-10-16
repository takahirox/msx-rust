//! Original firmware and cartridge construction; all emitted bytes are MIT licensed.
//! See firmware/boot.asm for the exact boot routine, including the native BASIC ABI.
use crate::font;
pub fn bios() -> Vec<u8> {
    let mut rom = vec![0xff; 32768];
    rom[..3].copy_from_slice(&[0xc3, 0x00, 0x01]); // JP 0100h
    rom[0x38..0x3a].copy_from_slice(&[0xed, 0x4d]);
    rom[0x66..0x68].copy_from_slice(&[0xed, 0x45]);
    let boot = [
        0xf3, 0x31, 0x80, 0xf3, 0xaf, 0xd3, 0xf1, // DI; LD SP,F380h; native BASIC boot
        0x3a, 0x00, 0x40, 0xfe, 0x41, 0x20, 0x13, // check AB at 4000h; else next
        0x3a, 0x01, 0x40, 0xfe, 0x42, 0x20, 0x0c, 0x2a, 0x02, 0x40, 0x7c, 0xb5, 0x28,
        0x1f, // init zero: poll
        0x11, 0x3b, 0x01, 0xd5, 0xe9, // LD DE,poll; PUSH DE; JP(HL); RET resumes BASIC
        0x3a, 0x00, 0x80, 0xfe, 0x41, 0x20, 0x13, // check AB at 8000h; else poll
        0x3a, 0x01, 0x80, 0xfe, 0x42, 0x20, 0x0c, 0x2a, 0x02, 0x80, 0x7c, 0xb5, 0x28, 0x05, 0x11,
        0x3b, 0x01, 0xd5, 0xe9, 0xdb, 0xf0, 0xb7, 0x28, 0xfb, 0xd3, 0xf1, 0x18,
        0xf7, // poll at 013Bh
    ];
    rom[0x100..0x100 + boot.len()].copy_from_slice(&boot);
    rom
}
/// A self-contained plain 16 KiB cartridge. Only standard VDP/PPI/PSG ports are used.
/// Shows text and a sprite; holds a 440 Hz tone while Space is pressed.
pub fn test_cartridge() -> Vec<u8> {
    let mut rom = vec![0; 16384];
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    let mut code = Vec::new();
    fn out(code: &mut Vec<u8>, port: u8, value: u8) {
        code.extend_from_slice(&[0x3e, value, 0xd3, port]);
    }
    fn address(code: &mut Vec<u8>, a: u16) {
        out(code, 0x99, a as u8);
        out(code, 0x99, ((a >> 8) as u8) | 0x40);
    }
    fn transfer(code: &mut Vec<u8>, source: u16, count: u16) {
        code.extend_from_slice(&[
            0x21,
            source as u8,
            (source >> 8) as u8,
            0x01,
            0x98,
            0x00,
            0x11,
            count as u8,
            (count >> 8) as u8,
        ]);
        let start = code.len();
        code.extend_from_slice(&[0x7e, 0xed, 0x79, 0x23, 0x1b, 0x7a, 0xb3, 0x20, 0]);
        let offset = start as isize - code.len() as isize;
        *code.last_mut().unwrap() = offset as i8 as u8;
    }
    code.extend_from_slice(&[0xf3, 0x31, 0x00, 0xf0]);
    for (i, r) in [0, 0x40, 6, 0x80, 0, 0x36, 7, 0xf4].iter().enumerate() {
        out(&mut code, 0x99, *r);
        out(&mut code, 0x99, 0x80 | i as u8);
    }
    address(&mut code, 0);
    transfer(&mut code, 0x4800, 2048);
    address(&mut code, 0x1800);
    transfer(&mut code, 0x5000, 768);
    address(&mut code, 0x2000);
    for _ in 0..32 {
        out(&mut code, 0x98, 0xf4);
    }
    address(&mut code, 0x3800);
    for row in [0x18, 0x3c, 0x7e, 0xff, 0xff, 0x7e, 0x3c, 0x18] {
        out(&mut code, 0x98, row);
    }
    address(&mut code, 0x1b00);
    for b in [95, 124, 0, 10, 208] {
        out(&mut code, 0x98, b);
    }
    for (r, v) in [(0, 254), (1, 0), (7, 0x3e), (8, 0)] {
        out(&mut code, 0xa0, r);
        out(&mut code, 0xa1, v);
    }
    let poll = 0x4010 + code.len() as u16;
    out(&mut code, 0xaa, 8);
    code.extend_from_slice(&[0xdb, 0xa9, 0xe6, 1, 0x3e, 0, 0x20, 2, 0x3e, 15, 0x47]);
    out(&mut code, 0xa0, 8);
    code.extend_from_slice(&[0x78, 0xd3, 0xa1, 0xc3, poll as u8, (poll >> 8) as u8]);
    assert!(code.len() < 0x7f0);
    rom[0x10..0x10 + code.len()].copy_from_slice(&code);
    for c in 0..256 {
        rom[0x800 + c * 8..0x800 + c * 8 + 8].copy_from_slice(&font::glyph(c as u8));
    }
    rom[0x1000..0x1300].fill(32);
    for (row, text) in [
        (4, "MSX-RUST TEST CARTRIDGE"),
        (7, "Z80 - VRAM - SPRITE - PPI - PSG"),
        (17, "HOLD SPACE FOR A 440 HZ TONE"),
    ] {
        let offset = 0x1000 + row * 32 + (32 - text.len()) / 2;
        rom[offset..offset + text.len()].copy_from_slice(text.as_bytes());
    }
    rom
}
