use super::*;
use crate::scc::Scc;

fn same_state(a: &Psg, b: &Psg) {
    assert_eq!(a.registers, b.registers);
    assert_eq!(a.selected, b.selected);
    assert_eq!(a.tone_count, b.tone_count);
    assert_eq!(a.tone, b.tone);
    assert_eq!(a.noise_count, b.noise_count);
    assert_eq!(a.noise, b.noise);
    assert_eq!(a.envelope_count, b.envelope_count);
    assert_eq!(a.envelope_step, b.envelope_step);
    assert_eq!(a.envelope_attack, b.envelope_attack);
    assert_eq!(a.envelope_hold, b.envelope_hold);
    assert_eq!(a.divider, b.divider);
    assert_eq!(a.sample_clock, b.sample_clock);
    assert_eq!(a.dc, b.dc);
    assert_eq!(a.samples, b.samples);
}
fn write(p: &mut Psg, register: u8, value: u8) {
    p.select(register);
    p.write(value);
}
fn configure(p: &mut Psg) {
    for (r, v) in [
        (0, 17),
        (2, 53),
        (4, 97),
        (6, 3),
        (7, 0),
        (8, 16),
        (9, 16),
        (10, 16),
        (11, 2),
        (13, 10),
    ] {
        write(p, r, v);
    }
}

#[test]
fn irregular_tick_partitions_preserve_all_state_and_pcm_with_writes_and_scc() {
    for with_scc in [false, true] {
        let mut a = Psg::default();
        let mut b = Psg::default();
        configure(&mut a);
        configure(&mut b);
        let mut sa = Scc::default();
        for i in 0..32 {
            sa.write(i, if i < 16 { 120 } else { 136 });
        }
        sa.write(0x80, 47);
        sa.write(0x8a, 15);
        sa.write(0x8f, 1);
        let mut sb = sa.clone();
        let mut total = 0_usize;
        // Same timestamped writes, one call per interval versus arbitrary partitions.
        for (duration, r, value) in [
            (47, 13, 10),
            (1003, 11, 1),
            (8197, 12, 1),
            (397, 11, 0),
            (31, 12, 0),
            (155, 13, 9),
            (3101, 13, 9),
            (99991, 13, 14),
        ] {
            a.tick_with_scc(duration, with_scc.then_some(&mut sa));
            let mut remaining = duration;
            for size in [1, 17, 3, 79, 2049].into_iter().cycle() {
                let count = size.min(remaining);
                b.tick_with_scc(count, with_scc.then_some(&mut sb));
                remaining -= count;
                if remaining == 0 {
                    break;
                }
            }
            total += duration as usize;
            write(&mut a, r, value);
            write(&mut b, r, value);
            same_state(&a, &b);
            assert_eq!(sa, sb);
            assert_eq!(a.samples.len(), total * 44100 / 3_579_545);
            assert!(a.samples.iter().all(|v| v.is_finite() && v.abs() <= 0.9));
        }
    }
}

#[test]
fn reset_reproduces_fresh_envelope_phase_state_and_pcm() {
    let mut used = Psg::default();
    configure(&mut used);
    used.tick(119003);
    write(&mut used, 13, 13);
    used.tick(973);
    used.reset();
    let mut fresh = Psg::default();
    same_state(&used, &fresh);
    configure(&mut used);
    configure(&mut fresh);
    used.tick(19997);
    fresh.tick(19997);
    same_state(&used, &fresh);
}

#[test]
fn scc_pcm_matches_clock_and_dac_oracle_across_irregular_tick_partitions() {
    let mut psg = Psg::default();
    let mut scc = Scc::default();
    for i in 0..32 {
        scc.write(i, if i < 16 { 120 } else { 136 });
    }
    scc.write(0x80, 253);
    scc.write(0x8a, 15);
    scc.write(0x8f, 1);
    let mut elapsed = 0u64;
    for duration in [1, 17, 3, 79, 2049, 59737, 2, 99991] {
        psg.tick_with_scc(duration, Some(&mut scc));
        elapsed += u64::from(duration);
        assert_eq!(psg.samples.len() as u64, elapsed * 44100 / 3_579_545);
        for (i, &sample) in psg.samples.iter().enumerate() {
            let instant = ((i as u64 + 1) * 3_579_545).div_ceil(44100);
            let expected = if instant / 254 % 32 < 16 {
                0.056
            } else {
                -0.0565
            };
            assert!((sample - expected).abs() < 0.000001);
        }
    }
}

#[test]
fn fm_mixing_preserves_abi_disabled_pcm_and_long_irregular_partitions() {
    use crate::ym2413::Ym2413;
    fn chip() -> Ym2413 {
        let mut fm = Ym2413::default();
        for (r, v) in [0x21, 0x21, 0x3f, 0, 0, 0xf0, 0, 0x0f]
            .into_iter()
            .enumerate()
            .map(|(r, v)| (r as u8, v))
            .chain([(0x30, 0), (0x10, 0x22), (0x20, 0x19)])
        {
            fm.select(r);
            fm.tick(72);
            fm.write(v);
            fm.tick(144);
        }
        fm
    }
    fn psg() -> Psg {
        let mut p = Psg::default();
        for (r, v) in [(0, 100), (7, 0x3e), (8, 15)] {
            p.select(r);
            p.write(v);
        }
        p
    }
    let mut one = psg();
    let mut split = psg();
    let mut f1 = chip();
    let mut f2 = f1.clone();
    let mut old = psg();
    let mut absent = psg();
    let mut zero = Ym2413::default();
    let mut silent_fm = psg();
    let total = 3_579_545 * 3 + 87;
    one.tick_with_expansions(total, None, Some(&mut f1));
    old.tick(total);
    absent.tick_with_expansions(total, None, None);
    silent_fm.tick_with_expansions(total, None, Some(&mut zero));
    assert_eq!(old.samples, absent.samples);
    assert_eq!(old.samples, silent_fm.samples);
    let mut left = total;
    let mut n = 1;
    while left > 0 {
        let part = left.min(n);
        split.tick_with_expansions(part, None, Some(&mut f2));
        left -= part;
        n = n % 10007 + 1;
    }
    assert_eq!(one.samples, split.samples);
    assert_eq!(
        one.samples.len(),
        (u64::from(total) * 44100 / 3_579_545) as usize
    );
    assert!(one.samples.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
    for (mixed, psg) in one.samples.iter().zip(&old.samples) {
        assert!((mixed - psg).abs() <= 0.016);
    }
    assert!(one
        .samples
        .iter()
        .zip(old.samples)
        .any(|(a, b)| (a - b).abs() > 0.01));
    assert_eq!(f1.clocks(), f2.clocks());
    assert_eq!(f1.internal_samples(), f2.internal_samples());
}

#[test]
fn two_fm_chips_preserve_partition_counts_independence_and_absent_pcm() {
    use crate::ym2413::Ym2413;
    let mut f = Ym2413::default();
    let mut g = Ym2413::default();
    for (chip, pitch) in [(&mut f, 0x22), (&mut g, 0x54)] {
        for (r, v) in [(0x30, 0x10), (0x10, pitch), (0x20, 0x19)] {
            chip.select(r);
            chip.write(v);
        }
    }
    let (mut f2, mut g2) = (f.clone(), g.clone());
    let mut one = Psg::default();
    configure(&mut one);
    let mut split = Psg::default();
    configure(&mut split);
    let mut old = Psg::default();
    configure(&mut old);
    let mut absent = Psg::default();
    configure(&mut absent);
    let total = 3_579_545 + 31;
    one.tick_with_fm_pac(total, None, Some(&mut f), Some(&mut g));
    let mut remaining = total;
    for n in [1, 7, 79, 3001, 53].into_iter().cycle() {
        let n = n.min(remaining);
        split.tick_with_fm_pac(n, None, Some(&mut f2), Some(&mut g2));
        remaining -= n;
        if remaining == 0 {
            break;
        }
    }
    old.tick(total);
    absent.tick_with_fm_pac(total, None, None, None);
    same_state(&one, &split);
    same_state(&old, &absent);
    assert_eq!(
        one.samples.len(),
        (u64::from(total) * 44100 / 3_579_545) as usize
    );
    assert!(one.samples.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    assert_eq!(f.clocks(), f2.clocks());
    assert_eq!(g.clocks(), g2.clocks());
    let mut different = false;
    // Restart sustained custom carriers to avoid comparing a coincidental zero
    // crossing or the final silence of a decaying preset.
    for (chip, pitch) in [(&mut f, 0x22), (&mut g, 0x54)] {
        *chip = Ym2413::default();
        for (r, v) in [0x21, 0x21, 0x3f, 0, 0, 0xf0, 0, 0x0f]
            .into_iter()
            .enumerate()
            .map(|(r, v)| (r as u8, v))
            .chain([(0x30, 0), (0x10, pitch), (0x20, 0x19)])
        {
            chip.select(r);
            chip.write(v);
        }
    }
    for _ in 0..1000 {
        f.tick(72);
        g.tick(72);
        different |= f.output() != g.output();
    }
    assert!(different);
}
