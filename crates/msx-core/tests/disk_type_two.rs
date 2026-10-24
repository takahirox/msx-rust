//! Original timestamp oracles for the selected raw-DSK profile (Issues #57/#59).
use msx_core::disk::{Controller, Disk};

const REV: u32 = 715_909;
// Independently calculated ceil(k * 715909 / 9), not controller helpers.
const IDS: [u32; 9] = [
    0, 79_546, 159_091, 238_637, 318_182, 397_728, 477_273, 556_819, 636_364,
];

// Independent rational oracle: 32 us at 3,579,545 CPU Hz. Never round each byte.
fn byte_at(id: u32, n: u32) -> u32 {
    id + (u64::from(n) * 3_579_545 * 32).div_ceil(1_000_000) as u32
}
fn request_at(id: u32, write: bool, index: u32) -> u32 {
    byte_at(
        id,
        if write {
            if index == 0 {
                9
            } else {
                43 + index
            }
        } else {
            46 + index
        },
    )
}

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
                            let first = request_at(id, write, 0);
                            at(&mut f, &mut now, first - 1);
                            state(&mut f, 1, false, false);
                            at(&mut f, &mut now, first);
                            state(&mut f, 3, false, true);
                            at(&mut f, &mut now, first + 1);
                            state(&mut f, 3, false, true);
                            for byte in 0..512 {
                                if byte != 0 {
                                    at(&mut f, &mut now, request_at(id, write, byte));
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
                            state(&mut f, 1, false, false);
                            at(&mut f, &mut now, byte_at(id, 559) - 1);
                            state(&mut f, 1, false, false);
                            assert!(!f.disk.as_ref().unwrap().dirty);
                            at(&mut f, &mut now, byte_at(id, 559));
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
    for target in [159_090, 159_091, 159_092, 880_269] {
        at(&mut f, &mut now, target);
        state(&mut f, 1, false, false);
    }
    at(&mut f, &mut now, 880_270); // next ID 3 + 46 bytes
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
                f.tick(5_270);
                if c == 0 || s >> 3 == side {
                    state(&mut f, 3, false, true);
                } else {
                    state(&mut f, 1, false, false);
                    f.tick(4 * REV - 5_270);
                    state(&mut f, 0x10, true, false);
                }
            }
        }
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
            // The CRC tail ends <30 ms before the next ID. Repeating E
            // would miss it; completion must start a new rotational search.
            for (sector, id) in [(8, 556_819), (9, 636_364)] {
                let first = request_at(id, write, 0);
                at(&mut f, &mut now, first - 1);
                state(&mut f, 1, false, false);
                at(&mut f, &mut now, first);
                state(&mut f, 3, false, true);
                for i in 0..512 {
                    if i != 0 {
                        at(&mut f, &mut now, request_at(id, write, i));
                    }
                    if write {
                        f.write(3, sector);
                    } else {
                        assert_eq!(f.read(3), [0xfe, 0xfb, 0xf8][(sector as usize - 1) % 3]);
                    }
                }
                assert_eq!(f.sector, sector);
                at(&mut f, &mut now, byte_at(id, 559));
                assert_eq!(f.sector, sector + 1);
                state(&mut f, 1, false, false);
                if write {
                    expected[(sector as usize - 1) * 512..sector as usize * 512].fill(sector);
                }
                assert_eq!(f.disk.as_ref().unwrap().bytes(), expected);
            }
            // Search begins at ceil(636364 + 559*114.54544) = 700395, first index 715909, fifth 3579545.
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
fn cancellation_in_settle_search_handoff_transfer_and_tail_never_resurrects_or_commits() {
    for write in [false, true] {
        // Sector 3 with E: settling, search, matched ID handoff, pending byte,
        // a partially serviced sector, and the final read byte's service window.
        // ReadTail: ceil(159091 + 557 * 114.54544) = 222893 through
        // ceil(159091 + 558 * 114.54544) - 1 = 223007 (independent timestamps).
        for elapsed in [
            107_386, 107_387, 159_091, 160_121, 160_122, 161_038, 161_039, 164_131, 164_360,
            164_361, 222_777, 222_893, 223_007, 223_008, 223_121,
        ] {
            for cancel in 0..7 {
                let mut f = drive(737_280);
                let original = f.disk.as_ref().unwrap().bytes().to_vec();
                f.write(2, 3);
                f.write(0, if write { 0xa4 } else { 0x84 });
                if write && elapsed > 161_038 {
                    f.tick(160_122);
                    f.write(3, 0x17);
                    f.tick(elapsed - 160_122);
                } else {
                    f.tick(elapsed);
                }
                if elapsed >= 160_122 && write {
                    f.write(3, 0x17);
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
    let mut now = 0;
    for i in 0..512 {
        at(&mut f, &mut now, request_at(0, true, i));
        f.write(3, 0x17);
    }
    at(&mut f, &mut now, byte_at(0, 559));
    expected[..512].fill(0x17);
    assert_eq!(f.sector, 2);
    state(&mut f, 1, false, false);
    assert!(f.eject().is_err());
    assert!(f
        .mount(Disk::new(vec![0; 368_640], false).unwrap())
        .is_err());
    at(&mut f, &mut now, request_at(79_546, true, 0));
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
        let first = if e == 0 { 84_816 } else { 800_725 };
        f.tick(first - 1);
        state(&mut f, 1, false, false);
        f.tick(1);
        state(&mut f, 3, false, true);
        assert_eq!(f.read(3), 0xfb);
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
        f.tick(if command == 0xa4 { 1_031 } else { 5_270 });
        state(&mut f, 3, false, true);
    }
}

// Advance the same timeline with one large tick, single cycles, or uneven chunks.
fn partition_at(f: &mut Controller, now: &mut u32, target: u32, mode: u8) {
    let mut remaining = target - *now;
    let mut i = 0;
    while remaining != 0 {
        let chunk = match mode {
            0 => remaining,
            1 => 1,
            _ => [3, 19, 1, 2003, 107_387][i % 5].min(remaining),
        };
        f.tick(chunk);
        remaining -= chunk;
        i += 1;
    }
    *now = target;
}
fn patterned(size: usize) -> Controller {
    let mut f = Controller::default();
    let bytes = (0..size).map(|i| (i * 73 + i / 256) as u8).collect();
    f.mount(Disk::new(bytes, false).unwrap()).unwrap();
    f.write(5, 0x81);
    f
}

#[test]
fn unread_arrivals_replace_the_register_and_complete_by_time_in_every_tick_partition() {
    // Include revolution wrap and an ID at a nonzero integral cycle phase.
    for (start, id, sector) in [(0, 0, 1), (REV - 10, REV, 1), (1, 79_546, 2)] {
        for mode in 0..3 {
            let mut f = patterned(737_280);
            let original = f.disk.as_ref().unwrap().bytes().to_vec();
            let mut now = start;
            f.tick(start);
            f.write(3, 0xa7);
            f.write(2, sector);
            f.write(0, 0x80);
            for i in 0..512 {
                let arrival = byte_at(id, 46 + i);
                partition_at(&mut f, &mut now, arrival - 1, mode);
                assert_eq!(
                    f.data,
                    if i == 0 {
                        0xa7
                    } else {
                        original[(sector as usize - 1) * 512 + i as usize - 1]
                    }
                );
                partition_at(&mut f, &mut now, arrival, mode);
                assert_eq!(f.data, original[(sector as usize - 1) * 512 + i as usize]);
                state(&mut f, if i == 0 { 3 } else { 7 }, false, true);
                partition_at(&mut f, &mut now, arrival + 1, mode);
                assert!(f.drq);
            }
            partition_at(&mut f, &mut now, byte_at(id, 559) - 1, mode);
            state(&mut f, 7, false, true);
            partition_at(&mut f, &mut now, byte_at(id, 559), mode);
            state(&mut f, 4, true, false);
            let last = original[sector as usize * 512 - 1];
            for _ in 0..4 {
                assert_eq!(f.read(3), last);
            }
            partition_at(&mut f, &mut now, byte_at(id, 559) + 1, mode);
            state(&mut f, 4, false, false);
            assert_eq!(f.disk.as_ref().unwrap().bytes(), original);
        }
    }
}

#[test]
fn read_service_acknowledges_only_current_data_and_final_starvation_is_sticky() {
    for missed in [vec![], vec![0], vec![0, 1, 5, 6, 7], vec![511]] {
        for mode in 0..3 {
            let mut f = patterned(368_640);
            let expected = f.disk.as_ref().unwrap().bytes()[..512].to_vec();
            let mut now = 0;
            f.write(0, 0x80);
            let mut lost = false;
            for i in 0..512 {
                let arrival = byte_at(0, 46 + i);
                partition_at(&mut f, &mut now, arrival - 1, mode);
                if i == 0 {
                    assert_eq!(f.read(3), 0);
                } else if !missed.contains(&(i - 1)) {
                    assert_eq!(f.read(3), expected[i as usize - 1]);
                    assert!(!f.drq); // repeated/early read cannot consume the next byte
                }
                partition_at(&mut f, &mut now, arrival, mode);
                if i != 0 && missed.contains(&(i - 1)) {
                    lost = true;
                }
                assert_eq!(f.data, expected[i as usize]);
                state(&mut f, 3 | if lost { 4 } else { 0 }, false, true);
                if !missed.contains(&i) {
                    // Alternating early/late service never moves an arrival.
                    partition_at(
                        &mut f,
                        &mut now,
                        arrival + if i % 2 == 0 { 80 } else { 1 },
                        mode,
                    );
                    assert_eq!(f.read(3), expected[i as usize]);
                    assert_eq!(f.read(3), expected[i as usize]);
                    assert!(!f.drq);
                }
            }
            partition_at(&mut f, &mut now, 63_916, mode); // just before final service expiry
            assert_eq!(f.read(0) & 4, if lost { 4 } else { 0 });
            partition_at(&mut f, &mut now, 63_917, mode); // ceil(558 * 114.54544)
            lost |= missed.contains(&511);
            assert_eq!(f.read(0) & 4, if lost { 4 } else { 0 });
            if missed.contains(&511) {
                assert!(f.drq); // still readable during CRC tail despite late service
                assert_eq!(f.read(3), expected[511]);
                assert!(!f.drq);
            }
            partition_at(&mut f, &mut now, 64_031, mode);
            state(&mut f, if lost { 4 } else { 0 }, true, false);
            f.write(0, 0x80);
            state(&mut f, 1, false, false); // accepted new command clears LOST DATA
            f.reset();
            assert_eq!(f.read(0) & 1, 0);
        }
    }
}

#[test]
fn initial_write_has_a_distinct_eight_byte_window_and_ignores_idle_data() {
    for mode in 0..3 {
        for late in [false, true] {
            let mut f = patterned(368_640);
            let original = f.disk.as_ref().unwrap().bytes().to_vec();
            f.write(3, 0x99); // idle writes are not future sector service
            f.write(0, 0xa0);
            let mut now = 0;
            partition_at(&mut f, &mut now, 1_030, mode);
            f.write(3, 0x98); // before request: likewise not service
            state(&mut f, 1, false, false);
            partition_at(&mut f, &mut now, 1_031, mode);
            state(&mut f, 3, false, true);
            assert_eq!(f.read(3), 0x98); // reading does not supply a write request
            assert!(f.drq);
            if late {
                partition_at(&mut f, &mut now, 1_947, mode);
                f.write(3, 0x61); // last integer cycle before initial check
                f.write(3, 0x62); // overwrite the holding register before the check
                assert_eq!(f.read(3), 0x62); // read during write cannot acknowledge future requests
            }
            partition_at(&mut f, &mut now, 1_948, mode);
            if late {
                state(&mut f, 1, false, false);
                partition_at(&mut f, &mut now, 5_039, mode);
                state(&mut f, 1, false, false);
                partition_at(&mut f, &mut now, 5_040, mode);
                state(&mut f, 3, false, true);
                partition_at(&mut f, &mut now, 64_030, mode);
                assert_eq!(f.disk.as_ref().unwrap().bytes(), original);
                partition_at(&mut f, &mut now, 64_031, mode);
                state(&mut f, 4, true, false);
                let mut expected = original;
                expected[..512].fill(0);
                expected[0] = 0x62;
                assert_eq!(f.disk.as_ref().unwrap().bytes(), expected);
                assert!(f.disk.as_ref().unwrap().dirty);
            } else {
                state(&mut f, 4, true, false);
                f.write(3, 0x61); // at/after the check cannot resurrect an aborted write
                partition_at(&mut f, &mut now, REV, mode);
                state(&mut f, 4, false, false);
                assert_eq!(f.disk.as_ref().unwrap().bytes(), original);
                assert!(!f.disk.as_ref().unwrap().dirty);
            }
        }
    }
}

#[test]
fn later_write_underruns_zero_fill_missed_positions_and_resume_without_moving_deadlines() {
    for id in [0, REV] {
        for missed in [vec![], vec![1], vec![1, 2, 3, 100, 101, 511], vec![511]] {
            for mode in 0..3 {
                let mut f = patterned(737_280);
                let original = f.disk.as_ref().unwrap().bytes().to_vec();
                let mut expected = original.clone();
                let mut now = if id == 0 { 0 } else { id - 10 };
                f.tick(now); // command before wrap; all partitions cross the ID/index
                f.write(0, 0xa0);
                for i in 0..512 {
                    let request = request_at(id, true, i);
                    partition_at(&mut f, &mut now, request - 1, mode);
                    assert!(!f.drq || (i > 1 && missed.contains(&(i - 1))));
                    partition_at(&mut f, &mut now, request, mode);
                    assert!(f.drq);
                    if missed.contains(&i) {
                        expected[i as usize] = 0;
                    } else {
                        partition_at(
                            &mut f,
                            &mut now,
                            request + if i % 2 == 0 { 80 } else { 1 },
                            mode,
                        );
                        let data = (i * 37 + 11) as u8;
                        expected[i as usize] = !data;
                        f.write(3, data);
                        f.write(3, !data); // overwrite current data without queuing another byte
                        assert!(!f.drq);
                    }
                    assert_eq!(f.disk.as_ref().unwrap().bytes(), original);
                }
                partition_at(&mut f, &mut now, id + 63_572, mode); // last byte not yet consumed
                assert!(!f.irq);
                partition_at(&mut f, &mut now, id + 63_573, mode);
                assert!(!f.drq);
                assert!(!f.irq);
                assert_eq!(f.disk.as_ref().unwrap().bytes(), original);
                partition_at(&mut f, &mut now, id + 64_030, mode);
                assert_eq!(f.disk.as_ref().unwrap().bytes(), original);
                partition_at(&mut f, &mut now, id + 64_031, mode);
                state(&mut f, if missed.is_empty() { 0 } else { 4 }, true, false);
                assert_eq!(f.disk.as_ref().unwrap().bytes(), expected);
                assert!(f.disk.as_ref().unwrap().dirty);
                partition_at(&mut f, &mut now, id + 64_032, mode);
                assert!(!f.irq);
            }
        }
    }
}
#[test]
fn write_consumption_samples_the_current_holding_register_without_queuing_bytes() {
    for size in [368_640, 737_280] {
        for mode in 0..3 {
            let mut f = patterned(size);
            let original = f.disk.as_ref().unwrap().bytes().to_vec();
            let mut expected = original.clone();
            let mut now = 0;
            f.write(0, 0xa0);
            partition_at(&mut f, &mut now, 1_031, mode);
            f.write(3, 0x61);
            assert!(!f.drq);
            partition_at(&mut f, &mut now, 1_947, mode);
            f.write(3, 0x62); // last cycle before the initial check at ceil(17B)
            assert_eq!(f.read(3), 0x62);
            partition_at(&mut f, &mut now, 1_948, mode);
            state(&mut f, 1, false, false);
            f.write(3, 0x63); // already latched: cannot replace byte 0 or supply byte 1
            partition_at(&mut f, &mut now, 5_039, mode);
            assert!(!f.drq);
            partition_at(&mut f, &mut now, 5_040, mode);
            assert!(f.drq);
            f.write(3, 0x71);
            partition_at(&mut f, &mut now, 5_041, mode);
            f.write(3, 0x72);
            assert!(!f.drq);
            partition_at(&mut f, &mut now, 5_154, mode);
            assert_eq!(f.read(3), 0x72);
            state(&mut f, 1, false, false);
            partition_at(&mut f, &mut now, 5_155, mode); // ceil(45B): consume byte 1
            state(&mut f, 3, false, true); // next request still needs its own service
            partition_at(&mut f, &mut now, 5_270, mode); // ceil(46B): byte 2 missed
            state(&mut f, 7, false, true);
            assert_eq!(f.disk.as_ref().unwrap().bytes(), original);
            partition_at(&mut f, &mut now, 64_031, mode);
            state(&mut f, 4, true, false);
            expected[..512].fill(0);
            expected[0] = 0x62;
            expected[1] = 0x72;
            assert_eq!(&f.disk.as_ref().unwrap().bytes()[..2], &[0x62, 0x72]);
            assert_eq!(f.disk.as_ref().unwrap().bytes(), expected);
            assert!(f.disk.as_ref().unwrap().dirty);
        }
    }
}

#[test]
fn write_service_at_a_missed_deadline_belongs_to_the_new_request() {
    let mut f = patterned(368_640);
    let mut expected = f.disk.as_ref().unwrap().bytes().to_vec();
    let mut now = 0;
    f.write(0, 0xa0);
    at(&mut f, &mut now, 1_031);
    f.write(3, 0x61);
    at(&mut f, &mut now, 5_154);
    state(&mut f, 3, false, true);
    at(&mut f, &mut now, 5_155); // byte 1 missed; request is now for byte 2
    state(&mut f, 7, false, true);
    f.write(3, 0x72);
    at(&mut f, &mut now, 64_031);
    expected[..512].fill(0);
    expected[0] = 0x61;
    expected[2] = 0x72;
    state(&mut f, 4, true, false);
    assert_eq!(f.disk.as_ref().unwrap().bytes(), expected);
}

#[test]
fn lost_data_survives_multi_record_searches_data_and_rnf_until_new_command() {
    for write in [false, true] {
        let mut f = drive(368_640);
        let mut now = 0;
        f.write(2, 8);
        f.write(0, if write { 0xb4 } else { 0x94 });
        if write {
            at(&mut f, &mut now, request_at(556_819, true, 0));
            f.write(3, 0x61);
        }
        at(&mut f, &mut now, byte_at(556_819, 559));
        state(&mut f, 5, false, false);
        assert_eq!(f.sector, 9);
        for i in 0..512 {
            at(&mut f, &mut now, request_at(636_364, write, i));
            state(&mut f, 7, false, true);
            if write {
                f.write(3, 0x72);
            } else {
                assert_eq!(f.read(3), 0xf8);
            }
        }
        at(&mut f, &mut now, byte_at(636_364, 559));
        state(&mut f, 5, false, false);
        assert_eq!(f.sector, 10);
        at(&mut f, &mut now, 5 * REV);
        state(&mut f, 0x14, true, false);
        f.write(0, 0x80);
        state(&mut f, 1, false, false);
    }
}
