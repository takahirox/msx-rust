//! Independently authored executable mapper probes (MIT); no game bytes.
use crate::Mapper;

/// SRAM probes boot through C-BIOS, execute bank 7, copy a routine to RAM,
/// restore/read SRAM, mirror-check it and show its low nibble as VDP backdrop.
/// Space increments once per press. R-Type executes the second chip's last bank.
pub fn mapper_test_cartridge(mapper: Mapper) -> Vec<u8> {
    fn store(code: &mut Vec<u8>, a: u16, v: u8) {
        code.extend([0x3e, v, 0x32, a as u8, (a >> 8) as u8]);
    }
    fn out(code: &mut Vec<u8>, port: u8, v: u8) {
        code.extend([0x3e, v, 0xd3, port]);
    }
    let ascii8 = matches!(mapper, Mapper::Ascii8Sram2 | Mapper::Ascii8Sram8);
    let rtype = mapper == Mapper::RType;
    assert!(rtype || mapper.sram_size() > 0);
    let stride = if ascii8 { 8192 } else { 16384 };
    let size = if rtype {
        384 * 1024
    } else if ascii8 {
        256 * 1024
    } else {
        128 * 1024
    };
    let mut rom = vec![0; size];
    let fixed = if rtype { 15 * stride } else { 0 };
    rom[fixed..fixed + 4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    let mut boot = vec![0xf3, 0x31, 0, 0xf0];
    out(&mut boot, 0xa8, 0xd4);
    store(
        &mut boot,
        if rtype { 0x7fff } else { 0x7000 },
        if rtype { 0x17 } else { 7 },
    );
    boot.extend([0xc3, 0, 0x80]);
    rom[fixed + 0x10..fixed + 0x10 + boot.len()].copy_from_slice(&boot);
    let bank = if rtype { 23 } else { 7 } * stride;
    if rtype {
        let mut code = vec![];
        out(&mut code, 0x99, 0);
        out(&mut code, 0x99, 0x81); // blank screen: backdrop is observable
        out(&mut code, 0x99, 10);
        out(&mut code, 0x99, 0x87);
        store(&mut code, 0xc002, 23);
        store(&mut code, 0xc000, 0xa5);
        code.extend([0x76, 0x18, 0xfd]);
        rom[bank..bank + code.len()].copy_from_slice(&code);
        return rom;
    }
    let mut code = vec![];
    let select = if ascii8 { 0x20 } else { 0x10 };
    store(&mut code, 0x7000, select);
    if ascii8 {
        store(&mut code, 0x7800, select);
    }
    // LD A,(8000); CP FF; JR NZ,+2; LD A,4; LD (8000),A.
    code.extend([0x3a, 0, 0x80, 0xfe, 0xff, 0x20, 2, 0x3e, 4, 0x32, 0, 0x80]);
    out(&mut code, 0x99, 0);
    out(&mut code, 0x99, 0x81);
    let display = 0xc100 + code.len() as u16;
    // Read physical SRAM through its mirror, store readback, set backdrop.
    let mirror: u16 = if mapper.sram_size() == 2048 {
        0xb800
    } else {
        0xa000
    };
    code.extend([
        0x3a,
        mirror as u8,
        (mirror >> 8) as u8,
        0x32,
        1,
        0xc0,
        0xd3,
        0x99,
    ]);
    out(&mut code, 0x99, 0x87);
    store(&mut code, 0xc000, 0xa5);
    out(&mut code, 0xaa, 8);
    // Wait for Space down; increment SRAM; wait for Space up; redraw.
    let poll = 0xc100 + code.len() as u16;
    code.extend([0xdb, 0xa9, 0xe6, 1, 0xc2, poll as u8, (poll >> 8) as u8]);
    code.extend([0x3a, 0, 0x80, 0x3c, 0xe6, 15, 0x32, 0, 0x80]);
    let release = 0xc100 + code.len() as u16;
    code.extend([
        0xdb,
        0xa9,
        0xe6,
        1,
        0xca,
        release as u8,
        (release >> 8) as u8,
        0xc3,
        display as u8,
        (display >> 8) as u8,
    ]);
    // Switched ROM code must execute before the SRAM window replaces it.
    let mut switched = vec![];
    store(&mut switched, 0xc002, 7);
    switched.extend([
        0x21,
        0,
        0x81,
        0x11,
        0,
        0xc1,
        0x01,
        code.len() as u8,
        (code.len() >> 8) as u8,
        0xed,
        0xb0,
        0xc3,
        0,
        0xc1,
    ]);
    rom[bank..bank + switched.len()].copy_from_slice(&switched);
    rom[bank + 0x100..bank + 0x100 + code.len()].copy_from_slice(&code);
    rom
}
