//! Original, independently calculated command edge/color vectors (Issues #45/#51).
use msx_core::vdp::Vdp;

fn reg(v: &mut Vdp, r: u8, value: u8) {
    v.control(value);
    v.control(0x80 | r);
}
fn word(v: &mut Vdp, r: u8, value: u16) {
    reg(v, r, value as u8);
    reg(v, r + 1, (value >> 8) as u8);
}
fn status(v: &mut Vdp, r: u8) -> u8 {
    reg(v, 15, r);
    v.read_status()
}
fn bitmap(mode: u8) -> Vdp {
    let mut v = Vdp::new(true);
    reg(&mut v, 0, mode);
    reg(&mut v, 1, 0x40);
    reg(&mut v, 8, 2);
    v
}
// Independent physical byte calculation, including the 512-row alias in 7/8.
fn location(mode: u8, x: usize, y: usize) -> (usize, u8, u8) {
    let (width, bits) = match mode {
        6 => (256, 4),
        8 => (512, 2),
        10 => (512, 4),
        14 => (256, 8),
        _ => unreachable!(),
    };
    let packed = 8 / bits;
    let logical = ((y & 1023) * (width / packed) + x / packed) & 0x1ffff;
    let physical = if mode >= 10 {
        (logical >> 1) | ((logical & 1) << 16)
    } else {
        logical
    };
    (
        physical,
        ((packed - 1 - x % packed) * bits) as u8,
        ((1u16 << bits) - 1) as u8,
    )
}
fn get(bytes: &[u8], mode: u8, x: usize, y: usize) -> u8 {
    let (a, shift, mask) = location(mode, x, y);
    (bytes[a] >> shift) & mask
}
fn put(bytes: &mut [u8], mode: u8, x: usize, y: usize, color: u8) {
    let (a, shift, mask) = location(mode, x, y);
    bytes[a] = (bytes[a] & !(mask << shift)) | ((color & mask) << shift);
}
fn same_vram(actual: &[u8], expected: &[u8]) {
    if let Some(a) = actual.iter().zip(expected).position(|(a, b)| a != b) {
        panic!(
            "physical VRAM {a:05x}: got {:02x}, expected {:02x}",
            actual[a], expected[a]
        );
    }
}
fn set_word(registers: &mut [u8], r: usize, value: i32) {
    registers[r] = value as u8;
    registers[r + 1] = ((value >> 8) & 3) as u8;
}

fn rectangles(code: u8) {
    for mode in [6, 8, 10, 14] {
        let width = if matches!(mode, 8 | 10) { 512 } else { 256 };
        let packed = match mode {
            8 => 4,
            14 => 1,
            _ => 2,
        };
        let unit = if code >= 12 { packed } else { 1 };
        let source = matches!(code, 9 | 10 | 13 | 14);
        let destination = code != 10;
        // Reverse: source first, destination first, both together, exact height,
        // excessive height and NY=0. Forward: ten-bit wrap and physical alias.
        for (sy, dy, ny, reverse) in [
            (0, 2, 3, true),
            (2, 0, 3, true),
            (0, 0, 3, true),
            (1, 1, 2, true),
            (1, 1, 5, true),
            (1, 1, 0, true),
            (1, 3, 5, true),
            (3, 1, 5, true),
            (511, 511, 3, false),
            (1023, 1023, 3, false),
            (0, 0, 0, false),
        ] {
            for reverse_x in [false, true] {
                let mut v = bitmap(mode);
                for (i, byte) in v.vram.iter_mut().enumerate() {
                    *byte = ((i * 17) ^ (i >> 8) ^ 0x5a) as u8;
                }
                // Unaligned byte coordinates must align before clipping.
                let odd = if code >= 12 { unit - 1 } else { 0 };
                let dx = if reverse_x { unit } else { width - 2 * unit };
                let sx = if reverse_x {
                    2 * unit
                } else {
                    width - 3 * unit
                };
                word(&mut v, 32, (sx + odd) as u16);
                word(&mut v, 34, sy);
                word(&mut v, 36, (dx + odd) as u16);
                word(&mut v, 38, dy);
                word(&mut v, 40, (4 * unit) as u16);
                word(&mut v, 42, ny);
                reg(&mut v, 44, 3);
                reg(
                    &mut v,
                    45,
                    if reverse { 8 } else { 0 } | if reverse_x { 4 } else { 0 },
                );
                reg(&mut v, 46, code << 4);
                let mut registers = v.registers;
                let mut expected = v.vram.clone();
                let requested = if ny == 0 { 1024 } else { ny as usize };
                let mut rows = requested;
                if reverse {
                    if source {
                        rows = rows.min(sy as usize + 1);
                    }
                    if destination {
                        rows = rows.min(dy as usize + 1);
                    }
                }
                let sx = if code == 14 { dx } else { sx };
                let edge = |x| {
                    if reverse_x {
                        x / unit + 1
                    } else {
                        (width - x) / unit
                    }
                };
                let mut columns = if code == 14 { edge(dx) } else { 4 };
                if source {
                    columns = columns.min(edge(sx));
                }
                if destination {
                    columns = columns.min(edge(dx));
                }
                let total = rows * columns;
                let mut final_data = 3;
                for i in 0..total {
                    let row = i / columns;
                    let col = i % columns;
                    let x = |start| {
                        if reverse_x {
                            start - col * unit
                        } else {
                            start + col * unit
                        }
                    };
                    let y = |start: u16| {
                        if reverse {
                            start as usize - row
                        } else {
                            start as usize + row
                        }
                    };
                    let value = if source {
                        if code >= 12 {
                            expected[location(mode, x(sx), y(sy)).0]
                        } else {
                            get(&expected, mode, x(sx), y(sy))
                        }
                    } else if matches!(code, 11 | 15) && i > 0 {
                        (i as u8).wrapping_add(3)
                    } else {
                        3
                    };
                    if matches!(code, 10 | 11 | 15) {
                        final_data = value;
                    }
                    if matches!(code, 11 | 15) && i > 0 {
                        assert_eq!(status(&mut v, 2) & 0x81, 0x81);
                        reg(&mut v, 44, value);
                    }
                    if destination {
                        if code >= 12 {
                            expected[location(mode, x(dx), y(dy)).0] = value;
                        } else {
                            put(&mut expected, mode, x(dx), y(dy), value);
                        }
                    }
                    v.tick(8);
                    if code == 10 {
                        assert_eq!(status(&mut v, 2) & 0x81, 0x81);
                        // Leave the last read pending as well: neither CE nor
                        // coordinates may complete until the CPU consumes it.
                        let coordinates = v.registers[32..44].to_vec();
                        v.tick(80);
                        assert_eq!(&v.registers[32..44], coordinates);
                        assert_eq!(status(&mut v, 2) & 0x81, 0x81);
                        assert_eq!(status(&mut v, 7), value);
                    } else if matches!(code, 11 | 15) && i + 1 < total {
                        let saved = v.vram.clone();
                        v.tick(80);
                        assert_eq!(status(&mut v, 2) & 0x81, 0x81);
                        same_vram(&v.vram, &saved);
                    }
                }
                assert_eq!(
                    status(&mut v, 2) & 0x81,
                    0,
                    "mode={mode} code={code} sy={sy} dy={dy} ny={ny}"
                );
                same_vram(&v.vram, &expected);
                let delta = rows as i32 * if reverse { -1 } else { 1 };
                if source {
                    set_word(&mut registers, 34, sy as i32 + delta);
                }
                if destination {
                    set_word(&mut registers, 38, dy as i32 + delta);
                }
                set_word(&mut registers, 42, (requested - rows) as i32);
                registers[46] = 0;
                registers[15] = 2;
                registers[44] = final_data;
                assert_eq!(v.registers, registers);
                v.tick(80);
                reg(&mut v, 44, 0xff); // A final transfer cannot resurrect CE.
                assert_eq!(status(&mut v, 2) & 0x81, 0);
                same_vram(&v.vram, &expected);
            }
        }
    }
}
macro_rules! rectangle_test {
    ($name:ident, $code:expr) => {
        #[test]
        fn $name() {
            rectangles($code);
        }
    };
}
rectangle_test!(reverse_top_lmmv, 8);
rectangle_test!(reverse_top_lmmm, 9);
rectangle_test!(reverse_top_lmcm, 10);
rectangle_test!(reverse_top_lmmc, 11);
rectangle_test!(reverse_top_hmmv, 12);
rectangle_test!(reverse_top_hmmm, 13);
rectangle_test!(reverse_top_ymmm, 14);
rectangle_test!(reverse_top_hmmc, 15);

#[test]
fn reverse_top_preserves_wrapped_sentinel() {
    for mode in [6, 8, 10, 14] {
        let mut v = bitmap(mode);
        v.vram.fill(0x55);
        let mut expected = v.vram.clone();
        put(&mut expected, mode, 0, 0, 3);
        word(&mut v, 40, 1);
        word(&mut v, 42, 3);
        reg(&mut v, 44, 3);
        reg(&mut v, 45, 8);
        reg(&mut v, 46, 0x80);
        v.tick(24); // Enough for the defective engine to corrupt rows 1023/1022.
        same_vram(&v.vram, &expected);
        assert_eq!(&v.registers[38..40], &[255, 3]);
        assert_eq!(&v.registers[42..44], &[2, 0]);
        assert_eq!(status(&mut v, 2) & 0x81, 0);
    }
}

#[test]
fn line_half_step_ties_advance_minor_axis() {
    let mut v = bitmap(6);
    word(&mut v, 40, 4);
    word(&mut v, 42, 2);
    reg(&mut v, 44, 3);
    reg(&mut v, 46, 0x70);
    v.tick(40);
    let mut expected = vec![0; v.vram.len()];
    for (x, y) in [(0, 0), (1, 1), (2, 1), (3, 2), (4, 2)] {
        put(&mut expected, 6, x, y, 3);
    }
    same_vram(&v.vram, &expected);
    assert_eq!(&v.registers[38..40], &[2, 0]);
}

#[test]
fn line_pixels_endpoints_and_unchanged_registers() {
    // Coordinates are explicit offsets, not generated with the engine's loop.
    // NX=4,NY=2 distinguishes the half-step tie break from the old NX/2 bias.
    type LineVector = (u16, u16, &'static [(usize, usize)]);
    let vectors: &[LineVector] = &[
        (0, 0, &[(0, 0)]),
        (3, 0, &[(0, 0), (1, 0), (2, 0), (3, 0)]),
        (4, 2, &[(0, 0), (1, 1), (2, 1), (3, 2), (4, 2)]),
        (3, 3, &[(0, 0), (1, 1), (2, 2), (3, 3)]),
        (5, 2, &[(0, 0), (1, 0), (2, 1), (3, 1), (4, 2), (5, 2)]),
    ];
    for mode in [6, 8, 10, 14] {
        let width = if matches!(mode, 8 | 10) { 512 } else { 256 };
        for &(major, minor, offsets) in vectors {
            for y_major in [false, true] {
                for reverse_x in [false, true] {
                    for reverse_y in [false, true] {
                        // Ordinary endpoints, the top edge and horizontal edge.
                        for (start_x, start_y) in [
                            (10, 10),
                            (10, 0),
                            (10, 1),
                            (0, 10),
                            (width - 1, 10),
                            (10, 1023),
                            (10, 511),
                        ] {
                            let mut v = bitmap(mode);
                            let mut expected = v.vram.clone();
                            let arg = u8::from(y_major)
                                | if reverse_x { 4 } else { 0 }
                                | if reverse_y { 8 } else { 0 };
                            word(&mut v, 36, start_x as u16);
                            word(&mut v, 38, start_y as u16);
                            word(&mut v, 40, major);
                            word(&mut v, 42, minor);
                            word(&mut v, 32, 123);
                            word(&mut v, 34, 456);
                            reg(&mut v, 44, 3);
                            reg(&mut v, 45, arg);
                            reg(&mut v, 46, 0x70);
                            let mut registers = v.registers;
                            let sign_x = if reverse_x { -1 } else { 1 };
                            let sign_y = if reverse_y { -1 } else { 1 };
                            let mut final_y = start_y;
                            let mut count = 0;
                            for (i, &(a, b)) in offsets.iter().enumerate() {
                                let (ox, oy) = if y_major { (b, a) } else { (a, b) };
                                let x = start_x + ox as i32 * sign_x;
                                let y = start_y + oy as i32 * sign_y;
                                if x < 0 || x >= width || y < 0 {
                                    break;
                                }
                                put(&mut expected, mode, x as usize, y as usize, 3);
                                count += 1;
                                final_y = y;
                                if y_major {
                                    final_y += sign_y;
                                    if final_y < 0 {
                                        break;
                                    }
                                } else {
                                    // Final X step is tested before a minor Y step.
                                    let next_x = x + sign_x;
                                    if i == major as usize || next_x < 0 || next_x >= width {
                                        break;
                                    }
                                    let next_y = offsets[i + 1].1 as i32 * sign_y + start_y;
                                    final_y = next_y;
                                    if final_y < 0 {
                                        break;
                                    }
                                }
                            }
                            v.tick(8 * count);
                            assert_eq!(status(&mut v, 2) & 1, 0, "mode={mode} arg={arg} NX={major} NY={minor} start=({start_x},{start_y})");
                            same_vram(&v.vram, &expected);
                            set_word(&mut registers, 38, final_y);
                            registers[46] = 0;
                            registers[15] = 2;
                            assert_eq!(v.registers, registers, "arg={arg} NX={major} NY={minor}");
                        }
                    }
                }
            }
        }
    }
}

fn color_write(v: &mut Vdp, indirect: bool, value: u8) {
    if indirect {
        reg(v, 17, 0x80 | 44); // R17 fixed at R44, through port 9Bh.
        v.write_indirect(value);
        assert_eq!(v.registers[17], 0x80 | 44);
    } else {
        reg(v, 44, value); // Two control-port writes, through port 99h.
    }
    assert_eq!(v.registers[44], value);
}

#[test]
fn live_line_fixed_screen5_and_tick_partitions() {
    for indirect in [false, true] {
        for partitioned in [false, true] {
            let mut v = bitmap(6);
            word(&mut v, 40, 7);
            reg(&mut v, 44, 1);
            reg(&mut v, 46, 0x70);
            for cycles in if partitioned {
                &[3, 5, 7, 9][..]
            } else {
                &[24][..]
            } {
                v.tick(*cycles);
            }
            assert_eq!(status(&mut v, 2) & 0x81, 1);
            color_write(&mut v, indirect, 2);
            assert_eq!(status(&mut v, 2) & 0x81, 1);
            for cycles in if partitioned {
                &[1, 6, 9, 24][..]
            } else {
                &[40][..]
            } {
                v.tick(*cycles);
            }
            let mut expected = vec![0; v.vram.len()];
            expected[..4].copy_from_slice(&[0x11, 0x12, 0x22, 0x22]);
            same_vram(&v.vram, &expected);
            assert_eq!(status(&mut v, 2) & 0x81, 0);
            assert_eq!(&v.registers[36..44], &[0, 0, 0, 0, 7, 0, 0, 0]);
        }
    }
}

fn live_drawing(code: u8) {
    for mode in [6, 8, 10, 14] {
        for indirect in [false, true] {
            for partitioned in [false, true] {
                // IMP, XOR, transparent IMP and transparent XOR. HMMV ignores LOP.
                for op in if code == 12 {
                    &[0][..]
                } else {
                    &[0, 3, 8, 11][..]
                } {
                    let mut v = bitmap(mode);
                    v.vram.fill(0x5a);
                    let unit = if code == 12 {
                        match mode {
                            8 => 4,
                            14 => 1,
                            _ => 2,
                        }
                    } else {
                        1
                    };
                    let start_x = unit;
                    // Byte commands discard the sub-byte X bits.
                    word(&mut v, 36, (start_x + unit - 1) as u16);
                    word(&mut v, 38, 3);
                    word(&mut v, 40, if code == 7 { 7 } else { 4 * unit as u16 });
                    word(&mut v, 42, if code == 7 { 0 } else { 2 });
                    reg(&mut v, 44, 0xf1);
                    reg(&mut v, 46, (code << 4) | op);
                    let coordinates = v.registers[32..44].to_vec();
                    let mut expected = v.vram.clone();
                    for cycles in if partitioned {
                        &[7, 1, 3, 8][..]
                    } else {
                        &[19][..]
                    } {
                        v.tick(*cycles);
                    }
                    color_write(&mut v, indirect, 0);
                    assert_eq!(status(&mut v, 2) & 0x81, 1);
                    assert_eq!(&v.registers[32..44], coordinates);
                    for cycles in if partitioned {
                        &[2, 5, 3][..]
                    } else {
                        &[10][..]
                    } {
                        v.tick(*cycles);
                    }
                    // The 5 accumulated T-states must survive both writes.
                    color_write(&mut v, !indirect, 0xe3);
                    assert_eq!(status(&mut v, 2) & 0x81, 1);
                    assert_eq!(&v.registers[32..44], coordinates);
                    for cycles in if partitioned {
                        &[3, 8, 1, 23][..]
                    } else {
                        &[35][..]
                    } {
                        v.tick(*cycles);
                    }
                    for i in 0..8 {
                        let (x, y) = if code == 7 {
                            (start_x + i, 3)
                        } else {
                            (start_x + (i % 4) * unit, 3 + i / 4)
                        };
                        let color = match i {
                            0 | 1 => 0xf1,
                            2 => 0,
                            _ => 0xe3,
                        };
                        if code == 12 {
                            // Full packed byte, including its high bits.
                            expected[location(mode, x, y).0] = color;
                        } else {
                            let src = color & location(mode, x, y).2;
                            let old = get(&expected, mode, x, y);
                            let result = match (*op, src) {
                                (8 | 11, 0) => old,
                                (0 | 8, _) => src,
                                (3 | 11, _) => old ^ src,
                                _ => unreachable!(),
                            };
                            put(&mut expected, mode, x, y, result);
                        }
                    }
                    same_vram(&v.vram, &expected);
                    assert_eq!(
                        status(&mut v, 2) & 0x81,
                        0,
                        "mode={mode} code={code} op={op}"
                    );
                    assert_eq!(v.registers[46], *op);
                    let mut completed = coordinates;
                    if code != 7 {
                        completed[6] = 5; // DY advances by two completed rows.
                        completed[10] = 0; // NY exhausted.
                    }
                    assert_eq!(&v.registers[32..44], completed);
                    color_write(&mut v, indirect, 2);
                    v.tick(80);
                    same_vram(&v.vram, &expected);
                    assert_eq!(status(&mut v, 2) & 0x81, 0);
                    // A subsequent PSET picks up the latest visible R44.
                    word(&mut v, 36, 30);
                    word(&mut v, 38, 10);
                    reg(&mut v, 46, 0x50);
                    v.tick(8);
                    put(&mut expected, mode, 30, 10, 2);
                    same_vram(&v.vram, &expected);
                    assert_eq!(status(&mut v, 2) & 0x81, 0);
                }
            }
        }
    }
}
#[test]
fn live_line_all_bitmap_modes_logic_and_neighbors() {
    live_drawing(7);
}
#[test]
fn live_lmmv_all_bitmap_modes_logic_and_neighbors() {
    live_drawing(8);
}
#[test]
fn live_hmmv_all_bitmap_modes_full_bytes() {
    live_drawing(12);
}

#[test]
fn live_pset_before_first_unit() {
    for mode in [6, 8, 10, 14] {
        for indirect in [false, true] {
            let mut v = bitmap(mode);
            v.vram.fill(0x55);
            let mut expected = v.vram.clone();
            word(&mut v, 36, 1);
            reg(&mut v, 44, 1);
            reg(&mut v, 46, 0x50);
            v.tick(7);
            color_write(&mut v, indirect, 0xfe);
            same_vram(&v.vram, &expected);
            assert_eq!(status(&mut v, 2) & 0x81, 1);
            v.tick(1);
            put(&mut expected, mode, 1, 0, 0xfe);
            same_vram(&v.vram, &expected);
            assert_eq!(status(&mut v, 2) & 0x81, 0);
        }
    }
}

#[test]
fn live_srch_before_next_comparison() {
    for mode in [6, 8, 10, 14] {
        for indirect in [false, true] {
            let mut v = bitmap(mode);
            for eq in [0, 2] {
                // Keep colors in range; retain the existing EQ contract.
                put(&mut v.vram, mode, 0, 1, 1);
                put(&mut v.vram, mode, 1, 1, 2);
                put(&mut v.vram, mode, 2, 1, 3);
                word(&mut v, 32, 0);
                word(&mut v, 34, 1);
                reg(&mut v, 45, eq);
                reg(&mut v, 44, if eq == 0 { 3 } else { 1 });
                reg(&mut v, 46, 0x60);
                let saved = v.vram.clone();
                v.tick(11); // First comparison fails; retain three T-states.
                assert_eq!(status(&mut v, 2) & 0x91, 1);
                color_write(&mut v, indirect, if eq == 0 { 2 } else { 3 });
                v.tick(5); // The next comparison succeeds at X=1.
                assert_eq!(status(&mut v, 2) & 0x91, 0x10);
                assert_eq!(status(&mut v, 8), 1);
                assert_eq!(status(&mut v, 9) & 1, 0);
                color_write(&mut v, !indirect, 0);
                v.tick(80);
                same_vram(&v.vram, &saved);
                assert_eq!(status(&mut v, 2) & 0x91, 0x10);
            }
        }
    }
}

#[test]
fn color_writes_preserve_accepted_transfer_data_and_final_completion() {
    for mode in [6, 8, 10, 14] {
        for code in [11, 15] {
            for indirect in [false, true] {
                let mut v = bitmap(mode);
                let unit = if code == 11 {
                    1
                } else {
                    match mode {
                        8 => 4,
                        14 => 1,
                        _ => 2,
                    }
                };
                word(&mut v, 40, 3 * unit as u16);
                word(&mut v, 42, 1);
                reg(&mut v, 44, 0xab);
                reg(&mut v, 46, code << 4);
                let mut expected = v.vram.clone();
                // Writes with TR clear must preserve initial/accepted data.
                for (i, accepted) in [0xab, 0x5c, 0xe3].into_iter().enumerate() {
                    if i != 0 {
                        assert_eq!(status(&mut v, 2) & 0x81, 0x81);
                        let saved = v.vram.clone();
                        v.tick(80); // Deliberate CPU transfer stall.
                        same_vram(&v.vram, &saved);
                        color_write(&mut v, indirect, accepted);
                    }
                    assert_eq!(status(&mut v, 2) & 0x81, 1);
                    v.tick(7);
                    color_write(&mut v, !indirect, 0x99);
                    v.tick(1);
                    if code == 15 {
                        expected[location(mode, i * unit, 0).0] = accepted;
                    } else {
                        put(&mut expected, mode, i, 0, accepted);
                    }
                    same_vram(&v.vram, &expected);
                }
                assert_eq!(status(&mut v, 2) & 0x81, 0);
                color_write(&mut v, indirect, 0xff);
                v.tick(80);
                same_vram(&v.vram, &expected);
                assert_eq!(status(&mut v, 2) & 0x81, 0);
            }
        }
    }
}
