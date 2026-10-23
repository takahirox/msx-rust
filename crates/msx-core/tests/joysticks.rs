use msx_core::{cpu::Bus, firmware, Firmware, Hardware, Machine, Model};

fn write(h: &mut Hardware, register: u8, value: u8) {
    h.output(0xa0, register);
    h.output(0xa1, value);
}
fn read(h: &mut Hardware, register: u8) -> u8 {
    h.output(0xa0, register);
    h.input(0xa2)
}

#[test]
fn independent_connectors_common_lines_and_fixed_direction_policy() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = Machine::default();
        m.set_model(model);
        let h = &mut m.hardware;
        // Explicit single switches, diagonals and all switches; expectations do
        // not call the implementation's routing/masking helpers.
        let inputs = [
            0x3f, 0x3e, 0x3d, 0x3b, 0x37, 0x2f, 0x1f, 0x36, 0x35, 0x39, 0x3a, 0,
        ];
        for one in inputs {
            for two in inputs {
                assert!(h.set_joystick(0, one));
                assert!(h.set_joystick(1, two));
                for (r15, expected) in [
                    (0, 0x40 | one),
                    (0x40, 0x40 | two),
                    (0x10, 0x7f),
                    (0x60, 0x7f),
                    (0x20, 0x40 | one),
                    (0x50, 0x40 | two),
                ] {
                    write(h, 15, r15);
                    assert_eq!(read(h, 14), expected, "{model:?}, R15={r15:02x}");
                }
            }
        }
        h.set_joystick(0, 0xee); // host bits 6/7 cannot alter R14 bits 6/7
        h.set_joystick(1, 0xd7);
        for direction in [0, 0x40, 0x80, 0xc0] {
            for mixer in [0, 0x15, 0x2a, 0x3f] {
                write(h, 7, direction | mixer);
                assert_eq!(read(h, 7), direction | mixer);
                write(h, 14, 0xff); // output latch cannot replace peripheral input
                for (r15, expected) in [
                    (0, 0x6e),
                    (0x40, 0x57),
                    (0x10, 0x7f),
                    (0x60, 0x7f),
                    (0x20, 0x6e),
                    (0x50, 0x57),
                ] {
                    // Unrelated R15 outputs keep their readback and do not gate inputs.
                    for unrelated in [0, 0x0f, 0x80, 0x8f] {
                        write(h, 15, r15 | unrelated);
                        assert_eq!(read(h, 15), r15 | unrelated);
                        assert_eq!(read(h, 14), expected);
                        assert_eq!(read(h, 7), direction | mixer);
                    }
                }
            }
        }
        for invalid in [2, 3, usize::MAX] {
            assert!(!h.set_joystick(invalid, 0));
        }
        write(h, 15, 0);
        assert_eq!(read(h, 14), 0x6e);
        write(h, 15, 0x40);
        assert_eq!(read(h, 14), 0x57);
        h.key(3, 6, true);
        h.release_keys();
        assert_eq!(h.keyboard, [255; 11]);
        for select in [0, 0x40] {
            write(h, 15, select);
            assert_eq!(read(h, 14), 0x7f);
        }
        h.set_joystick(0, 0);
        h.set_joystick(1, 0);
        h.key(4, 0, true);
        m.reset();
        assert_eq!(m.hardware.keyboard, [255; 11]);
        for select in [0, 0x40] {
            write(&mut m.hardware, 15, select);
            assert_eq!(read(&mut m.hardware, 14), 0x7f);
        }
    }
}

#[test]
fn r7_peripheral_direction_bits_leave_psg_audio_unchanged() {
    for mixer in [0, 0x15, 0x2a, 0x3f] {
        let mut reference = Vec::new();
        for direction in [0, 0x40, 0x80, 0xc0] {
            let mut h = Hardware::default();
            for (r, v) in [
                (0, 127),
                (2, 59),
                (4, 29),
                (6, 7),
                (8, 15),
                (9, 12),
                (10, 9),
                (7, mixer | direction),
            ] {
                write(&mut h, r, v);
            }
            h.psg.tick(70_000);
            if direction == 0 {
                reference = h.psg.samples.clone();
            } else {
                assert_eq!(h.psg.samples, reference);
            }
        }
    }
}

fn settle(m: &mut Machine) {
    for _ in 0..6 {
        m.frame();
    }
}
fn results(m: &mut Machine) -> Vec<u8> {
    (0xc200..0xc20c).map(|a| m.hardware.read(a)).collect()
}
fn visible(m: &Machine) -> Vec<u8> {
    (0..12)
        .map(|row| {
            let mut value = 0;
            for bit in 0..8 {
                let p = (((row + 4) * 8 * m.hardware.vdp.width()) + (20 + bit) * 8) * 4;
                let pixel = &m.hardware.vdp.pixels[p..p + 3];
                assert!(pixel == [255, 255, 255] || pixel == [0, 0, 0]);
                if pixel == [255, 255, 255] {
                    value |= 1 << bit;
                }
            }
            value
        })
        .collect()
}

#[test]
fn z80_cbios_diagnostic_reads_both_connectors_and_bios_triggers_on_both_models() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = Machine::default();
        m.set_model(model);
        m.load_cartridge(&firmware::joystick_test_cartridge())
            .unwrap();
        assert_eq!(m.hardware.firmware, Firmware::Cbios);
        assert_eq!(
            m.hardware.bios,
            if model == Model::Msx1 {
                firmware::CBIOS_MSX1_MAIN
            } else {
                firmware::CBIOS_MSX2_MAIN
            }
        );
        if model == Model::Msx2 {
            assert_eq!(m.hardware.subrom, firmware::CBIOS_MSX2_SUB);
        }
        for _ in 0..600 {
            m.frame();
            if m.hardware.read(0xc210) == 0xa5 {
                break;
            }
        }
        assert_eq!(m.hardware.read(0xc210), 0xa5, "PC={:04x}", m.cpu.pc);
        settle(&mut m);
        let released = vec![0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0, 0, 0, 0, 0, 0];
        assert_eq!(results(&mut m), released);
        assert_eq!(visible(&m), released);
        // Six independent switches, then diagonals + simultaneous triggers.
        for (one, two, expected) in [
            (
                0x3e,
                0x3d,
                vec![0x7e, 0x7d, 0x7f, 0x7f, 0x7e, 0x7d, 1, 5, 0, 0, 0, 0],
            ),
            (
                0x3b,
                0x37,
                vec![0x7b, 0x77, 0x7f, 0x7f, 0x7b, 0x77, 7, 3, 0, 0, 0, 0],
            ),
            (
                0x2f,
                0x1f,
                vec![0x6f, 0x5f, 0x7f, 0x7f, 0x6f, 0x5f, 0, 0, 255, 0, 0, 255],
            ),
            (
                0x1f,
                0x2f,
                vec![0x5f, 0x6f, 0x7f, 0x7f, 0x5f, 0x6f, 0, 0, 0, 255, 255, 0],
            ),
            (
                0x06,
                0x09,
                vec![0x46, 0x49, 0x7f, 0x7f, 0x46, 0x49, 2, 6, 255, 255, 255, 255],
            ),
            (
                0x3f,
                0x09,
                vec![0x7f, 0x49, 0x7f, 0x7f, 0x7f, 0x49, 0, 6, 0, 255, 0, 255],
            ),
        ] {
            m.hardware.set_joystick(0, one);
            m.hardware.set_joystick(1, two);
            settle(&mut m);
            assert_eq!(results(&mut m), expected, "{model:?}: {one:02x}/{two:02x}");
            assert_eq!(visible(&m), expected);
        }
        m.hardware.release_keys();
        settle(&mut m);
        assert_eq!(results(&mut m), released);
        assert_eq!(visible(&m), released);
        assert!(m.basic.console.transcript.is_empty());
    }
}
