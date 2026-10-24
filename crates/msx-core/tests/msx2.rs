use msx_core::{cpu::Bus, firmware, vdp::Vdp, Firmware, Machine, Model};
fn reg(v: &mut Vdp, r: u8, b: u8) {
    v.control(b);
    v.control(0x80 | r);
}
fn word(v: &mut Vdp, r: u8, n: u16) {
    reg(v, r, n as u8);
    reg(v, r + 1, (n >> 8) as u8);
}
fn status(v: &mut Vdp, r: u8) -> u8 {
    reg(v, 15, r);
    v.read_status()
}
fn address(v: &mut Vdp, a: usize, write: bool) {
    reg(v, 14, (a >> 14) as u8);
    v.control(a as u8);
    v.control(((a >> 8) & 63) as u8 | if write { 64 } else { 0 });
}
fn upload(v: &mut Vdp, a: usize, bytes: &[u8]) {
    address(v, a, true);
    for &byte in bytes {
        v.write(byte);
    }
}
fn bitmap(mode: u8) -> Vdp {
    let mut v = Vdp::new(true);
    reg(&mut v, 0, mode);
    reg(&mut v, 1, 0x40);
    reg(&mut v, 8, 2);
    v
}
fn rgb(v: &Vdp, x: usize, y: usize) -> [u8; 3] {
    let i = (y * v.width() + x) * 4;
    v.pixels[i..i + 3].try_into().unwrap()
}
fn setup(v: &mut Vdp, dx: u16, dy: u16, nx: u16, ny: u16, color: u8, cmd: u8) {
    word(v, 36, dx);
    word(v, 38, dy);
    word(v, 40, nx);
    word(v, 42, ny);
    reg(v, 44, color);
    reg(v, 46, cmd);
}
fn finish(v: &mut Vdp) {
    for _ in 0..131072 {
        if status(v, 2) & 1 == 0 {
            return;
        }
        v.tick(8);
    }
    panic!("VDP command did not finish within the bounded work budget");
}
#[test]
fn extended_ports_address_wrap_palette_and_indirect() {
    let mut v = bitmap(6);
    address(&mut v, 0x1ffff, true);
    v.write(0x12);
    v.write(0x34);
    assert_eq!(v.vram[0x1ffff], 0x12);
    assert_eq!(v.vram[0], 0x34);
    address(&mut v, 0x1ffff, false);
    assert_eq!(v.read(), 0x12);
    assert_eq!(v.read(), 0x34);
    reg(&mut v, 16, 15);
    v.write_palette(0x71);
    v.write_palette(3);
    assert_eq!(v.palette[15], [255, 109, 36]);
    assert_eq!(v.registers[16], 0);
    reg(&mut v, 17, 18);
    v.write_indirect(0x12);
    v.write_indirect(0x34);
    assert_eq!(&v.registers[18..20], &[0x12, 0x34]);
    assert_eq!(v.registers[17], 20);
    reg(&mut v, 17, 0x80 | 23);
    v.write_indirect(7);
    v.write_indirect(8);
    assert_eq!(v.registers[23], 8);
    assert_eq!(v.registers[17], 0x97);
    reg(&mut v, 17, 17);
    v.write_indirect(0xff);
    assert_eq!(v.registers[17], 18);
    // V9938 compatibility modes still wrap within the selected 16 KiB bank.
    reg(&mut v, 0, 0);
    address(&mut v, 0x7fff, true);
    v.write(0x55);
    v.write(0x66);
    assert_eq!(v.vram[0x4000], 0x66);
    assert_eq!(v.vram[0x8000], 0);
    let mut high = bitmap(0);
    reg(&mut high, 8, 0);
    reg(&mut high, 5, 0x80);
    reg(&mut high, 11, 2);
    reg(&mut high, 6, 48);
    high.vram[0x14000..0x14005].copy_from_slice(&[9, 20, 0, 15, 208]);
    high.vram[0x18000] = 0x80;
    high.render();
    assert_eq!(rgb(&high, 20, 10), high.palette[15]);
    let mut old = Vdp::default();
    old.control(7);
    old.control(0x8e);
    assert_eq!(old.registers[6], 7);
    assert_eq!(old.vram.len(), 16384);
}
#[test]
fn bitmap_modes_pages_scroll_and_212_lines() {
    for (mode, width, byte, expected) in [(6, 256, 0x23, 2), (8, 512, 0xb1, 2), (10, 512, 0x23, 2)]
    {
        let mut v = bitmap(mode);
        reg(&mut v, 9, 0x80);
        reg(&mut v, 2, 0x3f);
        let base = if mode == 10 { 0x10000 } else { 0x8000 };
        upload(&mut v, base, &[byte]);
        v.render();
        assert_eq!(v.width(), width);
        assert_eq!(v.height(), 212);
        assert_eq!(rgb(&v, 0, 0), v.palette[expected]);
        upload(&mut v, base + if mode == 10 { 256 } else { 128 }, &[byte]);
        reg(&mut v, 23, 1);
        v.render();
        assert_eq!(rgb(&v, 0, 0), v.palette[expected]);
    }
    let mut v = bitmap(14);
    reg(&mut v, 9, 0x80);
    reg(&mut v, 2, 0x3f);
    upload(&mut v, 0x10000, &[0xe3]);
    upload(&mut v, 0x10000 + 211 * 256 + 255, &[0x1c]);
    v.render();
    assert_eq!(rgb(&v, 0, 0), [0, 255, 255]);
    assert_eq!(rgb(&v, 255, 211), [255, 0, 0]);
    reg(&mut v, 9, 0x8c);
    v.vram[0] = 0x1c;
    v.tick(59_850);
    v.tick(59_850);
    assert_eq!(v.frames, 2);
    assert_eq!(v.height(), 424);
    assert_eq!(rgb(&v, 0, 0), [255, 0, 0]);
    assert_eq!(rgb(&v, 0, 1), [0, 255, 255]);
}
#[test]
fn text80_screen4_transparency_and_adjust() {
    let mut v = bitmap(4);
    reg(&mut v, 1, 0x50);
    reg(&mut v, 2, 4);
    reg(&mut v, 4, 0);
    reg(&mut v, 7, 0xf2);
    v.vram[0x1000] = 1;
    v.vram[0x1000 + 79] = 1;
    v.vram[8] = 0x80;
    v.render();
    assert_eq!(v.width(), 512);
    assert_eq!(rgb(&v, 16, 0), v.palette[15]);
    assert_eq!(rgb(&v, 490, 0), v.palette[15]);
    reg(&mut v, 0, 4);
    reg(&mut v, 1, 0x40);
    reg(&mut v, 2, 6);
    reg(&mut v, 3, 0xff);
    reg(&mut v, 4, 3);
    v.vram[0x1800] = 1;
    v.vram[8] = 0x80;
    v.vram[0x2008] = 0x32;
    v.render();
    assert_eq!(rgb(&v, 0, 0), v.palette[3]);
    assert_eq!(rgb(&v, 1, 0), v.palette[2]);
    reg(&mut v, 18, 0x0f);
    v.render();
    assert_eq!(rgb(&v, 1, 0), v.palette[3]);
    reg(&mut v, 18, 0);
    v.vram[0x2008] = 0x02;
    v.palette[0] = [1, 2, 3];
    v.render();
    assert_eq!(rgb(&v, 0, 0), v.palette[2]);
    reg(&mut v, 8, 0x22);
    v.render();
    assert_eq!(rgb(&v, 0, 0), [1, 2, 3]);
}
#[test]
fn text80_blink_table_ignores_low_r3_bits() {
    for high in [0, 5] {
        for low in 0..8 {
            let mut v = bitmap(4);
            reg(&mut v, 1, 0x50);
            reg(&mut v, 2, 4);
            reg(&mut v, 3, 0x28 | low);
            reg(&mut v, 9, 0x80);
            reg(&mut v, 10, high);
            reg(&mut v, 7, 0xf2);
            reg(&mut v, 12, 0x64);
            reg(&mut v, 13, 0x10);
            v.vram[0x1000] = 1;
            v.vram[0x1001] = 1;
            v.vram[0x1000 + 26 * 80 + 79] = 1;
            v.vram[8..16].fill(0x80);
            // TEXT2 aligns the blink table to 512 bytes, including R10's high bits.
            let base = ((high as usize) << 14) | 0x0a00;
            v.vram[base] = 0x80;
            v.vram[base + 269] = 1;
            v.render();
            assert_eq!(rgb(&v, 16, 0), v.palette[6]);
            assert_eq!(rgb(&v, 17, 0), v.palette[4]);
            assert_eq!(rgb(&v, 22, 0), v.palette[15]);
            assert_eq!(rgb(&v, 490, 208), v.palette[6]);
            reg(&mut v, 13, 1);
            v.render();
            assert_eq!(rgb(&v, 16, 0), v.palette[15]);
            assert_eq!(rgb(&v, 17, 0), v.palette[2]);
        }
    }
}
#[test]
fn pal_to_ntsc_switch_is_fixed_at_next_field_boundary() {
    for switch_line in [100, 261, 262, 300, 312] {
        let mut v = bitmap(6);
        reg(&mut v, 1, 0x60);
        reg(&mut v, 9, 0x82);
        v.tick(228 * switch_line);
        assert_eq!(v.frames, 0);
        reg(&mut v, 9, 0x80);
        // Selected fixation policy: finish this PAL field before adopting NTSC.
        assert_eq!(v.frame_cycles(), 71_364);
        let remaining = 313 - switch_line;
        v.tick(228 * (remaining - 1));
        assert_eq!(v.frames, 0);
        v.tick(228);
        assert_eq!(v.frames, 1);
        assert_eq!(v.line, 0);
        status(&mut v, 0);
        assert!(!v.irq());
        v.vram[0] = 0x30;
        v.tick(228);
        assert_eq!(rgb(&v, 0, 0), v.palette[3]);
        v.tick(228 * 211);
        assert!(v.irq());
        assert_eq!(status(&mut v, 0) & 0x80, 0x80);
        v.tick(228 * 50);
        assert_eq!(v.frames, 2);
        assert_eq!(v.line, 0);
    }
}
#[test]
fn vblank_line_irq_status_selection_and_pal() {
    let mut v = bitmap(6);
    reg(&mut v, 9, 0x82);
    reg(&mut v, 0, 0x16);
    reg(&mut v, 19, 10);
    v.tick(228 * 10 + 180);
    assert!(v.irq());
    assert_eq!(status(&mut v, 1) & 1, 1);
    assert!(!v.irq());
    reg(&mut v, 1, 0x60);
    v.tick(228 * 202 - 180);
    assert!(v.irq());
    assert_eq!(status(&mut v, 2) & 0x40, 0x40);
    assert!(v.irq());
    assert_eq!(status(&mut v, 0) & 0x80, 0x80);
    assert!(!v.irq());
    v.tick(228 * 101);
    assert_eq!(v.frames, 1);
    assert_eq!(v.frame_cycles(), 228 * 313);
    // Line zero matches at its right border, once per frame, not at line 256.
    let mut v = bitmap(0x16);
    reg(&mut v, 19, 0);
    v.tick(180);
    assert_eq!(status(&mut v, 1) & 1, 1);
    v.tick(228 * 256 - 180);
    assert!(!v.irq());
    v.tick(228 * 6 + 179);
    assert!(!v.irq());
    v.tick(1);
    assert!(v.irq());
    assert_eq!(status(&mut v, 1) & 1, 1);
}
#[test]
fn commands_are_timed_fill_copy_logic_line_search_point_and_stop() {
    let mut v = bitmap(6);
    setup(&mut v, 0, 0, 4, 1, 3, 0x80);
    assert_eq!(status(&mut v, 2) & 1, 1);
    assert_eq!(v.vram[0], 0);
    v.tick(23);
    assert_eq!(v.vram[0], 0);
    v.tick(1);
    assert_eq!(v.vram[0], 0x30);
    finish(&mut v);
    assert_eq!(status(&mut v, 2) & 1, 0);
    assert_eq!(&v.vram[..2], &[0x33, 0x33]);
    word(&mut v, 32, 0);
    word(&mut v, 34, 0);
    setup(&mut v, 2, 1, 4, 1, 0, 0x90);
    finish(&mut v);
    assert_eq!(&v.vram[129..131], &[0x33, 0x33]);
    setup(&mut v, 0, 0, 1, 1, 5, 0x53);
    finish(&mut v);
    assert_eq!(v.vram[0], 0x63);
    setup(&mut v, 0, 0, 1, 1, 0, 0x58);
    finish(&mut v);
    assert_eq!(v.vram[0], 0x63);
    setup(&mut v, 0, 2, 3, 0, 9, 0x70);
    finish(&mut v);
    assert_eq!(&v.vram[256..258], &[0x99, 0x99]);
    word(&mut v, 32, 0);
    word(&mut v, 34, 2);
    reg(&mut v, 44, 9);
    reg(&mut v, 46, 0x60);
    finish(&mut v);
    assert_eq!(status(&mut v, 2) & 0x10, 0x10);
    assert_eq!(status(&mut v, 8), 0);
    reg(&mut v, 46, 0x40);
    finish(&mut v);
    assert_eq!(status(&mut v, 7), 9);
    setup(&mut v, 0, 0, 256, 212, 0, 0xc0);
    v.tick(8);
    reg(&mut v, 46, 0);
    assert_eq!(status(&mut v, 2) & 1, 0);
    let saved = v.vram.clone();
    v.tick(1000);
    assert_eq!(v.vram, saved);
}
#[test]
fn cpu_transfers_handshake_and_byte_commands_reverse_overlap() {
    let mut v = bitmap(6);
    setup(&mut v, 0, 0, 3, 1, 2, 0xb0);
    v.tick(8);
    assert_eq!(status(&mut v, 2) & 0x81, 0x81);
    assert_eq!(v.vram[0], 0x20);
    v.tick(100);
    assert_eq!(v.vram[0], 0x20);
    reg(&mut v, 44, 3);
    v.tick(8);
    assert_eq!(v.vram[0], 0x23);
    reg(&mut v, 44, 4);
    v.tick(8);
    assert_eq!(status(&mut v, 2) & 1, 0);
    assert_eq!(v.vram[1], 0x40);
    word(&mut v, 32, 0);
    word(&mut v, 34, 0);
    word(&mut v, 40, 3);
    word(&mut v, 42, 1);
    reg(&mut v, 46, 0xa0);
    for value in [2, 3, 4] {
        v.tick(8);
        assert_eq!(status(&mut v, 2) & 0x81, 0x81);
        assert_eq!(status(&mut v, 7), value);
    }
    assert_eq!(status(&mut v, 2) & 1, 0);
    setup(&mut v, 0, 1, 4, 1, 0xab, 0xf0);
    v.tick(8);
    reg(&mut v, 44, 0xcd);
    v.tick(8);
    assert_eq!(&v.vram[128..130], &[0xab, 0xcd]);
    v.vram[..3].copy_from_slice(&[0x12, 0x34, 0x56]);
    word(&mut v, 32, 4);
    word(&mut v, 34, 0);
    reg(&mut v, 45, 4);
    setup(&mut v, 6, 0, 6, 1, 0, 0xd0);
    finish(&mut v);
    assert_eq!(&v.vram[..4], &[0x12, 0x12, 0x34, 0x56]);
}
#[test]
fn sprite2_per_line_color_combine_collision_and_ninth() {
    let mut v = bitmap(6);
    reg(&mut v, 8, 0);
    reg(&mut v, 5, 0x77);
    reg(&mut v, 6, 15);
    v.vram[0x7800..0x7808].fill(0x80);
    for n in 0..9 {
        v.vram[0x3a00 + n * 4..0x3a04 + n * 4].copy_from_slice(&[9, 20, 0, 0]);
        v.vram[0x3800 + n * 16..0x3810 + n * 16].fill(2);
    }
    v.vram[0x3a24] = 216;
    v.vram[0x3810] = 0x44;
    v.vram[0x7808] = 0xc0;
    v.vram[0x3a06] = 1;
    v.render();
    assert_eq!(rgb(&v, 20, 10), v.palette[6]);
    assert_eq!(rgb(&v, 21, 10), v.palette[4]);
    assert_eq!(v.status & 0x7f, 0x68);
    assert_eq!(status(&mut v, 3), 32);
    assert_eq!(status(&mut v, 5), 18);
    reg(&mut v, 8, 2);
    v.render();
    assert_eq!(rgb(&v, 20, 10), v.palette[0]);
}
#[test]
fn expanded_slot_register_subrom_protection_and_reset() {
    let mut m = Machine::default();
    m.set_model(Model::Msx2);
    m.use_diagnostic().unwrap();
    let h = &mut m.hardware;
    h.output(0xa8, 0xff);
    h.write(0xffff, 0xff);
    h.write(0, 0x55);
    assert_eq!(h.read(0), 0x55);
    assert_eq!(h.read(0xffff), 0);
    h.write(0xffff, 0xfc);
    assert_eq!(h.read(0), b'C');
    h.write(0, 0);
    assert_eq!(h.read(0), b'C');
    assert_eq!(h.read(0xffff), 3);
    h.output(0xa8, 0);
    h.write(0xffff, 0x22);
    assert_eq!(h.secondary[3], 0xfc);
    assert_eq!(h.read(0xffff), 255);
    m.reset();
    assert_eq!(m.hardware.secondary[3], 0xff);
    assert_eq!(m.hardware.slots, 0xf0);
    assert_eq!(m.hardware.read(0xc000), 0);
    assert!(m.load_bios(&vec![0; 32768]).is_err());
    assert!(m.load_msx2_firmware(&[0; 5], &[0; 4]).is_err());
    assert_eq!(m.hardware.firmware, Firmware::Diagnostic);
    m.set_model(Model::Msx1);
    assert_eq!(m.hardware.vdp.vram.len(), 16384);
    m.hardware.output(0xa8, 255);
    m.hardware.write(0xffff, 0x22);
    assert_eq!(m.hardware.read(0xffff), 0x22);
}
#[test]
fn original_msx2_cartridge_boots_and_uses_input_audio_and_extended_hardware() {
    fn command_edges(v: &Vdp, screen8: bool) {
        let ink = if screen8 { [0, 255, 0] } else { [0, 255, 255] };
        let white = [255, 255, 255];
        for (x, y, color) in [
            (20, 120, white),
            (23, 120, white),
            (30, 120, ink),
            (30, 123, ink),
            (30, 124, white),
            (33, 124, white),
            (30, 125, [255, 0, 0]),
        ] {
            assert_eq!(rgb(v, x, y), color, "CPU command probe ({x},{y})");
        }
        // Independent instruction budgets: direct R44 at 40 T-states,
        // indirect at 60; one work unit every eight T-states.
        for x in 63..=192 {
            let color = match x {
                63 | 192 => [255, 0, 0],
                69 | 70 => ink,
                _ => white,
            };
            assert_eq!(rgb(v, x, 140), color, "CPU live-color probe ({x},140)");
        }
    }
    let mut m = Machine::default();
    m.set_model(Model::Msx2);
    m.use_diagnostic().unwrap();
    m.load_cartridge(&firmware::msx2_cartridge()).unwrap();
    for _ in 0..15 {
        m.frame();
    }
    assert_eq!(m.hardware.read(0xc100), 0x2a);
    assert_eq!(m.hardware.vdp.mode(), 7);
    assert_eq!(m.hardware.vdp.width(), 512);
    assert_eq!(m.hardware.vdp.height(), 212);
    assert_eq!(rgb(&m.hardware.vdp, 0, 0), [255, 0, 0]);
    assert_eq!(rgb(&m.hardware.vdp, 10, 10), [0, 255, 255]);
    command_edges(&m.hardware.vdp, false);
    assert_eq!(status(&mut m.hardware.vdp, 2) & 0x81, 0);
    assert!(m.basic.console.transcript.is_empty());
    m.hardware.key(8, 0, true);
    m.frame();
    assert_eq!(m.hardware.psg.registers[8], 15);
    assert!(m.hardware.psg.samples.iter().any(|v| v.abs() > 0.05));
    m.hardware.key(8, 0, false);
    m.hardware.key(1, 0, true);
    m.frame();
    m.hardware.key(1, 0, false);
    for _ in 0..15 {
        m.frame();
    }
    assert_eq!(m.hardware.vdp.mode(), 8);
    assert_eq!(rgb(&m.hardware.vdp, 0, 211), [255, 0, 0]);
    assert_eq!(rgb(&m.hardware.vdp, 10, 10), [0, 255, 0]);
    command_edges(&m.hardware.vdp, true);
    assert_eq!(status(&mut m.hardware.vdp, 2) & 0x81, 0);
    assert_eq!(rgb(&m.hardware.vdp, 35, 32), [255, 255, 255]);
    m.reset();
    for _ in 0..15 {
        m.frame();
    }
    assert_eq!(m.hardware.vdp.mode(), 7);
    command_edges(&m.hardware.vdp, false);
}
#[test]
fn external_main_and_subrom_execute_real_slot_interactions() {
    let mut m = Machine::default();
    m.set_model(Model::Msx2);
    // Original main ROM remaps slots while executing in page 1. It calls the
    // supplied sub-ROM and then a cartridge, with all services implemented in Z80.
    let mut main = vec![0; 32768];
    main[..3].copy_from_slice(&[0xc3, 0x00, 0x40]);
    main[0x2d] = 1;
    let code = [
        0xf3, 0x3e, 0xf0, 0xd3, 0xa8, 0x3e, 0xfc, 0x32, 0xff, 0xff, 0x31, 0x00, 0xf0, 0x3e, 0xf3,
        0xd3, 0xa8, 0xcd, 0x00, 0x01, 0x3e, 0xf0, 0xd3, 0xa8, 0xc3, 0x00, 0x02,
    ];
    main[0x4000..0x4000 + code.len()].copy_from_slice(&code);
    main[0x200..0x20a].copy_from_slice(&[0x3e, 0xd4, 0xd3, 0xa8, 0x2a, 0x02, 0x40, 0xe9, 0, 0]);
    let mut cart = vec![0; 16384];
    cart[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
    cart[0x10..0x18].copy_from_slice(&[0x3e, 0x37, 0x32, 0x01, 0xc1, 0xd3, 0xf1, 0x76]);
    m.load_cartridge(&cart).unwrap();
    m.load_msx2_firmware(&main, &firmware::msx2_subrom())
        .unwrap();
    m.frame();
    assert!(m.cpu.halted);
    assert_eq!(
        (0xc100..0xc102)
            .map(|a| m.hardware.read(a))
            .collect::<Vec<_>>(),
        &[0x2a, 0x37]
    );
    assert!(m.basic.console.transcript.is_empty());
    let bios = m.hardware.bios.clone();
    assert!(m.load_msx2_firmware(&main, &[]).is_err());
    assert_eq!(m.hardware.bios, bios);
    assert!(m.cpu.halted);
    m.reset();
    m.frame();
    assert_eq!(m.hardware.read(0xc101), 0x37);
    m.use_diagnostic().unwrap();
    assert_eq!(m.hardware.firmware, Firmware::Diagnostic);
    assert!(m.hardware.cartridge.is_empty());
}
#[test]
fn rtc_banked_storage_clock_stop_rollover_and_reset_retention() {
    let mut m = Machine::default();
    m.set_model(Model::Msx2);
    let h = &mut m.hardware;
    h.output(0xb4, 13);
    h.output(0xb5, 10);
    h.output(0xb4, 4);
    h.output(0xb5, 0xa7);
    assert_eq!(h.input(0xb5), 0xf7);
    h.output(0xb4, 13);
    h.output(0xb5, 8);
    h.output(0xb4, 4);
    assert_eq!(h.input(0xb5), 0xf0);
    h.rtc.tick(msx_core::psg::CPU_HZ);
    h.output(0xb4, 0);
    assert_eq!(h.input(0xb5), 0xf1);
    h.output(0xb4, 13);
    h.output(0xb5, 0);
    h.rtc.tick(msx_core::psg::CPU_HZ);
    h.output(0xb4, 0);
    assert_eq!(h.input(0xb5), 0xf1);
    m.reset();
    m.hardware.output(0xb4, 13);
    m.hardware.output(0xb5, 10);
    m.hardware.output(0xb4, 4);
    assert_eq!(m.hardware.input(0xb5), 0xf7);
}

#[test]
fn logical_operations_obey_pixel_width_and_transparent_source() {
    for (mode, mask) in [(6, 15), (8, 3), (10, 15), (14, 255)] {
        for op in [0, 1, 2, 3, 4, 8, 9, 10, 11, 12] {
            for src in [0, 2, 15, 255] {
                let mut v = bitmap(mode);
                setup(&mut v, 1, 500, 1, 1, 5, 0x50);
                finish(&mut v);
                setup(&mut v, 1, 500, 1, 1, src, 0x50 | op);
                finish(&mut v);
                word(&mut v, 32, 1);
                word(&mut v, 34, 500);
                reg(&mut v, 46, 0x40);
                finish(&mut v);
                let src = src & mask;
                let dest = 5 & mask;
                let expected = if op & 8 != 0 && src == 0 {
                    dest
                } else {
                    (match op & 7 {
                        0 => src,
                        1 => src & dest,
                        2 => src | dest,
                        3 => src ^ dest,
                        4 => !src,
                        _ => unreachable!(),
                    }) & mask
                };
                assert_eq!(
                    status(&mut v, 7),
                    expected,
                    "mode {mode}, op {op}, src {src}"
                );
            }
        }
    }
}
#[test]
fn byte_fill_y_copy_clipping_and_absent_expansion_vram() {
    let mut v = bitmap(6);
    setup(&mut v, 254, 1023, 8, 2, 0xab, 0xc0);
    finish(&mut v);
    assert_eq!(v.vram[0x1ffff], 0xab);
    assert_eq!(v.vram[127], 0xab); // Y wraps; X clips
    v.vram[..3].copy_from_slice(&[1, 2, 3]);
    word(&mut v, 34, 0);
    reg(&mut v, 45, 0);
    setup(&mut v, 252, 1, 1, 1, 0, 0xe0);
    finish(&mut v);
    // YMMM ignores NX and copies from DX to the screen edge on each row.
    assert_eq!(&v.vram[254..256], &v.vram[126..128]);
    let saved = v.vram.clone();
    reg(&mut v, 45, 0x20);
    setup(&mut v, 0, 0, 16, 1, 255, 0xc0);
    finish(&mut v);
    assert_eq!(v.vram, saved);
    reg(&mut v, 45, 0x40);
    address(&mut v, 0, true);
    v.write(0x66);
    assert_eq!(v.vram[0], 1);
    address(&mut v, 0, false);
    assert_eq!(v.read(), 255);
}
#[test]
fn alternate_pages_fixed_sprite_colors_and_border_adjust_are_not_stale() {
    let mut v = bitmap(14);
    reg(&mut v, 2, 0x3f);
    upload(&mut v, 0, &[0x1c]);
    upload(&mut v, 0x10000, &[0xe0]);
    reg(&mut v, 13, 0x11);
    v.render();
    assert_eq!(rgb(&v, 0, 0), [255, 0, 0]);
    v.tick(228 * 262 * 10);
    v.render();
    assert_eq!(rgb(&v, 0, 0), [0, 255, 0]);
    reg(&mut v, 13, 0);
    reg(&mut v, 9, 4);
    v.tick(228 * 262);
    v.render();
    assert_eq!(rgb(&v, 0, 0), [0, 255, 0]);
    v.tick(228 * 262);
    v.render();
    assert_eq!(rgb(&v, 0, 0), [255, 0, 0]);
    // Blue DAC values in direct color mode use four of the eight levels.
    reg(&mut v, 9, 0);
    v.tick(59_736); // R9 page selection becomes active at the next boundary.
    upload(&mut v, 0x10000, &[1]);
    upload(&mut v, 0x10001, &[2]);
    v.render();
    assert_eq!(rgb(&v, 0, 0), [0, 0, 72]);
    assert_eq!(rgb(&v, 1, 0), [0, 0, 145]);
    reg(&mut v, 5, 0x77);
    reg(&mut v, 6, 15);
    reg(&mut v, 8, 0);
    upload(&mut v, 0x3a00, &[9, 20, 0, 0, 216]);
    upload(&mut v, 0x3800, &[15; 16]);
    upload(&mut v, 0x7800, &[0x80; 8]);
    v.palette[15] = [1, 2, 3];
    v.render();
    assert_eq!(rgb(&v, 20, 10), [255, 255, 255]);
    reg(&mut v, 18, 0x10);
    reg(&mut v, 7, 0x1c);
    v.render();
    assert_eq!(rgb(&v, 0, 191), [255, 0, 0]);
    let mut narrow = bitmap(8);
    reg(&mut narrow, 7, 0x0b);
    reg(&mut narrow, 1, 0);
    narrow.render();
    assert_eq!(rgb(&narrow, 0, 0), narrow.palette[2]);
    assert_eq!(rgb(&narrow, 1, 0), narrow.palette[3]);
}

#[test]
fn rtc_calendar_rolls_through_leap_day_and_midnight() {
    let mut rtc = msx_core::rtc::Rtc::default();
    for (r, v) in [
        (0, 9),
        (1, 5),
        (2, 9),
        (3, 5),
        (4, 3),
        (5, 2),
        (7, 8),
        (8, 2),
        (9, 2),
        (10, 0),
    ] {
        rtc.selected = r;
        rtc.write(v);
    }
    rtc.tick(msx_core::psg::CPU_HZ);
    for (r, v) in [(0, 0), (1, 0), (4, 0), (5, 0), (7, 9), (8, 2), (9, 2)] {
        rtc.selected = r;
        assert_eq!(rtc.read() & 15, v);
    }
    for (r, v) in [(0, 9), (1, 5), (2, 9), (3, 5), (4, 3), (5, 2)] {
        rtc.selected = r;
        rtc.write(v);
    }
    rtc.tick(msx_core::psg::CPU_HZ);
    for (r, v) in [(7, 1), (8, 0), (9, 3)] {
        rtc.selected = r;
        assert_eq!(rtc.read() & 15, v);
    }
}

// Fixed physical addresses are deliberately independent of the production decoder.
#[test]
fn interleaved_ports_reinterpret_storage_and_preserve_logical_counter_and_buffer() {
    let pairs = [
        (0x00000, 0x00000),
        (0x00001, 0x10000),
        (0x00002, 0x00001),
        (0x03fff, 0x11fff),
        (0x04000, 0x02000),
        (0x0ffff, 0x17fff),
        (0x10000, 0x08000),
        (0x10001, 0x18000),
        (0x1fffe, 0x0ffff),
        (0x1ffff, 0x1ffff),
    ];
    for mode in [10, 14] {
        let mut v = bitmap(6); // Linear SCREEN 5.
        for (i, &(_, physical)) in pairs.iter().enumerate() {
            upload(&mut v, physical, &[0x30 + i as u8]);
        }
        let saved = v.vram.clone();
        reg(&mut v, 0, mode);
        for (i, &(logical, _)) in pairs.iter().enumerate() {
            address(&mut v, logical, false);
            assert_eq!(v.read(), 0x30 + i as u8);
        }
        assert_eq!(v.vram, saved);
        for (i, &(logical, physical)) in pairs.iter().enumerate() {
            upload(&mut v, logical, &[0x60 + i as u8]);
            assert_eq!(v.vram[physical], 0x60 + i as u8);
        }
        let saved = v.vram.clone();
        reg(&mut v, 0, if mode == 10 { 14 } else { 10 });
        for (i, &(logical, _)) in pairs.iter().enumerate() {
            address(&mut v, logical, false);
            assert_eq!(v.read(), 0x60 + i as u8);
        }
        reg(&mut v, 0, 6);
        for (i, &(_, physical)) in pairs.iter().enumerate() {
            address(&mut v, physical, false);
            assert_eq!(v.read(), 0x60 + i as u8);
        }
        assert_eq!(v.vram, saved);
        reg(&mut v, 0, mode);
        for (start, next, high) in [(0x3fff, 0x4000, 1), (0xffff, 0x10000, 4), (0x1ffff, 0, 0)] {
            upload(&mut v, start, &[0xa1, 0xb2, 0xc3]);
            address(&mut v, start, false); // Prefetch advances logical counter.
            assert_eq!(v.registers[14], high);
            assert_eq!(v.read(), 0xa1);
            assert_eq!(v.read(), 0xb2);
            assert_eq!(v.read(), 0xc3);
            address(&mut v, next, false);
            assert_eq!(v.read(), 0xb2);
        }
        // An ordinary register-mode switch keeps the prefetched byte, but the
        // next read-ahead uses the new decoding of the next logical address.
        reg(&mut v, 0, 6);
        upload(&mut v, 0x10000, &[0x91]);
        upload(&mut v, 1, &[0x42, 0x53]);
        reg(&mut v, 0, mode);
        address(&mut v, 1, false);
        reg(&mut v, 0, 6);
        assert_eq!(v.read(), 0x91);
        assert_eq!(v.read(), 0x53);
        address(&mut v, 1, false);
        reg(&mut v, 0, mode);
        assert_eq!(v.read(), 0x42);
        assert_eq!(v.read(), 0x42); // Logical 2 maps to physical 1.
                                    // Status/data accesses must still cancel a half-written control latch.
        v.control(0xaa);
        v.read_status();
        address(&mut v, 2, true);
        v.write(0x67);
        assert_eq!(v.vram[1], 0x67);
    }
    let mut m = Machine::default();
    m.set_model(Model::Msx2);
    m.hardware.vdp.vram.fill(0x55);
    m.reset();
    assert!(m.hardware.vdp.vram.iter().all(|&b| b == 0));
}

#[test]
fn linear_uploads_render_both_interleaved_pages_and_scroll_without_moving_bytes() {
    for mode in [10, 14] {
        let mut v = bitmap(6);
        // Physical bank pairs for logical 00000/1, 10000/1 and 10100/1.
        for (a, bytes) in [
            (0, [0x23, 0x45]),
            (0x8000, [0x67, 0x89]),
            (0x8080, [0xab, 0xcd]),
        ] {
            upload(&mut v, a, &bytes[..1]);
            upload(&mut v, a + 0x10000, &bytes[1..]);
        }
        let saved = v.vram.clone();
        reg(&mut v, 0, mode);
        for (page, scroll, values) in [
            (0, 0, [0x23, 0x45]),
            (0x3f, 0, [0x67, 0x89]),
            (0x3f, 1, [0xab, 0xcd]),
        ] {
            reg(&mut v, 2, page);
            reg(&mut v, 23, scroll);
            v.render();
            if mode == 10 {
                for (x, c) in [
                    values[0] >> 4,
                    values[0] & 15,
                    values[1] >> 4,
                    values[1] & 15,
                ]
                .iter()
                .enumerate()
                {
                    assert_eq!(rgb(&v, x, 0), v.palette[*c as usize]);
                }
            } else {
                // Independently specified GRB values, including the blue DAC.
                let expected = match values[0] {
                    0x23 => [[0, 36, 255], [36, 72, 72]],
                    0x67 => [[36, 109, 255], [72, 145, 72]],
                    _ => [[72, 182, 255], [109, 218, 72]],
                };
                assert_eq!(rgb(&v, 0, 0), expected[0]);
                assert_eq!(rgb(&v, 1, 0), expected[1]);
            }
        }
        assert_eq!(v.vram, saved);
    }
}

#[test]
fn sprite_tables_uploaded_through_ports_work_in_both_interleaved_modes_and_cross_mode() {
    for mode in [10, 14] {
        for cross_mode in [false, true] {
            let mut v = bitmap(if cross_mode { 6 } else { mode });
            reg(&mut v, 5, 0x77);
            reg(&mut v, 6, 15);
            reg(&mut v, 8, 0);
            // Fixed bank pairs for attribute/color/pattern tables. Populate the
            // same logical table using linear ports when cross_mode is true.
            let mut put = |a: usize, b: u8| {
                let physical = match a {
                    0x3800..=0x388f => 0x1c00 + (a - 0x3800) / 2 + (a % 2) * 0x10000,
                    0x3a00..=0x3a24 => 0x1d00 + (a - 0x3a00) / 2 + (a % 2) * 0x10000,
                    0x7800..=0x7808 => 0x3c00 + (a - 0x7800) / 2 + (a % 2) * 0x10000,
                    _ => unreachable!(),
                };
                upload(&mut v, if cross_mode { physical } else { a }, &[b]);
            };
            for row in 0..8 {
                put(0x7800 + row, 0x80);
            }
            for n in 0..9 {
                for (i, b) in [9, 20, 0, 0].into_iter().enumerate() {
                    put(0x3a00 + n * 4 + i, b);
                }
                for row in 0..16 {
                    put(0x3800 + n * 16 + row, 2);
                }
            }
            put(0x3a24, 216);
            put(0x3810, 0x44);
            put(0x7808, 0xc0);
            put(0x3a06, 1);
            reg(&mut v, 0, mode);
            v.render();
            let expected = if mode == 10 {
                v.palette[6]
            } else {
                [109, 109, 0]
            };
            assert_eq!(rgb(&v, if mode == 10 { 40 } else { 20 }, 10), expected);
            assert_eq!(v.status & 0x7f, 0x68); // Collision plus ninth-sprite overflow.
            assert_eq!(status(&mut v, 3), 32);
            assert_eq!(status(&mut v, 5), 18);
        }
    }
}

fn read_bytes(v: &mut Vdp, a: usize, count: usize) -> Vec<u8> {
    address(v, a, false);
    (0..count).map(|_| v.read()).collect()
}

#[test]
fn interleaved_commands_share_ports_banks_pages_and_transfer_handshakes() {
    for (mode, unit, packed) in [(10, 2, 0x33), (14, 1, 3)] {
        let mut v = bitmap(mode);
        // Logical fill crosses the display-page boundary in Y.
        setup(&mut v, 0, 255, 4 * unit, 2, 3, 0x80);
        finish(&mut v);
        assert_eq!(read_bytes(&mut v, 0x10000, 4), [packed; 4]);
        assert_eq!(v.vram[0x8000], packed);
        assert_eq!(v.vram[0x18000], packed);
        reg(&mut v, 2, 0x3f);
        v.render();
        assert_eq!(
            rgb(&v, 0, 0),
            if mode == 10 {
                v.palette[3]
            } else {
                [0, 0, 255]
            }
        );
        // Pixel write/XOR and POINT use the same data as CPU ports.
        setup(&mut v, unit, 256, 1, 1, 5, 0x53);
        finish(&mut v);
        word(&mut v, 32, unit);
        word(&mut v, 34, 256);
        reg(&mut v, 46, 0x40);
        finish(&mut v);
        assert_eq!(status(&mut v, 7), 6);
        setup(&mut v, 0, 2, 3, 0, 9, 0x70);
        finish(&mut v);
        assert_eq!(
            read_bytes(&mut v, 0x200, 2),
            if mode == 10 { [0x99, 0x99] } else { [9, 9] }
        );
        word(&mut v, 32, 0);
        word(&mut v, 34, 2);
        reg(&mut v, 44, 9);
        reg(&mut v, 46, 0x60);
        finish(&mut v);
        assert_eq!(status(&mut v, 2) & 0x10, 0x10);
        assert_eq!(status(&mut v, 8), 0);
        // Logical copy back across the page boundary.
        word(&mut v, 32, 0);
        word(&mut v, 34, 256);
        setup(&mut v, 0, 0, 4 * unit, 1, 0, 0x90);
        finish(&mut v);
        assert_eq!(read_bytes(&mut v, 0, 4), read_bytes(&mut v, 0x10000, 4));
        // Reverse logical overlap crosses both nibble and physical bank boundaries.
        upload(&mut v, 0, &[0x12, 0x34, 0x56, 0x78]);
        word(&mut v, 32, 3 * unit - 1);
        word(&mut v, 34, 0);
        reg(&mut v, 45, 4);
        setup(&mut v, 4 * unit - 1, 0, 3 * unit, 1, 0, 0x90);
        finish(&mut v);
        assert_eq!(read_bytes(&mut v, 0, 4), [0x12, 0x12, 0x34, 0x56]);
        reg(&mut v, 45, 0);
        // CPU logical transfer, readback and TR stalls, alternating physical banks.
        setup(&mut v, 0, 1, 4 * unit, 1, 2, 0xb0);
        for i in 0..4 * unit {
            if i != 0 {
                reg(&mut v, 44, (i + 2) as u8);
            }
            v.tick(8);
            if i + 1 < 4 * unit {
                assert_eq!(status(&mut v, 2) & 0x81, 0x81);
                let saved = v.vram.clone();
                v.tick(32);
                assert_eq!(v.vram, saved);
            }
        }
        assert_eq!(status(&mut v, 2) & 0x81, 0);
        word(&mut v, 32, 0);
        word(&mut v, 34, 1);
        word(&mut v, 40, 4 * unit);
        word(&mut v, 42, 1);
        reg(&mut v, 46, 0xa0);
        for i in 0..4 * unit {
            v.tick(8);
            assert_eq!(status(&mut v, 2) & 0x81, 0x81);
            assert_eq!(status(&mut v, 7), (i + 2) as u8);
        }
        assert_eq!(status(&mut v, 2) & 0x81, 0);
        // Byte fill and CPU byte transfer at page 1.
        setup(&mut v, 0, 256, 4 * unit, 1, 0xab, 0xc0);
        finish(&mut v);
        assert_eq!(read_bytes(&mut v, 0x10000, 4), [0xab; 4]);
        setup(&mut v, 0, 256, 4 * unit, 1, 0x12, 0xf0);
        for value in [0x12, 0x34, 0x56, 0x78] {
            if value != 0x12 {
                reg(&mut v, 44, value);
            }
            v.tick(8);
        }
        assert_eq!(status(&mut v, 2) & 0x81, 0);
        assert_eq!(v.vram[0x8000..0x8002], [0x12, 0x56]);
        assert_eq!(v.vram[0x18000..0x18002], [0x34, 0x78]);
        v.render();
        assert_eq!(
            rgb(&v, unit as usize, 0),
            if mode == 10 {
                v.palette[3]
            } else {
                [182, 36, 0]
            }
        );
        // Reverse overlapping HMMM must read each source before overwriting it.
        word(&mut v, 32, 2 * unit);
        word(&mut v, 34, 256);
        reg(&mut v, 45, 4);
        setup(&mut v, 3 * unit, 256, 3 * unit, 1, 0, 0xd0);
        finish(&mut v);
        assert_eq!(read_bytes(&mut v, 0x10000, 4), [0x12, 0x12, 0x34, 0x56]);
        // YMMM copies through the same byte path, with reverse X and Y.
        word(&mut v, 34, 256);
        reg(&mut v, 45, 12);
        setup(&mut v, 3 * unit, 255, 1, 1, 0, 0xe0);
        finish(&mut v);
        assert_eq!(read_bytes(&mut v, 0xff00, 4), [0x12, 0x12, 0x34, 0x56]);
        // Inspect the command results in a linear mode, through ports.
        reg(&mut v, 0, 6);
        assert_eq!(read_bytes(&mut v, 0x8000, 2), [0x12, 0x34]);
        assert_eq!(read_bytes(&mut v, 0x18000, 2), [0x12, 0x56]);
        reg(&mut v, 0, mode);
        let saved = v.vram.clone();
        reg(&mut v, 45, 0x20);
        setup(&mut v, 0, 0, 4 * unit, 1, 255, 0xc0);
        finish(&mut v);
        assert_eq!(v.vram, saved);
        reg(&mut v, 45, 0x10);
        word(&mut v, 32, 0);
        word(&mut v, 34, 0);
        setup(&mut v, 0, 0, unit, 1, 0, 0xd0);
        finish(&mut v);
        assert_eq!(read_bytes(&mut v, 0, 1), [255]);
        // Byte fill wraps after logical 1ffffh, while X clips at the right edge.
        reg(&mut v, 45, 0);
        setup(
            &mut v,
            if mode == 10 { 512 - unit } else { 256 - unit },
            511,
            4 * unit,
            2,
            0xab,
            0xc0,
        );
        finish(&mut v);
        assert_eq!(v.vram[0x1ffff], 0xab);
        assert_eq!(v.vram[0x1007f], 0xab); // Logical 000ffh on wrapped row.
        let saved = v.vram.clone();
        reg(&mut v, 45, 0x40);
        address(&mut v, 0, true);
        v.write(0x99);
        assert_eq!(read_bytes(&mut v, 0, 1), [255]);
        assert_eq!(v.vram, saved);
        reg(&mut v, 45, 0);
        setup(&mut v, 0, 0, 4 * unit, 100, 0, 0xc0);
        v.tick(8);
        reg(&mut v, 46, 0);
        let stopped = v.vram.clone();
        v.tick(1000);
        assert_eq!(v.vram, stopped);
        assert_eq!(status(&mut v, 2) & 0x81, 0);
    }
}

#[test]
fn live_color_stop_replacement_and_reset_discard_command_state() {
    let mut m = Machine::default();
    m.set_model(Model::Msx2);
    m.use_diagnostic().unwrap();
    let v = &mut m.hardware.vdp;
    reg(v, 0, 6);
    setup(v, 0, 0, 16, 2, 1, 0x80);
    v.tick(37); // Two blank-display writes completed; next read is pending.
    reg(v, 17, 0x80 | 44);
    v.write_indirect(2);
    reg(v, 46, 0); // STOP, then another color write cannot resume work.
    let stopped = v.vram.clone();
    assert_eq!(&stopped[..2], &[0x11, 0]);
    reg(v, 44, 3);
    v.tick(80);
    assert_eq!(status(v, 2) & 0x81, 0);
    assert_eq!(v.vram, stopped);

    setup(v, 0, 2, 16, 2, 5, 0xb0); // Pending initial LMMC data.
    v.tick(7);
    reg(v, 44, 6); // Not accepted transfer data, but visible to replacement.
    word(v, 36, 4);
    reg(v, 46, 0x50); // Replacement PSET gets a fresh work budget and color.
    v.tick(1);
    assert_eq!(v.vram, stopped);
    assert_eq!(status(v, 2) & 0x81, 1);
    v.tick(7);
    assert_eq!(v.vram[258], 0x60);
    assert_eq!(status(v, 2) & 0x81, 0);

    setup(v, 0, 4, 16, 2, 1, 0x80);
    v.tick(7);
    v.write_indirect(2);
    m.reset();
    let v = &mut m.hardware.vdp;
    assert_eq!(status(v, 2) & 0x81, 0);
    assert_eq!(v.registers[44], 0);
    let reset = v.vram.clone();
    reg(v, 44, 3);
    v.tick(80);
    assert_eq!(v.vram, reset);
    assert_eq!(status(v, 2) & 0x81, 0);
}
