use msx_core::{
    cpu::Bus,
    firmware,
    psg::{Psg, CPU_HZ, SAMPLE_RATE},
    vdp::{Vdp, PALETTE},
    Hardware, Machine, Model,
};
fn pixel(v: &Vdp, x: usize, y: usize) -> &[u8] {
    &v.pixels[(y * 256 + x) * 4..(y * 256 + x) * 4 + 3]
}
#[test]
fn vdp_ports_readahead_wrap_and_irq() {
    let mut v = Vdp::default();
    v.control(0xff);
    v.control(0x7f);
    v.write(0x12);
    v.write(0x34);
    assert_eq!(v.vram[16383], 0x12);
    assert_eq!(v.vram[0], 0x34);
    v.control(0xff);
    v.control(0x3f);
    assert_eq!(v.read(), 0x12);
    assert_eq!(v.read(), 0x34);
    v.control(0x20);
    v.control(0x81);
    v.tick(228 * 192 - 1);
    assert!(!v.irq());
    v.tick(1);
    assert!(v.irq());
    assert_eq!(v.read_status() & 0x80, 0x80);
    assert!(!v.irq());
    v.tick(228 * 70);
    assert_eq!(v.frames, 1);
}
#[test]
fn vdp_four_modes() {
    let mut v = Vdp::default();
    v.registers[..8].copy_from_slice(&[0, 0x50, 2, 0, 0, 0x36, 7, 0xf4]);
    v.vram[0x800] = 1;
    v.vram[8] = 0x80;
    v.render();
    assert_eq!(pixel(&v, 8, 0), &PALETTE[15]);
    assert_eq!(pixel(&v, 9, 0), &PALETTE[4]);
    assert_eq!(pixel(&v, 0, 0), &PALETTE[4]);
    v.registers[..8].copy_from_slice(&[0, 0x40, 6, 0x80, 0, 0x36, 7, 0xf4]);
    v.vram[0x1800] = 1;
    v.vram[0x2000] = 0xa2;
    v.vram[0x1b00] = 208;
    v.render();
    assert_eq!(pixel(&v, 0, 0), &PALETTE[10]);
    assert_eq!(pixel(&v, 1, 0), &PALETTE[2]);
    v.registers[..8].copy_from_slice(&[2, 0x40, 6, 0xff, 3, 0x36, 7, 0xf4]);
    v.vram[0x1900] = 1;
    v.vram[0x808] = 0x80;
    v.vram[0x2808] = 0x65;
    v.render();
    assert_eq!(pixel(&v, 0, 64), &PALETTE[6]);
    assert_eq!(pixel(&v, 1, 64), &PALETTE[5]);
    v.registers[..8].copy_from_slice(&[0, 0x48, 6, 0, 0, 0x36, 7, 0xf4]);
    v.vram[0x1800] = 1;
    v.vram[8] = 0x23;
    v.render();
    assert_eq!(pixel(&v, 0, 0), &PALETTE[2]);
    assert_eq!(pixel(&v, 4, 0), &PALETTE[3]);
}
#[test]
fn sprites_priority_collision_fifth_and_large_early_clock() {
    let mut v = Vdp::default();
    v.registers[..8].copy_from_slice(&[0, 0x40, 6, 0x80, 0, 0x36, 7, 4]);
    v.vram[0x3800] = 0x80;
    for n in 0..5 {
        v.vram[0x1b00 + n * 4..0x1b04 + n * 4].copy_from_slice(&[9, 20, 0, 2 + n as u8]);
    }
    v.vram[0x1b14] = 208;
    v.render();
    assert_eq!(pixel(&v, 20, 10), &PALETTE[2]);
    assert_eq!(v.status & 0x7f, 0x64);
    assert_eq!(v.read_status() & 0x60, 0x60);
    assert_eq!(v.status & 0x60, 0);
    v.registers[1] = 0x43;
    v.vram[0x1b00..0x1b08].copy_from_slice(&[255, 32, 0, 0x8a, 208, 0, 0, 0]);
    v.vram[0x3800] = 0x80;
    v.render();
    assert_eq!(pixel(&v, 0, 0), &PALETTE[10]);
    assert_eq!(pixel(&v, 1, 0), &PALETTE[10]);
    assert_eq!(pixel(&v, 2, 0), &PALETTE[4]);
}
#[test]
fn slot_rom_protection_ppi_and_psg_io() {
    let mut h = Hardware::default();
    h.write(0, 0x99);
    assert_eq!(h.read(0), 0xc3);
    h.output(0xa8, 0xff);
    h.write(0, 0x99);
    assert_eq!(h.read(0), 0x99);
    h.output(0xa8, 0);
    assert_eq!(h.read(0), 0xc3);
    h.key(8, 0, true);
    h.output(0xaa, 8);
    assert_eq!(h.input(0xa9), 254);
    h.output(0xab, 0x0f);
    assert_eq!(h.ppi_c, 0x88);
    h.output(0xab, 0x0e);
    assert_eq!(h.ppi_c, 8);
    h.output(0xa0, 0);
    h.output(0xa1, 42);
    assert_eq!(h.input(0xa2), 42);
    h.output(0xa0, 14);
    h.joystick = 0x2f;
    assert_eq!(h.input(0xa2) & 0x3f, 0x2f);
    h.release_keys();
    assert_eq!(h.input(0xa9), 255);
}
#[test]
fn psg_tone_frequency_noise_and_envelopes() {
    let mut p = Psg::default();
    for (r, v) in [(0, 254), (7, 0x3e), (8, 15)] {
        p.select(r);
        p.write(v);
    }
    p.tick(CPU_HZ / 5);
    assert!((p.samples.len() as i32 - (SAMPLE_RATE / 5) as i32).abs() <= 1);
    let edges = p.samples.windows(2).filter(|w| w[1] - w[0] > 0.1).count();
    assert!((87..=89).contains(&edges), "{edges}");
    let periodic = p.samples.clone();
    p.samples.clear();
    p.select(7);
    p.write(0x37);
    p.select(6);
    p.write(3);
    p.tick(CPU_HZ / 5);
    assert_ne!(p.samples, periodic);
    assert!(p.samples.iter().all(|v| v.is_finite() && v.abs() <= 0.6));
    for shape in 0..16 {
        let mut p = Psg::default();
        p.select(11);
        p.write(1);
        p.select(13);
        p.write(shape);
        assert_eq!(p.envelope_level(), if shape & 4 != 0 { 0 } else { 15 });
        p.tick(512 * 16);
        assert_eq!(
            p.envelope_level(),
            match shape {
                0..=7 | 9 | 15 => 0,
                11 | 13 => 15,
                8 | 14 => 15,
                10 | 12 => 0,
                _ => unreachable!(),
            }
        );
    }
}
#[test]
fn psg_volume_zero_settles_to_silence() {
    // Exercise each channel independently through the register interface.
    for channel in 0..3 {
        let mut p = Psg::default();
        for (register, value) in [
            (channel * 2, 254),
            (7, 0x3f & !(1 << channel)),
            (8 + channel, 15),
        ] {
            p.select(register);
            p.write(value);
        }
        p.tick(CPU_HZ / 5);
        assert!(p.samples.iter().any(|sample| sample.abs() > 0.05));
        p.select(8 + channel);
        p.write(0);
        // Allow at most 250 ms for the AC-coupling filter to decay. Discard
        // these and all earlier tone samples; check a fresh 100 ms window.
        p.tick(CPU_HZ / 4);
        p.samples.clear();
        p.tick(CPU_HZ / 10);
        assert!((p.samples.len() as i32 - (SAMPLE_RATE / 10) as i32).abs() <= 1);
        assert!(
            p.samples
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() < 0.0001),
            "channel {channel} did not settle below the 0.0001 silence threshold"
        );
    }
}
fn enter(m: &mut Machine, line: &str) {
    for b in line.bytes().chain([13]) {
        m.text_key(b);
    }
    m.frame();
    m.frame();
}
#[test]
fn firmware_boot_basic_edit_list_run_and_pixels() {
    let mut m = Machine::default();
    m.frame();
    assert!(m.basic.console.transcript.contains("WEAVE BASIC"));
    assert_eq!(&m.hardware.vdp.vram[..11], b"WEAVE BASIC");
    assert!(m
        .hardware
        .vdp
        .pixels
        .chunks(4)
        .any(|p| p[..3] == PALETTE[15]));
    enter(&mut m, "10 FOR I=1 TO 5 : PRINT I : NEXT I");
    enter(&mut m, "LIST");
    enter(&mut m, "RUN");
    assert!(m.basic.console.transcript.contains("1\n2\n3\n4\n5\nREADY"));
    enter(&mut m, "10 PRINT 99");
    enter(&mut m, "RUN");
    assert!(m.basic.console.transcript.ends_with("99\nREADY\n"));
    enter(&mut m, "10");
    assert!(m.basic.program.is_empty());
    // Insert a missing I using left-arrow editing, and erase a typo with Backspace.
    for b in b"PRNT 8" {
        m.text_key(*b);
    }
    for _ in 0..4 {
        m.text_key(128);
    }
    m.text_key(b'I');
    m.text_key(131);
    m.text_key(8);
    m.text_key(b'7');
    m.text_key(13);
    m.frame();
    assert!(m.basic.console.transcript.ends_with("7\nREADY\n"));
}
#[test]
fn original_cartridge_executes_z80_graphics_and_keyboard_tone() {
    let mut m = Machine::default();
    m.load_cartridge(&firmware::test_cartridge()).unwrap();
    for _ in 0..4 {
        m.frame();
    }
    assert_eq!(m.hardware.slots, 0xd4);
    assert!(m.cpu.pc >= 0x4010 && m.cpu.pc < 0x4800);
    let text = String::from_utf8_lossy(&m.hardware.vdp.vram[0x1800..0x1b00]);
    assert!(text.contains("MSX-RUST TEST CARTRIDGE"));
    assert_eq!(m.hardware.psg.registers[8], 0);
    assert!(m
        .hardware
        .vdp
        .pixels
        .chunks(4)
        .any(|p| p[..3] == PALETTE[10]));
    m.hardware.key(8, 0, true);
    m.frame();
    assert_eq!(m.hardware.psg.registers[8], 15);
    assert!(m.hardware.psg.samples.iter().any(|s| s.abs() > 0.1));
    m.hardware.key(8, 0, false);
    m.frame();
    assert_eq!(m.hardware.psg.registers[8], 0);
    m.unload_cartridge();
    m.frame();
    assert!(m.basic.console.transcript.contains("WEAVE BASIC"));
}
#[test]
fn supplied_system_rom_uses_hardware_and_disables_native_services() {
    let mut m = Machine::default();
    let mut rom = vec![0; 32768];
    rom[..13].copy_from_slice(&[
        0x3e, 0x4a, 0xd3, 0x99, 0x3e, 0x87, 0xd3, 0x99, 0xaf, 0xd3, 0xf1, 0x76, 0,
    ]);
    assert!(m.load_bios(&rom[..16000]).is_err());
    m.load_bios(&rom).unwrap();
    m.frame();
    assert_eq!(m.hardware.vdp.registers[7], 0x4a);
    assert!(m.cpu.halted);
    assert!(m.basic.console.transcript.is_empty());
    m.use_builtin();
    m.frame();
    assert!(m.basic.console.transcript.contains("READY"));
}
#[test]
fn cartridge_sizes_mapping_and_invalid_load_preserves_machine() {
    for size in [8192, 16384, 32768, 49152] {
        let mut m = Machine::default();
        let mut rom = vec![0x5a; size];
        let header = if size == 49152 { 0x4000 } else { 0 };
        rom[header..header + 2].copy_from_slice(b"AB");
        m.load_cartridge(&rom).unwrap();
        assert_eq!(m.hardware.read(0x4000), b'A');
        assert_eq!(m.hardware.read(0xbfff), 0x5a);
        m.hardware.write(0x4000, 0);
        assert_eq!(m.hardware.read(0x4000), b'A');
        assert!(m.load_cartridge(&[0; 5]).is_err());
        assert_eq!(m.hardware.cartridge.len(), size);
    }
}
#[test]
fn transparent_sprite_pixels_collide_without_hiding_a_lower_priority_sprite() {
    let mut v = Vdp::default();
    v.registers[..8].copy_from_slice(&[0, 0x40, 6, 0x80, 0, 0x36, 7, 4]);
    v.vram[0x3800] = 0x80;
    v.vram[0x1b00..0x1b0c].copy_from_slice(&[9, 20, 0, 0, 9, 20, 0, 10, 208, 0, 0, 0]);
    v.render();
    assert_eq!(v.status & 0x20, 0x20);
    assert_eq!(pixel(&v, 20, 10), &PALETTE[10]);
}
#[test]
fn page_two_cartridge_header_boots_without_a_page_one_header() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = Machine::default();
        m.set_model(model);
        let mut rom = vec![0; 32768];
        rom[0x4000..0x4004].copy_from_slice(&[b'A', b'B', 0x10, 0x80]);
        rom[0x4010..0x4015].copy_from_slice(&[0x3e, 0x5a, 0x32, 0x00, 0xc0]);
        rom[0x4015..0x4017].copy_from_slice(&[0xf3, 0x76]);
        m.load_cartridge(&rom).unwrap();
        for _ in 0..600 {
            m.frame();
            if m.hardware.ram[0xc000] == 0x5a {
                break;
            }
        }
        assert!(m.cpu.halted);
        assert_eq!(m.hardware.ram[0xc000], 0x5a);
    }
}
#[test]
fn cartridge_init_can_return_to_native_basic() {
    let mut m = Machine::default();
    let mut rom = vec![0; 16384];
    rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    rom[0x10..0x16].copy_from_slice(&[0x3e, 0x5a, 0x32, 0x00, 0xc0, 0xc9]);
    m.load_cartridge(&rom).unwrap();
    m.frame();
    assert_eq!(m.hardware.ram[0xc000], 0x5a);
    enter(&mut m, "PRINT 42");
    assert!(m.basic.console.transcript.ends_with("42\nREADY\n"));
}
