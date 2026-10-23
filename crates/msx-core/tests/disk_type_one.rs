//! Original movement traces calculated from milliseconds and the CPU clock.
//! No production timing constants or physical-head access are used as an oracle.
use msx_core::disk::{Controller, Disk, BYTE_CYCLES};

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
    assert_eq!(f.read(1), track);
    assert!(!f.drq);
    assert_eq!(f.irq, irq);
    assert_eq!(f.read(7), if irq { 0x80 } else { 0 });
    assert_eq!(f.read(0) & 0x15, u8::from(busy) | if zero { 4 } else { 0 });
    assert!(!f.irq); // Status acknowledges; the signal latch does not.
}
fn read_position(f: &mut Controller, track: u8) {
    f.write(1, track);
    f.write(0, 0x80);
    f.tick(BYTE_CYCLES);
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
            signals(&mut f, track, false, false, command == 0xd8);
            f.tick(u32::MAX);
            signals(&mut f, track, false, false, false);
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
fn abort_and_media_lifecycle_cancel_future_movement() {
    for change in 0..6 {
        let mut f = drive();
        f.write(3, 79);
        f.write(0, 0x10);
        f.tick(30_000);
        assert_eq!(f.track, 2);
        match change {
            0 => f.write(4, 1),
            1 => f.write(5, 1),
            2 => f.mount(Disk::new(vec![0; 368640], true).unwrap()).unwrap(),
            3 => f.eject().unwrap(),
            4 => f.reset(),
            _ => f.disk = None, // Direct media loss is detected by tick.
        }
        f.tick(u32::MAX);
        let position = if change == 4 { 0 } else { 2 };
        assert_eq!(f.track, position);
        assert_eq!(f.irq, change != 4);
        assert_eq!(f.read(0) & 1, 0);
        assert!(!f.drq);
        assert!(!f.irq); // read(0) acknowledged any completion IRQ.
        if f.disk.is_none() {
            f.mount(Disk::new(vec![0; 368640], true).unwrap()).unwrap();
        }
        f.write(4, 0);
        f.write(5, 0x81);
        f.write(0, 0x80);
        f.tick(BYTE_CYCLES);
        assert!(f.drq, "lifecycle change {change} retained head {position}");
    }
}
