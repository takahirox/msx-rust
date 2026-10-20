use msx_core::{cpu::Bus, firmware, Firmware, Machine, Mapper, Model};

fn boot(m: &mut Machine) {
    for _ in 0..600 {
        m.frame();
        if m.hardware.ram[0xc202] == 0xa5 {
            return;
        }
    }
    panic!(
        "C-BIOS cartridge did not complete BIOS calls: PC={:04x}, slots={:02x}, secondary={:02x}",
        m.cpu.pc, m.hardware.slots, m.hardware.secondary[3]
    );
}
fn pixel(m: &Machine, x: usize, y: usize) -> &[u8] {
    let offset = (y * m.hardware.vdp.width() + x) * 4;
    &m.hardware.vdp.pixels[offset..offset + 3]
}

#[test]
fn bundled_cbios_executes_standard_services_and_expanded_slots_on_z80() {
    let mut m = Machine::default();
    m.set_model(Model::Msx2);
    m.load_cartridge(&firmware::cbios_test_cartridge()).unwrap();
    assert_eq!(m.hardware.bios, firmware::CBIOS_MSX2_MAIN);
    assert_eq!(m.hardware.subrom, firmware::CBIOS_MSX2_SUB);
    assert_eq!(m.hardware.firmware, Firmware::Cbios); // No native BASIC service ports.
    assert_eq!(m.hardware.secondary[3], 0); // BIOS must discover RAM itself.
    m.load_cartridge(&firmware::cbios_test_cartridge()).unwrap();
    boot(&mut m);
    m.frame();
    assert_eq!(m.hardware.ram[0xfcc4] & 0x80, 0x80); // BIOS EXPTBL.
    assert_eq!(m.hardware.ram[0xc200], 0x5a); // BIOS WRSLT/RDSLT round trip.
    assert_eq!(m.hardware.slots & 0xc3, 0xc0); // BIOS page zero, expanded RAM stack.
    assert_eq!(m.hardware.secondary[3] >> 6, 3);
    assert_eq!(&m.hardware.vdp.vram[0x1802..0x1815], b"C-BIOS SERVICE TEST");
    assert_eq!(m.hardware.vdp.mode(), 1);
    assert_eq!(pixel(&m, 0, 16), [255, 255, 255]);
    assert_eq!(pixel(&m, 1, 16), [0, 0, 0]);
    assert_eq!(pixel(&m, 8, 16), [0, 0, 0]);
    assert_eq!(m.hardware.ram[0xc201], 0);
    m.hardware.key(8, 0, true);
    for _ in 0..3 {
        m.frame();
    }
    assert_eq!(m.hardware.ram[0xc201], 1); // SNSMAT return, not host text input.
    assert_eq!(m.hardware.psg.registers[8], 15);
    assert!(m.hardware.psg.samples.iter().any(|s| s.abs() > 0.05));
    assert_eq!(pixel(&m, 8, 16), [255, 255, 255]);
    m.hardware.key(8, 0, false);
    for _ in 0..16 {
        m.frame();
    }
    assert_eq!(m.hardware.psg.registers[8], 0);
    assert!(m.hardware.psg.samples.iter().all(|s| s.abs() < 0.0001));
    assert_eq!(pixel(&m, 8, 16), [0, 0, 0]);
    assert!(m.basic.console.transcript.is_empty());
    // Invalid firmware/cartridge imports must preserve execution and banks.
    let pc = m.cpu.pc;
    let slots = m.hardware.slots;
    assert!(m
        .load_msx2_firmware(&[0; 32768], firmware::CBIOS_MSX2_SUB)
        .is_err());
    assert!(m.load_cartridge(&[0; 17]).is_err());
    assert_eq!(m.cpu.pc, pc);
    assert_eq!(m.hardware.slots, slots);
    assert_eq!(m.hardware.bios, firmware::CBIOS_MSX2_MAIN);
    // Reset starts with firmware mapping, releases input and boots the same cart.
    m.reset();
    assert_eq!(m.hardware.secondary[3], 0);
    assert_eq!(m.hardware.slots, 0xf0);
    assert_eq!(m.hardware.keyboard, [255; 11]);
    boot(&mut m);
    assert!(m.basic.console.transcript.is_empty());
}

#[test]
fn firmware_switches_release_cartridge_banks_and_restore_msx1_basic() {
    let mut m = Machine::default();
    m.use_cbios().unwrap();
    m.set_model(Model::Msx2);
    // A banked ROM makes stale mapper state observable after switching firmware.
    let mut rom = vec![0; 65536];
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    m.load_cartridge_with_mapper(&rom, Mapper::Konami).unwrap();
    m.hardware.output(0xa8, 0xd4);
    m.hardware.write(0x8000, 7);
    m.use_diagnostic().unwrap();
    assert_eq!(m.hardware.firmware, Firmware::Diagnostic);
    assert!(m.hardware.cartridge.is_empty());
    assert_eq!(m.hardware.bios, firmware::msx2_bios());
    m.use_cbios().unwrap();
    assert_eq!(m.hardware.firmware, Firmware::Cbios);
    assert!(m.hardware.cartridge.is_empty());
    m.load_cartridge(&firmware::cbios_test_cartridge()).unwrap();
    boot(&mut m);
    m.unload_cartridge();
    assert!(m.hardware.cartridge.is_empty());
    for _ in 0..300 {
        m.frame();
    }
    assert_eq!(m.hardware.ram[0xc202], 0);
    assert!(
        String::from_utf8_lossy(&m.hardware.vdp.vram[0x1800..0x1b00])
            .contains("No cartridge found.")
    );
    assert!(m.basic.console.transcript.is_empty()); // No fabricated BASIC.
    m.load_msx2_firmware(firmware::CBIOS_MSX2_MAIN, firmware::CBIOS_MSX2_SUB)
        .unwrap();
    m.set_model(Model::Msx1);
    assert!(m.hardware.subrom.is_empty());
    assert!(m.hardware.cartridge.is_empty());
    assert_eq!(m.hardware.firmware, Firmware::WeaveBasic);
    m.frame();
    assert!(m.basic.console.transcript.contains("READY"));
    for key in b"PRINT 42\r" {
        m.text_key(*key);
    }
    for _ in 0..4 {
        m.frame();
    }
    assert!(m.basic.console.transcript.contains("42"));
}
