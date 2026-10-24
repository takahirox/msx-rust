//! Original firmware/cartridge generators (MIT) and separately licensed C-BIOS binaries.
//! See firmware/boot.asm for the exact boot routine, including the native BASIC ABI.
use crate::font;
mod cassette_fixture;
pub use cassette_fixture::{cassette_test_cartridge, cassette_test_image};
mod disk;
pub(crate) use disk::write_disk_boot;
pub use disk::{demo_disk, disk_rom};
mod music_fixture;
pub use music_fixture::{music_diagnostic_firmware, music_test_cartridge};
mod fh_fixture;
pub use fh_fixture::fh_test_cartridge;
mod joystick_fixture;
pub use joystick_fixture::joystick_test_cartridge;
mod ram_mapper_fixture;
pub use ram_mapper_fixture::ram_mapper_cartridge;
mod scc_fixture;
mod sram_fixture;
pub use scc_fixture::scc_test_cartridge;
pub use sram_fixture::mapper_test_cartridge;
pub fn bios() -> Vec<u8> {
    let mut rom = vec![0xff; 32768];
    write_bios(&mut rom, false);
    rom
}
pub(crate) fn write_bios(rom: &mut [u8], msx2: bool) {
    rom.fill(0xff);
    rom[..3].copy_from_slice(&[0xc3, 0x00, 0x01]); // JP 0100h
    rom[0x2d] = u8::from(msx2);
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
    if msx2 {
        mapper_boot(rom);
    }
}
/// Original reset trampoline: all mapper setup runs through Z80 OUT instructions.
/// No stack or separate RAM pages are used before initialization completes.
fn mapper_boot(rom: &mut [u8]) {
    let entry = [rom[0], rom[1], rom[2]];
    let init = [
        0xf3, // DI
        0x3e, 3, 0xd3, 0xfc, 0x3e, 2, 0xd3, 0xfd, 0x3e, 1, 0xd3, 0xfe, 0x3e, 0, 0xd3, 0xff,
    ];
    rom[0x80..0x80 + init.len()].copy_from_slice(&init);
    rom[0x80 + init.len()..0x83 + init.len()].copy_from_slice(&entry);
    rom[..3].copy_from_slice(&[0xc3, 0x80, 0]);
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

/// Original minimal MSX2 cartridge loader. This is not MSX BIOS or BASIC.
/// Only self-contained AB cartridges are supported; external firmware executes
/// its own slot discovery and BIOS entry points without Rust interception.
pub fn msx2_bios() -> Vec<u8> {
    let mut rom = vec![0xff; 32768];
    rom[..3].copy_from_slice(&[0xc3, 0x00, 0x01]);
    rom[6] = 0x98;
    rom[7] = 0x98;
    rom[0x2d] = 1;
    let mut boot = vec![
        0xf3, 0x3e, 0xf0, 0xd3, 0xa8, // DI; map page 3 to expanded slot 3
        0x3e, 0xff, 0x32, 0xff, 0xff, // select 3-3 RAM on all pages
        0x3e, 0xd4, 0xd3, 0xa8, 0x31, 0x00, 0xf0, // BIOS, cartridge, cartridge, RAM
    ];
    for page in [0x40, 0x80] {
        boot.extend_from_slice(&[
            0x3a, 0x00, page, 0xfe, 0x41, 0x20, 0x0f, 0x3a, 0x01, page, 0xfe, 0x42, 0x20, 0x08,
            0x2a, 0x02, page, 0x7c, 0xb5, 0x28, 0x01, 0xe9,
        ]);
    }
    boot.extend_from_slice(&[
        0x3e, 4, 0xd3, 0x99, 0x3e, 0x87, 0xd3, 0x99, 0x76, 0x18, 0xfd,
    ]);
    rom[0x100..0x100 + boot.len()].copy_from_slice(&boot);
    mapper_boot(&mut rom);
    rom
}
/// An original diagnostic sub-ROM, not an implementation of the MSX2 BIOS API.
/// A Z80 inter-slot call to 0100h returns 2Ah and writes C100h.
pub fn msx2_subrom() -> Vec<u8> {
    let mut rom = vec![0xff; 16384];
    rom[..2].copy_from_slice(b"CD");
    rom[0x3ffe..].copy_from_slice(b"WV");
    rom[0x100..0x106].copy_from_slice(&[0x3e, 0x2a, 0x32, 0x00, 0xc1, 0xc9]);
    rom
}

/// Independent MIT-licensed V9938 diagnostic cartridge. Keys 7/8 select SCREEN
/// 7/8; Space controls a PSG tone. Every hardware operation is executed by Z80.
pub fn msx2_cartridge() -> Vec<u8> {
    struct Code {
        bytes: Vec<u8>,
        labels: Vec<(String, u16)>,
        fixups: Vec<(usize, String)>,
    }
    impl Code {
        fn emit(&mut self, b: &[u8]) {
            self.bytes.extend_from_slice(b);
        }
        fn out(&mut self, port: u8, value: u8) {
            self.emit(&[0x3e, value, 0xd3, port]);
        }
        fn reg(&mut self, r: u8, value: u8) {
            self.out(0x99, value);
            self.out(0x99, 0x80 | r);
        }
        fn word(&mut self, r: u8, v: u16) {
            self.reg(r, v as u8);
            self.reg(r + 1, (v >> 8) as u8);
        }
        fn label(&mut self, name: &str) {
            self.labels
                .push((name.into(), 0x4010 + self.bytes.len() as u16));
        }
        fn jump(&mut self, opcode: u8, name: &str) {
            self.emit(&[opcode, 0, 0]);
            self.fixups.push((self.bytes.len() - 2, name.into()));
        }
        fn address(&mut self, a: usize) {
            self.reg(14, (a >> 14) as u8);
            self.out(0x99, a as u8);
            self.out(0x99, 0x40 | ((a >> 8) & 63) as u8);
        }
        fn wait(&mut self) {
            self.jump(0xcd, "wait");
        }
    }
    let mut c = Code {
        bytes: Vec::new(),
        labels: Vec::new(),
        fixups: Vec::new(),
    };
    c.emit(&[0xf3, 0x31, 0x00, 0xf0]);
    // Code remains in slot 1. Select expanded slot 3-3 RAM, then select its
    // sub-ROM on page zero, call real Z80 code and restore the main ROM.
    c.out(0xa8, 0xd4);
    c.emit(&[0x3e, 0xff, 0x32, 0xff, 0xff]);
    c.emit(&[0x3e, 0xfc, 0x32, 0xff, 0xff]);
    c.out(0xa8, 0xd7);
    c.emit(&[0x3a, 0xfe, 0x3f, 0xfe, b'W']);
    c.jump(0xc2, "restore-slots");
    c.emit(&[0x3a, 0xff, 0x3f, 0xfe, b'V']);
    c.jump(0xc2, "restore-slots");
    c.emit(&[0xcd, 0x00, 0x01, 0xfe, 0x2a]);
    c.jump(0xc2, "failure");
    c.label("restore-slots");
    c.out(0xa8, 0xd4);
    c.emit(&[0x3e, 0xff, 0x32, 0xff, 0xff]);
    // RTC battery RAM bank 2, nibble round trip.
    c.out(0xb4, 13);
    c.out(0xb5, 10);
    c.out(0xb4, 0);
    c.out(0xb5, 5);
    c.emit(&[0xdb, 0xb5, 0xe6, 15, 0xfe, 5]);
    c.jump(0xc2, "failure");
    c.out(0xb4, 13);
    c.out(0xb5, 8);
    for (r, v) in [(0, 254), (1, 0), (7, 0x3e), (8, 0)] {
        c.out(0xa0, r);
        c.out(0xa1, v);
    }
    c.jump(0xcd, "screen7");
    c.label("poll");
    c.out(0xaa, 1);
    c.emit(&[0xdb, 0xa9, 0xe6, 1]);
    c.jump(0xcc, "screen8");
    c.out(0xaa, 0);
    c.emit(&[0xdb, 0xa9, 0xe6, 0x80]);
    c.jump(0xcc, "screen7");
    c.out(0xaa, 8);
    c.emit(&[0xdb, 0xa9, 0xe6, 1, 0x3e, 0, 0x20, 2, 0x3e, 15, 0x47]);
    c.out(0xa0, 8);
    c.emit(&[0x78, 0xd3, 0xa1]);
    c.jump(0xc3, "poll");
    for (label, mode, fill, line, width) in [
        ("screen7", 10, 0x22, 3, 512),
        ("screen8", 14, 0x1c, 0xe0, 256),
    ] {
        c.label(label);
        c.reg(1, 0);
        c.reg(0, mode);
        c.reg(9, 0x80);
        c.reg(2, 0x3f);
        c.reg(7, 1);
        c.reg(8, 0);
        c.reg(16, 2);
        c.out(0x9a, 0x70);
        c.out(0x9a, 0); // red
        c.out(0x9a, 0x07);
        c.out(0x9a, 7); // cyan
                        // Extended VRAM page 1, 512x212 or 256x212 byte fill.
        c.word(36, 0);
        c.word(38, 256);
        c.word(40, width);
        c.word(42, 212);
        c.reg(44, fill);
        c.reg(45, 0);
        c.reg(46, 0xc0);
        c.wait();
        // Logical line and rectangle copy, all in the high 64 KiB.
        c.word(36, 40);
        c.word(38, 300);
        c.word(40, 150);
        c.word(42, 60);
        c.reg(44, line);
        c.reg(46, 0x70);
        c.wait();
        c.word(32, 40);
        c.word(34, 300);
        c.word(36, 40);
        c.word(38, 400);
        c.word(40, 150);
        c.word(42, 10);
        c.reg(46, 0x90);
        c.wait();
        // CPU-to-VDP logical transfer with first pixel preloaded in R44.
        c.word(36, 10);
        c.word(38, 266);
        c.word(40, 1);
        c.word(42, 1);
        c.reg(44, line);
        c.reg(46, 0xb0);
        c.wait();
        // A 256-byte-wide color ramp in extended VRAM. The bytes reside in
        // our cartridge and are transferred by Z80 OUT instructions.
        c.address(0x1a000);
        let source: u16 = if mode == 10 { 0x6000 } else { 0x7000 };
        c.emit(&[0x21, source as u8, (source >> 8) as u8, 0x11, 0x00, 0x10]);
        c.label(&format!("ramp-{label}"));
        c.emit(&[0x7e, 0xd3, 0x98, 0x23, 0x1b, 0x7a, 0xb3]);
        c.jump(0xc2, &format!("ramp-{label}"));
        // Sprite mode 2 tables away from the displayed bitmap.
        c.reg(5, 0x77);
        c.reg(11, 0);
        c.reg(6, 15);
        c.address(0x3a00);
        for b in [31, 32, 0, 0, 216] {
            c.out(0x98, b);
        }
        c.address(0x3800);
        for _ in 0..16 {
            c.out(0x98, 15);
        }
        c.address(0x7800);
        for b in [0x18, 0x3c, 0x7e, 0xff, 0xff, 0x7e, 0x3c, 0x18] {
            c.out(0x98, b);
        }
        // Independently authored cross-mode probe: physical C100h/1C100h
        // become logical 18200h/18201h (display page 1, row 130).
        // A flat implementation instead leaves the normal fill at these pixels.
        c.reg(0, 6); // Linear SCREEN 5; do not clear VRAM.
        for (physical, value) in [
            (0xc100, if mode == 10 { 0x23 } else { 0x1c }),
            (0x1c100, if mode == 10 { 0x32 } else { 0xe0 }),
        ] {
            c.address(physical);
            c.out(0x98, value);
        }
        c.reg(0, mode);
        // Read the first logical byte through the buffered CPU data port too.
        c.reg(14, 6);
        c.out(0x99, 0);
        c.out(0x99, 2); // Read setup at 18200h, including prefetch.
        c.emit(&[0xdb, 0x98, 0xfe, if mode == 10 { 0x23 } else { 0x1c }]);
        c.jump(0xc2, "failure");
        // Original reverse-top probe. Row 1023 aliases physical row 511 in
        // these modes. A reverse fill from row zero must leave its sentinel
        // alone; copy it into the displayed page so Canvas can observe it.
        c.word(36, 20);
        c.word(38, 511);
        c.word(40, 4);
        c.word(42, 1);
        c.reg(44, if mode == 10 { 15 } else { 255 });
        c.reg(45, 0);
        c.reg(46, 0x80);
        c.wait();
        c.word(38, 0);
        c.word(42, 3);
        c.reg(44, line);
        c.reg(45, 8);
        c.reg(46, 0x80);
        c.wait();
        c.word(32, 20);
        c.word(34, 511);
        c.word(38, 376);
        c.word(42, 1);
        c.reg(45, 0);
        c.reg(46, 0x90);
        c.wait();
        // Y-major LINE draws rows 376..379 and leaves DY=380. The fill
        // deliberately reuses DY without writing it: row 124 must be white.
        c.word(36, 30);
        c.word(38, 376);
        c.word(40, 3);
        c.word(42, 0);
        c.reg(44, line);
        c.reg(45, 1);
        c.reg(46, 0x70);
        c.wait();
        c.word(40, 4);
        c.word(42, 1);
        c.reg(44, if mode == 10 { 15 } else { 255 });
        c.reg(45, 0);
        c.reg(46, 0x80);
        c.wait();
        // Original live-color probe: 128-pixel horizontal LINE at displayed
        // row 140. Including the MSX M1 wait, OUT takes 12 T-states and LD A,n
        // takes 8. R44's direct write arrives at t=40, then its indirect
        // write at t=60: pixels 64..68 white, 69..70 ink, 71..191 white.
        // Both writes happen while CE is set and must preserve LINE progress.
        c.word(36, 64);
        c.word(38, 396);
        c.word(40, 127);
        c.word(42, 0);
        c.reg(17, 0x80 | 44);
        c.reg(44, if mode == 10 { 15 } else { 255 });
        c.reg(46, 0x70);
        c.reg(44, line);
        c.out(0x9b, if mode == 10 { 15 } else { 255 });
        c.wait();
        c.reg(1, 0x40);
        c.emit(&[0xc9]);
    }
    c.label("wait");
    c.reg(15, 2);
    c.label("busy");
    c.emit(&[0xdb, 0x99, 0xe6, 1]);
    c.jump(0xc2, "busy");
    c.reg(15, 0);
    c.emit(&[0xc9]);
    c.label("failure");
    c.reg(7, 8);
    c.emit(&[0x76]);
    c.jump(0xc3, "failure");
    for (p, label) in &c.fixups {
        let a = c.labels.iter().find(|(s, _)| s == label).unwrap().1;
        c.bytes[*p..*p + 2].copy_from_slice(&a.to_le_bytes());
    }
    let mut rom = vec![0; 16384];
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    for y in 0..16 {
        for x in 0..256 {
            let color = (x / 16) as u8;
            rom[0x2000 + y * 256 + x] = color * 17;
            rom[0x3000 + y * 256 + x] = x as u8;
        }
    }
    assert!(c.bytes.len() < 0x1ff0);
    rom[0x10..0x10 + c.bytes.len()].copy_from_slice(&c.bytes);
    rom
}
/// Unmodified international/60 Hz C-BIOS 0.29a binaries. BSD-2-Clause;
/// provenance and complete notices are in firmware/cbios/, separate from MIT.
pub const CBIOS_MSX1_MAIN: &[u8; 32768] =
    include_bytes!("../../../firmware/cbios/cbios_main_msx1.rom");
pub const CBIOS_MSX2_MAIN: &[u8; 32768] =
    include_bytes!("../../../firmware/cbios/cbios_main_msx2.rom");
pub const CBIOS_MSX2_SUB: &[u8; 16384] = include_bytes!("../../../firmware/cbios/cbios_sub.rom");

/// Independent MIT-licensed standard-BIOS cartridge. It intentionally requires
/// BIOS slot discovery and real RDSLT/WRSLT, EXTROM, CHGMOD, CHPUT, FILVRM,
/// WRTVRM, SNSMAT, WRTPSG and WRTVDP execution; the diagnostic loader cannot run it.
/// SCREEN 1 has an original striped tile at (0,16). Space changes (8,16) from
/// black to white and plays a tone. C200h-C202h hold the slot/input/pass results.
pub fn cbios_test_cartridge() -> Vec<u8> {
    cbios_service_cartridge(true)
}
pub fn cbios_msx1_test_cartridge() -> Vec<u8> {
    cbios_service_cartridge(false)
}
fn cbios_service_cartridge(msx2: bool) -> Vec<u8> {
    let mut rom = vec![0; 16384];
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    let mut code = vec![0xf3, 0x31, 0x00, 0xf0]; // DI; LD SP,F000h
    let fail = [0xc2, 0x00, 0x43]; // JP NZ,4300h

    if msx2 {
        // C-BIOS must have discovered expanded slot 3 before calling AB init.
        code.extend_from_slice(&[0x3a, 0xc4, 0xfc, 0xe6, 0x80, 0xfe, 0x80]);
        code.extend_from_slice(&fail);
        // Read the expanded sub-ROM, try a protected write, then read it again.
        code.extend_from_slice(&[0x3e, 0x83, 0x21, 0x00, 0x00, 0xcd, 0x0c, 0x00, 0xfe, b'C']);
        code.extend_from_slice(&fail);
        code.extend_from_slice(&[0x3e, 0x83, 0x21, 0x00, 0x00, 0x1e, 0x00, 0xcd, 0x14, 0x00]);
        code.extend_from_slice(&[0x3e, 0x83, 0x21, 0x00, 0x00, 0xcd, 0x0c, 0x00, 0xfe, b'C']);
        code.extend_from_slice(&fail);
        // Write/read expanded RAM in slot 3-3, while main BIOS stays in page zero.
        code.extend_from_slice(&[0x3e, 0x8f, 0x21, 0x00, 0xc2, 0x1e, 0x5a, 0xcd, 0x14, 0x00]);
        code.extend_from_slice(&[0x3e, 0x8f, 0x21, 0x00, 0xc2, 0xcd, 0x0c, 0x00, 0xfe, 0x5a]);
        code.extend_from_slice(&fail);
        // EXTROM INIPLT (MSX2 only).
        code.extend_from_slice(&[0xdd, 0x21, 0x41, 0x01, 0xcd, 0x5f, 0x01]);
    } else {
        // Unexpanded MSX1 RAM via standard WRSLT/RDSLT.
        code.extend_from_slice(&[0x3e, 3, 0x21, 0x00, 0xc2, 0x1e, 0x5a, 0xcd, 0x14, 0]);
        code.extend_from_slice(&[0x3e, 3, 0x21, 0x00, 0xc2, 0xcd, 0x0c, 0, 0xfe, 0x5a]);
        code.extend_from_slice(&fail);
    }
    code.extend_from_slice(&[0x3e, 1, 0xcd, 0x5f, 0, 0x3e, 12, 0xcd, 0xa2, 0]);
    for ch in b"C-BIOS SERVICE TEST" {
        code.extend_from_slice(&[0x3e, *ch, 0xcd, 0xa2, 0]);
    }
    // FILVRM: original alternating pattern for character 128, blank for 136.
    for (address, value) in [(0x0400u16, 0xaa), (0x0440, 0)] {
        code.extend_from_slice(&[0x21, address as u8, (address >> 8) as u8]);
        code.extend_from_slice(&[0x01, 8, 0, 0x3e, value, 0xcd, 0x56, 0]);
    }
    // WRTVRM: color groups and two cells in the third row.
    for (address, value) in [
        (0x2010u16, 0xf1),
        (0x2011, 0xf0),
        (0x1840, 128),
        (0x1841, 136),
    ] {
        code.extend_from_slice(&[0x21, address as u8, (address >> 8) as u8]);
        code.extend_from_slice(&[0x3e, value, 0xcd, 0x4d, 0]);
    }
    // WRTPSG: channel A, ~440 Hz, tone only. Loop changes volume via BIOS.
    for (register, value) in [(0, 254), (1, 0), (7, 0xbe), (8, 0)] {
        code.extend_from_slice(&[0x3e, register, 0x1e, value, 0xcd, 0x93, 0]);
    }
    code.extend_from_slice(&[0x3e, 0xa5, 0x32, 0x02, 0xc2]); // All services returned.
    let loop_address = 0x4010 + code.len() as u16;
    code.extend_from_slice(&[0x3e, 8, 0xcd, 0x41, 1, 0xe6, 1, 0xee, 1, 0x32, 1, 0xc2]);
    // E = volume, B = backdrop; NZ means Space pressed.
    code.extend_from_slice(&[0x1e, 0, 0x06, 1, 0x28, 4, 0x1e, 15, 0x06, 15]);
    code.extend_from_slice(&[0x3e, 8, 0xcd, 0x93, 0, 0x0e, 7, 0xcd, 0x47, 0]);
    code.extend_from_slice(&[0xc3, loop_address as u8, (loop_address >> 8) as u8]);
    assert!(code.len() < 0x2f0);
    rom[0x10..0x10 + code.len()].copy_from_slice(&code);
    rom[0x300..0x305].copy_from_slice(&[0xf3, 0x76, 0xc3, 0x01, 0x43]);
    rom
}
