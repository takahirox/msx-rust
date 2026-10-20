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
