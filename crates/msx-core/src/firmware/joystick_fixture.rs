//! Original MIT-licensed Z80 joystick diagnostic. Runs under unmodified C-BIOS;
//! every CALL below executes the firmware, with no host service ports or traps.
use crate::font;

/// C200h..C205h: direct R14 reads for R15=00,40,10,60,20,50h.
/// C206h..C207h: GTSTCK(1,2); C208h..C20Bh: GTTRIG(1,2,3,4).
/// C20Ch..C20Eh: SNSMAT rows 3,4,5. C210h=A5h after a complete display update.
/// SCREEN 1 rows 4..18 show these bytes, bit 0 first, at columns 20..27;
/// white means one, black means zero. Labels and glyphs are original.
pub fn joystick_test_cartridge() -> Vec<u8> {
    fn out(code: &mut Vec<u8>, port: u8, value: u8) {
        code.extend_from_slice(&[0x3e, value, 0xd3, port]);
    }
    fn vram(code: &mut Vec<u8>, address: u16, value: u8) {
        code.extend_from_slice(&[0x21, address as u8, (address >> 8) as u8]);
        code.extend_from_slice(&[0x3e, value, 0xcd, 0x4d, 0]); // WRTVRM
    }
    let mut rom = vec![0; 16384];
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    let mut code = vec![0xf3, 0x31, 0, 0xf0]; // DI; LD SP,F000h (BIOS-discovered RAM)
    code.extend_from_slice(&[0x3e, 1, 0xcd, 0x5f, 0, 0x3e, 12, 0xcd, 0xa2, 0]);
    let labels = [
        "PSG 00",
        "PSG 40",
        "PSG 10",
        "PSG 60",
        "PSG 20",
        "PSG 50",
        "GTSTCK 1",
        "GTSTCK 2",
        "GTTRIG 1 A",
        "GTTRIG 2 A",
        "GTTRIG 1 B",
        "GTTRIG 2 B",
        "MATRIX 3",
        "MATRIX 4",
        "MATRIX 5",
    ];
    for ch in 32..=90u8 {
        for (y, bits) in font::glyph(ch).into_iter().enumerate() {
            vram(&mut code, u16::from(ch) * 8 + y as u16, bits);
        }
    }
    for ch in [128u8, 136] {
        for y in 0..8 {
            vram(
                &mut code,
                u16::from(ch) * 8 + y,
                if ch == 136 { 255 } else { 0 },
            );
        }
        vram(&mut code, 0x2000 + u16::from(ch) / 8, 0xf1);
    }
    for (row, label) in std::iter::once((0, "TWO JOYSTICKS / BIT 0 FIRST")).chain(
        labels
            .into_iter()
            .enumerate()
            .map(|(i, label)| (i + 4, label)),
    ) {
        for (x, ch) in label.bytes().enumerate() {
            vram(&mut code, 0x1800 + (row * 32 + x) as u16, ch);
        }
    }
    let start = 0x4010 + code.len() as u16;
    // Direct hardware samples explicitly drive each connector's common line.
    for (offset, control) in [0, 0x40, 0x10, 0x60, 0x20, 0x50].into_iter().enumerate() {
        out(&mut code, 0xa0, 15);
        out(&mut code, 0xa1, control);
        out(&mut code, 0xa0, 14);
        code.extend_from_slice(&[0xdb, 0xa2, 0x32, offset as u8, 0xc2]);
    }
    // Standard joysticks need both common outputs low for BIOS polling.
    out(&mut code, 0xa0, 15);
    out(&mut code, 0xa1, 0);
    // GTSTCK=00D5h, GTTRIG=00D8h, SNSMAT=0141h, actual standard BIOS calls.
    for (offset, argument, address) in [
        (6, 1, 0x00d5u16),
        (7, 2, 0x00d5),
        (8, 1, 0x00d8),
        (9, 2, 0x00d8),
        (10, 3, 0x00d8),
        (11, 4, 0x00d8),
        (12, 3, 0x0141),
        (13, 4, 0x0141),
        (14, 5, 0x0141),
    ] {
        code.extend_from_slice(&[0x3e, argument, 0xcd, address as u8, (address >> 8) as u8]);
        code.extend_from_slice(&[0x32, offset, 0xc2]);
    }
    for row in 0..15u8 {
        code.extend_from_slice(&[0x3a, row, 0xc2, 0x47]); // LD A,(result); LD B,A
        for bit in 0..8 {
            // Rotate the next bit into carry, select an original blank/solid tile.
            code.extend_from_slice(&[0x78, 0x0f, 0x47, 0x3e, 128, 0x30, 2, 0x3e, 136]);
            let address = 0x1800u16 + (u16::from(row) + 4) * 32 + 20 + bit;
            code.extend_from_slice(&[0x21, address as u8, (address >> 8) as u8]);
            code.extend_from_slice(&[0xc5, 0xcd, 0x4d, 0, 0xc1]); // preserve bits over WRTVRM
        }
    }
    code.extend_from_slice(&[0x3e, 0xa5, 0x32, 0x10, 0xc2]);
    code.extend_from_slice(&[0xc3, start as u8, (start >> 8) as u8]);
    assert!(code.len() < rom.len() - 0x10);
    rom[0x10..0x10 + code.len()].copy_from_slice(&code);
    rom
}
