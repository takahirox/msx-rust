//! Original movement traces calculated from milliseconds and the CPU clock.
//! No production timing constants or physical-head access are used as an oracle.
use msx_core::disk::{Controller, Disk};

fn drive() -> Controller {
    let mut image = vec![0; 368640];
    for track in 0..80 {
        image[track * 9 * 512..(track + 1) * 9 * 512].fill(track as u8);
    }
    let mut f = Controller::default();
    f.mount(Disk::new(image, false).unwrap()).unwrap();
    f.write(5, 0x81);
    f
}
fn event(ms: u32, intervals: u32) -> u32 {
    // Round the absolute deadline up, rather than rounding each interval.
    (3_579_545_u64 * u64::from(ms) * u64::from(intervals)).div_ceil(1000) as u32
}
fn seek(f: &mut Controller, track: u8) {
    f.write(3, track);
    f.write(0, 0x10);
    f.tick(6_000_000);
}
fn signals(f: &mut Controller, track: u8, busy: bool, zero: bool, irq: bool) {
    signals_with_latch(f, track, busy, zero, irq, false);
}
fn signals_with_latch(
    f: &mut Controller,
    track: u8,
    busy: bool,
    zero: bool,
    irq: bool,
    latched: bool,
) {
    assert_eq!(f.read(1), track);
    assert!(!f.drq);
    assert_eq!(f.irq, irq);
    assert_eq!(f.read(7), if irq { 0x80 } else { 0 });
    assert_eq!(f.read(0) & 0x15, u8::from(busy) | if zero { 4 } else { 0 });
    assert_eq!(f.irq, latched); // Status acknowledges only ordinary IRQ.
}
fn byte_ready(f: &mut Controller) {
    for _ in 0..45_100 {
        f.tick(16);
        if f.drq {
            return;
        }
    }
    panic!("retained head did not find sector: {:02x}", f.read(0));
}
fn read_position(f: &mut Controller, track: u8) {
    f.write(1, track);
    f.write(0, 0x80);
    byte_ready(f);
    assert!(f.drq, "head must address track {track}");
    assert_eq!(f.read(3), track);
    f.write(0, 0xd0);
}

#[test]
fn seek_traces_all_rates_distances_and_directions() {
    for (rate, ms) in [6, 12, 20, 30].into_iter().enumerate() {
        for distance in [0, 1, 5, 79] {
            for inward in [true, false] {
                let mut f = drive();
                let start = if inward { 0 } else { distance };
                seek(&mut f, start);
                let target = if inward { distance } else { 0 };
                f.write(3, target);
                f.write(0, 0x10 | rate as u8);
                let position = |steps: u8| if inward { steps } else { start - steps };
                let first = position(u8::from(distance != 0));
                signals(&mut f, first, distance != 0, first == 0, distance == 0);
                let mut elapsed = 0;
                for interval in 1..=u32::from(distance) {
                    let at = event(ms, interval);
                    f.tick(at - elapsed - 1);
                    let before = position(interval as u8);
                    signals(&mut f, before, true, before == 0, false);
                    f.tick(1);
                    let after = position((interval as u8 + 1).min(distance));
                    signals(
                        &mut f,
                        after,
                        interval < u32::from(distance),
                        after == 0,
                        interval == u32::from(distance),
                    );
                    f.tick(1);
                    signals(
                        &mut f,
                        after,
                        interval < u32::from(distance),
                        after == 0,
                        false,
                    );
                    elapsed = at + 1;
                }
                read_position(&mut f, target);
            }
        }
    }
    assert_eq!(event(6, 79), 1_696_705); // 474 ms, not one 6 ms delay.
}

#[test]
fn restore_follows_sensor_and_repairs_register_at_each_rate() {
    for (rate, ms) in [6, 12, 20, 30].into_iter().enumerate() {
        for distance in [0, 1, 5, 79] {
            let mut f = drive();
            seek(&mut f, distance);
            f.write(1, 17); // Deliberately unrelated to the physical position.
            f.write(0, rate as u8);
            if distance == 0 {
                signals(&mut f, 0, false, true, true);
            } else {
                // RESTORE initializes Track to FF, then decrements per pulse.
                signals(&mut f, 254, true, distance == 1, false);
                let mut elapsed = 0;
                for interval in 1..=u32::from(distance) {
                    let at = event(ms, interval);
                    f.tick(at - elapsed - 1);
                    signals(
                        &mut f,
                        255 - interval as u8,
                        true,
                        interval == u32::from(distance),
                        false,
                    );
                    f.tick(1);
                    if interval == u32::from(distance) {
                        signals(&mut f, 0, false, true, true);
                    } else {
                        signals(
                            &mut f,
                            254 - interval as u8,
                            true,
                            interval + 1 == u32::from(distance),
                            false,
                        );
                    }
                    elapsed = at;
                }
            }
            assert_eq!(f.data, 0);
            read_position(&mut f, 0);
        }
    }
}

#[test]
fn step_variants_update_only_when_requested_and_remember_both_directions() {
    for (rate, ms) in [6, 12, 20, 30].into_iter().enumerate() {
        for inward in [true, false] {
            for update in [false, true] {
                let mut f = drive();
                seek(&mut f, 5);
                let delta = if inward { 1_i8 } else { -1 };
                let explicit = if inward { 0x40 } else { 0x60 };
                for (i, command) in [explicit, 0x20].into_iter().enumerate() {
                    let physical = (5 + delta * (i as i8 + 1)) as u8;
                    let track = if update { physical } else { 5 };
                    f.write(0, command | if update { 0x10 } else { 0 } | rate as u8);
                    signals(&mut f, track, true, false, false);
                    f.tick(event(ms, 1) - 1);
                    signals(&mut f, track, true, false, false);
                    f.tick(1);
                    signals(&mut f, track, false, false, true);
                }
                read_position(&mut f, (5 + 2 * delta) as u8);
            }
        }
    }
}

#[test]
fn force_interrupt_retains_issued_steps_and_never_resumes() {
    for command in [0xd0, 0xd8] {
        for outward in [false, true] {
            let mut f = drive();
            seek(&mut f, if outward { 5 } else { 0 });
            f.write(3, if outward { 0 } else { 5 });
            f.write(0, 0x10);
            f.tick(30_000); // Between the second pulse (21478) and third (42955).
            let track = if outward { 3 } else { 2 };
            assert_eq!(f.track, track);
            f.write(0, command);
            signals_with_latch(
                &mut f,
                track,
                false,
                false,
                command == 0xd8,
                command == 0xd8,
            );
            f.tick(u32::MAX);
            signals_with_latch(
                &mut f,
                track,
                false,
                false,
                command == 0xd8,
                command == 0xd8,
            );
            read_position(&mut f, track);
        }
    }
    let mut f = drive();
    seek(&mut f, 5);
    f.write(0, 0);
    f.tick(30_000);
    f.write(0, 0xd0);
    assert_eq!(f.track, 253); // Two restore pulses have really been issued.
    read_position(&mut f, 3);
}

#[test]
fn seek_preserves_offset_during_motion_and_zero_sensor_prevents_underflow() {
    let mut f = drive();
    f.write(0, 0x40); // Head 1, Track 0.
    f.tick(21_478);
    f.write(3, 5);
    f.write(0, 0x10);
    assert_eq!(f.track, 1);
    f.tick(21_478);
    assert_eq!(f.track, 2);
    f.write(0, 0xd0);
    read_position(&mut f, 3);
    f.write(1, 5); // Head 3, Track 5; seek outward past the zero sensor.
    f.write(3, 0);
    f.write(0, 0x10);
    f.tick(event(6, 3) - 1);
    signals(&mut f, 2, true, true, false);
    f.tick(1);
    signals(&mut f, 0, false, true, true);
    for track in [0, 17, 255] {
        for cmd in [0x60, 0x70, 0x20, 0x30] {
            f.write(1, track);
            f.write(0, cmd);
            signals(&mut f, 0, false, true, true); // No outward pulse at zero.
        }
    }
    read_position(&mut f, 0);
}

#[test]
fn upper_limit_keeps_completed_motion_and_rejects_invalid_sector_aliases() {
    for target in [80, 255] {
        let mut f = drive();
        seek(&mut f, 77);
        f.write(3, target);
        f.write(0, 0x10);
        signals(&mut f, 78, true, false, false);
        f.tick(21_478);
        signals(&mut f, 79, true, false, false);
        f.tick(42_955 - 21_478 - 1);
        signals(&mut f, 79, true, false, false);
        f.tick(1); // Attempted pulse at the bounded drive's upper stop.
        assert_eq!(f.track, 80);
        assert_eq!(f.read(0) & 0x11, 0x10);
        f.tick(u32::MAX);
        for command in [0x80, 0xa0] {
            f.write(0, command);
            assert_eq!(f.read(0) & 0x11, 1);
            f.tick(5 * 715_909);
            assert_eq!(f.read(0) & 0x11, 0x10);
            assert!(!f.drq);
        }
        read_position(&mut f, 79); // Prior steps were retained.
        f.write(0, 0x50);
        assert_eq!(f.track, 80);
        assert_eq!(f.read(0) & 0x11, 0x10);
        read_position(&mut f, 79);
        f.write(0, 0);
        f.tick(event(6, 79));
        read_position(&mut f, 0);
    }
    for command in [0x20, 0x30, 0x40, 0x50] {
        let mut f = drive();
        seek(&mut f, 79); // Remembered direction is inward.
        f.write(0, command);
        assert_eq!(f.track, if command & 0x10 == 0 { 79 } else { 80 });
        assert!(f.irq);
        assert_eq!(f.read(0) & 0x11, 0x10);
        assert!(!f.drq);
        read_position(&mut f, 79);
    }
}

fn partition(f: &mut Controller, total: u32, chunks: &[u32]) {
    let mut elapsed = 0;
    let mut i = 0;
    while elapsed < total {
        let cycles = chunks[i % chunks.len()].min(total - elapsed);
        f.tick(cycles);
        elapsed += cycles;
        i += 1;
    }
}

#[test]
fn movement_is_independent_of_tick_partition_and_rounding_does_not_drift() {
    for (rate, ms) in [6, 12, 20, 30].into_iter().enumerate() {
        for command in [0x10, 0x00] {
            for chunks in [&[u32::MAX][..], &[1][..], &[7, 43_001, 3, 100_003, 19][..]] {
                let mut f = drive();
                if command == 0 {
                    seek(&mut f, 79);
                }
                f.write(3, 79);
                f.write(0, command | rate as u8);
                let end = event(ms, 79);
                partition(&mut f, end - 1, chunks);
                assert_eq!(f.read(0) & 1, 1);
                assert!(!f.irq);
                f.tick(1);
                assert!(f.irq);
                signals(
                    &mut f,
                    if command == 0 { 0 } else { 79 },
                    false,
                    command == 0,
                    true,
                );
                read_position(&mut f, if command == 0 { 0 } else { 79 });
            }
        }
    }
}

#[test]
fn media_lifecycle_cancels_future_movement() {
    for change in 0..3 {
        let mut f = drive();
        f.write(3, 79);
        f.write(0, 0x10);
        f.tick(30_000);
        assert_eq!(f.track, 2);
        match change {
            0 => f.mount(Disk::new(vec![0; 368640], true).unwrap()).unwrap(),
            1 => f.eject().unwrap(),
            _ => f.reset(),
        }
        f.tick(u32::MAX);
        let position = if change == 2 { 0 } else { 2 };
        assert_eq!(f.track, position);
        assert_eq!(f.irq, change != 2);
        assert_eq!(f.read(0) & 1, 0);
        assert!(!f.drq);
        assert!(!f.irq); // read(0) acknowledged any completion IRQ.
        if f.disk.is_none() {
            f.mount(Disk::new(vec![0; 368640], true).unwrap()).unwrap();
        }
        f.write(4, 0);
        f.write(5, 0x81);
        f.write(0, 0x80);
        byte_ready(&mut f);
        assert!(f.drq, "lifecycle change {change} retained head {position}");
    }
}

// Independent normal-track oracle: 200 ms revolution at 3,579,545 Hz;
// ceil(k * 715909 / 9) cycle positions, sector numbers 1..9.
const IDS: [u64; 9] = [
    0, 79546, 159091, 238637, 318182, 397728, 477273, 556819, 636364,
];
fn next_id(time: u64) -> u32 {
    let base = time / 715_909_000 * 715_909_000;
    let at = IDS
        .iter()
        .map(|p| {
            let candidate = base + p * 1000;
            if candidate < time {
                candidate + 715_909_000
            } else {
                candidate
            }
        })
        .min()
        .unwrap();
    at.div_ceil(1000) as u32
}
fn state(f: &mut Controller, track: u8, status: u8, irq: bool) {
    assert_eq!(f.read(1), track);
    assert!(!f.drq);
    assert_eq!(f.read(7), if irq { 128 } else { 0 });
    assert_eq!(f.read(0) & 0x91, status);
    assert!(!f.irq);
}

#[test]
fn all_families_rates_and_verify_values_have_separate_movement_settle_and_id_deadlines() {
    for size in [368640, 737280] {
        for (rate, ms) in [6_u64, 12, 20, 30].into_iter().enumerate() {
            for v in [0, 4] {
                for (cmd, track, head, intervals) in [
                    (0x00, 0, 0, 5),
                    (0x10, 7, 7, 2),
                    (0x20, 5, 6, 1),
                    (0x30, 6, 6, 1),
                    (0x40, 5, 6, 1),
                    (0x50, 6, 6, 1),
                    (0x60, 5, 4, 1),
                    (0x70, 4, 4, 1),
                ] {
                    let mut f = drive();
                    f.mount(Disk::new(vec![0xa5; size], false).unwrap())
                        .unwrap();
                    seek(&mut f, 5); // Exactly 6,000,000 cycles since reset.
                    f.write(2, 255); // ID verification never consults Sector.
                    f.write(3, 7);
                    f.write(0, cmd | v | rate as u8);
                    let movement = 3_579_545 * ms * intervals;
                    let end_move = movement.div_ceil(1000) as u32;
                    f.tick(end_move - 1);
                    assert_eq!(f.read(0) & 0x11, 1);
                    assert!(!f.irq);
                    f.tick(1);
                    state(&mut f, track, u8::from(v != 0), v == 0);
                    if v != 0 {
                        // Settling starts at the fractional movement deadline,
                        // not at the rounded tick that observed completion.
                        let settling = 6_000_000_000 + movement + 107_386_350;
                        let settle_at = settling.div_ceil(1000) as u32 - 6_000_000;
                        f.tick(settle_at - end_move - 1);
                        state(&mut f, track, 1, false);
                        f.tick(1);
                        state(&mut f, track, 1, false);
                        f.tick(1);
                        state(&mut f, track, 1, false);
                        let end = if track == head {
                            next_id(settling)
                        } else {
                            (settling.div_ceil(715_909_000) as u32 + 4) * 715_909
                        } - 6_000_000;
                        f.tick(end - settle_at - 2);
                        state(&mut f, track, 1, false);
                        f.tick(1);
                        state(&mut f, track, if track == head { 0 } else { 16 }, true);
                        f.tick(1);
                        state(&mut f, track, if track == head { 0 } else { 16 }, false);
                    }
                    // A later sector access infers retained mechanical position.
                    f.write(2, 1);
                    f.write(1, head);
                    f.write(0, 0x80);
                    byte_ready(&mut f);
                    assert!(f.drq);
                    assert_eq!(f.read(3), 0xa5);
                }
            }
        }
    }
}

#[test]
fn no_step_verification_waits_for_settling_and_the_next_id_on_both_geometries() {
    for size in [368640, 737280] {
        for command in [0x04, 0x14, 0x64, 0x74] {
            for side in [0, 1] {
                let mut f = Controller::default();
                f.mount(Disk::new(vec![0xff; size], false).unwrap())
                    .unwrap();
                f.write(5, 0x81);
                f.write(4, side);
                f.write(2, 0); // Invalid Sector is unrelated to ID cylinder matching.
                f.write(0, command);
                state(&mut f, 0, 1, false);
                f.tick(107_386);
                state(&mut f, 0, 1, false);
                f.tick(1); // ceil(30 ms), still before next ID at 159091.
                state(&mut f, 0, 1, false);
                f.tick(1);
                state(&mut f, 0, 1, false);
                let missing = size == 368640 && side == 1;
                let end = if missing { 3_579_545 } else { 159_091 };
                f.tick(end - 107_388 - 1);
                state(&mut f, 0, 1, false);
                f.tick(1);
                state(&mut f, 0, if missing { 16 } else { 0 }, true);
                f.tick(1);
                state(&mut f, 0, if missing { 16 } else { 0 }, false);
            }
        }
    }
}

#[test]
fn verification_processes_every_id_position_and_index_before_fifth_index_failure() {
    for (i, phase) in IDS.into_iter().enumerate() {
        let mut f = drive();
        // Arrange no-step SEEK so settling ends just before each ID; ID 1 at
        // phase zero also wins over index. Wrap once for positions < 30 ms.
        let id = 715_909 + phase as u32;
        let start = id - 107_387;
        f.tick(start);
        f.write(0, 0x14);
        f.tick(107_386);
        state(&mut f, 0, 1, false);
        f.tick(1);
        state(&mut f, 0, 0, true);
        assert_eq!(f.sector, 1, "ID {} does not update Sector", i + 1);
    }
    // Mismatch consumes four complete rising-index events and fails on fifth.
    let mut f = drive();
    f.write(1, 1);
    f.write(3, 1);
    f.write(0, 0x14);
    f.tick(4 * 715_909);
    state(&mut f, 1, 1, false);
    f.tick(715_908);
    state(&mut f, 1, 1, false);
    f.tick(1);
    state(&mut f, 1, 16, true);
}

#[test]
fn diskless_and_motor_off_a_position_without_ready_and_verification_waits_for_input() {
    for media in [false, true] {
        let mut f = drive();
        if !media {
            f.eject().unwrap();
        }
        f.write(5, 1); // Installed A, motor off, zero sensor still connected.
        assert_eq!(f.read(0) & 0x84, 0x84);
        f.write(3, 5);
        f.write(0, 0x14);
        f.tick(event(6, 5));
        state(&mut f, 5, 0x81, false);
        f.tick(u32::MAX); // No invented elapsed revolutions or seek error.
        state(&mut f, 5, 0x81, false);
        f.write(5, 0x81);
        f.tick(715_909);
        if media {
            state(&mut f, 5, 0, true);
        } else {
            state(&mut f, 5, 0x81, false);
            f.write(0, 0xd0);
            f.mount(Disk::new(vec![5; 368640], false).unwrap()).unwrap();
        }
        read_position(&mut f, 5);
        f.eject().unwrap();
        f.write(0, 0);
        f.tick(event(6, 5));
        state(&mut f, 0, 0x80, true);
        assert_eq!(f.read(0) & 4, 4);
    }
}

#[test]
fn absent_drive_steps_update_registers_without_a_motion_or_sensor_leaks() {
    for select in [0, 2, 3, 0x80, 0x82, 0x83] {
        let mut f = drive();
        f.disk.as_mut().unwrap().write_protected = true;
        f.write(5, select);
        assert_eq!(f.read(0) & 0xc6, 0x80); // no zero/index/protect from A
        for cmd in [0x50, 0x30, 0x40, 0x20, 0x70, 0x60] {
            f.write(0, cmd);
            f.tick(event(6, 1));
            assert_eq!(f.read(0) & 0x91, 0x80);
        }
        f.write(3, 5);
        f.write(0, 0x10);
        f.tick(event(6, 10));
        state(&mut f, 5, 0x80, true);
        f.write(5, 0x81);
        read_position(&mut f, 0); // A was never pulsed.
    }
}

#[test]
fn restore_without_zero_feedback_exhausts_exactly_255_pulses_at_all_rates() {
    for (rate, ms) in [6, 12, 20, 30].into_iter().enumerate() {
        for v in [0, 4] {
            let mut f = drive();
            seek(&mut f, 3);
            f.write(5, 0x82); // B absent: cannot see A's sensor.
            f.write(0, v | rate as u8);
            state(&mut f, 254, 0x81, false);
            f.tick(event(ms, 254));
            state(&mut f, 0, 0x81, false); // 255 issued pulses, no wrap to FF.
            f.tick(event(ms, 255) - event(ms, 254) - 1);
            state(&mut f, 0, 0x81, false);
            f.tick(1);
            state(&mut f, 0, 0x90, true); // Exhaustion skips verification.
            f.tick(1);
            state(&mut f, 0, 0x90, false);
            f.write(5, 0x81);
            read_position(&mut f, 3);
        }
    }
}

#[test]
fn control_writes_route_subsequent_pulses_and_do_not_abort_type_one() {
    let mut f = drive();
    seek(&mut f, 3);
    f.write(3, 8);
    f.write(0, 0x10); // A head 4, Track 4.
    f.tick(event(6, 1)); // A head 5, Track 5.
    f.write(5, 0x82);
    assert_eq!(f.read(0) & 0x85, 0x81);
    f.tick(event(6, 2) - event(6, 1)); // B: Track 6 only.
    f.write(5, 1); // A, motor off.
    f.tick(event(6, 3) - event(6, 2)); // A head 6, Track 7.
    f.write(4, 1);
    f.tick(event(6, 4) - event(6, 3)); // A head 7, Track 8.
    f.tick(event(6, 5) - event(6, 4));
    state(&mut f, 8, 0x80, true);
    f.write(4, 0);
    f.write(5, 0x81);
    read_position(&mut f, 7);
}

#[test]
fn verification_partitioning_and_cancellation_cover_all_phases() {
    for missing_side in [false, true] {
        for chunks in [&[u32::MAX][..], &[1][..], &[31, 87_311, 2, 900_001][..]] {
            let mut f = drive();
            f.write(4, u8::from(missing_side));
            f.write(3, 3);
            f.write(0, 0x14);
            // Movement ends at 18 ms; settle at 48 ms; next ID 238637,
            // or fail at fifth index (one second) for the missing side.
            let end = if missing_side { 3_579_545 } else { 238_637 };
            partition(&mut f, end - 1, chunks);
            state(&mut f, 3, 1, false);
            f.tick(1);
            state(&mut f, 3, if missing_side { 16 } else { 0 }, true);
        }
    }
    for elapsed in [30_000, 80_000, 180_000, 2_000_000] {
        for cancel in [0xd0, 0xd8, 0xff] {
            let mut f = drive();
            f.write(4, 1); // Search cannot succeed.
            f.write(3, 3);
            f.write(0, 0x14);
            f.tick(elapsed);
            let track = if elapsed == 30_000 { 2 } else { 3 };
            if cancel == 0xff {
                f.reset();
            } else {
                f.write(0, cancel);
            }
            assert_eq!(f.irq, cancel == 0xd8);
            f.tick(u32::MAX);
            assert_eq!(f.track, if cancel == 0xff { 0 } else { track });
            assert_eq!(f.read(0) & 0x11, 0);
            if cancel != 0xff {
                f.write(4, 0);
                read_position(&mut f, track);
            }
        }
    }
}

#[test]
fn search_resumes_on_side_drive_and_motor_changes_and_counts_only_input_indexes() {
    let mut f = drive();
    f.write(4, 1);
    f.write(0, 4);
    f.tick(4 * 715_909);
    state(&mut f, 0, 1, false);
    f.write(5, 0x82);
    f.tick(10 * 715_909);
    state(&mut f, 0, 0x81, false);
    f.write(5, 1);
    f.tick(10 * 715_909);
    state(&mut f, 0, 0x81, false);
    f.write(4, 0);
    f.write(5, 0x81);
    f.tick(79_545);
    state(&mut f, 0, 1, false);
    f.tick(1);
    state(&mut f, 0, 0, true); // Matching ID, no remembered wall-clock timeout.
}

#[test]
fn matching_id_wins_at_the_fifth_index_and_ready_status_is_live() {
    let mut f = drive();
    f.write(4, 1);
    f.write(0, 4);
    f.tick(5 * 715_909 - 1);
    state(&mut f, 0, 1, false);
    f.write(4, 0); // Recover the ID input one cycle before the fifth index.
    f.tick(1);
    state(&mut f, 0, 0, true);
    f.eject().unwrap();
    f.write(5, 0x82);
    f.write(0, 0x14); // No-step, absent B, no index input.
    f.tick(107_387);
    state(&mut f, 0, 0x81, false);
    f.write(5, 0x81);
    f.tick(715_909);
    state(&mut f, 0, 0x81, false);
    f.write(0, 0xd0);
    f.mount(Disk::new(vec![0; 368640], false).unwrap()).unwrap();
    f.write(5, 1);
    state(&mut f, 0, 0x80, true);
    f.write(5, 0x81);
    state(&mut f, 0, 0, false);
    f.write(0, 0xd4);
    f.tick(10 * 715_909 + 300_000); // Includes wraps even if final phase is higher.
    assert!(f.irq);
}

#[test]
fn mount_and_eject_cancel_settling_and_verification_without_resuming() {
    for elapsed in [30_000, 80_000, 180_000, 2_000_000] {
        for eject in [false, true] {
            let mut f = drive();
            f.write(4, 1);
            f.write(3, 3);
            f.write(0, 0x14);
            f.tick(elapsed);
            let head = if elapsed == 30_000 { 2 } else { 3 };
            if eject {
                f.eject().unwrap();
            } else {
                f.mount(Disk::new(vec![0; 368640], false).unwrap()).unwrap();
            }
            f.read(0);
            f.tick(u32::MAX);
            state(&mut f, head, if eject { 128 } else { 0 }, false);
            if eject {
                f.mount(Disk::new(vec![0; 368640], false).unwrap()).unwrap();
            }
            f.write(4, 0);
            f.write(1, head);
            f.write(0, 0x80);
            byte_ready(&mut f);
            assert!(f.drq);
        }
    }
}

#[test]
fn fractional_settling_deadline_can_coincide_exactly_with_id_and_index() {
    for missing in [false, true] {
        for chunks in [&[u32::MAX][..], &[1][..], &[2, 91_017, 13][..]] {
            let mut f = drive();
            f.write(4, u8::from(missing));
            f.write(3, 19);
            f.write(0, 0x17); // 19 * 30 ms movement + 30 ms settle = 600 ms.
                              // Movement ends at 2,040,340.65 cycles; settling must not round
                              // independently to 2,147,728. ID/index are exactly at 2,147,727.
            partition(&mut f, 2_147_726, chunks);
            state(&mut f, 19, 1, false);
            f.tick(1);
            state(&mut f, 19, u8::from(missing), !missing);
            f.tick(1);
            state(&mut f, 19, u8::from(missing), false);
            if missing {
                // The index at settling/search start is the first of five.
                f.tick(4 * 715_909 - 2);
                state(&mut f, 19, 1, false);
                f.tick(1);
                state(&mut f, 19, 16, true);
            }
        }
    }
}
