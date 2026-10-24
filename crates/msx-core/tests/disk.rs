use msx_core::{
    cpu::Bus,
    disk::{Controller, Disk},
    firmware, Firmware, Machine, Model,
};
fn drive(size: usize) -> Controller {
    let mut f = Controller::default();
    f.mount(Disk::new(vec![0; size], false).unwrap()).unwrap();
    f.write(5, 0x81);
    f
}
fn byte_ready(f: &mut Controller) {
    // One revolution plus the synthetic handoff; stop on the first DRQ.
    for _ in 0..45_100 {
        f.tick(16);
        if f.drq {
            return;
        }
    }
    panic!("sector did not reach DRQ: {:02x}", f.read(0));
}
fn wait(f: &mut Controller) {
    // More than 79 intervals at the slowest (30 ms) rate.
    f.tick(8_500_000);
}
#[test]
fn geometry_and_media_are_bounded() {
    for (size, sides) in [(368640, 1), (737280, 2)] {
        let d = Disk::new(vec![0; size], true).unwrap();
        assert_eq!(d.sides(), sides);
        assert_eq!(d.offset(79, sides - 1, 9), Some(size - 512));
        assert_eq!(d.offset(0, 0, 1), Some(0));
        for (t, h, s) in [
            (80, 0, 1),
            (0, sides, 1),
            (0, 0, 0),
            (0, 0, 10),
            (255, 255, 255),
        ] {
            assert_eq!(d.offset(t, h, s), None);
        }
    }
    for size in [0, 512, 368639, 368641, 737279, 737281] {
        assert!(Disk::new(vec![0; size], false).is_err());
    }
}
#[test]
fn commands_status_signals_overrun_and_private_writes() {
    let mut f = drive(737280);
    assert_eq!(f.read(6), 0x83);
    assert_eq!(f.read(6), 0x81);
    f.write(3, 79);
    f.write(0, 0x10);
    assert_eq!(f.read(0) & 1, 1);
    wait(&mut f);
    assert_eq!(f.track, 79);
    assert!(f.irq);
    assert_eq!(f.read(0) & 0x15, 0);
    assert!(!f.irq);
    f.write(4, 1);
    f.write(2, 9);
    f.write(0, 0xa0);
    for i in 0..512 {
        byte_ready(&mut f);
        assert!(f.drq);
        f.write(3, i as u8);
    }
    f.tick(1_000); // serializer and CRC tail
    assert!(f.irq);
    assert!(!f.drq);
    assert_eq!(f.read(0), 0);
    let disk = f.disk.as_ref().unwrap();
    assert!(disk.dirty);
    assert_eq!(
        &disk.bytes()[737280 - 512..],
        &(0..512).map(|i| i as u8).collect::<Vec<_>>()
    );
    assert!(disk.bytes()[..737280 - 512].iter().all(|v| *v == 0));
    assert!(f.eject().is_err());
    assert!(f.mount(Disk::new(vec![0; 368640], true).unwrap()).is_err());
    f.write(0, 0x80);
    for i in 0..512 {
        assert!(!f.drq);
        byte_ready(&mut f);
        assert_eq!(f.read(3), i as u8);
    }
    f.tick(1_000); // complete previous read
    f.write(0, 0x80);
    byte_ready(&mut f);
    f.tick(60_000); // unread bytes overrun, but transfer completes
    assert_eq!(f.read(0) & 5, 4);
    assert!(!f.irq);
    f.write(0, 0x80);
    byte_ready(&mut f);
    f.write(0, 0xd8);
    assert!(f.irq);
    assert!(!f.drq);
    f.write(0, 0x00);
    wait(&mut f);
    assert_eq!(f.track, 0);
    assert_eq!(f.read(0) & 4, 4);
    f.disk.as_mut().unwrap().write_protected = true;
    f.write(0, 0xa0);
    assert_eq!(f.read(0) & 0x40, 0x40);
    f.write(2, 10);
    f.write(0, 0x80);
    assert_eq!(f.read(0) & 0x11, 1);
    wait(&mut f);
    assert_eq!(f.read(0) & 0x10, 0x10);
    f.write(5, 1);
    f.write(0, 0x80);
    assert_eq!(f.read(0) & 0x80, 0x80);
    f.disk.as_mut().unwrap().dirty = false;
    f.eject().unwrap();
    f.write(5, 0x81);
    f.write(0, 0x80);
    assert_eq!(f.read(0) & 0x80, 0x80);
}
#[test]
fn z80_disk_boot_and_write_on_both_models() {
    // A sector is bounded by one revolution + 559*114.54544 cycles.
    // Twelve boot reads fit in 600 NTSC frames; a read/write in 120 frames.
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = Machine::default();
        m.set_model(model);
        m.frame();
        assert!(m.boot_disk().is_err());
        let disk = firmware::demo_disk();
        m.mount_disk(disk.clone(), false).unwrap();
        assert_eq!(m.hardware.firmware, Firmware::WeaveBasic);
        m.boot_disk().unwrap();
        for _ in 0..600 {
            m.frame();
        }
        assert_eq!(
            (0xd080..0xd08f)
                .map(|a| m.hardware.read(a))
                .collect::<Vec<_>>(),
            b"WEAVE DISK BOOT",
            "pc={:04x} status={:02x}",
            m.cpu.pc,
            m.hardware.fdc.read(0)
        );
        assert_eq!(&m.hardware.vdp.vram[0x1880..0x188f], b"WEAVE DISK BOOT");
        assert!(m.basic.console.transcript.is_empty());
        m.hardware.key(8, 0, true);
        for _ in 0..120 {
            m.frame();
        }
        assert_eq!(
            &m.hardware.vdp.vram[0x1a00..0x1a05],
            b"SAVED",
            "pc={:04x}",
            m.cpu.pc
        );
        let changed = m.hardware.fdc.disk.as_ref().unwrap().bytes();
        let mut expected = disk;
        expected[9216] = 1;
        assert_eq!(changed, expected);
        m.reset();
        for _ in 0..600 {
            m.frame();
        }
        assert_eq!(m.hardware.fdc.disk.as_ref().unwrap().bytes(), expected);
        assert!(m.eject_disk().is_err());
        m.hardware.fdc.disk.as_mut().unwrap().dirty = false;
        m.eject_disk().unwrap();
        m.frame();
        assert!(m.basic.console.transcript.contains("READY"));
    }
}
#[test]
fn slots_failures_overrides_and_parked_sram() {
    let mut m = Machine::default();
    m.set_model(Model::Msx2);
    m.frame();
    m.mount_disk(firmware::demo_disk(), true).unwrap();
    m.use_cbios().unwrap();
    let bios = m.hardware.bios.clone();
    assert!(m.boot_disk().is_err());
    assert_eq!(m.hardware.bios, bios);
    m.use_builtin();
    m.boot_disk().unwrap();
    m.hardware.slots = 0xf0;
    assert_eq!(m.hardware.read(0x4000), 255);
    m.hardware.slots = 0xf8;
    assert_eq!(m.hardware.read(0x4000), b'A');
    assert_eq!(m.hardware.read(0x7ffe), 0x83);
    m.hardware.fdc.irq = true;
    assert!(!m.hardware.irq()); // interface IRQ is polled, never wired to CPU IRQ
    m.reset();
    for _ in 0..600 {
        m.frame();
    }
    m.hardware.key(8, 0, true);
    for _ in 0..120 {
        m.frame();
    }
    assert_eq!(
        m.hardware.vdp.vram[0x1a00..0x1a0f].to_vec(),
        b"WRITE PROTECTED",
        "pc={:04x} src={:?} buf={:?}",
        m.cpu.pc,
        (0xd320..0xd32f)
            .map(|a| m.hardware.read(a))
            .collect::<Vec<_>>(),
        (0xf100..0xf104)
            .map(|a| m.hardware.read(a))
            .collect::<Vec<_>>()
    );
    assert!(!m.hardware.fdc.disk.as_ref().unwrap().dirty);
    let pc = m.cpu.pc;
    assert!(m.mount_disk(vec![0; 17], false).is_err());
    assert_eq!(m.cpu.pc, pc);
    m.load_cartridge_with_mapper(
        &firmware::mapper_test_cartridge(msx_core::Mapper::Ascii8Sram2),
        msx_core::Mapper::Ascii8Sram2,
    )
    .unwrap();
    m.restore_cartridge_sram(&vec![42; 2048]).unwrap();
    m.boot_disk().unwrap();
    m.reset();
    assert_eq!(m.cartridge_sram(), vec![42; 2048]);
    assert!(!m.hardware.cartridge.is_empty());
    m.eject_disk().unwrap();
    assert_eq!(m.hardware.firmware, Firmware::Cbios);
    assert_eq!(m.cartridge_sram(), vec![42; 2048]);
}

#[test]
fn step_multi_record_and_partial_write_lifecycle() {
    let mut f = drive(368640);
    f.write(0, 0x50);
    wait(&mut f);
    assert_eq!(f.track, 1);
    f.write(0, 0x40);
    wait(&mut f);
    assert_eq!(f.track, 1); // physical 2, register unchanged
    f.write(0, 0x80);
    assert_eq!(f.read(0) & 0x11, 1);
    wait(&mut f);
    assert_eq!(f.read(0) & 0x10, 0x10);
    f.write(0, 0x30);
    wait(&mut f);
    assert_eq!(f.track, 2); // remembered step-in, physical 3
    f.write(0, 0x70);
    wait(&mut f);
    assert_eq!(f.track, 1); // physical 2
    f.write(0, 0);
    wait(&mut f);
    assert_eq!(f.track, 0);
    f.write(4, 1);
    f.write(0, 0x80);
    assert_eq!(f.read(0) & 0x11, 1);
    wait(&mut f);
    assert_eq!(f.read(0) & 0x10, 0x10); // absent side
    f.write(4, 0);
    f.write(2, 8);
    f.write(0, 0xb0);
    for i in 0..1024 {
        byte_ready(&mut f);
        assert!(f.drq);
        f.write(3, if i < 512 { 0x5a } else { 0xa5 });
    }
    f.tick(1_000);
    assert_eq!(f.sector, 10);
    assert_eq!(f.read(0) & 0x11, 1);
    wait(&mut f);
    assert_eq!(f.read(0) & 0x11, 0x10);
    assert_eq!(
        &f.disk.as_ref().unwrap().bytes()[7 * 512..8 * 512],
        vec![0x5a; 512]
    );
    assert_eq!(
        &f.disk.as_ref().unwrap().bytes()[8 * 512..9 * 512],
        vec![0xa5; 512]
    );
    f.write(2, 1);
    f.write(0, 0xa0);
    byte_ready(&mut f);
    f.write(3, 42);
    f.write(0, 0xd0);
    assert!(!f.irq); // cancellation does not commit partial sector
    assert_eq!(&f.disk.as_ref().unwrap().bytes()[..512], vec![0; 512]);
    f.write(0, 0xa0);
    byte_ready(&mut f);
    f.write(3, 42);
    byte_ready(&mut f);
    f.tick(1_000);
    assert_eq!(f.read(0) & 4, 4);
    assert_eq!(&f.disk.as_ref().unwrap().bytes()[..512], vec![0; 512]);
    f.write(0, 0x80);
    f.write(2, 9);
    f.write(0, 0x10); // ignored while busy
    assert_eq!(f.sector, 1);
    f.reset();
    assert!(!f.irq);
    assert!(!f.drq);
    assert_eq!(f.control, 0);
    assert!(f.disk.as_ref().unwrap().dirty);
    assert_eq!(f.read(6) & 2, 2);
    assert_eq!(f.read(6) & 2, 0);
    f.write(5, 0x81);
    f.write(0, 0xd2);
    f.write(5, 1);
    assert!(f.irq);
    f.read(0);
    f.write(0, 0xd1);
    f.write(5, 0x81);
    assert!(f.irq);
    f.read(0);
    f.write(0, 0xd4);
    for _ in 0..720 {
        f.tick(1000);
    }
    assert!(f.irq);
    f.read(0);
    f.write(0, 0xe0); // Read Track remains outside the normal raw-sector profile.
    assert_eq!(f.read(0) & 0x10, 0x10);
}

#[test]
fn updating_steps_preserve_track_mismatch_for_reads_and_writes() {
    for (commands, track, physical_track) in [
        ([0x50, 0x40, 0x30], 2, 3), // remembered step-in
        ([0x50, 0x40, 0x70], 0, 1), // explicit step-out
    ] {
        let mut f = drive(368640);
        for command in commands {
            f.write(0, command);
            wait(&mut f);
        }
        assert_eq!(f.read(1), track);
        for command in [0x80, 0xa0] {
            f.write(0, command);
            wait(&mut f);
            assert!(f.irq);
            assert!(!f.drq);
            assert_eq!(f.read(0), 0x10); // record-not-found, not busy
            f.write(3, 0x5a); // a rejected write must not accept sector data
        }
        let disk = f.disk.as_ref().unwrap();
        assert!(!disk.dirty);
        assert!(disk.bytes().iter().all(|byte| *byte == 0));

        // Matching the register to the actual head position permits a read.
        f.write(1, physical_track);
        f.write(0, 0x80);
        byte_ready(&mut f);
        assert!(f.drq);
        assert_eq!(f.read(0), 3); // busy + DRQ
    }
}

#[test]
fn seek_preserves_head_offset_for_reads_and_writes() {
    let mut f = drive(368640);
    f.write(0, 0x40); // physical 1, Track 0
    wait(&mut f);
    // Equal Track/Data emits no steps; subsequent seeks move by Data - Track.
    for (destination, physical_track) in [(0, 1), (5, 6), (2, 3)] {
        f.write(3, destination);
        f.write(0, 0x10);
        wait(&mut f);
        assert_eq!(f.read(1), destination);
        assert_eq!(f.read(0) & 0x11, 0);
        for command in [0x80, 0xa0] {
            f.write(0, command);
            wait(&mut f);
            assert!(f.irq);
            assert!(!f.drq);
            assert_eq!(f.read(0), 0x10);
            f.write(3, 0x5a);
        }
        assert!(!f.disk.as_ref().unwrap().dirty);
        assert!(f.disk.as_ref().unwrap().bytes().iter().all(|b| *b == 0));

        // The head really moved by the register delta, independently of Track.
        f.write(1, physical_track);
        f.write(0, 0x80);
        byte_ready(&mut f);
        assert!(f.drq);
        assert_eq!(f.read(3), 0);
        f.write(0, 0xd0);
        f.write(1, destination);
    }
}

#[test]
fn type_one_verify_checks_resulting_track_and_media_geometry() {
    for verify in [0, 4] {
        for (command, track, expected_track, physical_track) in [
            (0x00, 20, 0, 0), // restore repairs a mismatch
            (0x10, 5, 7, 7),
            (0x10, 4, 7, 8), // seek preserves a mismatch
            (0x10, 6, 7, 6),
            (0x10, 8, 7, 4),
            (0x10, 7, 7, 5), // seek with no steps preserves a mismatch
            (0x20, 6, 6, 6),
            (0x20, 5, 5, 6),
            (0x30, 5, 6, 6),
            (0x30, 4, 5, 6),
            (0x40, 6, 6, 6),
            (0x40, 5, 5, 6),
            (0x50, 5, 6, 6),
            (0x50, 4, 5, 6),
            (0x60, 4, 4, 4),
            (0x60, 5, 5, 4),
            (0x70, 5, 4, 4),
            (0x70, 6, 5, 4),
        ] {
            let mut f = drive(368640);
            f.write(3, 5);
            f.write(0, 0x10); // physical 5; remembered direction is in
            wait(&mut f);
            f.write(1, track);
            f.write(3, 7);
            f.write(0, command | verify);
            let moving = verify != 0 || command != 0x10 || track != 7;
            assert_eq!(f.irq, !moving);
            assert_eq!(f.read(0) & 1, u8::from(moving));
            wait(&mut f);
            assert_eq!(f.irq, moving);
            assert!(!f.drq);
            assert_eq!(f.read(1), expected_track);
            let error = if verify != 0 && expected_track != physical_track {
                0x10
            } else {
                0
            };
            assert_eq!(f.read(0) & 0x11, error, "command={:02x}", command | verify);
            // Verification failure still completes the movement and register update.
            f.write(1, physical_track);
            f.write(0, 0x80);
            byte_ready(&mut f);
            assert!(f.drq);
        }
        for (size, error) in [(368640, 0x10), (737280, 0)] {
            let mut f = drive(size);
            f.write(4, 1);
            f.write(0, verify); // restore on a missing/present side
            wait(&mut f);
            assert!(f.irq);
            assert_eq!(f.read(0) & 0x11, if verify != 0 { error } else { 0 });
        }
    }
}

#[test]
fn independent_360k_z80_boot_sector_reads_and_writes_through_diskio() {
    // Three transfers plus a 79-track seek fit within 180 PAL frames.
    // This fixture is separate from the supplied demo generator. It is a hand-
    // authored program in sector zero, executed at C000h after firmware boot.
    // Read the last sector, write 6Bh at byte 7, write it back, leave a RAM marker.
    let mut bytes = vec![0x35; 368640];
    let program = [
        0xaf, 0x06, 1, 0x0e, 0xf8, 0x11, 0xcf, 2, 0x21, 0, 0xe0, 0xcd, 0x10, 0x40, 0x3e, 0x6b,
        0x32, 7, 0xe0, 0xaf, 0x37, 0x06, 1, 0x0e, 0xf8, 0x11, 0xcf, 2, 0x21, 0, 0xe0, 0xcd, 0x10,
        0x40, 0x38, 6, 0x3e, 0xa5, 0x32, 0, 0xdf, 0x76, 0x18, 0xfd,
    ];
    bytes[..program.len()].copy_from_slice(&program);
    bytes[510..512].copy_from_slice(b"WV");
    let mut expected = bytes.clone();
    expected[368640 - 512 + 7] = 0x6b;
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = Machine::default();
        m.set_model(model);
        m.mount_disk(bytes.clone(), false).unwrap();
        m.boot_disk().unwrap();
        for _ in 0..180 {
            m.frame();
            if m.cpu.halted {
                break;
            }
        }
        assert_eq!(m.hardware.read(0xdf00), 0xa5, "pc={:04x}", m.cpu.pc);
        assert_eq!(m.hardware.fdc.disk.as_ref().unwrap().bytes(), expected);
        // Pulling media at the controller level makes a new read complete not-ready.
        m.hardware.fdc.disk.as_mut().unwrap().dirty = false;
        m.hardware.fdc.eject().unwrap();
        m.hardware.slots = 0xf8;
        m.hardware.write(0x7ff8, 0x80);
        assert_eq!(m.hardware.read(0x7ff8) & 0x80, 0x80);
    }
}

#[test]
fn seek_direction_and_late_write_protection() {
    let mut f = drive(368640);
    for (target, step, expected) in [(10, 0x30, 11), (5, 0x30, 4)] {
        f.write(3, target);
        f.write(0, 0x10);
        wait(&mut f);
        f.write(0, step);
        wait(&mut f);
        assert_eq!(f.track, expected);
    }
    f.write(3, 80);
    f.write(0, 0x10);
    wait(&mut f);
    assert_eq!(f.read(0) & 0x10, 0x10);
    f.write(0, 0);
    wait(&mut f);
    f.write(0, 0xa0);
    for _ in 0..511 {
        byte_ready(&mut f);
        f.write(3, 42);
    }
    f.disk.as_mut().unwrap().write_protected = true;
    byte_ready(&mut f);
    f.write(3, 42);
    f.tick(1_000);
    assert_eq!(f.read(0) & 0x40, 0x40);
    assert!(f.disk.as_ref().unwrap().bytes().iter().all(|b| *b == 0));
    assert!(!f.disk.as_ref().unwrap().dirty);
}

#[test]
fn firmware_buffer_bounds_preserve_stack_and_report_completed_sectors() {
    for (buffer, completed) in [
        (0x7fff, 0),
        (0xed00, 1),
        (0xed01, 0),
        (0xee00, 0),
        (0xffff, 0),
    ] {
        let mut m = Machine::default();
        m.set_model(Model::Msx2);
        let image = firmware::demo_disk();
        m.mount_disk(image.clone(), true).unwrap();
        m.boot_disk().unwrap();
        // Call the real driver from a small independent RAM program, without
        // executing the demo. The second sector would overwrite reserved stack.
        m.hardware.slots = 0xf8;
        m.hardware.secondary[3] = 0xff;
        let code = [0xcd, 0x10, 0x40, 0x76];
        for (i, byte) in code.into_iter().enumerate() {
            m.hardware.write(0xc000 + i as u16, byte);
        }
        m.cpu.pc = 0xc000;
        m.cpu.sp = 0xf000;
        m.cpu.set_bc(0x02f9);
        m.cpu.set_hl(buffer);
        for _ in 0..60 {
            m.frame();
        }
        assert!(m.cpu.halted);
        assert_eq!(m.cpu.a(), 12);
        assert_eq!(m.cpu.r[0], completed);
        assert_eq!(m.cpu.f & 1, 1);
        if completed == 1 {
            assert_eq!(
                (0xed00..0xef00)
                    .map(|a| m.hardware.read(a))
                    .collect::<Vec<_>>(),
                &image[..512]
            );
        }
        assert_eq!(m.hardware.fdc.disk.as_ref().unwrap().bytes(), image);
    }
}
