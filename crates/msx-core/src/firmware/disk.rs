//! Independently authored Z80 disk firmware and demo; MIT. No external ROM bytes.
//! This small bytecode assembler is also the complete reproducible source.
use crate::font;
struct Code {
    base: u16,
    bytes: Vec<u8>,
    labels: Vec<(&'static str, u16)>,
    fixups: Vec<(usize, &'static str)>,
}
impl Code {
    fn new(base: u16) -> Self {
        Self {
            base,
            bytes: vec![],
            labels: vec![],
            fixups: vec![],
        }
    }
    fn emit(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }
    fn label(&mut self, name: &'static str) {
        self.labels
            .push((name, self.base + self.bytes.len() as u16));
    }
    fn jump(&mut self, op: u8, name: &'static str) {
        self.emit(&[op, 0, 0]);
        self.fixups.push((self.bytes.len() - 2, name));
    }
    fn out(&mut self, port: u8, value: u8) {
        self.emit(&[0x3e, value, 0xd3, port]);
    }
    fn store(&mut self, address: u16, value: u8) {
        self.emit(&[0x3e, value, 0x32, address as u8, (address >> 8) as u8]);
    }
    fn finish(mut self) -> Vec<u8> {
        for (offset, name) in self.fixups {
            let address = self.labels.iter().find(|(n, _)| *n == name).unwrap().1;
            self.bytes[offset..offset + 2].copy_from_slice(&address.to_le_bytes());
        }
        self.bytes
    }
}
/// Main reset loader. Slot 0 page 0, slot 2 page 1, RAM pages 2/3.
pub fn write_disk_boot(rom: &mut [u8], msx2: bool) {
    rom.fill(0xff);
    rom[..3].copy_from_slice(&[0xc3, 0, 1]);
    rom[0x2d] = u8::from(msx2);
    let code = [
        0xf3, 0x3e, 0xf0, 0xd3, 0xa8, 0x3e, 0xff, 0x32, 0xff, 0xff, 0x3e, 0xf8, 0xd3, 0xa8, 0x31,
        0, 0xf0, 0xc3, 0, 0x41,
    ];
    rom[0x100..0x100 + code.len()].copy_from_slice(&code);
}
pub fn disk_rom() -> Vec<u8> {
    let mut rom = vec![0xff; 16384];
    rom[..4].copy_from_slice(&[b'A', b'B', 0, 0x41]);
    let mut c = Code::new(0x4100);
    c.label("boot");
    c.emit(&[
        0x3a, 0xfe, 0x7f, 0xe6, 1, 0xf6, 0xf8, 0x4f, // C media from controller geometry
        0xaf, 0x06, 1, 0x11, 0, 0, 0x21, 0, 0xc0, 0xcd, 0x10, 0x40,
    ]);
    c.jump(0xda, "boot-error");
    c.emit(&[0x2a, 0xfe, 0xc1, 0x11, b'W', b'V', 0xb7, 0xed, 0x52]);
    c.jump(0xc2, "boot-error");
    c.store(0xf104, 1); // firmware boot progress, observed by browser status
    c.emit(&[0xc3, 0, 0xc0]);
    c.label("boot-error");
    c.store(0xf104, 2);
    c.out(0x99, 6);
    c.out(0x99, 0x87);
    c.emit(&[0x76]);
    c.jump(0xc3, "boot-error");

    // Standard DISKIO ABI, buffers in RAM below reserved EF00h..EFFFh stack.
    c.label("io");
    c.emit(&[
        0xf5, 0xaf, 0x32, 2, 0xf1, 0x3e, 0x80, 0x32, 0x01, 0xf1, 0xf1,
    ]); // preserve incoming carry
    c.jump(0xd2, "read-mode");
    c.emit(&[0xf5]);
    c.store(0xf101, 0xa0);
    c.emit(&[0xf1]);
    c.label("read-mode");
    c.emit(&[0xb7]);
    c.jump(0xc2, "not-ready"); // only A=0
    c.emit(&[0x78, 0xb7]);
    c.jump(0xca, "parameter-error");
    c.emit(&[0x78, 0x32, 0, 0xf1]);
    c.store(0xf102, 0);
    c.emit(&[0x79, 0xfe, 0xf8]);
    c.jump(0xca, "media-ok");
    c.emit(&[0xfe, 0xf9]);
    c.jump(0xc2, "parameter-error");
    c.label("media-ok");
    c.emit(&[0xe6, 1, 0x32, 3, 0xf1]);
    c.label("next-sector");
    // Reject buffers that would overwrite code, stack or driver scratch; no wrap.
    c.emit(&[0x7c, 0xfe, 0x80]);
    c.jump(0xda, "parameter-error");
    c.emit(&[0xfe, 0xed]);
    c.jump(0xda, "buffer-ok");
    c.jump(0xc2, "parameter-error");
    c.emit(&[0x7d, 0xb7]);
    c.jump(0xc2, "parameter-error");
    c.label("buffer-ok");
    c.emit(&[0xc5, 0xd5, 0xe5, 0xeb, 0x01, 9, 0, 0x11, 0, 0]); // HL=LBA, DE=0 quotient
    c.label("divide");
    c.emit(&[0xb7, 0xed, 0x42]);
    c.jump(0xda, "divided");
    c.emit(&[0x13]);
    c.jump(0xc3, "divide");
    c.label("divided");
    c.emit(&[0x09, 0x7d, 0x3c, 0x32, 0xfa, 0x7f, 0x3a, 3, 0xf1, 0xb7]);
    c.jump(0xca, "single-side");
    c.emit(&[0x7b, 0xe6, 1, 0x32, 0xfc, 0x7f, 0xcb, 0x3a, 0xcb, 0x1b]);
    c.jump(0xc3, "track-ready");
    c.label("single-side");
    c.store(0x7ffc, 0);
    c.label("track-ready");
    c.emit(&[0x7a, 0xb7]);
    c.jump(0xc2, "invalid-track");
    c.emit(&[0x7b, 0xfe, 80]);
    c.jump(0xd2, "invalid-track");
    c.store(0x7ffd, 0x81);
    c.emit(&[0x7b, 0x32, 0xfb, 0x7f]);
    c.store(0x7ff8, 0x10);
    c.jump(0xcd, "wait-done");
    c.emit(&[0xe1, 0xd1, 0xc1, 0xe6, 0x98]);
    c.jump(0xc2, "hardware-error");
    c.emit(&[0x3a, 1, 0xf1, 0x32, 0xf8, 0x7f, 0xc5, 0xd5, 0x11, 0, 2]); // count512
    c.label("byte");
    c.emit(&[0x3a, 0xff, 0x7f, 0xe6, 0x40]);
    c.jump(0xc2, "transfer-byte");
    c.emit(&[0x3a, 0xf8, 0x7f, 0xe6, 1]);
    c.jump(0xc2, "byte");
    c.emit(&[0xd1, 0xc1]);
    c.jump(0xc3, "hardware-error");
    c.label("transfer-byte");
    c.emit(&[0x3a, 1, 0xf1, 0xfe, 0xa0]);
    c.jump(0xca, "write-byte");
    c.emit(&[0x3a, 0xfb, 0x7f, 0x77]);
    c.jump(0xc3, "byte-done");
    c.label("write-byte");
    c.emit(&[0x7e, 0x32, 0xfb, 0x7f]);
    c.label("byte-done");
    c.emit(&[0x23, 0x1b, 0x7a, 0xb3]);
    c.jump(0xc2, "byte");
    c.jump(0xcd, "wait-done");
    c.emit(&[0xd1, 0xc1]);
    c.jump(0xda, "hardware-error");
    c.emit(&[
        0x3a, 2, 0xf1, 0x3c, 0x32, 2, 0xf1, 0x13, 0x3a, 0, 0xf1, 0x3d, 0x32, 0, 0xf1,
    ]);
    c.jump(0xc2, "next-sector");
    c.emit(&[0x3a, 2, 0xf1, 0x47, 0xaf, 0xc9]);
    c.label("invalid-track");
    c.emit(&[0xe1, 0xd1, 0xc1, 0x3e, 8]);
    c.jump(0xc3, "error");
    c.label("wait-done");
    c.emit(&[0x3a, 0xf8, 0x7f, 0xcb, 0x47]);
    c.jump(0xc2, "wait-done");
    c.emit(&[0xe6, 0xfc, 0xc8, 0x37, 0xc9]);
    c.label("hardware-error");
    c.emit(&[0x3a, 0xf8, 0x7f, 0xcb, 0x7f]);
    c.jump(0xc2, "not-ready");
    c.emit(&[0xcb, 0x77, 0x3e, 0]);
    c.jump(0xc2, "error");
    c.emit(&[0x3e, 8]);
    c.jump(0xc3, "error");
    c.label("not-ready");
    c.emit(&[0x3e, 2]);
    c.jump(0xc3, "error");
    c.label("parameter-error");
    c.emit(&[0x3e, 12]);
    c.label("error");
    c.emit(&[0x4f, 0x3a, 2, 0xf1, 0x47, 0x79, 0x37, 0xc9]);
    c.label("change");
    c.emit(&[0xb7]);
    c.jump(0xc2, "not-ready");
    c.emit(&[0x3a, 0xfe, 0x7f, 0xcb, 0x7f]);
    c.jump(0xca, "not-ready");
    c.emit(&[0xe6, 2, 0x06, 1, 0xc8, 0x06, 0xff, 0xaf, 0xc9]);
    c.label("choice");
    c.emit(&[0x21, 0, 0, 0xc9]);
    c.label("unsupported");
    c.emit(&[0x3e, 12, 0x37, 0xc9]);
    c.label("stop");
    c.store(0x7ffd, 1);
    c.emit(&[0xc9]);
    for (entry, label) in [
        (0x10, "io"),
        (0x13, "change"),
        (0x16, "unsupported"),
        (0x19, "choice"),
        (0x1c, "unsupported"),
        (0x1f, "stop"),
    ] {
        let address = c.labels.iter().find(|(n, _)| *n == label).unwrap().1;
        rom[entry..entry + 3].copy_from_slice(&[0xc3, address as u8, (address >> 8) as u8]);
    }
    let bytes = c.finish();
    assert!(bytes.len() < 0x3e00);
    rom[0x100..0x100 + bytes.len()].copy_from_slice(&bytes);
    rom
}
/// Raw 720 KiB original boot disk. Sector zero loads ten more sectors via DISKIO.
/// Space reads/increments/writes byte 0 of LBA 18; all other bytes are preserved.
pub fn demo_disk() -> Vec<u8> {
    let mut disk = vec![0; 737280];
    let loader = [
        0xaf, 0x06, 10, 0x0e, 0xf9, 0x11, 1, 0, 0x21, 0, 0xc2, 0xcd, 0x10, 0x40, 0xda, 0, 0x41,
        0xc3, 0, 0xc2,
    ];
    disk[..loader.len()].copy_from_slice(&loader);
    disk[510..512].copy_from_slice(b"WV");
    let mut c = Code::new(0xc200);
    for (r, v) in [0, 0x40, 6, 0x80, 0, 0x36, 7, 0xf1].into_iter().enumerate() {
        c.out(0x99, v);
        c.out(0x99, 0x80 | r as u8);
    }
    c.out(0x99, 0);
    c.out(0x99, 0x40);
    c.emit(&[0x21, 0, 0xc8, 0x11, 0, 8]);
    c.jump(0xcd, "vram-copy");
    c.out(0x99, 0);
    c.out(0x99, 0x58);
    c.emit(&[0x21, 0, 0xd0, 0x11, 0, 3]);
    c.jump(0xcd, "vram-copy");
    // Graphics I color table: white foreground, black background. Hide sprites.
    c.out(0x99, 0);
    c.out(0x99, 0x60);
    c.emit(&[0x06, 32, 0x3e, 0xf1, 0xd3, 0x98, 0x10, 0xfc]);
    c.out(0x99, 0);
    c.out(0x99, 0x5b);
    c.out(0x98, 208);
    c.store(0xde00, 0); // key edge state
    c.label("poll");
    c.out(0xaa, 8);
    c.emit(&[0xdb, 0xa9, 0xe6, 1]);
    c.jump(0xca, "pressed");
    c.store(0xde00, 0);
    c.jump(0xc3, "poll");
    c.label("pressed");
    c.emit(&[0x3a, 0, 0xde, 0xb7]);
    c.jump(0xc2, "poll");
    c.store(0xde00, 1);
    c.emit(&[
        0xaf, 0x06, 1, 0x0e, 0xf9, 0x11, 18, 0, 0x21, 0, 0xe0, 0xcd, 0x10, 0x40,
    ]);
    c.jump(0xda, "failed");
    c.emit(&[
        0x21, 0, 0xe0, 0x34, 0xaf, 0x37, 0x06, 1, 0x0e, 0xf9, 0x11, 18, 0, 0xcd, 0x10, 0x40,
    ]);
    c.jump(0xda, "failed");
    c.emit(&[0x21, 0, 0xd3]);
    c.jump(0xc3, "result");
    c.label("failed");
    c.emit(&[0x21, 0x20, 0xd3]);
    c.label("result");
    c.out(0x99, 0);
    c.out(0x99, 0x5a);
    c.emit(&[0x11, 32, 0]);
    c.jump(0xcd, "vram-copy");
    c.jump(0xc3, "poll");
    c.label("vram-copy");
    c.emit(&[0x7e, 0xd3, 0x98, 0x23, 0x1b, 0x7a, 0xb3]);
    c.jump(0xc2, "vram-copy");
    c.emit(&[0xc9]);
    let bytes = c.finish();
    assert!(bytes.len() <= 0x600);
    disk[512..512 + bytes.len()].copy_from_slice(&bytes);
    for ch in 0..256 {
        disk[2048 + ch * 8..2056 + ch * 8].copy_from_slice(&font::glyph(ch as u8));
    }
    disk[4096..5120].fill(b' '); // names and result text; result strings loaded separately below
    for (row, text) in [
        (4, "WEAVE DISK BOOT"),
        (7, "Z80 LOADED FROM DRIVE A"),
        (10, "SPACE: WRITE SECTOR 18"),
        (16, "READY"),
    ] {
        let offset = 4096 + row * 32;
        disk[offset..offset + text.len()].copy_from_slice(text.as_bytes());
    }
    // Result strings need to reside in the ten loaded sectors (D300h/D320h).
    disk[4864..4896].fill(b' ');
    disk[4864..4869].copy_from_slice(b"SAVED");
    disk[4896..4928].fill(b' ');
    disk[4896..4911].copy_from_slice(b"WRITE PROTECTED");
    // Put deterministic unrelated content in the writable sector for preservation assertions.
    for (i, b) in disk[9216..9728].iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(7);
    }
    disk
}
