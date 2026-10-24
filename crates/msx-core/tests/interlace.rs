//! Original boundary expectations derived from the V9938 timing tables, not VDP helpers.
use msx_core::vdp::Vdp;

fn reg(v: &mut Vdp, r: u8, value: u8) {
    v.control(value);
    v.control(0x80 | r);
}
fn status(v: &mut Vdp, n: u8) -> u8 {
    reg(v, 15, n);
    v.read_status()
}
fn configured(r9: u8) -> Vdp {
    let mut v = Vdp::new(true);
    for (r, value) in [(0, 6), (1, 0x60), (8, 2), (9, r9)] {
        reg(&mut v, r, value);
    }
    v
}

#[test]
fn literal_field_boundaries_and_continuous_horizontal_phase() {
    for (r9, edge, pair) in [(8, 59_850, 119_700), (10, 71_250, 142_500)] {
        for ln in [0, 0x80] {
            let mut v = configured(r9 | ln);
            assert_eq!(v.frame_cycles(), edge);
            v.tick(edge as u32 - 1);
            assert_eq!(v.frames, 0);
            assert_eq!(status(&mut v, 2) & 0x22, 0); // H=113 T, first field.
            v.tick(1);
            assert_eq!(v.frames, 1);
            assert_eq!(status(&mut v, 2) & 0x62, 0x42); // H=114, top half-line blank.
            v.tick(56);
            assert_eq!(status(&mut v, 2) & 0x20, 0);
            v.tick(1); // H=171: legacy HR convention remains continuous.
            assert_eq!(status(&mut v, 2) & 0x20, 0x20);
            v.tick(57); // H wraps; start second field's first complete display line.
            assert_eq!(v.line, 0);
            assert_eq!(status(&mut v, 2) & 0x62, 2);
            v.tick((pair - edge - 114 - 1) as u32);
            assert_eq!(v.frames, 1);
            v.tick(1);
            assert_eq!(v.frames, 2);
            assert_eq!(v.line, 0);
            assert_eq!(status(&mut v, 2) & 0x62, 0);
            v.tick(1);
            assert_eq!(v.frames, 2);
        }
    }
}

#[test]
fn hundred_fields_and_irregular_partitions_have_literal_totals() {
    for (r9, total) in [(8, 5_985_000), (10, 7_125_000)] {
        for ln in [0, 0x80] {
            let mut bulk = configured(r9 | ln);
            let mut split = configured(r9 | ln);
            bulk.tick(total - 1);
            let mut left = total - 1;
            let mut index = 0;
            let parts = [1, 113, 115, 59_851, 7, 71_249, 229, 4441];
            while left != 0 {
                let part = parts[index % parts.len()].min(left);
                split.tick(part);
                left -= part;
                index += 1;
            }
            assert_eq!(bulk.frames, 99);
            assert_eq!(split.frames, 99);
            assert_eq!(bulk.line, split.line);
            assert_eq!(status(&mut bulk, 2), status(&mut split, 2));
            assert_eq!(bulk.pixels, split.pixels);
            for v in [&mut bulk, &mut split] {
                v.tick(1);
                assert_eq!((v.frames, v.line), (100, 0));
                v.tick(1);
                assert_eq!((v.frames, v.line), (100, 0));
            }
        }
    }
}

#[test]
fn r9_writes_fix_at_boundary_latest_wins_without_restarting_time_or_irq() {
    for indirect in [false, true] {
        let write = |v: &mut Vdp, value| {
            if indirect {
                reg(v, 17, 0x89);
                v.write_indirect(value);
            } else {
                reg(v, 9, value);
            }
        };
        let mut v = configured(0);
        v.tick(0); // Zero elapsed time must not fix the initial state.
        write(&mut v, 0x8a);
        assert_eq!((v.frame_cycles(), v.height()), (71_250, 424));
        v.tick(68_400); // Already beyond NTSC length: writes cannot end this field.
        assert_eq!(status(&mut v, 0) & 0x80, 0x80);
        assert!(!v.irq());
        for value in [0, 0x88, 0x82, 4, 0x8c] {
            write(&mut v, value);
            assert_eq!((v.frame_cycles(), v.height()), (71_250, 424));
            assert_eq!(v.frames, 0);
            assert!(!v.irq());
        }
        v.tick(2849);
        assert_eq!(v.frames, 0);
        v.tick(1);
        assert_eq!((v.frames, v.frame_cycles(), v.height()), (1, 59_850, 424));
        assert!(!v.irq());
        write(&mut v, 0);
        v.tick(59_849);
        assert_eq!(v.frames, 1);
        v.tick(1);
        assert_eq!((v.frames, v.frame_cycles(), v.height()), (2, 59_736, 192));
        // Every subsequent switch uses the currently active field, even with
        // carried H phase; no write adds, removes or resets elapsed time.
        for (new, old_budget, next_budget) in [
            (10, 59_736, 71_250),
            (8, 71_250, 59_850),
            (2, 59_850, 71_364),
            (0, 71_364, 59_736),
        ] {
            let before = v.frames;
            v.tick(113);
            write(&mut v, new);
            v.tick(old_budget - 114);
            assert_eq!(v.frames, before);
            v.tick(1);
            assert_eq!(v.frames, before + 1);
            assert_eq!(v.frame_cycles(), next_budget);
        }
    }
}

#[test]
fn every_woven_row_is_rendered_once_per_field_with_eo_off_and_on() {
    for (ln, visible) in [(0, 192), (0x80, 212)] {
        for pal in [0, 2] {
            for eo in [0, 4] {
                let mut v = configured(8 | ln | pal | eo);
                reg(&mut v, 2, 0x3f); // Upper bitmap page; EO can select the lower one.
                v.pixels.fill(0xcc);
                let budget = if pal == 0 { 59_850 } else { 71_250 };
                for field in 0..4 {
                    // Change both pages every field so skipped refreshes or
                    // writes to the other field's rows cannot hide in old pixels.
                    for y in 0..visible {
                        v.vram[y * 128..(y + 1) * 128]
                            .fill((((y + field * 3) % 15 + 1) as u8) * 17);
                        v.vram[0x8000 + y * 128..0x8000 + (y + 1) * 128]
                            .fill((((y + field * 3 + 7) % 15 + 1) as u8) * 17);
                    }
                    let color =
                        |f, y| (y + f * 3 + if eo != 0 && f % 2 == 0 { 0 } else { 7 }) % 15 + 1;
                    if field != 0 {
                        // First display starts 114 T after the odd field edge.
                        v.tick(if field & 1 != 0 { 114 } else { 0 });
                    }
                    v.tick(visible as u32 * 228 - 1);
                    let last = ((2 * visible - 2 + field % 2) * 256) * 4;
                    if field < 2 {
                        assert_eq!(v.pixels[last], 0xcc, "last row cannot render early");
                    }
                    v.tick(1);
                    for y in 0..visible {
                        for x in [0, 255] {
                            let offset = ((y * 2 + field % 2) * 256 + x) * 4;
                            assert_eq!(&v.pixels[offset..offset + 3], &v.palette[color(field, y)]);
                            let other = ((y * 2 + (field + 1) % 2) * 256 + x) * 4;
                            if field == 0 {
                                assert_eq!(&v.pixels[other..other + 3], &[0xcc; 3]);
                            } else {
                                assert_eq!(
                                    &v.pixels[other..other + 3],
                                    &v.palette[color(field - 1, y)]
                                );
                            }
                        }
                    }
                    assert_eq!(status(&mut v, 2) & 2, (field as u8 & 1) * 2);
                    assert_eq!(status(&mut v, 0) & 0x80, 0x80);
                    let delay = if field & 1 != 0 { 114 } else { 0 };
                    v.tick(budget - visible as u32 * 228 - delay);
                    assert_eq!(v.frames, field as u64 + 1);
                }
                assert_eq!(v.height(), visible * 2);
            }
        }
    }
}

#[test]
fn interlaced_fh_and_vblank_follow_display_phase_with_independent_acknowledgement() {
    for (r9, budget) in [(8, 59_850), (10, 71_250), (0x88, 59_850), (0x8a, 71_250)] {
        for enabled in [false, true] {
            let mut v = configured(r9);
            reg(&mut v, 0, if enabled { 0x16 } else { 6 });
            reg(&mut v, 19, 0);
            v.tick(budget);
            status(&mut v, 1); // Acknowledge the initial field's enabled FH latch.
            for delay in [114, 0, 114, 0] {
                v.tick(delay + 179);
                assert_eq!(status(&mut v, 1) & 1, 0);
                v.tick(1);
                assert_eq!(status(&mut v, 1) & 1, 1);
                assert_eq!(status(&mut v, 1) & 1, u8::from(!enabled));
                // Acknowledging FH cannot clear the preceding field's VBlank.
                assert!(v.irq());
                assert_eq!(status(&mut v, 0) & 0x80, 0x80);
                assert!(!v.irq());
                v.tick(1);
                assert_eq!(status(&mut v, 1) & 1, u8::from(!enabled));
                v.tick(47);
                assert_eq!(status(&mut v, 1) & 1, 0);
                let visible = if r9 & 0x80 == 0 { 192 } else { 212 };
                v.tick((visible - 1) * 228 - 1);
                assert!(!v.irq());
                v.tick(1);
                assert!(v.irq());
                v.tick(budget - delay - visible * 228);
            }
        }
    }
}

#[test]
fn reset_and_model_switch_discard_pending_timing_and_phase() {
    use msx_core::{Machine, Model};
    let mut m = Machine::default();
    m.set_model(Model::Msx2);
    reg(&mut m.hardware.vdp, 9, 10);
    m.hardware.vdp.tick(71_250);
    reg(&mut m.hardware.vdp, 9, 0x88);
    m.reset();
    assert_eq!(
        (
            m.hardware.vdp.frames,
            m.hardware.vdp.line,
            m.hardware.vdp.frame_cycles(),
            m.hardware.vdp.height()
        ),
        (0, 0, 59_736, 192)
    );
    reg(&mut m.hardware.vdp, 9, 8);
    m.hardware.vdp.tick(59_850);
    m.set_model(Model::Msx1);
    assert_eq!(
        (m.hardware.vdp.frames, m.hardware.vdp.frame_cycles()),
        (0, 59_736)
    );
    reg(&mut m.hardware.vdp, 9, 10); // MSX1 register-port aliasing cannot enable extended timing.
    assert_eq!(m.hardware.vdp.frame_cycles(), 59_736);
    m.set_model(Model::Msx2);
    reg(&mut m.hardware.vdp, 9, 0x8a);
    assert_eq!(
        (m.hardware.vdp.frame_cycles(), m.hardware.vdp.height()),
        (71_250, 424)
    );
}

#[test]
fn independent_z80_port_probe_timestamps_parity_and_timing_transitions() {
    use msx_core::{cpu::Bus, Machine, Model};
    let code: Vec<u8> = include_str!("../../../tests/fixtures/interlace-probe.hex")
        .lines()
        .flat_map(|line| line.split('#').next().unwrap().split_whitespace())
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect();
    let mut main = vec![0; 32768];
    main[..3].copy_from_slice(&[0xc3, 0, 1]);
    main[0x2d] = 1;
    main[0x100..0x100 + code.len()].copy_from_slice(&code);
    let mut sub = vec![0; 16384];
    sub[..2].copy_from_slice(b"CD");
    let mut m = Machine::default();
    m.set_model(Model::Msx2);
    m.load_msx2_firmware(&main, &sub).unwrap();
    let mut edges = Vec::new();
    let mut budgets = Vec::new();
    let mut last = 0;
    while edges.len() < 35 && m.cpu.tstates < 2_600_000 {
        let before = m.cpu.tstates;
        let elapsed = m.step();
        assert_eq!(m.cpu.tstates - before, u64::from(elapsed));
        let count = m.hardware.read(0xc201);
        if count != last && (1..=32).contains(&count) {
            let parity = m.hardware.read(0xc200);
            assert_eq!(parity, if count & 1 == 0 { 0 } else { 2 });
            edges.push(m.cpu.tstates);
            budgets.push(m.hardware.vdp.frame_cycles());
            last = count;
        }
    }
    assert_eq!(edges.len(), 35);
    eprintln!("EO edge CPU T-states: {edges:?}");
    for (i, pair) in edges.windows(2).enumerate() {
        let delta = pair[1] - pair[0];
        // The transition write occurs after observing an edge, so the old
        // field remains active for one more interval. Polling jitter <= 60 T.
        let expected = match i + 2 {
            2..=9 | 34.. => 59_850,
            10..=17 => 71_250,
            18..=25 => 71_364,
            _ => 59_736,
        };
        assert!(
            delta.abs_diff(expected) <= 60,
            "edge {}: {delta}, expected {expected}",
            i + 2
        );
    }
    assert!(budgets.contains(&59_850));
    assert!(budgets.contains(&71_250));
    assert!(budgets.contains(&71_364));
    assert!(budgets.contains(&59_736));
    assert_eq!(
        m.hardware.psg.samples.len() as u64,
        m.cpu.tstates * 44_100 / 3_579_545
    );
}
