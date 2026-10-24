//! Original timestamp oracles for the selected raw-DSK profile (Issue #57).
use msx_core::disk::{Controller, Disk};

const REV: u32 = 715_909;
// Independently calculated ceil(k * 715909 / 9), not controller helpers.
const IDS: [u32; 9] = [
    0, 79_546, 159_091, 238_637, 318_182, 397_728, 477_273, 556_819, 636_364,
];

fn drive(size: usize) -> Controller {
    let mut f = Controller::default();
    let mut bytes = vec![0; size];
    for (i, sector) in bytes.chunks_mut(512).enumerate() {
        sector.fill([0xfe, 0xfb, 0xf8][i % 3]); // payload marks are not CRC/ID metadata
    }
    f.mount(Disk::new(bytes, false).unwrap()).unwrap();
    f.write(5, 0x81);
    f
}
fn at(f: &mut Controller, now: &mut u32, target: u32) {
    f.tick(target - *now);
    *now = target;
}
fn state(f: &mut Controller, status: u8, irq: bool, drq: bool) {
    assert_eq!((f.irq, f.drq), (irq, drq));
    assert_eq!(
        f.read(7),
        (if irq { 0x80 } else { 0 }) | (if drq { 0x40 } else { 0 })
    );
    assert_eq!(f.read(0), status);
    assert!(!f.irq); // status acknowledges IRQ, signal read did not
}
fn next_sector_id(start: u32, settle: bool, sector: usize) -> u32 {
    // Compare exact thousandths so 107386.35 cannot round down onto an ID.
    let search = u64::from(start) * 1000 + if settle { 107_386_350 } else { 0 };
    let mut event = start / REV * REV + IDS[sector - 1];
    while u64::from(event) * 1000 < search {
        event += REV;
    }
    event
}

#[test]
fn every_id_read_write_e_side_and_geometry_has_an_independent_first_drq_deadline() {
    for size in [368_640, 737_280] {
        for side in 0..if size == 737_280 { 2 } else { 1 } {
            for write in [false, true] {
                for settle in [false, true] {
                    for sector in 1..=9 {
                        for start in [0, 1, 79_546, 636_365, REV - 1, REV] {
                            let mut f = drive(size);
                            let mut now = start;
                            f.tick(start);
                            f.write(4, side);
                            f.write(2, sector as u8);
                            f.write(
                                0,
                                (if write { 0xa0 } else { 0x80 }) | if settle { 4 } else { 0 },
                            );
                            state(&mut f, 1, false, false);
                            let id = next_sector_id(start, settle, sector);
                            if id > start {
                                at(&mut f, &mut now, id - 1);
                                state(&mut f, 1, false, false);
                            }
                            at(&mut f, &mut now, id);
                            state(&mut f, 1, false, false);
                            at(&mut f, &mut now, id + 113);
                            state(&mut f, 1, false, false);
                            at(&mut f, &mut now, id + 114);
                            state(&mut f, 3, false, true);
                            at(&mut f, &mut now, id + 115);
                            state(&mut f, 3, false, true);
                            for byte in 0..512 {
                                if byte != 0 {
                                    f.tick(114);
                                }
                                assert!(f.drq);
                                if write {
                                    f.write(3, byte as u8);
                                } else {
                                    assert_eq!(
                                        f.read(3),
                                        [0xfe, 0xfb, 0xf8][(side as usize * 9 + sector - 1) % 3]
                                    );
                                }
                            }
                            state(&mut f, 0, true, false);
                            if write {
                                let disk = f.disk.as_ref().unwrap();
                                let offset = (side as usize * 9 + sector - 1) * 512;
                                let mut expected = drive(size).disk.unwrap().bytes().to_vec();
                                for byte in 0..512 {
                                    expected[offset + byte] = byte as u8;
                                }
                                assert_eq!(disk.bytes(), expected);
                                assert!(disk.dirty);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn fractional_settling_does_not_accept_the_id_just_before_its_deadline() {
    let mut f = drive(368_640);
    let mut now = 51_705;
    f.tick(now);
    f.write(2, 3);
    f.write(0, 0x84); // search starts at 159091.35, after ID 3 at 159091
    for target in [159_090, 159_091, 159_092, 875_113] {
        at(&mut f, &mut now, target);
        state(&mut f, 1, false, false);
    }
    at(&mut f, &mut now, 875_114); // next ID 3 + 114
    state(&mut f, 3, false, true);
}

#[test]
fn unmatched_searches_fail_on_fifth_observed_index_for_both_e_values() {
    for size in [368_640, 737_280] {
        for cmd in [0x80, 0x84, 0xa0, 0xa4] {
            for start in [0, 1, REV - 1, REV] {
                for mismatch in 0..6 {
                    let mut f = drive(size);
                    let mut now = start;
                    f.tick(start);
                    let mut command = cmd;
                    match mismatch {
                        0 => f.write(1, 1), // cylinder mismatch
                        1 => f.write(2, 0),
                        2 => f.write(2, 10),
                        3 => f.write(2, 255),
                        4 => command |= 0x0a, // C=1, S=1, selected ID side 0
                        _ if size == 368_640 => f.write(4, 1), // no IDs, still index
                        _ => {
                            f.write(4, 1);
                            command |= 2;
                        } // C=1, S=0 vs side 1
                    }
                    f.write(0, command);
                    state(&mut f, 1, false, false);
                    let search =
                        u64::from(start) * 1000 + if cmd & 4 != 0 { 107_386_350 } else { 0 };
                    let first_index = search.div_ceil(u64::from(REV) * 1000) as u32 * REV;
                    let failure = first_index + 4 * REV;
                    for index in 0..4 {
                        at(&mut f, &mut now, first_index + index * REV);
                        state(&mut f, 1, false, false);
                    }
                    at(&mut f, &mut now, failure - 1);
                    state(&mut f, 1, false, false);
                    at(&mut f, &mut now, failure);
                    state(&mut f, 0x10, true, false);
                    at(&mut f, &mut now, failure + 1);
                    state(&mut f, 0x10, false, false);
                    assert!(!f.disk.as_ref().unwrap().dirty);
                }
            }
        }
    }
}

#[test]
fn c_controls_comparison_of_s_against_the_id_side_lsb() {
    for side in [0, 1] {
        for c in [0, 2] {
            for s in [0, 8] {
                let mut f = drive(737_280);
                f.write(4, side);
                f.write(0, 0x80 | c | s);
                f.tick(114);
                if c == 0 || s >> 3 == side {
                    state(&mut f, 3, false, true);
                } else {
                    state(&mut f, 1, false, false);
                    f.tick(4 * REV - 114);
                    state(&mut f, 0x10, true, false);
                }
            }
        }
    }
}

#[test]
fn tick_partitions_agree_through_settle_search_handoff_and_pending_byte_timeout() {
    for (start, cmd, sector) in [
        (0, 0x84, 1),
        (51_705, 0xa4, 3),
        (0, 0x80, 1),
        (636_365, 0xa0, 9),
    ] {
        let first = next_sector_id(start, cmd & 4 != 0, sector) + 114;
        let mut large = drive(368_640);
        let mut single = drive(368_640);
        let mut irregular = drive(368_640);
        for f in [&mut large, &mut single, &mut irregular] {
            f.tick(start);
            f.write(2, sector as u8);
            f.write(0, cmd);
        }
        let mut now = start;
        for target in [
            first - 1,
            first,
            first + 1,
            first + 4095,
            first + 4096,
            first + 4097,
        ] {
            large.tick(target - now);
            for _ in now..target {
                single.tick(1);
            }
            let mut remaining = target - now;
            for chunk in [3, 107_387, 17, 400_003, u32::MAX] {
                let step = remaining.min(chunk);
                irregular.tick(step);
                remaining -= step;
            }
            let (status, irq, drq) = if target < first {
                (1, false, false)
            } else if target < first + 4096 {
                (3, false, true)
            } else {
                (4, target == first + 4096, false)
            };
            for f in [&mut large, &mut single, &mut irregular] {
                state(f, status, irq, drq);
            }
            now = target;
        }
        assert!(!large.disk.as_ref().unwrap().dirty);
    }
}

#[test]
fn multi_record_searches_again_without_settling_and_sector_ten_cannot_cross_tracks() {
    for size in [368_640, 737_280] {
        for write in [false, true] {
            let mut f = drive(size);
            let original = f.disk.as_ref().unwrap().bytes().to_vec();
            let mut expected = original.clone();
            let mut now = 0;
            f.write(2, 8);
            f.write(0, if write { 0xb4 } else { 0x94 });
            // ID8 + 114, then ID9 + 114. Repeating the 30 ms delay would
            // miss ID9: sector 8 finishes only 21177 cycles before its ID.
            for (sector, first) in [(8, 556_933), (9, 636_478)] {
                at(&mut f, &mut now, first - 1);
                state(&mut f, 1, false, false);
                at(&mut f, &mut now, first);
                state(&mut f, 3, false, true);
                for i in 0..512 {
                    if i != 0 {
                        at(&mut f, &mut now, first + i * 114);
                    }
                    if write {
                        f.write(3, sector);
                    } else {
                        assert_eq!(f.read(3), [0xfe, 0xfb, 0xf8][(sector as usize - 1) % 3]);
                    }
                }
                assert_eq!(f.sector, sector + 1);
                state(&mut f, 1, false, false);
                if write {
                    expected[(sector as usize - 1) * 512..sector as usize * 512].fill(sector);
                }
                assert_eq!(f.disk.as_ref().unwrap().bytes(), expected);
            }
            // Search begins at 694732, first index 715909, fifth 3579545.
            at(&mut f, &mut now, 5 * REV - 1);
            state(&mut f, 1, false, false);
            at(&mut f, &mut now, 5 * REV);
            state(&mut f, 0x10, true, false);
            at(&mut f, &mut now, 5 * REV + 1);
            state(&mut f, 0x10, false, false);
            assert_eq!(f.track, 0);
            assert_eq!(f.sector, 10);
            assert_eq!(f.disk.as_ref().unwrap().bytes(), expected);
            assert_eq!(f.disk.as_ref().unwrap().dirty, write);
        }
    }
}

#[test]
fn cancellation_in_settle_search_handoff_and_partial_transfer_never_resurrects_or_commits() {
    for write in [false, true] {
        // Sector 3 with E: settling, search, matched ID handoff, pending byte,
        // and a partially serviced sector.
        for elapsed in [107_386, 107_387, 159_091, 159_204, 159_205, 159_206] {
            for cancel in 0..7 {
                let mut f = drive(737_280);
                let original = f.disk.as_ref().unwrap().bytes().to_vec();
                f.write(2, 3);
                f.write(0, if write { 0xa4 } else { 0x84 });
                f.tick(elapsed);
                if elapsed == 159_206 {
                    if write {
                        f.write(3, 0x17);
                    } else {
                        f.read(3);
                    }
                }
                // Busy writes cannot change ID matching registers or commands.
                f.write(1, 10);
                f.write(2, 7);
                f.write(0, 0x10);
                assert_eq!((f.track, f.sector), (0, 3));
                match cancel {
                    0 => f.write(0, 0xd0),
                    1 => f.write(0, 0xd8),
                    2 => f.reset(),
                    3 => f
                        .mount(Disk::new(original.clone(), false).unwrap())
                        .unwrap(),
                    4 => f.eject().unwrap(),
                    5 => f.write(4, 1),
                    _ => f.write(5, 1),
                }
                assert!(!f.drq);
                assert_eq!(f.read(0) & 1, 0);
                f.tick(u32::MAX);
                assert!(!f.drq);
                assert!(!f.irq); // no new completion after acknowledgement
                assert_eq!(f.read(0) & 1, 0);
                if let Some(disk) = &f.disk {
                    assert_eq!(disk.bytes(), original);
                    assert!(!disk.dirty);
                } else {
                    assert_eq!(cancel, 4);
                }
            }
        }
    }
}

#[test]
fn multi_write_cancellation_keeps_completed_sectors_and_dirty_media_guards() {
    let mut f = drive(368_640);
    let mut expected = f.disk.as_ref().unwrap().bytes().to_vec();
    f.write(0, 0xb0);
    f.tick(114);
    for i in 0..512 {
        if i != 0 {
            f.tick(114);
        }
        f.write(3, 0x17);
    }
    expected[..512].fill(0x17);
    assert_eq!(f.sector, 2);
    state(&mut f, 1, false, false);
    assert!(f.eject().is_err());
    assert!(f
        .mount(Disk::new(vec![0; 368_640], false).unwrap())
        .is_err());
    f.tick(79_660 - 58_368); // ID2 + handoff, no repeated settling
    f.write(3, 0x23);
    f.write(0, 0xd8);
    f.read(0);
    f.tick(u32::MAX);
    assert!(!f.irq);
    assert!(!f.drq);
    assert_eq!(f.disk.as_ref().unwrap().bytes(), expected);
    assert!(f.disk.as_ref().unwrap().dirty);
}

#[test]
fn not_ready_and_write_protection_reject_before_settling_but_protected_reads_search() {
    for e in [0, 4] {
        for control in [0, 1, 2, 0x80, 0x82] {
            for command in [0x80, 0xa0] {
                let mut f = drive(368_640);
                f.write(5, control);
                f.write(0, command | e);
                state(&mut f, 0x80, true, false);
                f.tick(u32::MAX);
                state(&mut f, 0x80, false, false);
            }
        }
        let mut f = drive(368_640);
        f.disk.as_mut().unwrap().write_protected = true;
        f.write(0, 0xa0 | e);
        state(&mut f, 0x40, true, false);
        f.write(2, 2);
        f.write(0, 0x80 | e);
        let first = if e == 0 { 79_660 } else { 795_569 };
        f.tick(first - 1);
        state(&mut f, 1, false, false);
        f.tick(1);
        state(&mut f, 3, false, true);
        assert_eq!(f.read(3), 0xfb);
    }
}

#[test]
fn one_tick_can_cross_settle_id_first_drq_and_timeout_without_reusing_elapsed_time() {
    for (command, sector, first) in [(0x80, 2, 79_660), (0x84, 1, 716_023), (0xa4, 3, 159_205)] {
        for target in [
            first - 1,
            first,
            first + 4095,
            first + 4096,
            first + 4097,
            10 * REV,
        ] {
            let mut f = drive(368_640);
            f.write(2, sector);
            f.write(0, command);
            f.tick(target);
            if target < first {
                state(&mut f, 1, false, false);
            } else if target < first + 4096 {
                state(&mut f, 3, false, true);
            } else {
                state(&mut f, 4, true, false);
            }
        }
    }
}

#[test]
fn unchanged_controls_do_not_cancel_settling_or_search() {
    for command in [0x84, 0xa4] {
        let mut f = drive(737_280);
        f.write(2, 3);
        f.write(0, command);
        let mut now = 0;
        for elapsed in [107_386, 107_387, 159_091] {
            at(&mut f, &mut now, elapsed);
            f.write(4, 0);
            f.write(5, 0x81);
            state(&mut f, 1, false, false);
        }
        f.tick(114);
        state(&mut f, 3, false, true);
    }
}
