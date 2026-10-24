//! Independent port-level expectations for the selected V9938 FH convention.
use msx_core::{vdp::Vdp, Machine, Model};

fn reg(v: &mut Vdp, r: u8, value: u8, indirect: bool) {
    if indirect {
        reg(v, 17, 0x80 | r, false);
        v.write_indirect(value);
    } else {
        v.control(value);
        v.control(0x80 | r);
    }
}
fn fh(v: &mut Vdp) -> u8 {
    reg(v, 15, 1, false);
    v.read_status() & 1
}
fn configured(r0: u8, r1: u8, r9: u8, target: u8) -> Vdp {
    let mut v = Vdp::new(true);
    for (r, value) in [(0, r0), (1, r1), (8, 2), (9, r9), (19, target)] {
        reg(&mut v, r, value, false);
    }
    v
}

#[test]
fn disabled_poll_boundaries_and_repeated_reads_in_all_stable_modes() {
    // Literal CPU-cycle boundaries, independent of production phase helpers:
    // graphics/multicolor [180,228), text [175 1/3,228).
    for (r0, r1, first_high) in [
        (0, 0x40, 180),
        (0, 0x48, 180),
        (2, 0x40, 180),
        (4, 0x40, 180),
        (6, 0x40, 180),
        (8, 0x40, 180),
        (10, 0x40, 180),
        (14, 0x40, 180),
        (0, 0x50, 176),
        (4, 0x50, 176),
    ] {
        for r9 in [0, 2, 0x80, 0x82] {
            let mut v = configured(r0, r1, r9, 7);
            v.tick(228 * 7 + first_high - 1);
            assert_eq!(fh(&mut v), 0, "mode {r0}/{r1} before edge");
            v.tick(1);
            assert_eq!(fh(&mut v), 1, "mode {r0}/{r1} at edge");
            assert_eq!(fh(&mut v), 1, "disabled read must not acknowledge");
            assert!(!v.irq());
            v.tick(227 - first_high);
            assert_eq!(fh(&mut v), 1);
            v.tick(1);
            assert_eq!(fh(&mut v), 0, "pulse expires without acknowledgement");
            assert!(!v.irq());
        }
    }
}

#[test]
fn enabled_latch_and_enable_transition_table() {
    for indirect in [false, true] {
        // (initial IE1, write time relative to matching line, new IE1,
        // expected IRQ after the pulse). Includes before, at, during, after.
        for (initial, time, enable, expected) in [
            (false, 179, true, true),
            (false, 180, true, false),
            (false, 200, true, false),
            (false, 229, true, false),
            (true, 179, false, false),
            (true, 180, false, false),
            (true, 200, false, false),
            (true, 229, false, false),
        ] {
            let mut v = configured(if initial { 0x16 } else { 6 }, 0x40, 0, 8);
            v.tick(228 * 8 + time);
            reg(&mut v, 0, if enable { 0x16 } else { 6 }, indirect);
            v.tick(300 - time);
            assert_eq!(v.irq(), expected, "{initial}/{time}/{enable}/{indirect}");
            if !enable {
                reg(&mut v, 0, 0x16, indirect);
                assert!(!v.irq(), "reenable cannot resurrect discarded latch");
            }
            assert_eq!(fh(&mut v), u8::from(expected));
            assert!(!v.irq());
            v.tick(228 * 262); // A later match can latch normally.
            assert!(v.irq());
            assert_eq!(fh(&mut v), 1);
            assert_eq!(fh(&mut v), 0);
        }
        let mut v = configured(0x16, 0x40, 0, 8);
        v.tick(228 * 8 + 180);
        assert!(v.irq());
        reg(&mut v, 0, 6, indirect);
        assert!(!v.irq());
        assert_eq!(fh(&mut v), 1); // Disabled pulse is independent of the latch.
        assert_eq!(fh(&mut v), 1);
        reg(&mut v, 0, 0x16, indirect);
        assert_eq!(fh(&mut v), 0); // Enabling during a pulse cannot latch it.
        v.tick(228 * 262);
        assert!(v.irq());
        v.tick(228 * 3);
        assert!(v.irq(), "enabled latch outlives border pulse");
        assert_eq!(fh(&mut v), 1);
        assert!(!v.irq());
        // Acknowledgement during the pulse must not relatch it.
        let mut v = configured(0x16, 0x40, 0, 8);
        v.tick(228 * 8 + 180);
        assert_eq!(fh(&mut v), 1);
        v.tick(1);
        assert_eq!(fh(&mut v), 0);
    }
}

#[test]
fn retargeting_cancels_future_matches_without_touching_latches() {
    for indirect in [false, true] {
        for target_register in [19, 23] {
            let mut v = configured(0x16, 0x40, 0, 10);
            v.tick(228 * 9);
            // Move line 10's future match to line 12 using either register.
            reg(
                &mut v,
                target_register,
                if target_register == 19 { 12 } else { 254 },
                indirect,
            );
            v.tick(228 * 2);
            assert!(!v.irq(), "canceled match must not execute");
            v.tick(228 + 180);
            assert!(v.irq());
            // Retarget a latched event to a line already passed.
            reg(
                &mut v,
                target_register,
                if target_register == 19 { 8 } else { 2 },
                indirect,
            );
            assert!(v.irq());
            assert_eq!(fh(&mut v), 1);
            v.tick(228);
            assert!(!v.irq(), "past target must not fire retroactively");
        }
        let mut v = configured(0x16, 0x40, 0, 20);
        v.tick(228 * 10 + 200);
        reg(&mut v, 19, 10, indirect); // Within a disabled-poll window, but past edge.
        assert!(!v.irq());
        v.tick(28);
        assert!(!v.irq());
    }
}

#[test]
fn wrapping_targets_line_zero_255_and_frame_wrap() {
    for r9 in [0, 2, 0x80, 0x82] {
        let lines = if r9 & 2 != 0 { 313 } else { 262 };
        for (r19, r23, target) in [(0, 0, 0), (0, 1, 255), (255, 255, 0), (1, 2, 255)] {
            let mut v = configured(0x16, 0x40, r9, r19);
            reg(&mut v, 23, r23, false);
            v.tick(target * 228 + 179);
            assert!(!v.irq());
            v.tick(1);
            assert_eq!(fh(&mut v), 1);
            // No second comparison at line 256, nor premature frame-start match.
            v.tick((lines - target) * 228 - 180);
            assert_eq!(v.line, 0);
            assert!(!v.irq());
            v.tick(target * 228 + 179);
            assert!(!v.irq());
            v.tick(1);
            assert!(v.irq());
        }
    }
}

#[test]
fn horizontal_and_vblank_acknowledge_only_their_own_source() {
    for visible in [192, 212] {
        for horizontal_first in [false, true] {
            let mut v = configured(
                0x16,
                0x60,
                if visible == 212 { 0x80 } else { 0 },
                visible as u8 - 1,
            );
            v.tick(visible * 228);
            assert!(v.irq());
            if horizontal_first {
                assert_eq!(fh(&mut v), 1);
                assert!(v.irq());
            }
            reg(&mut v, 15, 0, false);
            assert_ne!(v.read_status() & 0x80, 0);
            assert_eq!(v.irq(), !horizontal_first);
            if !horizontal_first {
                assert_eq!(fh(&mut v), 1);
            }
            assert!(!v.irq());
        }
    }
}

#[test]
fn tick_partitions_preserve_fractional_edges_and_crossed_events() {
    for (r0, r1) in [(6, 0x40), (0, 0x50), (4, 0x50)] {
        for enabled in [false, true] {
            for r9 in [0, 2, 0x80, 0x82] {
                let mut bulk = configured(r0 | if enabled { 16 } else { 0 }, r1, r9, 0);
                let mut single = configured(r0 | if enabled { 16 } else { 0 }, r1, r9, 0);
                // Includes multiple frames in one call, fractional text edges and expiry.
                for delta in [175, 1, 3, 1, 47, 1, 59_736, 71_364, 228, 200] {
                    bulk.tick(delta);
                    for _ in 0..delta {
                        single.tick(1);
                    }
                    assert_eq!(
                        (bulk.line, bulk.frames, bulk.irq(), fh(&mut bulk)),
                        (single.line, single.frames, single.irq(), fh(&mut single))
                    );
                }
            }
        }
    }
}

#[test]
fn reset_model_changes_and_msx1_keep_interrupt_sources_clean() {
    let mut m = Machine::default();
    m.set_model(Model::Msx2);
    reg(&mut m.hardware.vdp, 0, 0x16, false);
    m.hardware.vdp.tick(180);
    assert!(m.hardware.vdp.irq());
    m.reset();
    reg(&mut m.hardware.vdp, 0, 0x16, false);
    assert!(!m.hardware.vdp.irq());
    m.hardware.vdp.tick(180);
    assert!(m.hardware.vdp.irq());
    m.set_model(Model::Msx1);
    reg(&mut m.hardware.vdp, 0, 0x10, false);
    reg(&mut m.hardware.vdp, 1, 0x20, false);
    m.hardware.vdp.tick(228 * 192 - 1);
    assert!(!m.hardware.vdp.irq());
    m.hardware.vdp.tick(1);
    assert!(m.hardware.vdp.irq());
    assert_ne!(m.hardware.vdp.read_status() & 0x80, 0);
    assert!(!m.hardware.vdp.irq());
    m.set_model(Model::Msx2);
    assert_eq!(fh(&mut m.hardware.vdp), 0);
}

#[test]
fn original_cpu_probe_observes_ports_and_real_im1_interrupts() {
    use msx_core::{cpu::Bus, firmware};
    for diagnostic in [false, true] {
        let mut m = Machine::default();
        m.set_model(Model::Msx2);
        if diagnostic {
            m.use_diagnostic().unwrap();
        } else {
            m.use_cbios().unwrap();
        }
        m.load_cartridge(&firmware::fh_test_cartridge()).unwrap();
        for _ in 0..360 {
            m.frame();
            if matches!(m.hardware.read(0xc210), 0xa5 | 0xee) {
                break;
            }
        }
        let results: Vec<_> = (0xc200..=0xc210).map(|a| m.hardware.read(a)).collect();
        assert_eq!(results[16], 0xa5, "probe results: {results:02x?}");
        assert_eq!(&results[1..7], &[0xa5; 6]);
        assert!(results[0] >= 1, "must enter the real IM1 handler");
        m.frame(); // Let every result scanline reach the framebuffer.
        let offset = (16 * m.hardware.vdp.width()) * 4;
        assert_eq!(&m.hardware.vdp.pixels[offset..offset + 3], &[36, 218, 36]);
    }
}

#[test]
fn fractional_text_edges_and_adjusted_pulse_expiry_have_literal_boundaries() {
    for (r0, r1, adjust, first, last) in [
        (6, 0x40, 0, 180, 227),
        (0, 0x50, 0, 176, 227),
        (4, 0x50, 0, 176, 227),
        (6, 0x40, 1, 180, 227),
        (6, 0x40, 15, 181, 228),
        (0, 0x50, 1, 175, 227),
        (0, 0x50, 15, 176, 228),
    ] {
        for enabled in [false, true] {
            let mut v = configured(r0 | if enabled { 16 } else { 0 }, r1, 0, 20);
            reg(&mut v, 18, adjust, false);
            v.tick(228 * 20 + first - 1);
            assert_eq!(fh(&mut v), 0);
            v.tick(1);
            assert_eq!(v.irq(), enabled);
            if !enabled {
                assert_eq!(fh(&mut v), 1);
            }
            v.tick(last - first);
            if !enabled {
                assert_eq!(fh(&mut v), 1);
            }
            v.tick(1);
            assert_eq!(fh(&mut v), u8::from(enabled));
            assert!(!v.irq());
        }
    }
}

#[test]
fn disable_reenable_before_edge_still_allows_future_match() {
    for indirect in [false, true] {
        let mut v = configured(0x16, 0x40, 0, 12);
        v.tick(228 * 12 + 130);
        reg(&mut v, 0, 6, indirect);
        v.tick(20);
        reg(&mut v, 0, 0x16, indirect);
        v.tick(29);
        assert!(!v.irq());
        v.tick(1);
        assert!(v.irq());
    }
}
