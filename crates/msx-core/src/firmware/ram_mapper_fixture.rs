//! Original MIT-licensed Z80 mapper/capacity probe; no external program bytes.
use crate::font;

pub fn ram_mapper_cartridge() -> Vec<u8> {
    fn out(code: &mut Vec<u8>, port: u8, value: u8) {
        code.extend_from_slice(&[0x3e, value, 0xd3, port]);
    }
    fn address(code: &mut Vec<u8>, address: u16) {
        out(code, 0x99, address as u8);
        out(code, 0x99, 0x40 | (address >> 8) as u8);
    }
    fn result(code: &mut Vec<u8>, pass: bool) {
        // The stack remains in segment zero; no BIOS calls or host services.
        out(code, 0xfc, 3);
        for (r, v) in [0, 0x40, 6, 0x80, 0, 0x36, 7, 0xf1].into_iter().enumerate() {
            out(code, 0x99, v);
            out(code, 0x99, 0x80 | r as u8);
        }
        // Clear startup text and hide sprites before drawing the result.
        address(code, 0x1800);
        code.extend_from_slice(&[
            0x01, 0, 3, // LD BC,768
            0x3e, 32, 0xd3, 0x98, 0x0b, 0x78, 0xb1, 0x20, 0xf7,
        ]);
        address(code, 0x1b00);
        out(code, 0x98, 208);
        let text = if pass {
            b"RAM MAPPER PASS"
        } else {
            b"RAM MAPPER FAIL"
        };
        // Original glyphs at the ASCII pattern indices and a solid result tile.
        for ch in text {
            address(code, u16::from(*ch) * 8);
            for byte in font::glyph(*ch) {
                out(code, 0x98, byte);
            }
        }
        address(code, 0x0400);
        for _ in 0..8 {
            out(code, 0x98, 0xff);
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
        // Publish completion only after the visible result has been drawn.
        code.extend_from_slice(&[0x3e, if pass { 0xa5 } else { 0xee }, 0x32, 0x10, 0xc2]);
        code.extend_from_slice(&[0x76, 0x18, 0xfd]); // HALT forever, interrupts disabled
    }
    let mut rom = vec![0; 16384];
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    let mut code = vec![0xf3];
    // Firmware must initialize the mapper itself before invoking the cartridge.
    let fail = [0xc2, 0, 0x50]; // JP NZ,5000h
    for (port, segment) in [(0xfc, 3), (0xfd, 2), (0xfe, 1), (0xff, 0)] {
        code.extend_from_slice(&[0xdb, port, 0xfe, 0xf0 | segment]);
        code.extend_from_slice(&fail);
    }
    out(&mut code, 0xa8, 0xf7); // expanded RAM, cartridge, expanded RAM, expanded RAM
    code.extend_from_slice(&[0x3e, 0xff, 0x32, 0xff, 0xff]);
    // Write all segments BEFORE reading any: a smaller aliased capacity fails.
    // Both ends of the page are tested, including segments above 64 KiB.
    for segment in 0..16 {
        out(&mut code, 0xfc, segment);
        for (lo, hi, value) in [(0, 0, 0x40 + segment), (0xff, 0x3f, 0x90 + segment)] {
            code.extend_from_slice(&[0x3e, value, 0x32, lo, hi]);
        }
    }
    for segment in 0..16 {
        out(&mut code, 0xfc, segment);
        for (lo, hi, value) in [(0, 0, 0x40 + segment), (0xff, 0x3f, 0x90 + segment)] {
            code.extend_from_slice(&[0x3a, lo, hi, 0xfe, value]);
            code.extend_from_slice(&fail);
        }
    }
    result(&mut code, true);
    assert!(code.len() < 0xff0);
    rom[0x10..0x10 + code.len()].copy_from_slice(&code);
    code.clear();
    result(&mut code, false);
    rom[0x1000..0x1000 + code.len()].copy_from_slice(&code);
    rom
}
