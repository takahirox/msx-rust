//! Original synthetic programs: elapsed MSX time must reach every clocked device.
use msx_core::{
    cpu::Bus,
    disk::{Disk, BYTE_CYCLES},
    firmware,
    psg::{Psg, CPU_HZ, SAMPLE_RATE},
    Machine, Mapper, Model,
};

fn machine(model: Model) -> Machine {
    let mut m = Machine::default();
    m.set_model(model);
    m.cpu.pc = 0xc100; // Fresh RAM contains NOPs, under either slot profile.
    m
}
fn step(m: &mut Machine, expected: u32) {
    let before = m.cpu.tstates;
    assert_eq!(m.step(), expected);
    assert_eq!(m.cpu.tstates - before, u64::from(expected));
}
fn nop(m: &mut Machine) {
    m.hardware.slots |= 0xc0;
    m.hardware.secondary[3] |= 0xc0; // Select RAM after firmware reset slot probing.
    m.cpu.pc = 0xc100;
    m.hardware.write(0xc100, 0);
    step(m, 5);
}

#[test]
fn machine_fetch_policy_survives_lifecycle_transitions() {
    nop(&mut Machine::default());
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = machine(model);
        nop(&mut m);
        m.reset();
        nop(&mut m);
        m.use_cbios().unwrap();
        nop(&mut m);
        m.load_cartridge(&firmware::test_cartridge()).unwrap();
        nop(&mut m);
        assert!(m.load_cartridge(&[]).is_err());
        nop(&mut m);
        m.unload_cartridge();
        nop(&mut m);
        m.use_builtin();
        nop(&mut m);
        if model == Model::Msx2 {
            m.use_diagnostic().unwrap();
            nop(&mut m);
            let mut main = vec![0; 32768];
            main[0x2d] = 1;
            let mut sub = vec![0; 16384];
            sub[..2].copy_from_slice(b"CD");
            m.load_msx2_firmware(&main, &sub).unwrap();
        } else {
            m.load_bios(&vec![0; 32768]).unwrap();
        }
        nop(&mut m);
        m.use_builtin();
        m.mount_disk(firmware::demo_disk(), true).unwrap();
        m.boot_disk().unwrap();
        nop(&mut m);
    }
}

#[test]
fn elapsed_fetch_time_crosses_vdp_irq_fdc_and_rtc_boundaries() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = machine(model);
        m.hardware.vdp.set_register(1, 0x20); // Enable VBlank IRQ.
        m.hardware.vdp.tick(228 * 192 - 115);
        m.hardware.rtc.tick(CPU_HZ - 115);
        m.hardware
            .fdc
            .mount(Disk::new(vec![0x5a; 368640], true).unwrap())
            .unwrap();
        m.hardware.fdc.write(5, 0x81); // Drive A, motor on.
        m.hardware.fdc.write(2, 1);
        m.hardware.fdc.write(0, 0x80); // Read sector, first byte after 114 T.
        assert_eq!(BYTE_CYCLES, 114);
        for _ in 0..22 {
            step(&mut m, 5);
        }
        assert_eq!(m.cpu.tstates, 110);
        assert!(!m.hardware.vdp.irq());
        assert!(!m.hardware.fdc.drq);
        assert_eq!(m.hardware.rtc.read() & 15, 0);
        step(&mut m, 5);
        assert_eq!(m.hardware.vdp.line, 192);
        assert!(m.hardware.vdp.irq());
        assert!(m.hardware.fdc.drq);
        assert_eq!(m.hardware.fdc.read(3), 0x5a);
        assert_eq!(m.hardware.rtc.read() & 15, u8::from(model == Model::Msx2));
        m.cpu.iff1 = true;
        m.cpu.im = 1;
        step(&mut m, 13); // IRQ recognized on the next instruction boundary.
        assert_eq!(m.cpu.pc, 0x38);
        // A subsequent byte has its own 114-T delay, including IRQ elapsed time.
        for _ in 0..20 {
            nop(&mut m);
        }
        assert!(!m.hardware.fdc.drq); // 13 + 100 = 113 T.
        nop(&mut m);
        assert!(m.hardware.fdc.drq);
    }
}

#[test]
fn psg_samples_and_scc_phase_follow_elapsed_fetch_time() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = machine(model);
        let mut reference = Psg::default();
        for (r, value) in [(0, 19), (7, 0x3e), (8, 15)] {
            m.hardware.psg.select(r);
            m.hardware.psg.write(value);
            reference.select(r);
            reference.write(value);
        }
        for _ in 0..1000 {
            step(&mut m, 5);
        }
        reference.tick(5000);
        assert_eq!(
            m.hardware.psg.samples.len(),
            (5000 * u64::from(SAMPLE_RATE) / u64::from(CPU_HZ)) as usize
        );
        assert_eq!(m.hardware.psg.samples, reference.samples);
        assert!(m.hardware.psg.samples.iter().any(|x| x.abs() > 0.05));

        let mut rom = vec![0; 65536];
        rom[..4].copy_from_slice(&[b'A', b'B', 0x10, 0x40]);
        m.load_cartridge_with_mapper(&rom, Mapper::KonamiScc)
            .unwrap();
        m.hardware.slots = 0xd4;
        m.hardware.secondary[3] = 0xff;
        m.hardware.write(0x9000, 0x3f);
        for i in 0..32 {
            m.hardware.write(0x9800 + i, i as u8);
        }
        m.hardware.write(0x9880, 19); // Wave advances once per 20 elapsed T.
        m.hardware.write(0x98e0, 0x40); // Rotating wave reads expose oscillator phase.
        for _ in 0..23 {
            nop(&mut m);
        }
        assert_eq!(m.cpu.tstates, 115);
        assert_eq!(m.hardware.read(0x9800), 27); // (-floor(115/20)) mod 32.
    }
}

#[test]
fn msx2_command_engine_receives_fetch_wait_time() {
    let mut m = machine(Model::Msx2);
    let v = &mut m.hardware.vdp;
    for (r, value) in [(0, 6), (1, 0x40), (40, 3), (42, 1), (44, 3), (46, 0x80)] {
        v.set_register(r, value);
    }
    // LMMV writes one pixel per 8 T. Five NOPs take 25 T, enough for three.
    // Advancing devices by bare 4-T NOP counts would miss the final pixel.
    step(&mut m, 5);
    assert_eq!(m.hardware.vdp.vram[0], 0);
    step(&mut m, 5);
    assert_eq!(m.hardware.vdp.vram[0], 0x30);
    step(&mut m, 5);
    assert_eq!(m.hardware.vdp.vram[0], 0x30);
    step(&mut m, 5);
    assert_eq!(m.hardware.vdp.vram[0], 0x33);
    step(&mut m, 5);
    assert_eq!(m.hardware.vdp.vram[1], 0x30);
    m.hardware.vdp.set_register(15, 2);
    assert_eq!(m.hardware.vdp.read_status() & 1, 0);
}

#[test]
fn frame_budgets_preserve_overshoot_and_clock_frequencies() {
    assert_eq!((CPU_HZ, SAMPLE_RATE), (3_579_545, 44_100));
    for (model, pal, lines) in [
        (Model::Msx1, false, 262),
        (Model::Msx2, false, 262),
        (Model::Msx2, true, 313),
    ] {
        let mut m = machine(model);
        m.hardware.write(0xc100, 0x76);
        if pal {
            m.hardware.vdp.set_register(9, 2);
        }
        let budget: u64 = 228 * lines;
        assert_eq!(m.hardware.vdp.frame_cycles(), budget as i32);
        let mut samples = 0;
        for frame in 1..=10 {
            m.frame();
            let expected = (frame * budget).div_ceil(5) * 5;
            assert_eq!(m.cpu.tstates, expected);
            assert_eq!(m.hardware.vdp.frames, frame);
            samples += m.hardware.psg.samples.len();
            assert_eq!(
                samples,
                (expected * u64::from(SAMPLE_RATE) / u64::from(CPU_HZ)) as usize
            );
        }
    }
}
