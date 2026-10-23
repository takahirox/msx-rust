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
