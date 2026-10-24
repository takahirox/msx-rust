//! Original timing vectors for Issue #77. Literal /RAS event times are calculated
//! from the published bitmap service grids and memory-cycle thresholds; they do
//! not call or import the emulator's timing helpers. Times are master clocks
//! after the modeled R46 callback (local phase + 202 = upstream line phase).
use msx_core::{cpu::Bus, firmware, vdp::Vdp, Machine, Model};

fn reg(v: &mut Vdp, r: u8, value: u8) {
    v.control(value);
    v.control(0x80 | r);
}
fn word(v: &mut Vdp, r: u8, value: u16) {
    reg(v, r, value as u8);
    reg(v, r + 1, (value >> 8) as u8);
}
fn ce(v: &mut Vdp) -> u8 {
    reg(v, 15, 2);
    v.read_status() & 1
}
fn location(mode: u8, x: usize, y: usize) -> (usize, u8, u8) {
    let (stride, packed, bits) = match mode {
        6 => (128, 2, 4),
        8 => (128, 4, 2),
        10 => (256, 2, 4),
        14 => (256, 1, 8),
        _ => unreachable!(),
    };
    let logical = (y * stride + x / packed) & 0x1ffff;
    let address = if mode >= 10 {
        (logical >> 1) | ((logical & 1) << 16)
    } else {
        logical
    };
    (
        address,
        ((packed - 1 - x % packed) * bits) as u8,
        (255 >> (8 - bits)) as u8,
    )
}
fn put(bytes: &mut [u8], mode: u8, x: usize, y: usize, color: u8) {
    let (a, shift, mask) = location(mode, x, y);
    bytes[a] = (bytes[a] & !(mask << shift)) | ((color & mask) << shift);
}
fn configured(mode: u8, schedule: u8) -> Vdp {
    let mut v = Vdp::new(true);
    reg(&mut v, 0, mode);
    reg(&mut v, 1, if schedule == 0 { 0 } else { 0x40 });
    reg(&mut v, 8, if schedule == 2 { 0 } else { 2 });
    // An empty sprite table must still reserve sprite-fetch slots.
    reg(&mut v, 5, 0x36);
    v.vram.fill(0x55);
    // Mode-2 attributes start at logical 1a00h with R5=36h.
    v.vram[if mode >= 10 { 0x1a00 >> 1 } else { 0x1a00 }] = 216;
    for y in 4..7 {
        for x in 8..11 {
            put(&mut v.vram, mode, x, y, 2);
        }
    }
    v
}
fn start(v: &mut Vdp, code: u8, nx: u16, ny: u16) {
    for (r, value) in [(32, 8), (34, 4), (36, 0), (38, 0), (40, nx), (42, ny)] {
        word(v, r, value);
    }
    reg(v, 44, 3);
    reg(v, 46, code << 4);
}

// (command, service grid, start CPU T, NX, NY, write master clocks).
type TimingVector = (u8, u8, u32, u16, u16, [u32; 3]);
const VECTORS: &[TimingVector] = &[
    // These starts specifically distinguish padding / packed-slot / sprite addends.
    (8, 2, 15, 3, 1, [184, 312, 440]),
    (9, 1, 8, 3, 1, [156, 284, 412]),
    (8, 0, 121, 1, 3, [116, 284, 448]),
    (8, 1, 117, 1, 3, [142, 362, 526]),
    (8, 2, 111, 1, 3, [152, 396, 662]),
    (9, 0, 136, 3, 1, [122, 274, 398]),
    (9, 1, 131, 3, 1, [154, 302, 426]),
    (9, 2, 83, 1, 3, [256, 564, 856]),
    (8, 0, 0, 3, 1, [114, 218, 314]),
    (8, 0, 0, 1, 3, [114, 274, 434]),
    (8, 0, 180, 3, 1, [118, 250, 346]),
    (8, 0, 180, 1, 3, [118, 274, 434]),
    (8, 0, 220, 3, 1, [114, 210, 306]),
    (8, 0, 220, 1, 3, [114, 274, 434]),
    (8, 1, 0, 3, 1, [140, 268, 396]),
    (8, 1, 0, 1, 3, [140, 300, 460]),
    (8, 1, 180, 3, 1, [116, 248, 364]),
    (8, 1, 180, 1, 3, [116, 274, 460]),
    (8, 1, 220, 3, 1, [124, 252, 380]),
    (8, 1, 220, 1, 3, [124, 316, 476]),
    (8, 2, 0, 3, 1, [146, 274, 402]),
    (8, 2, 0, 1, 3, [146, 370, 562]),
    (8, 2, 180, 3, 1, [178, 306, 434]),
    (8, 2, 180, 1, 3, [178, 402, 594]),
    (8, 2, 220, 3, 1, [162, 322, 450]),
    (8, 2, 220, 1, 3, [162, 354, 578]),
    (9, 0, 0, 3, 1, [122, 242, 370]),
    (9, 0, 0, 1, 3, [122, 306, 498]),
    (9, 0, 180, 3, 1, [126, 274, 402]),
    (9, 0, 180, 1, 3, [126, 314, 506]),
    (9, 0, 220, 3, 1, [122, 242, 362]),
    (9, 0, 220, 1, 3, [122, 306, 490]),
    (9, 1, 0, 3, 1, [140, 268, 396]),
    (9, 1, 0, 1, 3, [140, 332, 524]),
    (9, 1, 180, 3, 1, [124, 274, 428]),
    (9, 1, 180, 1, 3, [124, 332, 524]),
    (9, 1, 220, 3, 1, [156, 284, 412]),
    (9, 1, 220, 1, 3, [156, 348, 540]),
    (9, 2, 0, 3, 1, [242, 402, 626]),
    (9, 2, 0, 1, 3, [242, 530, 786]),
    (9, 2, 180, 3, 1, [248, 434, 658]),
    (9, 2, 180, 1, 3, [248, 530, 818]),
    (9, 2, 220, 3, 1, [194, 418, 578]),
    (9, 2, 220, 1, 3, [194, 450, 706]),
];

fn literal_vectors(command: u8) {
    for mode in [6, 8, 10, 14] {
        for &(code, grid, phase, nx, ny, writes) in VECTORS {
            if code != command {
                continue;
            }
            let mut v = configured(mode, grid);
            let mut single = configured(mode, grid);
            v.tick(phase);
            single.tick(phase);
            start(&mut v, code, nx, ny);
            start(&mut single, code, nx, ny);
            let mut expected = v.vram.clone();
            let mut elapsed = 0;
            for (i, master) in writes.into_iter().enumerate() {
                let observable = master.div_ceil(6);
                for target in [observable - 1, observable] {
                    let delta = target - elapsed;
                    v.tick(delta);
                    for _ in 0..delta {
                        single.tick(1);
                    }
                    elapsed = target;
                    if target == observable {
                        let col = i % nx as usize;
                        let row = i / nx as usize;
                        let color = if code == 8 { 3 } else { 2 };
                        put(&mut expected, mode, col, row, color);
                    }
                    assert!(v.vram == expected, "VRAM mismatch: mode={mode} code={code} grid={grid} start={phase} shape={nx}x{ny} T={target}");
                    assert!(single.vram == expected);
                    let busy = u8::from(i != 2 || target != observable);
                    assert_eq!(ce(&mut v), busy);
                    assert_eq!(ce(&mut v), busy, "status reads cannot advance work");
                    assert_eq!(ce(&mut single), busy);
                    assert_eq!(v.registers, single.registers);
                }
            }
            assert_eq!(&v.registers[38..40], &[ny as u8, 0]);
            assert_eq!(&v.registers[42..44], &[0, 0]);
            if code == 9 {
                assert_eq!(&v.registers[34..36], &[4 + ny as u8, 0]);
            }
            v.tick(1000);
            assert!(v.vram == expected);
        }
    }
}

#[test]
fn source_and_destination_reads_latch_at_distinct_phases() {
    // Blank grid at phase zero: source 66, destination 98, write 122 clocks.
    for mode in [6, 8, 10, 14] {
        for op in [0, 1, 2, 3, 4, 8, 9, 10, 11, 12, 5, 13] {
            for source in [0, 3] {
                let mut v = configured(mode, 0);
                start(&mut v, 9, 1, 1);
                reg(&mut v, 46, 0x90 | op);
                v.tick(10);
                put(&mut v.vram, mode, 8, 4, source);
                v.tick(1); // Source read at exactly 66 clocks.
                put(&mut v.vram, mode, 8, 4, 2); // Must not change the latched source.
                v.tick(5);
                let (a, shift, mask) = location(mode, 0, 0);
                v.vram[a] = 0xa5;
                v.tick(1); // Destination read at 98 clocks, observable at T=17.
                v.vram[a] = 0x78; // Write must use the old full packed byte.
                v.tick(3);
                assert_eq!(v.vram[a], 0x78);
                assert_eq!(ce(&mut v), 1);
                v.tick(1);
                let old_pixel = (0xa5 >> shift) & mask;
                let value = match op & 7 {
                    0 => Some(source),
                    1 => Some(source & old_pixel),
                    2 => Some(source | old_pixel),
                    3 => Some(source ^ old_pixel),
                    4 => Some(!source),
                    _ => None,
                };
                let expected = if op & 8 != 0 && source == 0 {
                    0x78
                } else if let Some(value) = value {
                    (0xa5 & !(mask << shift)) | ((value & mask) << shift)
                } else {
                    0x78
                };
                assert_eq!(v.vram[a], expected, "mode={mode} op={op} source={source}");
                assert_eq!(ce(&mut v), 0);
            }
        }
    }
}

#[test]
fn live_lmmv_color_during_each_pending_phase_uses_both_ports() {
    // (grid, destination read, write), in master clocks at local phase zero.
    for (grid, read, write) in [(0, 90u32, 114u32), (1, 108, 140), (2, 114, 146)] {
        for mode in [6, 8, 10, 14] {
            for at in [0, read.div_ceil(6), write.div_ceil(6) - 1] {
                for indirect in [false, true] {
                    for (op, color) in [(0, 2), (3, 3), (8, 0), (11, 0)] {
                        let mut v = configured(mode, grid);
                        start(&mut v, 8, 1, 1);
                        reg(&mut v, 46, 0x80 | op);
                        v.tick(at);
                        if indirect {
                            reg(&mut v, 17, 0xac);
                            v.write_indirect(color);
                        } else {
                            reg(&mut v, 44, color);
                        }
                        assert_eq!(ce(&mut v), 1);
                        v.tick(write.div_ceil(6) - at);
                        let mut expected = configured(mode, grid).vram;
                        if op & 8 == 0 || color != 0 {
                            let (_, shift, mask) = location(mode, 0, 0);
                            let old = (0x55 >> shift) & mask;
                            put(
                                &mut expected,
                                mode,
                                0,
                                0,
                                if op == 3 { old ^ color } else { color },
                            );
                        }
                        assert!(v.vram == expected);
                        assert_eq!(ce(&mut v), 0);
                        reg(&mut v, 44, 1);
                        v.tick(100);
                        assert!(v.vram == expected);
                        assert_eq!(ce(&mut v), 0);
                    }
                }
            }
        }
    }
}

#[test]
fn stop_replacement_and_reset_discard_every_pending_phase() {
    for mode in [6, 8, 10, 14] {
        for (grid, code, phases) in [
            (0, 8, &[0, 15, 18][..]),
            (1, 8, &[0, 18, 23][..]),
            (2, 8, &[0, 19, 24][..]),
            (0, 9, &[0, 11, 17, 20][..]),
            (1, 9, &[0, 13, 18, 23][..]),
            (2, 9, &[0, 19, 30, 40][..]),
        ] {
            for &at in phases {
                for action in 0..3 {
                    let mut m = Machine::default();
                    m.set_model(Model::Msx2);
                    m.hardware.vdp = configured(mode, grid);
                    let v = &mut m.hardware.vdp;
                    start(v, code, 3, 1);
                    v.tick(at);
                    assert_eq!(ce(v), 1);
                    let mut expected = v.vram.clone();
                    match action {
                        0 => {
                            reg(v, 46, 0);
                            reg(v, 44, 7);
                        }
                        1 => {
                            word(v, 36, 20);
                            word(v, 38, 3);
                            reg(v, 44, 3);
                            reg(v, 46, 0x50);
                            put(&mut expected, mode, 20, 3, 3);
                        }
                        _ => {
                            m.reset();
                            expected.fill(0);
                        }
                    }
                    let v = &mut m.hardware.vdp;
                    v.tick(1000);
                    assert!(
                        v.vram == expected,
                        "grid={grid} code={code} T={at} action={action}"
                    );
                    assert_eq!(ce(v), 0);
                }
            }
        }
    }
}

#[test]
fn changing_service_grids_preserves_delay_and_partition_equivalence() {
    for (code, writes) in [(8, [146u32, 266, 402]), (9, [204, 322, 530])] {
        for mode in [6, 8, 10, 14] {
            let mut bulk = configured(mode, 0);
            let mut single = configured(mode, 0);
            start(&mut bulk, code, 3, 1);
            start(&mut single, code, 3, 1);
            let mut expected = bulk.vram.clone();
            let mut elapsed = 0;
            let mut checkpoints = vec![10, 25, 40, 60];
            for master in writes {
                checkpoints.extend([master.div_ceil(6) - 1, master.div_ceil(6)]);
            }
            checkpoints.sort_unstable();
            checkpoints.dedup();
            for target in checkpoints {
                bulk.tick(target - elapsed);
                for _ in elapsed..target {
                    single.tick(1);
                }
                elapsed = target;
                for (i, master) in writes.into_iter().enumerate() {
                    if target == master.div_ceil(6) {
                        put(&mut expected, mode, i, 0, if code == 8 { 3 } else { 2 });
                    }
                }
                assert!(bulk.vram == expected, "code={code} T={target}");
                assert!(single.vram == expected);
                assert_eq!(ce(&mut bulk), u8::from(target < writes[2].div_ceil(6)));
                assert_eq!(ce(&mut bulk), ce(&mut single));
                for v in [&mut bulk, &mut single] {
                    // R1/R8 writes change service without replacing pending accesses.
                    match target {
                        10 => {
                            reg(v, 1, 0x40);
                            reg(v, 8, 0);
                        }
                        25 => {
                            reg(v, 17, 0x88);
                            v.write_indirect(2);
                        }
                        40 => reg(v, 1, 0),
                        60 => {
                            reg(v, 1, 0x40);
                            reg(v, 8, 0);
                        }
                        _ => (),
                    }
                }
            }
        }
    }
}

#[test]
fn vblank_boundary_changes_service_chronologically_and_rendering_sees_progress() {
    for (code, writes) in [(8, [178u32, 290, 386]), (9, [248, 378, 506])] {
        let mut v = configured(6, 2);
        v.tick(191 * 228 + 180);
        start(&mut v, code, 3, 1);
        let mut expected = v.vram.clone();
        let mut elapsed = 0;
        for (i, master) in writes.into_iter().enumerate() {
            for target in [master.div_ceil(6) - 1, master.div_ceil(6)] {
                v.tick(target - elapsed);
                elapsed = target;
                if target == master.div_ceil(6) {
                    put(&mut expected, 6, i, 0, if code == 8 { 3 } else { 2 });
                }
                assert!(v.vram == expected, "code={code} T={target}");
                assert_eq!(ce(&mut v), u8::from(i != 2 || target != master.div_ceil(6)));
            }
        }
    }
    // Rendering a crossed scanline must precede command writes later in the tick.
    let mut bulk = configured(6, 0);
    let mut single = configured(6, 0);
    for v in [&mut bulk, &mut single] {
        v.tick(220);
        start(v, 8, 3, 1);
        reg(v, 1, 0x40);
        reg(v, 8, 2);
    }
    bulk.tick(400);
    for _ in 0..400 {
        single.tick(1);
    }
    assert!(bulk.pixels == single.pixels);
    assert!(bulk.vram == single.vram);
}

#[test]
fn original_cpu_diagnostic_checks_progress_and_ce_with_fetch_waits() {
    for diagnostic in [false, true] {
        let mut m = Machine::default();
        m.set_model(Model::Msx2);
        if diagnostic {
            m.use_diagnostic().unwrap();
        } else {
            m.use_cbios().unwrap();
        }
        m.load_cartridge(&firmware::command_timing_cartridge())
            .unwrap();
        for _ in 0..360 {
            m.frame();
            if matches!(m.hardware.read(0xc210), 0xa5 | 0xee) {
                break;
            }
        }
        let result: Vec<_> = (0xc200..=0xc245).map(|a| m.hardware.read(a)).collect();
        assert_eq!(
            result[16],
            0xa5,
            "stage={}, CE={:?}, first={:?}, last={:?}",
            result[17],
            &result[32..38],
            &result[64..70],
            &result[48..54]
        );
        assert_eq!(&result[..6], &[0xa5; 6]);
        assert_eq!(&result[32..38], &[1; 6]);
        assert_eq!(&result[48..54], &[0; 6]);
        assert_eq!(&result[64..70], &[0x33, 0x33, 0x33, 0x22, 0x22, 0x22]);
        m.frame();
        let offset = 16 * m.hardware.vdp.width() * 4;
        assert_eq!(&m.hardware.vdp.pixels[offset..offset + 3], &[36, 218, 36]);
    }
}

#[test]
fn literal_lmmv_all_layouts_grids_phases_and_shapes() {
    literal_vectors(8);
}

#[test]
fn literal_lmmm_all_layouts_grids_phases_and_shapes() {
    literal_vectors(9);
}

#[test]
fn absent_expansion_preserves_phases_and_pixel_masking() {
    for mode in [6, 8, 10, 14] {
        for (code, grid, write) in [
            (8, 0, 114u32),
            (8, 1, 140),
            (8, 2, 146),
            (9, 0, 122),
            (9, 1, 140),
            (9, 2, 242),
        ] {
            for arg in [0x10, 0x20, 0x30] {
                let mut v = configured(mode, grid);
                reg(&mut v, 45, arg);
                start(&mut v, code, 1, 1);
                let mut expected = v.vram.clone();
                v.tick(write.div_ceil(6) - 1);
                assert!(v.vram == expected);
                assert_eq!(ce(&mut v), 1);
                v.tick(1);
                if arg & 0x20 == 0 {
                    put(&mut expected, mode, 0, 0, if code == 8 { 3 } else { 255 });
                }
                assert!(v.vram == expected);
                assert_eq!(ce(&mut v), 0);
            }
        }
    }
}

#[test]
fn pending_phases_cross_interlaced_field_boundaries_without_partition_debt() {
    for code in [8, 9] {
        for r9 in [0, 2, 8, 10, 0x88, 0x8a] {
            let mut bulk = configured(6, 2);
            let mut single = configured(6, 2);
            for v in [&mut bulk, &mut single] {
                reg(v, 9, r9);
                // Includes the carry into the second field's pre-display half-line.
                let field = v.frame_cycles() as u32;
                v.tick(field - 20);
                start(v, code, 64, 2);
            }
            for field in 0..2 {
                bulk.tick(5000);
                for _ in 0..5000 {
                    single.tick(1);
                }
                assert!(bulk.vram == single.vram);
                assert!(bulk.pixels == single.pixels);
                assert_eq!(
                    (bulk.frames, bulk.line, bulk.irq(), ce(&mut bulk)),
                    (single.frames, single.line, single.irq(), ce(&mut single))
                );
                assert_eq!(bulk.registers, single.registers);
                if field == 0 {
                    let until_next_start = bulk.frame_cycles() as u32 - 5000;
                    bulk.tick(until_next_start);
                    single.tick(until_next_start);
                    start(&mut bulk, code, 64, 2);
                    start(&mut single, code, 64, 2);
                }
            }
        }
    }
}

#[test]
fn legacy_transfer_stall_budget_is_unchanged_by_raster_splitting() {
    let mut v = configured(6, 0);
    word(&mut v, 40, 2);
    word(&mut v, 42, 1);
    reg(&mut v, 44, 3);
    reg(&mut v, 46, 0xb0);
    v.tick(8);
    assert_eq!(v.vram[0], 0x35);
    v.tick(224); // Cross the raster boundary, ending four T into the next line.
    reg(&mut v, 44, 2);
    v.tick(4);
    assert_eq!(
        v.vram[0], 0x35,
        "legacy stall did not carry this raster tail"
    );
    assert_eq!(ce(&mut v), 1);
    v.tick(4);
    assert_eq!(v.vram[0], 0x32);
    assert_eq!(ce(&mut v), 0);
}

#[test]
fn issue_screen5_blank_vector_has_exact_preceding_bytes_and_ce() {
    let mut v = Vdp::new(true);
    reg(&mut v, 0, 6);
    word(&mut v, 40, 3);
    word(&mut v, 42, 1);
    reg(&mut v, 44, 3);
    reg(&mut v, 46, 0x80);
    let mut elapsed = 0;
    for (target, bytes, busy) in [
        (18, [0, 0], 1),
        (19, [0x30, 0], 1),
        (36, [0x30, 0], 1),
        (37, [0x33, 0], 1),
        (52, [0x33, 0], 1),
        (53, [0x33, 0x30], 0),
    ] {
        v.tick(target - elapsed);
        elapsed = target;
        assert_eq!(&v.vram[..2], &bytes);
        assert_eq!(ce(&mut v), busy);
    }
}
