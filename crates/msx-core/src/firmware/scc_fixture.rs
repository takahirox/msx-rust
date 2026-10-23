//! Original MIT-licensed SCC diagnostic, generated without third-party ROM bytes.
/// A 64 KiB Konami SCC MegaROM for either model. Space: 440 Hz SCC square;
/// Up: all five SCC voices; Left: PSG; Right: hide registers; Down: hide slot.
/// Hidden registers do not mute sound. Release Space/Up: explicit voice silence.
pub fn scc_test_cartridge() -> Vec<u8> {
    fn store(code: &mut Vec<u8>, address: u16, value: u8) {
        code.extend([0x3e, value, 0x32, address as u8, (address >> 8) as u8]);
    }
    fn out(code: &mut Vec<u8>, port: u8, value: u8) {
        code.extend([0x3e, value, 0xd3, port]);
    }
    let mut rom = vec![0; 65536];
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    let mut code = vec![0xf3, 0x31, 0, 0xf0]; // DI; stack in BIOS-discovered RAM.

    // Both cartridge pages, RAM page 3. Genuine mapper accesses provide
    // four-window evidence for Auto inference.
    out(&mut code, 0xa8, 0xd4);
    for (a, v) in [(0x5000, 0), (0x7000, 1), (0xb000, 3), (0x9000, 0x3f)] {
        store(&mut code, a, v);
    }
    // LD HL,4800; LD DE,9800; LD BC,128; LDIR (four distinct waveforms).
    code.extend([0x21, 0, 0x48, 0x11, 0, 0x98, 0x01, 128, 0, 0xed, 0xb0]);
    for (c, (period, volume)) in [(253u16, 15), (199, 12), (159, 9), (126, 6), (99, 3)]
        .into_iter()
        .enumerate()
    {
        store(&mut code, 0x9880 + c as u16 * 2, period as u8);
        store(&mut code, 0x9881 + c as u16 * 2, (period >> 8) as u8);
        store(&mut code, 0x988a + c as u16, volume);
    }
    for (r, v) in [(0, 190), (1, 0), (7, 0x3e), (8, 0), (9, 0), (10, 0)] {
        out(&mut code, 0xa0, r);
        out(&mut code, 0xa1, v);
    }
    out(&mut code, 0x99, 4);
    out(&mut code, 0x99, 0x87); // Blue backdrop, independent of BIOS display mode.
    store(&mut code, 0xc000, 0xa5); // Readiness marker for Rust execution tests.
    let poll = 0x4010 + code.len() as u16;
    // Briefly expose the registers before applying explicit channel controls.
    // This keeps key release effective even while Right/Down remain held.
    out(&mut code, 0xa8, 0xd4);
    store(&mut code, 0x9000, 0x3f);
    out(&mut code, 0xaa, 8);
    code.extend([0xdb, 0xa9, 0x47]); // IN A,(A9); LD B,A (active-low row 8).
    code.extend([0x78, 0xe6, 0x20, 0x3e, 31, 0x28, 2, 0x3e, 0, 0x4f]);
    code.extend([0x78, 0xe6, 1, 0x3e, 1, 0x28, 2, 0x3e, 0, 0xb1]);
    code.extend([0x32, 0x8f, 0x98]); // Up: five voices; Space: first voice.
    code.extend([0x78, 0xe6, 0x10, 0x3e, 15, 0x28, 2, 0x3e, 0, 0x4f]);
    out(&mut code, 0xa0, 8);
    code.extend([0x79, 0xd3, 0xa1]);
    // Right: bank 2 instead of 3Fh. Down: BIOS instead of cartridge in page 2.
    // Code and stack remain in pages 1/3, so actual Z80 slot writes are safe.
    code.extend([0x78, 0xe6, 0x80, 0x3e, 2, 0x28, 2, 0x3e, 0x3f]);
    code.extend([0x32, 0, 0x90]);
    code.extend([
        0x78, 0xe6, 0x40, 0x3e, 0xc4, 0x28, 2, 0x3e, 0xd4, 0xd3, 0xa8,
    ]);
    // Attempt destructive writes only while hidden. They must never reach SCC.
    code.extend([0x78, 0xe6, 0xc0, 0xfe, 0xc0, 0x28, 25]);
    for a in [0x9800, 0x9880, 0x9881, 0x988a, 0x988f] {
        store(&mut code, a, 0);
    }
    code.extend([0x78, 0x32, 1, 0xc0]); // Last polled row, for execution tests.
    code.extend([0xc3, poll as u8, (poll >> 8) as u8]);
    assert!(code.len() < 0x7f0);
    rom[0x10..0x10 + code.len()].copy_from_slice(&code);
    for i in 0..32 {
        rom[0x800 + i] = if i < 16 { 120 } else { (-120i8) as u8 };
        rom[0x820 + i] = ((i as i16 * 8 - 124) as i8) as u8; // Sawtooth.
        rom[0x840 + i] = ((if i < 16 { i } else { 31 - i }) as i16 * 16 - 120) as u8;
        rom[0x860 + i] = if i < 8 { 96 } else { (-32i8) as u8 }; // Pulse, shared 4/5.
    }
    rom
}
