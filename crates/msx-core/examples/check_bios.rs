//! Boot a locally supplied, lawfully obtained BIOS and our authored test cartridge.
//! The BIOS is read as opaque machine code, never incorporated into build output.
use msx_core::{firmware, Machine, Model};
use std::{env, fs};
fn main() {
    let path = env::args()
        .nth(1)
        .expect("usage: check_bios MAIN_ROM [MSX2_SUB_ROM]");
    let mut m = Machine::default();
    if let Some(sub) = env::args().nth(2) {
        m.set_model(Model::Msx2);
        m.load_msx2_firmware(&fs::read(path).unwrap(), &fs::read(sub).unwrap())
            .unwrap();
    } else {
        m.load_bios(&fs::read(path).unwrap()).unwrap();
    }
    m.load_cartridge(&firmware::test_cartridge()).unwrap();
    for _ in 0..600 {
        m.frame();
    }
    let screen = String::from_utf8_lossy(&m.hardware.vdp.vram[0x1800..0x1b00]);
    assert!(
        screen.contains("MSX-RUST TEST CARTRIDGE"),
        "Cartridge did not boot: PC={:04x}, slots={:02x}, VDP={:02x?}",
        m.cpu.pc,
        m.hardware.slots,
        m.hardware.vdp.registers
    );
    m.hardware.key(8, 0, true);
    m.frame();
    assert_eq!(m.hardware.psg.registers[8], 15);
    assert!(m.hardware.psg.samples.iter().any(|s| s.abs() > 0.1));
    println!("BIOS booted the authored cartridge; VDP text, PPI input and PSG audio passed (600 frames).");
}
