//! Independently authored AY timing vectors; no upstream emulator code or ROMs.
use msx_core::psg::Psg;

fn write(p: &mut Psg, register: u8, value: u8) {
    p.select(register);
    p.write(value);
}
fn envelope(period: u16, shape: u8) -> Psg {
    let mut p = Psg::default();
    write(&mut p, 11, period as u8);
    write(&mut p, 12, (period >> 8) as u8);
    write(&mut p, 13, shape);
    p
}

#[test]
fn aligned_envelope_uses_32_cpu_cycles_per_level() {
    let mut p = envelope(1, 8);
    // 256 PSG clocks/ramp * 2 CPU clocks/PSG clock / 16 levels = 32.
    p.tick(31);
    assert_eq!(p.envelope_level(), 15);
    p.tick(1);
    assert_eq!(
        p.envelope_level(),
        14,
        "first AY level transition at CPU cycle 32"
    );
    p.tick(448);
    assert_eq!(p.envelope_level(), 0);
    p.tick(31);
    assert_eq!(p.envelope_level(), 0);
    p.tick(1);
    assert_eq!(p.envelope_level(), 15);
}

// Explicit visible-level sequences rather than the implementation's shape flags.
fn shape_level(shape: u8, step: u32) -> u8 {
    let ramp = step / 16;
    let position = (step % 16) as u8;
    match shape {
        0..=3 | 9 => {
            if ramp == 0 {
                15 - position
            } else {
                0
            }
        }
        4..=7 | 15 => {
            if ramp == 0 {
                position
            } else {
                0
            }
        }
        8 => 15 - position,
        10 => {
            if ramp.is_multiple_of(2) {
                15 - position
            } else {
                position
            }
        }
        11 => {
            if ramp == 0 {
                15 - position
            } else {
                15
            }
        }
        12 => position,
        13 => {
            if ramp == 0 {
                position
            } else {
                15
            }
        }
        14 => {
            if ramp.is_multiple_of(2) {
                position
            } else {
                15 - position
            }
        }
        _ => unreachable!(),
    }
}

#[test]
fn all_shapes_at_both_ramp_turnarounds_and_holds() {
    for period in [1, 2, 0x0101, 0xffff] {
        for shape in 0..16 {
            let mut p = envelope(period, shape);
            let interval = 256 * 2 / 16 * u32::from(period);
            let mut now = 0;
            // Three complete ramps expose both alternate directions and repeated holds.
            for step in 1..=49 {
                let boundary = step * interval;
                p.tick(boundary - 1 - now);
                assert_eq!(
                    p.envelope_level(),
                    shape_level(shape, step - 1),
                    "shape {shape}, EP {period}, cycle {}",
                    boundary - 1
                );
                p.tick(1);
                assert_eq!(
                    p.envelope_level(),
                    shape_level(shape, step),
                    "shape {shape}, EP {period}, cycle {boundary}"
                );
                p.tick(1);
                assert_eq!(p.envelope_level(), shape_level(shape, step));
                now = boundary + 1;
            }
        }
    }
}

// All times in the following vectors are absolute CPU cycles since reset.
fn levels(p: &mut Psg, now: &mut u32, points: &[(u32, u8)]) {
    for &(time, level) in points {
        p.tick(time - *now);
        assert_eq!(p.envelope_level(), level, "at CPU cycle {time}");
        *now = time;
    }
}

#[test]
fn period_writes_retain_index_and_clamp_internal_counter_on_shortening() {
    for phase in 0..16 {
        let mut p = envelope(4, 8);
        let mut now = 0;
        levels(&mut p, &mut now, &[(48 + phase, 15)]);
        write(&mut p, 11, 1);
        // Clamp 6 half-ticks to 1, preserving divider phase. Step at 64 is hidden.
        levels(
            &mut p,
            &mut now,
            &[
                (63, 15),
                (64, 15),
                (65, 15),
                (79, 15),
                (80, 14),
                (81, 14),
                (112, 13),
            ],
        );
    }

    let mut p = envelope(4, 8);
    p.tick(304);
    assert_eq!(p.envelope_level(), 13);
    write(&mut p, 11, 1);
    levels(
        &mut p,
        &mut 304,
        &[(319, 13), (320, 13), (335, 13), (336, 12)],
    );

    let mut p = envelope(1, 8);
    p.tick(96);
    assert_eq!(p.envelope_level(), 12);
    write(&mut p, 12, 1); // high-byte increase to 0101h cannot restart the index.
    levels(
        &mut p,
        &mut 96,
        &[(4207, 12), (4208, 12), (8319, 12), (8320, 11)],
    );

    let mut p = envelope(2, 8);
    let mut now = 0;
    levels(&mut p, &mut now, &[(48, 15)]);
    write(&mut p, 11, 4);
    // One internal step has already elapsed. Increasing EP retains its index/phase.
    levels(
        &mut p,
        &mut now,
        &[(95, 15), (96, 14), (97, 14), (223, 14), (224, 13)],
    );
}

#[test]
fn low_and_high_period_writes_take_effect_separately_in_bus_order() {
    for phase in 0..16 {
        for low_first in [true, false] {
            let mut p = envelope(0x0101, 8);
            p.tick(48 + phase);
            if low_first {
                write(&mut p, 11, 2); // EP=0102, counter remains 6.
                assert_eq!(p.registers[12], 1);
                write(&mut p, 12, 0); // EP=0002, counter clamps to 3.
            } else {
                write(&mut p, 12, 0); // EP=0001, counter clamps to 1.
                assert_eq!(p.registers[11], 1);
                write(&mut p, 11, 2); // EP=0002, counter stays 1.
            }
            assert_eq!(p.envelope_level(), 15);
            let first = if low_first { 96 } else { 112 };
            levels(
                &mut p,
                &mut (48 + phase),
                &[(first - 1, 15), (first, 14), (first + 1, 14)],
            );
        }
    }
}

#[test]
fn every_r13_write_restarts_running_and_held_shapes_at_all_divider_phases() {
    for phase in 0..16 {
        let mut p = envelope(2, 8);
        p.tick(64 + phase); // already at visible level 14, count zero.
        write(&mut p, 13, 8); // same value is still a retrigger.
        let mut now = 64 + phase;
        assert_eq!(p.envelope_level(), 15);
        levels(
            &mut p,
            &mut now,
            &[(95, 15), (96, 15), (127, 15), (128, 14), (129, 14)],
        );

        let mut p = envelope(1, 9);
        p.tick(512 + phase); // held at zero; the period counter still runs.
        assert_eq!(p.envelope_level(), 0);
        write(&mut p, 13, 9);
        let mut now = 512 + phase;
        levels(&mut p, &mut now, &[(543, 15), (544, 14), (545, 14)]);
    }
    let mut held = envelope(2, 9);
    held.tick(1119);
    assert_eq!(held.envelope_level(), 0);
    write(&mut held, 13, 9);
    levels(&mut held, &mut 1119, &[(1151, 15), (1152, 14), (1153, 14)]);

    let mut p = envelope(2, 8);
    p.tick(47); // retained nonzero counter: next internal step at 64, visible at 96.
    write(&mut p, 13, 12); // opposite attack starts at zero.
    levels(
        &mut p,
        &mut 47,
        &[(63, 0), (64, 0), (95, 0), (96, 1), (97, 1)],
    );
}

#[test]
fn subperiod_retriggers_reset_hidden_steps_as_well_as_visible_levels() {
    let mut p = envelope(1, 8);
    for _ in 0..10 {
        p.tick(16); // one internal step, still at visible level 15.
        assert_eq!(p.envelope_level(), 15);
        write(&mut p, 13, 8);
    }
    // No visible YM-style dip is promised on AY while rewriting each internal step.
    levels(&mut p, &mut 160, &[(191, 15), (192, 14), (193, 14)]);
    let mut p = envelope(1, 8);
    for expected in [15, 14, 15, 14] {
        p.tick(24); // faster than a visible level, but crossing two internal steps on alternate writes.
        assert_eq!(p.envelope_level(), expected);
        write(&mut p, 13, 8);
    }
}

#[test]
fn zero_period_matches_one_under_the_provisional_policy() {
    for shape in 0..16 {
        let mut zero = envelope(0, shape);
        let mut one = envelope(1, shape);
        for p in [&mut zero, &mut one] {
            write(p, 7, 0x3f);
            write(p, 8, 16);
        }
        for time in [17, 15, 1, 479, 127, 901] {
            zero.tick(time);
            one.tick(time);
            assert_eq!(zero.envelope_level(), one.envelope_level());
            assert_eq!(zero.samples, one.samples);
            write(&mut zero, 13, shape);
            write(&mut one, 13, shape);
        }
    }
}

#[test]
fn sampled_envelope_matches_independent_clock_and_dac_calculation_on_each_channel() {
    // Independent 16-level voltage contract; sample times come from the CPU clock,
    // not a production divider or envelope helper. Gate tone/noise to constant high.
    let voltage = [
        0.0_f64, 0.009, 0.013, 0.019, 0.027, 0.040, 0.058, 0.085, 0.123, 0.178, 0.258, 0.375,
        0.544, 0.689, 0.838, 1.0,
    ];
    for channel in 0..3 {
        let mut p = envelope(257, 8);
        write(&mut p, 7, 0x3f);
        write(&mut p, 8 + channel, 0x1f); // low bits ignored when envelope selected.
        let cycles = 512 * 257 * 3;
        p.tick(cycles);
        assert_eq!(p.samples.len(), cycles as usize * 44100 / 3_579_545);
        let mut dc = 0.0_f64;
        for (i, &sample) in p.samples.iter().enumerate() {
            let time = ((i as u64 + 1) * 3_579_545).div_ceil(44100);
            let level = 15 - ((time / (32 * 257)) % 16) as usize;
            let input = voltage[level] / 3.0;
            dc += 0.001 * (input - dc);
            let expected = (input - dc) * 0.6;
            assert!(
                (f64::from(sample) - expected).abs() < 0.000001,
                "channel {channel}, sample {i}, CPU cycle {time}: {sample} != {expected}"
            );
            assert!(sample.is_finite() && sample.abs() <= 0.6);
        }
    }
}

#[test]
fn fixed_volume_channels_are_isolated_from_envelope_period_and_retriggers() {
    for channel in 0..3 {
        let mut a = envelope(1, 8);
        let mut b = envelope(257, 14);
        for p in [&mut a, &mut b] {
            write(p, 7, 0x3f);
            write(p, 8 + channel, 13);
        }
        for shape in 0..16 {
            a.tick(1073);
            b.tick(1073);
            write(&mut a, 13, shape);
            assert_eq!(a.samples, b.samples);
        }
    }
}

#[test]
fn original_z80_port_probe_measures_envelope_repeat_on_both_models() {
    use msx_core::{Machine, Model};
    let code: Vec<u8> = include_str!("../../../tests/fixtures/envelope-probe.hex")
        .lines()
        .flat_map(|line| line.split('#').next().unwrap().split_whitespace())
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect();
    for model in [Model::Msx1, Model::Msx2] {
        let mut rom = vec![0; 32768];
        rom[..3].copy_from_slice(&[0xc3, 0, 1]);
        rom[0x100..0x100 + code.len()].copy_from_slice(&code);
        rom[0x2d] = u8::from(model == Model::Msx2);
        let mut m = Machine::default();
        m.set_model(model);
        if model == Model::Msx1 {
            m.load_bios(&rom).unwrap();
        } else {
            let mut sub = vec![0; 16384];
            sub[..2].copy_from_slice(b"CD");
            m.load_msx2_firmware(&rom, &sub).unwrap();
        }
        let mut elapsed = 0;
        while elapsed < 512 * 257 * 5 {
            elapsed += m.step();
        }
        assert!(m.cpu.halted);
        assert_eq!(&m.hardware.psg.registers[11..14], &[1, 1, 8]);
        let samples = &m.hardware.psg.samples;
        assert_eq!(samples.len(), elapsed as usize * 44100 / 3_579_545);
        let repeats: Vec<usize> = samples
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w[1] - w[0] > 0.12)
            .map(|(i, _)| i + 1)
            .collect();
        assert!(repeats.len() >= 4, "{model:?}: {repeats:?}");
        let interval = 512 * 257 * 44100_u64 / 3_579_545;
        for pair in repeats.windows(2) {
            assert!((interval..=interval + 1).contains(&((pair[1] - pair[0]) as u64)));
        }
        assert!(samples.iter().all(|v| v.is_finite() && v.abs() <= 0.6));
        let first = samples.clone();
        m.reset();
        let mut replay = 0;
        while replay < elapsed {
            replay += m.step();
        }
        assert_eq!(replay, elapsed);
        assert_eq!(m.hardware.psg.samples, first);
    }
}
