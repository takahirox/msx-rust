use msx_core::{cassette::Cassette, cpu::Bus, firmware, Machine, Model};
const CLOCK: u64 = 3_579_545;
const MARKER: [u8; 8] = [0x1f, 0xa6, 0xde, 0xba, 0xcc, 0x13, 0x7d, 0x74];
fn image(blocks: &[&[u8]]) -> Vec<u8> {
    let mut v = Vec::new();
    for block in blocks {
        v.extend_from_slice(&MARKER);
        v.extend_from_slice(block);
    }
    v
}
fn advance(t: &mut Cassette, mut cycles: u64, partition: &[u32]) {
    let mut i = 0;
    while cycles != 0 {
        let c = cycles.min(u64::from(partition[i % partition.len()]));
        t.tick(c as u32, true);
        cycles -= c;
        i += 1;
    }
}
fn at(quarter: u64, rate: u32) -> u64 {
    (quarter * CLOCK).div_ceil(u64::from(rate) * 4)
}

#[test]
fn independently_calculated_pulse_frames_gaps_and_terminal_bytes() {
    // Reference waveform authored directly from specified frequencies and framing,
    // never using a production encoding/parser helper. Quarter-bit samples suffice
    // because every edge lies at an integer multiple of CLOCK/(4*baud).
    let bytes = [0x00u8, 0xff, 0x55, 0xaa, 0x80, 0x81, 0xfe, 0x7f];
    for rate in [1200, 2400] {
        let mut expected = vec![false; rate as usize * 8]; // 2 second silence
        for _ in 0..(16000 * rate / 1200) {
            expected.extend([true, false]);
        }
        for byte in bytes {
            for bit in std::iter::once(false)
                .chain((0..8).map(|b| byte & (1 << b) != 0))
                .chain([true, true])
            {
                expected.extend(if bit {
                    [true, false, true, false]
                } else {
                    [true, true, false, false]
                });
            }
        }
        for partitions in [&[u32::MAX][..], &[17, 997, 3, 4567][..], &[1][..]] {
            let mut t = Cassette::default();
            t.mount(image(&[&bytes]), rate).unwrap();
            let mut now = 0;
            // Sample before/at every edge in the data and selected leader edges.
            let leader_end = u64::from(rate) * 8 + 32000 * u64::from(rate / 1200);
            let mut points = vec![0, 1, at(u64::from(rate) * 8, rate) - 1];
            for q in u64::from(rate) * 8..u64::from(rate) * 8 + 8 {
                points.extend([at(q, rate) - 1, at(q, rate)]);
            }
            for q in leader_end..=expected.len() as u64 {
                points.extend([at(q, rate) - 1, at(q, rate)]);
            }
            points.sort_unstable();
            points.dedup();
            for target in points {
                advance(&mut t, target - now, partitions);
                now = target;
                let q = (target * u64::from(rate) * 4 / CLOCK) as usize;
                assert_eq!(
                    t.input(true),
                    expected.get(q).copied().unwrap_or(false),
                    "rate={rate}, t={target}, q={q}"
                );
            }
            assert!(t.ended());
            assert!(!t.input(true));
            let end = t.position_cycles();
            t.tick(u32::MAX, true);
            assert_eq!(t.position_cycles(), end);
            t.rewind();
            assert_eq!(t.position_cycles(), 0);
            assert!(!t.ended());
        }
    }
}
#[test]
fn standard_sequences_ascii_continuations_multiple_files_and_unknown_blocks() {
    for rate in [1200, 2400] {
        let binary = [vec![0xd0; 10], b"BINARY".to_vec()].concat();
        let basic = [vec![0xd3; 10], b"BASIC ".to_vec()].concat();
        let ascii = [vec![0xea; 10], b"ASCII ".to_vec()].concat();
        let payloads: Vec<&[u8]> = vec![
            &binary,
            &[0, 255, 128],
            &basic,
            &[0, 0],
            &ascii,
            b"10 PRINT",
            b"20 END\x1a",
            &[0x55],
            &[0xaa, 0x80],
        ];
        let mut t = Cassette::default();
        t.mount(image(&payloads), rate).unwrap();
        let mut q = 0;
        for (bytes, long) in payloads
            .iter()
            .zip([true, false, true, false, true, false, false, true, true])
        {
            let leader = q + u64::from(rate) * 4 * if long { 2 } else { 1 };
            let data = leader + (if long { 32000 } else { 8000 }) * u64::from(rate / 1200);
            let mut previous = at(q, rate);
            for (point, signal) in [
                (at(leader, rate) - 1, false),
                (at(leader, rate), true),
                (at(leader + 1, rate), false),
                (at(data, rate), true),
                (at(data + 1, rate), true),
                (at(data + 2, rate), false),
            ] {
                advance(&mut t, point - previous, &[u32::MAX]);
                previous = point;
                assert_eq!(t.input(true), signal);
            }
            q = data + bytes.len() as u64 * 44;
            advance(&mut t, at(q, rate) - previous, &[312, 999, 4321]);
        }
        assert!(t.ended());
    }
}
#[test]
fn malformed_imports_are_transactional_and_motor_freezes_fractional_phase() {
    let mut t = Cassette::default();
    t.mount(image(&[&[0x80]]), 2400).unwrap();
    advance(&mut t, 2 * CLOCK + 173, &[u32::MAX]);
    let mut reference = Cassette::default();
    reference.mount(image(&[&[0x80]]), 2400).unwrap();
    advance(&mut reference, 2 * CLOCK + 173, &[u32::MAX]);
    for bad in [
        vec![],
        vec![0],
        MARKER.to_vec(),
        image(&[&[], &[1]]),
        image(&[&[1], &[]]),
        image(&[&[0xd0; 10]]),
        image(&[&[0xea; 16]]),
    ] {
        assert!(t.mount(bad, 1200).is_err());
        assert_eq!(t.rate(), 2400);
    }
    for rate in [0, 1199, 2401, u32::MAX] {
        assert!(t.mount(image(&[&[2]]), rate).is_err());
    }
    assert!(t.mount(vec![0; 16 * 1024 * 1024 + 1], 1200).is_err());
    let position = t.position_cycles();
    for _ in 0..10 {
        t.tick(u32::MAX, false);
        assert!(!t.input(false));
    }
    assert_eq!(t.position_cycles(), position);
    for _ in 0..10000 {
        t.tick(37, true);
        reference.tick(37, true);
        assert_eq!(t.input(true), reference.input(true));
        assert_eq!(t.position_cycles(), reference.position_cycles());
    }
    t.eject();
    assert!(!t.mounted());
    assert!(!t.input(true));
    assert_eq!(t.rate(), 2400);
}
#[test]
fn psg_ppi_routing_and_lifecycle_preserve_the_session() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = Machine::default();
        m.set_model(model);
        m.hardware.cassette.mount(image(&[&[0x55]]), 1200).unwrap();
        m.cpu.pc = 0x1234;
        m.hardware.ram[123] = 77;
        m.hardware.set_joystick(0, 0x2e);
        m.hardware.set_joystick(1, 0x17);
        advance(&mut m.hardware.cassette, 2 * CLOCK, &[u32::MAX]);
        let h = &mut m.hardware;
        for r7 in [0, 0x40, 0x80, 0xc0] {
            h.output(0xa0, 7);
            h.output(0xa1, r7);
            for (r15, low) in [
                (0, 0x6e),
                (0x40, 0x57),
                (0x10, 0x7f),
                (0x60, 0x7f),
                (0x20, 0x6e),
                (0x50, 0x57),
            ] {
                h.output(0xa0, 15);
                h.output(0xa1, r15);
                h.output(0xa0, 14);
                h.output(0xaa, 0xe5);
                assert_eq!(h.input(0xa2), low | 0x80);
                h.key(5, 2, true);
                assert_eq!(h.input(0xa9), 0xfb);
                h.output(0xab, 9);
                assert_eq!(h.ppi_c, 0xf5);
                assert_eq!(h.input(0xa2), low);
                h.output(0xab, 8);
                assert_eq!(h.ppi_c, 0xe5);
                assert_eq!(h.input(0xa2), low | 0x80);
            }
        }
        assert_eq!(m.cpu.pc, 0x1234);
        assert_eq!(m.hardware.ram[123], 77);
        let position = m.hardware.cassette.position_cycles();
        m.reset();
        assert_eq!(m.hardware.ppi_c, 0);
        assert_eq!(m.hardware.cassette.position_cycles(), position);
        assert!(m.hardware.cassette.input(true));
        m.use_cbios().unwrap();
        assert_eq!(m.hardware.cassette.position_cycles(), position);
        m.set_model(if model == Model::Msx1 {
            Model::Msx2
        } else {
            Model::Msx1
        });
        assert_eq!(m.hardware.cassette.position_cycles(), position);
        assert_eq!(m.hardware.cassette.rate(), 1200);
        m.hardware.cassette.rewind();
        assert_eq!(m.hardware.cassette.position_cycles(), 0);
        m.hardware.cassette.eject();
        assert!(!m.hardware.cassette.mounted());
    }
}
// Independently specified expected header/body, including all executable bytes.
pub const RECOVERED: &[u8] = &[
    0xd0, 0xd0, 0xd0, 0xd0, 0xd0, 0xd0, 0xd0, 0xd0, 0xd0, 0xd0, b'T', b'A', b'P', b'E', 32, 32,
    0x16, 0xc2, 0x31, 0xc2, 0x16, 0xc2, 0x3e, 0x22, 0xd3, 0x99, 0x3e, 0x87, 0xd3, 0x99, 0x3e, 0xa5,
    0x32, 0xff, 0xc1, 0x3e, 9, 0xd3, 0xab, 0x76, 0x18, 0xfd, 0, 255, 0x55, 0xaa, 0x80, 0x81, 0xfe,
    0x7f,
];
#[test]
fn original_z80_loader_measures_decodes_and_executes_both_models_and_rates() {
    for model in [Model::Msx1, Model::Msx2] {
        for rate in [1200, 2400] {
            let mut m = Machine::default();
            m.set_model(model);
            m.load_cartridge(&firmware::cassette_test_cartridge())
                .unwrap();
            m.hardware
                .cassette
                .mount(firmware::cassette_test_image(), rate)
                .unwrap();
            for _ in 0..1600 {
                m.frame();
                if m.hardware.read(0xc1ff) == 0xa5 {
                    break;
                }
            }
            let recovered: Vec<_> = (0xc200..0xc200 + RECOVERED.len() as u16)
                .map(|a| m.hardware.read(a))
                .collect();
            assert_eq!(
                recovered,
                RECOVERED,
                "{model:?}/{rate} PC={:04x} pulse={} counts={:?}",
                m.cpu.pc,
                m.hardware.read(0xc184),
                (0xc180..0xc186)
                    .map(|a| m.hardware.read(a))
                    .collect::<Vec<_>>()
            );
            assert_eq!(m.hardware.read(0xc1ff), 0xa5);
            assert_eq!(m.hardware.read(0xc185), 0);
            for (address, cycles) in [(0xc180, 16000 * rate / 1200), (0xc182, 4000 * rate / 1200)] {
                let count =
                    u16::from_le_bytes([m.hardware.read(address), m.hardware.read(address + 1)]);
                assert!(
                    (cycles - 5..=cycles + 5).contains(&u32::from(count)),
                    "{count} vs {cycles}"
                );
            }
            assert_eq!(m.hardware.ppi_c, 0x15);
            for _ in 0..3 {
                m.frame();
            }
            assert_eq!(m.hardware.vdp.registers[7], 0x22);
            let pixels = &m.hardware.vdp.pixels;
            for (i, byte) in RECOVERED.iter().enumerate() {
                for bit in 0..8 {
                    let tile = i * 8 + bit;
                    let p = ((tile / 32 * 8) * m.hardware.vdp.width() + tile % 32 * 8) * 4;
                    assert_eq!(
                        &pixels[p..p + 3],
                        if byte & (1 << bit) != 0 {
                            &[255, 255, 255]
                        } else {
                            if model == Model::Msx1 {
                                &[33, 200, 66]
                            } else {
                                &[36, 218, 36]
                            }
                        }
                    );
                }
            }
        }
    }
}

#[test]
fn long_playback_retains_exact_phase_and_block_count_limit_is_transactional() {
    for rate in [1200, 2400] {
        let data = vec![0x55; 500000];
        let mut t = Cassette::default();
        t.mount(image(&[&data]), rate).unwrap();
        // More than one u32 tick and over twenty minutes of emulated playback. Derive
        // the expected bit directly from elapsed CPU clocks, including fractions.
        let elapsed = 5_000_000_123u64;
        advance(&mut t, elapsed, &[u32::MAX, 3, 17091]);
        let data_begin = u64::from(rate) * 8 + 32000 * u64::from(rate / 1200);
        for extra in 0..5000u64 {
            let quarter = (elapsed + extra) * u64::from(rate) * 4 / CLOCK - data_begin;
            let bit = quarter % 44 / 4;
            let one = matches!(bit, 1 | 3 | 5 | 7 | 9 | 10);
            assert_eq!(
                t.input(true),
                if one {
                    quarter.is_multiple_of(2)
                } else {
                    quarter % 4 < 2
                }
            );
            t.tick(1, true);
        }
        let pos = t.position_cycles();
        let mut excessive = Vec::new();
        for _ in 0..65537 {
            excessive.extend_from_slice(&MARKER);
            excessive.push(0x55);
        }
        assert!(t
            .mount(excessive, 1200)
            .unwrap_err()
            .contains("too many blocks"));
        assert_eq!(t.position_cycles(), pos);
        assert_eq!(t.rate(), rate);
    }
}

#[test]
fn tape_only_operations_preserve_cartridge_sram_firmware_disk_cpu_and_audio() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = Machine::default();
        m.set_model(model);
        m.load_cartridge_with_mapper(
            &firmware::mapper_test_cartridge(msx_core::Mapper::Ascii16Sram8),
            msx_core::Mapper::Ascii16Sram8,
        )
        .unwrap();
        m.hardware.secondary[3] = 255;
        m.hardware.slots = 0xd4;
        m.hardware.write(0x7000, 0x10);
        m.hardware.write(0x8123, 0x5a);
        m.mount_disk(vec![0x35; 368640], false).unwrap();
        m.hardware.fdc.disk.as_mut().unwrap().dirty = true;
        m.cpu.pc = 0x4321;
        m.hardware.ppi_c = 0xe5;
        let bios = m.hardware.bios.clone();
        let cartridge = m.hardware.cartridge.clone();
        let mut control = msx_core::psg::Psg::default();
        for psg in [&mut m.hardware.psg, &mut control] {
            for (r, v) in [(0, 73), (1, 1), (7, 0x3e), (8, 15)] {
                psg.select(r);
                psg.write(v);
            }
            psg.tick(1781);
        }
        for rate in [1200, 2400] {
            m.hardware
                .cassette
                .mount(firmware::cassette_test_image(), rate)
                .unwrap();
            m.hardware.cassette.tick(1234567, true);
            m.hardware.cassette.rewind();
            m.hardware
                .cassette
                .mount(firmware::cassette_test_image(), rate)
                .unwrap();
            m.hardware.cassette.eject();
        }
        assert_eq!(m.cpu.pc, 0x4321);
        assert_eq!(m.hardware.ppi_c, 0xe5);
        assert_eq!(m.hardware.slots, 0xd4);
        assert_eq!(m.hardware.bios, bios);
        assert_eq!(m.hardware.cartridge, cartridge);
        assert_eq!(m.hardware.read(0x8123), 0x5a);
        assert_eq!(m.cartridge_sram()[0x123], 0x5a);
        let disk = m.hardware.fdc.disk.as_ref().unwrap();
        assert_eq!(disk.bytes(), vec![0x35; 368640]);
        assert!(disk.dirty);
        assert!(!disk.write_protected);
        m.hardware.psg.tick(22117);
        control.tick(22117);
        assert_eq!(m.hardware.psg.samples, control.samples);
        assert!(control.samples.iter().any(|s| s.abs() > 0.1));
    }
}
