//! Independent Read Address byte/status/timestamp oracles for Issue #69.
use msx_core::disk::{Controller, Disk};

const REV: u32 = 715_909;
const IDS: [u32; 9] = [
    0, 79_546, 159_091, 238_637, 318_182, 397_728, 477_273, 556_819, 636_364,
];
// ceil(n * 32us * 3579545Hz), n=2..9. FE starts at H, C arrives at H+2B,
// CRC-low at H+7B, its service budget ends H+8B, completion is H+9B.
const EVENTS: [u32; 8] = [230, 344, 459, 573, 688, 802, 917, 1031];
// Independent CRC-HQX table (Python binascii.crc_hqx, init FFFF), not core helpers.
const CRCS: [[u16; 9]; 6] = [
    [
        0xca6f, 0x9f3c, 0xac0d, 0x359a, 0x06ab, 0x53f8, 0x60c9, 0x70f7, 0x43c6,
    ],
    [
        0xfd5f, 0xa80c, 0x9b3d, 0x02aa, 0x319b, 0x64c8, 0x57f9, 0x47c7, 0x74f6,
    ],
    [
        0x2707, 0x7254, 0x4165, 0xd8f2, 0xebc3, 0xbe90, 0x8da1, 0x9d9f, 0xaeae,
    ],
    [
        0x1037, 0x4564, 0x7655, 0xefc2, 0xdcf3, 0x89a0, 0xba91, 0xaaaf, 0x999e,
    ],
    [
        0x701d, 0x254e, 0x167f, 0x8fe8, 0xbcd9, 0xe98a, 0xdabb, 0xca85, 0xf9b4,
    ],
    [
        0x472d, 0x127e, 0x214f, 0xb8d8, 0x8be9, 0xdeba, 0xed8b, 0xfdb5, 0xce84,
    ],
];
fn drive(size: usize) -> Controller {
    let mut f = Controller::default();
    // Payloads deliberately resemble address/data marks. They are not IDs.
    let bytes = (0..size).map(|i| [0xa1, 0xfe, 0xfb, 0xf8][i % 4]).collect();
    f.mount(Disk::new(bytes, true).unwrap()).unwrap();
    f.write(5, 0x81);
    f
}
fn at(f: &mut Controller, now: &mut u32, target: u32, mode: u8) {
    f.tick(0); // consume an inclusive ID at the command-start timestamp
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
fn state(f: &mut Controller, status: u8, irq: bool, drq: bool) {
    for _ in 0..3 {
        assert_eq!((f.irq, f.drq), (irq, drq));
        assert_eq!(f.read(7), (u8::from(irq) * 0x80) | (u8::from(drq) * 0x40));
    }
    assert_eq!(f.read(0), status);
    assert!(!f.irq);
}
fn next_id(start: u32, e: bool) -> (u32, usize) {
    // Retain fractional 30ms: search at 159091.35 must skip ID at 159091.
    let search = u64::from(start) * 100 + if e { 10_738_635 } else { 0 };
    (0..9)
        .map(|i| {
            let mut id = start / REV * REV + IDS[i];
            while u64::from(id) * 100 < search {
                id += REV;
            }
            (id, i)
        })
        .min()
        .unwrap()
}

#[test]
fn both_geometries_sides_cylinders_all_ids_settling_and_boundary_starts() {
    for size in [368_640, 737_280] {
        for (ci, cylinder) in [0, 2, 79].into_iter().enumerate() {
            for side in 0..if size == 737_280 { 2 } else { 1 } {
                for e in [false, true] {
                    // Each ID exactly, one cycle later, wrap, and fractional settle edge.
                    for start in
                        IDS.into_iter()
                            .flat_map(|id| [id, id + 1])
                            .chain([REV - 1, REV, 51_705])
                    {
                        let mut f = drive(size);
                        let original = f.disk.as_ref().unwrap().bytes().to_vec();
                        f.write(3, cylinder);
                        f.write(0, 0x10);
                        f.tick(3 * REV); // all seeks finish, rotation returns to zero
                        f.write(1, 123); // deliberately wrong Track and Sector must be ignored
                        f.write(2, 255);
                        f.write(4, side);
                        f.tick(start);
                        let mut now = 3 * REV + start;
                        let (id, index) = next_id(now, e);
                        let crc = CRCS[ci * 2 + side as usize][index];
                        let expected = [
                            cylinder,
                            side,
                            index as u8 + 1,
                            2,
                            (crc >> 8) as u8,
                            crc as u8,
                        ];
                        f.write(0, if e { 0xc4 } else { 0xc0 });
                        state(&mut f, 1, false, false);
                        if id > now {
                            at(&mut f, &mut now, id - 1, 0);
                            assert_eq!(f.sector, 255);
                        }
                        at(&mut f, &mut now, id, 0);
                        assert_eq!((f.track, f.sector), (123, cylinder));
                        for (i, byte) in expected.into_iter().enumerate() {
                            at(&mut f, &mut now, id + EVENTS[i] - 1, 0);
                            state(&mut f, 1, false, false);
                            at(&mut f, &mut now, id + EVENTS[i], 0);
                            state(&mut f, 3, false, true);
                            for _ in 0..3 {
                                assert_eq!(f.read(3), byte);
                            }
                            assert!(!f.drq);
                        }
                        at(&mut f, &mut now, id + 1030, 0);
                        state(&mut f, 1, false, false);
                        at(&mut f, &mut now, id + 1031, 0);
                        state(&mut f, 0, true, false);
                        assert_eq!((f.track, f.sector), (123, cylinder));
                        // Physical head was preserved: a correct Type-II Track can read here.
                        f.write(1, cylinder);
                        f.write(2, 1);
                        f.write(0, 0x80);
                        f.tick(REV + 5270);
                        assert_eq!(f.read(0) & 0x10, 0);
                        assert_eq!(f.disk.as_ref().unwrap().bytes(), original);
                        assert!(!f.disk.as_ref().unwrap().dirty);
                    }
                }
            }
        }
    }
}

#[test]
fn successive_ids_and_wrap_ignore_previous_type_two_write_multi_and_side_compare() {
    let mut f = drive(737_280);
    f.disk.as_mut().unwrap().write_protected = false;
    f.write(0, 0xba); // multi write, compare side 1 against selected 0
    f.write(0, 0xd0);
    let mut now = 0;
    for (id, r, high, low) in [
        (0, 1, 0xca, 0x6f),
        (79_546, 2, 0x9f, 0x3c),
        (159_091, 3, 0xac, 0x0d),
        (715_909, 1, 0xca, 0x6f),
    ] {
        if r == 1 && id != 0 {
            at(&mut f, &mut now, 636_365, 0);
        }
        f.write(0, 0xc0);
        for (i, byte) in [0, 0, r, 2, high, low].into_iter().enumerate() {
            at(&mut f, &mut now, id + EVENTS[i], 0);
            assert!(f.drq);
            assert_eq!(f.read(3), byte);
        }
        at(&mut f, &mut now, id + 1031, 0);
        state(&mut f, 0, true, false);
        assert_eq!(f.sector, 0); // no multi-record increment
    }
    assert!(!f.disk.as_ref().unwrap().dirty);
}

#[test]
fn service_starvation_recovery_and_final_byte_edges_are_tick_partition_independent() {
    for mode in 0..3 {
        // Final byte service before, on, after its budget, or after completion.
        for last in [801, 802, 916, 917, 918, 1030, 1031] {
            let mut f = drive(368_640);
            let mut now = 0;
            f.write(0, 0xc0);
            for (i, byte) in [0, 0, 1, 2, 0xca].into_iter().enumerate() {
                at(&mut f, &mut now, EVENTS[i], mode);
                assert_eq!(f.read(3), byte);
            }
            at(&mut f, &mut now, last, mode);
            assert_eq!(f.read(3), if last < 802 { 0xca } else { 0x6f });
            at(&mut f, &mut now, 1031, mode);
            // Reading before arrival cannot service the future final byte.
            state(
                &mut f,
                if (802..917).contains(&last) { 0 } else { 4 },
                true,
                false,
            );
        }
        for missed in [vec![], vec![0], vec![0, 1, 2, 3, 4, 5], vec![5]] {
            let mut f = drive(737_280);
            let mut now = REV - 1;
            f.tick(now);
            f.write(0, 0xc0);
            let mut lost = false;
            for (i, byte) in [0, 0, 1, 2, 0xca, 0x6f].into_iter().enumerate() {
                at(&mut f, &mut now, REV + EVENTS[i] - 1, mode);
                at(&mut f, &mut now, REV + EVENTS[i], mode);
                if i > 0 && missed.contains(&(i - 1)) {
                    lost = true;
                }
                state(&mut f, if lost { 7 } else { 3 }, false, true);
                assert_eq!(f.data, byte);
                if !missed.contains(&i) {
                    assert_eq!(f.read(3), byte);
                }
            }
            at(&mut f, &mut now, REV + 917, mode);
            state(
                &mut f,
                if missed.contains(&5) {
                    7
                } else if lost {
                    5
                } else {
                    1
                },
                false,
                missed.contains(&5),
            );
            if missed.contains(&5) {
                assert_eq!(f.read(3), 0x6f);
                lost = true;
            }
            at(&mut f, &mut now, REV + 1031, mode);
            state(&mut f, if lost { 4 } else { 0 }, true, false);
            // Fresh command clears LOST DATA, and servicing it recovers normally.
            f.write(0, 0xc0);
            for (i, byte) in [0, 0, 2, 2, 0x9f, 0x3c].into_iter().enumerate() {
                at(&mut f, &mut now, REV + 79_546 + EVENTS[i], mode);
                assert_eq!(f.read(3), byte);
            }
            at(&mut f, &mut now, REV + 79_546 + 1031, mode);
            state(&mut f, 0, true, false);
        }
        // One uninterrupted large/irregular/single-cycle run must reach completion.
        let mut f = drive(368_640);
        let mut now = 0;
        f.write(0, 0xc4);
        at(&mut f, &mut now, 160_122, mode);
        state(&mut f, 4, true, false);
        assert_eq!(f.data, 0x0d);
    }
}

#[test]
fn ready_is_sampled_before_e_and_missing_side_times_out_at_fifth_index() {
    for e in [false, true] {
        for control in [0, 1, 2, 0x80, 0x82] {
            let mut f = drive(368_640);
            f.write(5, control);
            f.write(0, if e { 0xc4 } else { 0xc0 });
            state(&mut f, 0x80, true, false);
            f.write(5, 0x81);
            f.tick(5 * REV);
            state(&mut f, 0, false, false); // live READY, no resumed transfer
        }
        let mut empty = Controller::default();
        empty.write(5, 0x81);
        empty.write(0, if e { 0xc4 } else { 0xc0 });
        state(&mut empty, 0x80, true, false);
        for start in [0, 1, REV - 1, REV] {
            let mut f = drive(368_640);
            f.tick(start);
            let mut now = start;
            f.write(4, 1);
            f.write(3, 0xa7);
            f.write(2, 255);
            f.write(0, if e { 0xc4 } else { 0xc0 });
            let search = u64::from(start) * 100 + if e { 10_738_635 } else { 0 };
            let first = search.div_ceil(u64::from(REV) * 100) as u32 * REV;
            for i in 0..4 {
                at(&mut f, &mut now, first + i * REV, 0);
                state(&mut f, 1, false, false);
            }
            at(&mut f, &mut now, first + 4 * REV - 1, 0);
            state(&mut f, 1, false, false);
            at(&mut f, &mut now, first + 4 * REV, 0);
            state(&mut f, 0x10, true, false);
            assert_eq!((f.sector, f.data), (255, 0xa7));
        }
    }
}

#[test]
fn cancel_every_phase_preserves_media_and_immediate_irq_and_never_resumes() {
    // E settle, search, ID match, each read arrival, final budget and completion edge.
    for elapsed in [
        0, 107_386, 107_387, 159_090, 159_091, 159_320, 159_321, 159_435, 159_550, 159_664,
        159_779, 159_893, 160_007, 160_008, 160_121,
    ] {
        for cancel in 0..9 {
            let mut f = drive(737_280);
            let original = f.disk.as_ref().unwrap().bytes().to_vec();
            f.write(0, 0xc4);
            f.tick(elapsed);
            f.write(1, 27);
            f.write(2, 88);
            f.write(0, 0xa0); // ignored while busy
            assert_eq!(f.track, 0);
            match cancel {
                0 => f.write(0, 0xd0),
                1 => f.write(0, 0xd8),
                2 => f.reset(),
                3 => f.mount(Disk::new(original.clone(), true).unwrap()).unwrap(),
                4 => f.eject().unwrap(),
                5 => f.write(4, 1),
                6 => f.write(5, 1),
                7 => f.write(5, 0x82),
                _ => f.write(5, 0x80),
            }
            assert!(!f.drq);
            assert_eq!(f.read(0) & 1, 0);
            f.write(4, 0);
            f.write(5, 0x81);
            f.tick(u32::MAX);
            assert!(!f.drq);
            assert_eq!(f.read(0) & 1, 0);
            assert_eq!(f.irq, cancel == 1);
            if let Some(disk) = &f.disk {
                assert_eq!(disk.bytes(), original);
                assert!(!disk.dirty);
            }
        }
    }
    // Ordinary completion and status acknowledgement do not release the D8 latch.
    let mut f = drive(368_640);
    f.write(0, 0xd8);
    f.write(0, 0xc0);
    f.tick(1031);
    assert_eq!(f.read(0), 4);
    assert!(f.irq);
    f.write(0, 0xd0);
    assert!(!f.irq);
}
