//! Independent register sequences for FORCE INTERRUPT (Issue #61).
use msx_core::disk::{Controller, Disk};

const REV: u32 = 715_909; // 300 rpm at 3,579,545 Hz
fn drive() -> Controller {
    let mut f = Controller::default();
    f.mount(Disk::new(vec![0x35; 368_640], false).unwrap())
        .unwrap();
    f.write(5, 0x81);
    f.read(0);
    f
}
fn signals(f: &mut Controller, irq: bool, drq: bool) {
    assert_eq!((f.irq, f.drq), (irq, drq));
    assert_eq!(f.read(7), (u8::from(irq) * 0x80) | (u8::from(drq) * 0x40));
    assert_eq!(
        f.read_philips(7),
        0xff ^ ((u8::from(irq) * 0x40) | (u8::from(drq) * 0x80))
    );
}
#[test]
fn busy_cancellation_keeps_type_two_lost_data_but_idle_force_selects_live_type_one() {
    let mut f = drive();
    f.write(0, 0x80);
    f.tick(5_385); // ceil(47 * 32 us): two arrivals, first unread
    assert_eq!(f.read(0), 7);
    f.write(0, 0xd0);
    assert_eq!(f.read(0), 4); // LOST DATA, not Track 0 + index
    signals(&mut f, false, false);
    f.tick(10 * REV);
    assert_eq!(f.read(0), 4);
    f.write(0, 0xd0); // idle: Type I, zero + live index, errors refreshed
    assert_eq!(f.read(0), 6);
    f.disk.as_mut().unwrap().write_protected = true;
    assert_eq!(f.read(0), 0x46);
    f.disk.as_mut().unwrap().write_protected = false;
    f.write(5, 1);
    assert_eq!(f.read(0), 0x84); // stopped, zero, no index/WP
    f.write(5, 0x82);
    assert_eq!(f.read(0), 0x80); // absent B has no zero sensor
}
#[test]
fn idle_force_refreshes_status_after_completed_error_and_busy_type_one_retains_class() {
    let mut f = drive();
    f.write(2, 10);
    f.write(0, 0x80);
    f.tick(4 * REV);
    assert_eq!(f.read(0), 0x10);
    f.write(0, 0xd0);
    assert_eq!(f.read(0), 6);
    f.write(3, 5);
    f.write(0, 0x10);
    f.tick(30_000); // two issued steps
    f.write(0, 0xd0);
    assert_eq!(f.read(0), 0);
    f.tick(10 * REV);
    assert_eq!(f.track, 2);
    assert_eq!(f.read(0), 0);
    signals(&mut f, false, false);
}
#[test]
fn immediate_irq_survives_status_time_ordinary_commands_and_other_nonzero_masks() {
    let mut f = drive();
    f.write(0, 0xd8);
    for _ in 0..3 {
        assert_eq!(f.read(0), 6);
        signals(&mut f, true, false);
    }
    f.tick(10 * REV);
    for mask in [1, 2, 4, 7, 9, 15] {
        f.write(0, 0xd0 | mask);
        f.read(0);
        signals(&mut f, true, false);
    }
    f.write(0, 0x80); // accepted ordinary command must preserve immediate IRQ
    signals(&mut f, true, false);
    f.tick(5_270);
    signals(&mut f, true, true);
    assert_eq!(f.read(0), 3);
    signals(&mut f, true, true);
    f.write(0, 0xd0); // release at this command boundary, cancel transfer
    signals(&mut f, false, false);
    assert_eq!(f.read(0), 0); // busy Type II interpretation
    f.write(0, 0x80);
    signals(&mut f, false, false);
    f.tick(2 * REV);
    signals(&mut f, true, false); // ordinary completion
    assert_eq!(f.read(0), 4);
    signals(&mut f, false, false);
}
#[test]
fn d0_restores_ordinary_acknowledgement_and_reset_clears_all_irq_sources_and_masks() {
    for next_command in [false, true] {
        let mut f = drive();
        f.write(0, 0xd8);
        f.write(0, 0xd0);
        if next_command {
            f.write(0, 0x00); // already at zero: completes immediately
            signals(&mut f, true, false);
        }
        assert_eq!(f.read(0), 6);
        signals(&mut f, false, false);
        f.write(0, 0xdf);
        f.reset();
        signals(&mut f, false, false);
        f.write(5, 0x81);
        f.tick(10 * REV);
        signals(&mut f, false, false);
    }
}
#[test]
fn ready_masks_are_edges_recur_and_combine_with_index_through_both_control_wirings() {
    for philips in [false, true] {
        for mask in 1..=7 {
            let mut f = drive();
            f.write(0, 0xd0 | mask);
            for _ in 0..2 {
                for (value, ready, bit) in [(1, false, 2), (0x82, false, 0), (0x81, true, 1)] {
                    if philips {
                        f.write_philips(
                            5,
                            match value {
                                1 => 0,
                                0x82 => 0x81,
                                _ => 0x80,
                            },
                        );
                    } else {
                        f.write(5, value);
                    }
                    signals(&mut f, mask & bit != 0, false);
                    assert_eq!(f.read(0) & 0x80 == 0, ready);
                    signals(&mut f, false, false);
                    f.tick(1);
                    signals(&mut f, false, false); // unchanged READY level
                }
            }
            f.tick(REV - 7);
            signals(&mut f, false, false);
            f.tick(1); // exact first rising index
            signals(&mut f, mask & 4 != 0, false);
            f.read(0);
            f.tick(REV);
            signals(&mut f, mask & 4 != 0, false);
            f.read(0);
            f.write(0, 0xd0);
            f.write(5, 1);
            f.write(5, 0x81);
            f.tick(REV);
            signals(&mut f, false, false);
        }
    }
}
#[test]
fn media_ready_edges_keep_lifecycle_irqs_and_arm_later_edges_without_level_retrigger() {
    for mask in 1..=7 {
        let mut f = drive();
        f.write(0, 0xd0 | mask);
        for _ in 0..2 {
            f.eject().unwrap();
            signals(&mut f, true, false); // separately documented lifecycle IRQ
            assert_eq!(f.read(0), 0x84);
            f.tick(2 * REV);
            signals(&mut f, false, false); // no media => no index
            f.mount(Disk::new(vec![0x35; 368_640], false).unwrap())
                .unwrap();
            signals(&mut f, true, false); // insertion lifecycle IRQ
            f.read(0);
            f.tick(1);
            signals(&mut f, false, false);
            f.write(5, 1);
            signals(&mut f, mask & 2 != 0, false);
            f.read(0);
            f.tick(2 * REV);
            signals(&mut f, false, false); // stopped => no index
            f.write(5, 0x81);
            signals(&mut f, mask & 1 != 0, false);
            f.read(0);
        }
    }
}
#[test]
fn recurring_index_has_exact_boundaries_for_large_irregular_and_single_cycle_ticks() {
    for mode in 0..3 {
        let mut f = drive();
        f.write(0, 0xd4);
        let mut now = 0;
        for target in [
            REV - 1,
            REV,
            REV + 1,
            2 * REV - 1,
            2 * REV,
            12 * REV,
            12 * REV + 1,
        ] {
            let start = now;
            while now < target {
                let step = match mode {
                    0 => target - now,
                    1 => [17, 90_001, 3, 771][now as usize % 4],
                    _ => 1,
                }
                .min(target - now);
                f.tick(step);
                now += step;
            }
            signals(&mut f, target / REV > start / REV, false);
            f.read(0);
            signals(&mut f, false, false);
        }
    }
}
#[test]
fn accepted_ordinary_command_disarms_conditions_and_normal_irq_acknowledges_once() {
    let mut f = drive();
    f.write(0, 0xd7);
    f.write(0, 0x00);
    signals(&mut f, true, false);
    assert_eq!(f.read(0), 6);
    signals(&mut f, false, false);
    f.write(5, 1);
    f.write(5, 0x81);
    f.tick(10 * REV);
    signals(&mut f, false, false);
}

#[test]
fn canceled_read_and_write_overruns_keep_lost_data_through_transfer_and_crc_tail() {
    for write in [false, true] {
        for elapsed in [5_385, 63_801, 63_917, 64_030] {
            for command in [0xd0, 0xd8] {
                let mut f = drive();
                let original = f.disk.as_ref().unwrap().bytes().to_vec();
                f.write(0, if write { 0xa0 } else { 0x80 });
                if write {
                    f.tick(1_031);
                    f.write(3, 0x61);
                    f.tick(elapsed - 1_031);
                } else {
                    f.tick(elapsed);
                }
                assert_eq!(f.read(0), if write && elapsed > 63_573 { 5 } else { 7 });
                f.write(0, command);
                assert_eq!(f.read(0), 4);
                signals(&mut f, command == 0xd8, false);
                f.tick(u32::MAX);
                assert_eq!(f.read(0), 4);
                signals(&mut f, command == 0xd8, false);
                assert_eq!(f.disk.as_ref().unwrap().bytes(), original);
                assert!(!f.disk.as_ref().unwrap().dirty);
            }
        }
    }
}
