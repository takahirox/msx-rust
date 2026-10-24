use super::*;
fn write(chip: &mut Ym2413, reg: u8, data: u8) {
    chip.select(reg);
    chip.tick(72);
    chip.write(data);
    chip.tick(144);
}
fn sine() -> Ym2413 {
    let mut chip = Ym2413::default();
    for (r, d) in [0x21, 0x21, 0x3f, 0, 0, 0xf0, 0, 0x0f]
        .into_iter()
        .enumerate()
    {
        write(&mut chip, r as u8, d);
    }
    chip
}
#[test]
fn pinned_c_reference_melodic_presets_releases_and_all_five_drums() {
    let mut lines = include_str!("../../../../tests/reference/emu2413-trace.txt").lines();
    let mut compare = |chip: &mut Ym2413| {
        let line = lines.next().unwrap();
        for value in line.split_whitespace().skip(1) {
            chip.tick(72);
            assert_eq!(chip.output, value.parse::<i32>().unwrap(), "{line}");
        }
    };
    for preset in 0..16 {
        let mut chip = sine();
        write(&mut chip, 0x30, preset << 4);
        write(&mut chip, 0x10, 0x22);
        write(&mut chip, 0x20, 0x19);
        chip.tick(4096 * 72);
        compare(&mut chip);
        write(&mut chip, 0x20, 0x29);
        chip.tick(1024 * 72);
        compare(&mut chip);
    }
    for bit in 0..5 {
        let mut chip = Ym2413::default();
        for ch in 6..9 {
            write(&mut chip, 0x10 + ch, 0x80 + ch * 7);
            write(&mut chip, 0x20 + ch, 0x0b);
            write(&mut chip, 0x30 + ch, 0);
        }
        write(&mut chip, 14, 32 | (1 << bit));
        chip.tick(256 * 72);
        compare(&mut chip);
    }
    let mut chip = Ym2413::default();
    for (r, v) in [0xf5, 0xf2, 0x9b, 0xdf, 0x85, 0xb6, 0x32, 0x48]
        .into_iter()
        .enumerate()
    {
        write(&mut chip, r as u8, v);
    }
    for ch in 0..9 {
        write(&mut chip, 0x30 + ch, ch);
        write(&mut chip, 0x10 + ch, 0x22 + ch * 13);
        write(&mut chip, 0x20 + ch, 0x19);
        compare(&mut chip);
    }
    chip.tick(2048 * 72);
    compare(&mut chip);
    write(&mut chip, 1, 0x71);
    write(&mut chip, 3, 7);
    compare(&mut chip);
    write(&mut chip, 0x27, 0x39);
    compare(&mut chip);
    write(&mut chip, 14, 0x3f);
    compare(&mut chip);
    write(&mut chip, 0x27, 9);
    write(&mut chip, 14, 32);
    compare(&mut chip);
    write(&mut chip, 14, 0);
    compare(&mut chip);
    assert!(lines.next().is_none());
}
#[test]
fn custom_sine_pitch_and_volume_follow_independent_clock_arithmetic() {
    let mut chip = sine();
    write(&mut chip, 0x10, 0x22);
    write(&mut chip, 0x20, 0x19);
    chip.tick(4096 * 72);
    let mut crossing = Vec::new();
    let mut prev = chip.output;
    let mut peak = 0;
    for n in 0..20000 {
        chip.tick(72);
        if prev <= 0 && chip.output > 0 {
            crossing.push(n);
        }
        peak = peak.max(chip.output.abs());
        prev = chip.output;
    }
    // f=Fnum*2^block*clock/(72*2^19), multiplier 1.
    let period = (1_u64 << 19) as f64 / (290.0 * 16.0);
    for (n, &at) in crossing.iter().enumerate() {
        assert!(((at - crossing[0]) as f64 - n as f64 * period).abs() < 1.1);
    }
    assert!((2000..=2048).contains(&peak));
    write(&mut chip, 0x30, 8);
    let mut quiet = 0;
    for _ in 0..512 {
        chip.tick(72);
        quiet = quiet.max(chip.output.abs());
    }
    // Eight 3 dB volume steps = 24 dB attenuation (~1/16 amplitude).
    assert!((120..=135).contains(&quiet));
    write(&mut chip, 0x20, 9);
    chip.tick(20000 * 72);
    assert_eq!(chip.output, 0);
    assert_eq!(chip.operators[1].state as u8, Envelope::Release as u8);
}
#[test]
fn aliases_reserved_test_repeated_writes_and_custom_updates() {
    let mut chip = sine();
    write(&mut chip, 0x10, 0x22);
    write(&mut chip, 0x20, 0x19);
    chip.tick(512 * 72);
    let mut repeated = chip.clone();
    repeated.select(0x20);
    repeated.write(0x19);
    repeated.write(0x19);
    chip.tick(4096);
    repeated.tick(4096);
    assert_eq!(chip.output, repeated.output);
    assert_eq!(chip.operators[1].phase, repeated.operators[1].phase);
    let mut alias = chip.clone();
    alias.select(0x29);
    alias.write(0x19);
    let mut reserved = chip.clone();
    for r in [8, 9, 10, 11, 12, 13, 0x40, 0xff] {
        reserved.select(r);
        reserved.write(255);
    }
    for _ in 0..100 {
        chip.tick(72);
        alias.tick(72);
        reserved.tick(72);
        assert_eq!(chip.output, alias.output);
        assert_eq!(chip.output, reserved.output);
    }
    let mut custom = chip.clone();
    custom.select(1);
    custom.write(0x22);
    custom.tick(100 * 72);
    chip.tick(100 * 72);
    assert_ne!(custom.output, chip.output);
    custom.select(15);
    custom.write(7);
    custom.tick(72);
    assert_eq!(custom.operators[1].envelope, 0);
    assert_eq!(custom.operators[1].pg, 18); // reset phase then one increment at ML=2
}
#[test]
fn exact_clocks_and_long_partition_invariance() {
    let mut one = sine();
    // Keep AM, vibrato and feedback audible across many LFO/envelope wraps.
    for (r, v) in [(0, 0xe1), (1, 0xe1), (2, 20), (3, 7), (4, 0xf0)] {
        write(&mut one, r, v);
    }
    write(&mut one, 0x10, 0x22);
    write(&mut one, 0x20, 0x19);
    let mut split = one.clone();
    let before = one.clocks();
    one.tick(12_345_678);
    let mut left = 12_345_678;
    let mut n = 1;
    while left > 0 {
        let part = left.min(n);
        split.tick(part);
        left -= part;
        n = n % 1234 + 1;
    }
    assert_eq!(one.clocks(), before + 12_345_678);
    assert_eq!(one.internal_samples(), one.clocks() / 72);
    for _ in 0..2048 {
        one.tick(72);
        split.tick(72);
        assert_eq!(one.output, split.output);
    }
}
#[test]
fn independent_channels_and_rhythm_switch_do_not_leave_stale_melody() {
    let mut a = sine();
    write(&mut a, 0x10, 0x22);
    write(&mut a, 0x20, 0x19);
    a.tick(512 * 72);
    let mut b = a.clone();
    write(&mut b, 0x11, 0xc0);
    write(&mut b, 0x21, 0x17);
    a.tick(6 * 72); // two writes' elapsed clocks
    for _ in 0..128 {
        a.tick(72);
        b.tick(72);
        assert_eq!(a.operators[0].output, b.operators[0].output);
        assert_eq!(a.operators[1].output, b.operators[1].output);
    }
    assert_ne!(a.output, b.output);
    let mut chip = sine();
    write(&mut chip, 0x16, 0x22);
    write(&mut chip, 0x26, 0x19);
    chip.tick(1000 * 72);
    write(&mut chip, 0x26, 9);
    write(&mut chip, 14, 32);
    chip.tick(100000 * 72);
    assert_eq!(chip.output, 0);
    write(&mut chip, 14, 0);
    chip.tick(100000 * 72);
    assert_eq!(chip.output, 0);
}
