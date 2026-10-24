//! Original MIT-licensed cassette diagnostic, authored from the wire protocol.
//! No BIOS tape calls, host traps, decoded-byte injection or proprietary assets.
use std::collections::BTreeMap;

// Binary body: start/end/entry addresses, executable VDP result, terminal vectors.
// The loader stores the entire block at C210h and jumps to recovered code C216h.
const BODY: &[u8] = &[
    0x16, 0xc2, 0x31, 0xc2, 0x16, 0xc2, 0x3e, 0x22, 0xd3, 0x99, 0x3e, 0x87, 0xd3, 0x99, 0x3e, 0xa5,
    0x32, 0xff, 0xc1, 0x3e, 9, 0xd3, 0xab, 0x76, 0x18, 0xfd, 0x00, 0xff, 0x55, 0xaa, 0x80, 0x81,
    0xfe, 0x7f,
];
pub fn cassette_test_image() -> Vec<u8> {
    // Kept independent of the playback parser/encoder.
    let marker = [0x1f, 0xa6, 0xde, 0xba, 0xcc, 0x13, 0x7d, 0x74];
    let mut bytes = marker.to_vec();
    bytes.extend_from_slice(&[0xd0; 10]);
    bytes.extend_from_slice(b"TAPE  ");
    bytes.extend_from_slice(&marker);
    bytes.extend_from_slice(BODY);
    bytes
}
struct Asm {
    code: Vec<u8>,
    labels: BTreeMap<&'static str, u16>,
    fixups: Vec<(usize, &'static str)>,
}
impl Asm {
    fn emit(&mut self, bytes: &[u8]) {
        self.code.extend_from_slice(bytes);
    }
    fn label(&mut self, label: &'static str) {
        assert!(self
            .labels
            .insert(label, 0x4010 + self.code.len() as u16)
            .is_none());
    }
    fn branch(&mut self, opcode: u8, label: &'static str) {
        self.emit(&[opcode, 0, 0]);
        self.fixups.push((self.code.len() - 2, label));
    }
    fn out(&mut self, port: u8, value: u8) {
        self.emit(&[0x3e, value, 0xd3, port]);
    }
    fn vram(&mut self, address: u16) {
        self.out(0x99, address as u8);
        self.out(0x99, (address >> 8) as u8 | 0x40);
    }
}
/// Runs unchanged on both models and both rates. C180/C182: measured leader cycle
/// counts; C184: high pulse width; C185: idle input during motor stop; C1FF: A5 on
/// executed payload. Header/body C200..C231 are also rendered as LSB-first tiles,
/// four bytes per screen row, white=1/green=0 after success. Green backdrop comes from tape code.
pub fn cassette_test_cartridge() -> Vec<u8> {
    let mut a = Asm {
        code: Vec::new(),
        labels: BTreeMap::new(),
        fixups: Vec::new(),
    };
    a.emit(&[0xf3, 0x31, 0, 0xf0]); // DI; SP in BIOS-discovered RAM
    a.out(0xaa, 0x15); // motor off, keyboard row 5; independent of R15 joystick bits
    for (r, v) in [
        (0, 0),
        (1, 0x40),
        (2, 6),
        (3, 0x80),
        (4, 0),
        (5, 0x36),
        (6, 7),
        (7, 0x11),
    ] {
        a.out(0x99, v);
        a.out(0x99, 0x80 | r);
    }
    a.vram(0);
    for _ in 0..8 {
        a.out(0x98, 0);
    }
    for _ in 0..8 {
        a.out(0x98, 255);
    }
    a.vram(0x2000);
    a.out(0x98, 0xf0);
    a.vram(0x1800); // clear name table
    a.emit(&[0x01, 0, 3]); // BC=768
    a.label("clear");
    a.emit(&[0xaf, 0xd3, 0x98, 0x0b, 0x78, 0xb1]);
    a.branch(0xc2, "clear");
    a.out(0xa0, 14);
    a.out(0xab, 8); // BSR C4 reset: motor on

    // Calibrate a threshold from one full high pulse, then measure the leaders.
    a.branch(0xcd, "pulse");
    a.emit(&[0x32, 0x84, 0xc1, 0x4f, 0xcb, 0x3f, 0x81, 0x4f]);
    a.emit(&[0x11, 1, 0]);
    a.branch(0xcd, "leader");
    a.emit(&[0xed, 0x53, 0x80, 0xc1, 0x21, 0, 0xc2, 0x3e, 16]);
    a.branch(0xcd, "block");
    // The motor is stopped in the low half of a stop pulse, not a block gap.
    a.out(0xaa, 0x15);
    a.emit(&[0x11, 0, 0x10]);
    a.label("pause");
    a.emit(&[0x1b, 0x7a, 0xb3]);
    a.branch(0xc2, "pause");
    a.emit(&[0xdb, 0xa2, 0xe6, 0x80, 0x32, 0x85, 0xc1]);
    a.branch(0xc2, "error");
    a.out(0xab, 8);
    a.emit(&[0x11, 0, 0]);
    a.branch(0xcd, "leader");
    a.emit(&[0xed, 0x53, 0x82, 0xc1, 0x3e, BODY.len() as u8]);
    a.branch(0xcd, "block");
    // Display all recovered bytes, including binary addresses and terminal bytes.
    a.vram(0x1800);
    a.emit(&[0x21, 0, 0xc2, 0x16, 16 + BODY.len() as u8]);
    a.label("display");
    a.emit(&[0x5e, 0x23, 0x06, 8]);
    a.label("pixel");
    a.emit(&[0x7b, 0xe6, 1, 0xd3, 0x98, 0xcb, 0x3b, 0x10, 0xf7, 0x15]);
    a.branch(0xc2, "display");
    a.emit(&[0xc3, 0x16, 0xc2]); // execute recovered payload; it alone turns green
    a.label("error");
    a.out(0x99, 0x66);
    a.out(0x99, 0x87);
    a.emit(&[0x76]);
    a.branch(0xc3, "error");
    // Wait for the next rising edge, count high width in ~35 CPU cycles/iteration.
    // All waits are bounded externally by diagnostic tests; absent tape stays dark.
    a.label("pulse");
    a.emit(&[
        0xdb, 0xa2, 0xe6, 0x80, 0x20, 0xfa, // wait low
        0x06, 0, 0xdb, 0xa2, 0xe6, 0x80, 0x28, 0xfa, // wait high
        0x04, 0xdb, 0xa2, 0xe6, 0x80, 0x20, 0xf9, 0x78, 0xc9,
    ]);
    a.label("leader");
    a.branch(0xcd, "pulse");
    a.emit(&[0xb9, 0xd0, 0x13]); // CP C; RET NC (zero start); INC DE
    a.branch(0xc3, "leader");
    a.label("bit");
    a.branch(0xcd, "pulse");
    a.emit(&[0xb9]);
    a.branch(0xda, "one");
    a.emit(&[0xb7, 0xc9]); // zero: clear carry
    a.label("one");
    a.branch(0xcd, "pulse");
    a.emit(&[0x37, 0xc9]);
    a.label("block");
    a.emit(&[0x32, 0x90, 0xc1]);
    a.label("byte");
    a.emit(&[0x1e, 0, 0x16, 8]);
    a.label("data");
    a.branch(0xcd, "bit");
    a.emit(&[0xcb, 0x1b, 0x15]);
    a.branch(0xc2, "data");
    a.emit(&[0x73, 0x23]); // LD (HL),E; INC HL
    for _ in 0..2 {
        a.branch(0xcd, "bit");
        a.branch(0xd2, "error");
    }
    a.emit(&[0x3a, 0x90, 0xc1, 0x3d, 0x32, 0x90, 0xc1, 0xc8]); // return after last byte
    a.branch(0xcd, "bit");
    a.branch(0xda, "error");
    a.branch(0xc3, "byte");
    for (offset, label) in a.fixups {
        a.code[offset..offset + 2].copy_from_slice(&a.labels[label].to_le_bytes());
    }
    let mut rom = vec![0; 16384];
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    rom[0x10..0x10 + a.code.len()].copy_from_slice(&a.code);
    rom
}
