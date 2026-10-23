use super::Scc;

fn voice(chip: &mut Scc, c: usize, period: u16, volume: u8) {
    chip.write(0x80 + c as u16 * 2, period as u8);
    chip.write(0x81 + c as u16 * 2, (period >> 8) as u8);
    chip.write(0x8a + c as u16, volume);
}

#[test]
fn exact_register_map_masks_mirrors_reserved_and_open_bus() {
    for page in 0x98..=0x9f {
        let mut chip = Scc::default();
        for r in 0..128u16 {
            chip.write(page * 256 + r, r as u8 ^ 0x89);
            assert_eq!(chip.read(0x9800 + r), r as u8 ^ 0x89);
        }
        for r in 0x80..=0x9f {
            chip.write(page * 256 + r, 0xff);
            assert_eq!(chip.read(page * 256 + r), 0xff);
        }
        assert_eq!(chip.frequency, [4095; 5]);
        assert_eq!(chip.volume, [15; 5]);
        assert_eq!(chip.enabled, 31);
        for r in 0xa0..=0xdf {
            let before = chip.clone();
            chip.write(page * 256 + r, 0x55);
            assert_eq!(chip.read(page * 256 + r), 0xff);
            assert_eq!(chip, before); // No SCC+ fifth-wave/read-back invention.
        }
        for r in 0xe0..=0xff {
            chip.write(page * 256 + r, r as u8);
            assert_eq!(chip.deformation, r as u8);
        }
    }
    let mut normal = Scc::default();
    let mut mirror = Scc::default();
    for r in 0x80..=0x8f {
        normal.write(r, r as u8);
        mirror.write(r + 16, r as u8);
    }
    assert_eq!(normal, mirror);
}

#[test]
fn all_five_voices_have_independent_period_volume_phase_and_shared_last_wave() {
    let mut chip = Scc::default();
    for bank in 0..4 {
        for i in 0..32 {
            chip.write(bank * 32 + i, (i as i16 * 7 - 112 + bank as i16) as u8);
        }
    }
    for c in 0..5 {
        voice(&mut chip, c, 19 + c as u16 * 13, (15 - c * 2) as u8);
    }
    chip.write(0x8f, 31);
    for elapsed in [1u32, 19, 20, 39, 101, 1057, 100003] {
        let mut current = chip.clone();
        current.tick(elapsed);
        let mut expected = 0i32;
        for c in 0..5 {
            let index = (elapsed / (20 + c as u32 * 13)) % 32;
            let wave = index as i32 * 7 - 112 + c.min(3);
            expected += (wave * (15 - c * 2)) >> 4;
            current.write(0x8f, 1 << c);
            let single = ((wave * (15 - c * 2)) >> 4) as f32 * 0.0005;
            assert!((current.output() - single).abs() < 0.000001);
        }
        current.write(0x8f, 31);
        assert!((current.output() - expected as f32 * 0.0005).abs() < 0.000001);
        current.write(0x8f, 0);
        assert_eq!(current.output(), 0.0);
        current.write(0x8f, 31);
        for c in 0..5 {
            current.write(0x8a + c, 0);
        }
        assert_eq!(current.output(), 0.0);
    }
    let mut chunked = chip.clone();
    chip.tick(1_000_000);
    for _ in 0..1000 {
        chunked.tick(1000);
    }
    assert_eq!(chip, chunked);
}

#[test]
fn signed_dac_rounding_zero_periods_and_frequency_write_resets() {
    let mut chip = Scc::default();
    for i in 0..32 {
        chip.write(i, 0x80);
    }
    chip.write(0x8f, 1);
    for period in 0..=8 {
        voice(&mut chip, 0, period, 15);
        chip.tick(1000);
        assert_eq!(chip.position[0], 0);
        assert_eq!(chip.output(), 0.0);
    }
    voice(&mut chip, 0, 99, 15);
    assert!((chip.output() + 0.06).abs() < 0.000001);
    chip.tick(99);
    assert_eq!(chip.position[0], 0);
    chip.tick(1);
    assert_eq!(chip.position[0], 1);
    chip.tick(90);
    chip.write(0x80, 99); // Reload counter, retain wave position in normal mode.
    chip.tick(10);
    assert_eq!(chip.position[0], 1);
    chip.tick(90);
    assert_eq!(chip.position[0], 2);
    chip.write(0xe0, 0x20);
    chip.write(0x81, 0xf0); // High nibble ignored; bit 5 restarts the wave.
    assert_eq!(chip.position[0], 0);
    assert_eq!(chip.count[0], 0);
    assert_eq!(chip.frequency[0], 99);
    chip.write(0, 0xff);
    chip.write(0x8a, 1);
    assert!((chip.output() + 0.0005).abs() < 0.000001); // -1 * 1 >> 4 = -1.
}

#[test]
fn shared_wave_rotation_selects_voice_five_for_bit_six_and_voice_four_for_bit_seven() {
    for (mode, offsets) in [
        (0x00, [0, 0, 0]),
        (0x40, [0, 1, 0]),
        (0x80, [1, 2, 0]),
        (0xc0, [0, 0, 0]),
    ] {
        let mut chip = Scc::default();
        for i in 0..32 {
            chip.write(0x9860 + i, i as u8);
        }
        voice(&mut chip, 3, 19, 15); // Voice 4 advances every 20 cycles.
        voice(&mut chip, 4, 39, 15); // Voice 5 advances every 40 cycles.
        chip.write(0x98e0, mode);
        // Check before/after voice 5 advances, then after both phases wrap.
        for (cycles, offset) in [20, 20, 1240].into_iter().zip(offsets) {
            chip.tick(cycles);
            for i in 0..32u8 {
                assert_eq!(
                    chip.read(0x9860 + u16::from(i)),
                    i.wrapping_sub(offset) & 31,
                    "mode {mode:02x}, offset {offset}, sample {i}"
                );
            }
        }
    }
}

#[test]
fn deformation_dividers_rotation_protection_and_read_side_effect() {
    let mut initial = Scc::default();
    for c in 0..5 {
        voice(&mut initial, c, 0xbcd, 15);
    }
    for bank in 0..4 {
        for i in 0..32 {
            initial.write(bank * 32 + i, (bank * 32 + i) as u8);
        }
    }
    for (mode, period) in [(0, 0xbcd), (1, 0xb), (2, 0xcd), (3, 0xcd)] {
        let mut chip = initial.clone();
        chip.write(0xe1, mode);
        chip.tick(period + 1);
        assert_eq!(chip.position, [1; 5]);
    }
    for mode in [0x40, 0x80, 0xc0] {
        let mut chip = initial.clone();
        chip.write(0xe0, mode);
        chip.tick(0xbce);
        for bank in 0..4 {
            let rotates = if bank == 3 {
                mode != 0xc0
            } else {
                mode & 0x40 != 0
            };
            let protected = mode & 0x40 != 0 || bank == 3;
            assert_eq!(
                chip.read(bank * 32),
                (bank * 32 + if rotates { 31 } else { 0 }) as u8
            );
            chip.write(bank * 32, 0x7f);
            assert_eq!(
                chip.waves[bank as usize][0],
                if protected { (bank * 32) as i8 } else { 127 }
            );
        }
    }
    let mut chip = initial;
    assert_eq!(chip.read(0x9fee), 0xff);
    assert_eq!(chip.deformation, 0xff);
    chip.write(0, 0x80);
    assert_eq!(chip.waves[0][0], 0); // Test read made waveform RAM read-only.
    chip.write(0x98ff, 0);
    chip.write(0, 0x80);
    assert_eq!(chip.read(0), 0x80);
}

#[test]
fn scci_maps_fifth_wave_masks_deformation_and_reset() {
    use super::Mode;
    let mut chip = Scc::default();
    chip.set_mode(Mode::Plus);
    for page in 0xb8..=0xbf {
        for r in 0..160 {
            chip.write(page * 256 + r, r as u8 ^ 0xa5);
            assert_eq!(chip.read(0xb800 + r), r as u8 ^ 0xa5);
        }
    }
    chip.write(0xbfa7, 0xff);
    chip.write(0xbfa9, 0xff);
    chip.write(0xbfad, 0xff);
    chip.write(0xbfae, 0xff);
    chip.write(0xbfaf, 0xff);
    assert_eq!(chip.frequency[3..], [0xf00, 0xf00]);
    assert_eq!(chip.volume[3..], [15, 15]);
    assert_eq!(chip.enabled, 31);
    for r in 0xa0..0xc0 {
        assert_eq!(chip.read(r), 0xff);
    }
    chip.set_mode(Mode::Compatible);
    assert_eq!(chip.read(0xa0), 0x25); // independent fifth survives mode change
    chip.write(0xa0, 0x77); // compatible fifth is read-only
    assert_eq!(chip.read(0xa0), 0x25);
    chip.write(0x60, 0x33); // compatible shared update
    assert_eq!(chip.read(0xa0), 0x33);
    chip.set_mode(Mode::Plus);
    assert_eq!(chip.read(0x80), 0x33);
    for mode in [Mode::Compatible, Mode::Plus] {
        chip.set_mode(mode);
        for deform in [0x80, 0, 0x40, 0xc0] {
            chip.write(0xc0, deform);
            chip.write(0, 0x72);
            assert_eq!(chip.read(0), 0x72); // bit 7 alone never protects SCC-I
        }
        chip.write(0xc0, 0);
        chip.write(0, 0x24);
        chip.write(0xe0, 0x40); // reserved, unlike classic SCC
        assert_eq!(chip.read(0xe0), 0xff);
        chip.write(0, 0x42);
        assert_eq!(chip.read(0), 0x42);
        assert_eq!(chip.read(0xdf), 0xff); // deformation read latches FF
        chip.write(0, 0x12);
        assert_eq!(chip.waves[0][0], 0x42);
        assert_eq!(chip.deformation, 255);
    }
    // Every SCC-I wave ignores bit 7, and bit 6 protects every writable wave.
    chip.set_mode(Mode::Plus);
    for deform in [0x80, 0x40, 0xc0] {
        chip.write(0xc0, 0);
        for bank in 0..5u16 {
            chip.write(bank * 32, 0x11 + bank as u8);
        }
        chip.write(0xc0, deform);
        for bank in 0..5u16 {
            chip.write(bank * 32, 0x61 + bank as u8);
            assert_eq!(
                chip.waves[bank as usize][0] as u8,
                if deform == 0x80 {
                    0x61 + bank as u8
                } else {
                    0x11 + bank as u8
                }
            );
        }
    }
    chip.tick(12345);
    let position = chip.position;
    let count = chip.count;
    let waves = chip.waves;
    let frequency = chip.frequency;
    let volume = chip.volume;
    chip.reset_scci();
    assert_eq!(chip.waves, waves);
    assert_eq!(chip.position, position);
    assert_eq!(chip.count, count);
    assert_eq!(chip.frequency, frequency);
    assert_eq!(chip.volume, volume);
    assert_eq!(chip.mode, Mode::Compatible);
    assert_eq!(chip.deformation, 0);
    assert_eq!(chip.enabled, 0);
}

#[test]
fn scci_independent_pcm_clock_masks_signed_dac_and_chunk_invariance() {
    use super::Mode;
    use crate::psg::Psg;
    let mut chip = Scc::default();
    chip.set_mode(Mode::Plus);
    for c in 0..5u16 {
        for i in 0..32u16 {
            chip.write(c * 32 + i, (i as i16 * 5 - 110 + c as i16 * 7) as u8);
        }
        chip.write(0xa0 + 2 * c, (19 + c * 13) as u8);
        chip.write(0xa1 + 2 * c, 0xf0); // high bits masked
        chip.write(0xaa + c, 0xf0 | (15 - c as u8 * 2));
    }
    chip.write(0xaf, 0xff);
    for elapsed in [1u32, 19, 20, 39, 101, 1057, 100003] {
        let mut current = chip.clone();
        current.tick(elapsed);
        let mut sum = 0;
        for c in 0..5u32 {
            let position = elapsed / (20 + c * 13) % 32;
            let wave = position as i32 * 5 - 110 + c as i32 * 7;
            let dac = (wave * (15 - c as i32 * 2)).div_euclid(16);
            current.write(0xaf, 1 << c);
            assert!((current.output() - dac as f32 / 2000.0).abs() < 1e-6);
            sum += dac;
        }
        current.write(0xaf, 31);
        assert!((current.output() - sum as f32 / 2000.0).abs() < 1e-6);
    }
    let mut a = Psg::default();
    let mut b = Psg::default();
    let mut other = chip.clone();
    a.tick_with_scc(100003, Some(&mut chip));
    let mut remaining = 100003;
    for chunk in [1, 19, 3, 512, 7919].into_iter().cycle() {
        let n = remaining.min(chunk);
        b.tick_with_scc(n, Some(&mut other));
        remaining -= n;
        if remaining == 0 {
            break;
        }
    }
    assert_eq!(chip, other);
    assert_eq!(a.samples, b.samples);
    for (deform, period) in [(0, 0xbcd), (1, 0xb), (2, 0xcd), (3, 0xcd)] {
        let mut chip = Scc::default();
        chip.set_mode(Mode::Plus);
        chip.write(0xa0, 0xcd);
        chip.write(0xa1, 0xfb);
        chip.write(0xc0, deform);
        chip.tick(period + 1);
        assert_eq!(chip.position[0], 1);
    }
    for period in 0..=8 {
        let mut chip = Scc::default();
        chip.set_mode(Mode::Plus);
        chip.write(0xa8, period);
        chip.write(0xae, 15);
        chip.write(0xaf, 16);
        chip.tick(10000);
        assert_eq!(chip.output(), 0.0);
        assert_eq!(chip.position[4], 0);
    }
}

#[test]
fn scci_rotation_protection_and_compatible_raw_clock_selection() {
    use super::Mode;
    for mode in [Mode::Compatible, Mode::Plus] {
        for deform in [0x40, 0xc0] {
            let mut chip = Scc::default();
            chip.set_mode(Mode::Plus);
            for c in 0..5u16 {
                for i in 0..32 {
                    chip.write(c * 32 + i, i as u8);
                }
                chip.write(0xa0 + c * 2, 19 + c as u8 * 10);
            }
            chip.set_mode(mode);
            chip.write(0xc0, deform);
            chip.tick(100);
            for (address, plus, compatible) in [
                (0, 5, 5),
                (0x60, 2, if deform == 0x40 { 1 } else { 2 }),
                (0x80, 1, 255),
                (0xa0, 255, 1),
            ] {
                assert_eq!(
                    chip.read(address),
                    if mode == Mode::Plus { plus } else { compatible }
                );
            }
        }
    }
}

#[test]
fn scci_compatible_pcm_retains_plus_fifth_until_shared_wave_is_written() {
    use super::Mode;
    let mut chip = Scc::default();
    chip.set_mode(Mode::Plus);
    for i in 0..32 {
        chip.write(0x60 + i, 64);
        chip.write(0x80 + i, 0x80);
    }
    for (r, v) in [(0xa6, 99), (0xa8, 199), (0xad, 15), (0xae, 15)] {
        chip.write(r, v);
    }
    chip.set_mode(Mode::Compatible);
    chip.write(0x9f, 16);
    assert!((chip.output() + 0.06).abs() < 1e-6);
    chip.write(0x9f, 8);
    assert!((chip.output() - 0.03).abs() < 1e-6);
    for i in 0..32 {
        chip.write(0x60 + i, 0x80);
    }
    for enable in [8, 16] {
        chip.write(0x8f, enable);
        assert!((chip.output() + 0.06).abs() < 1e-6);
    }
}
