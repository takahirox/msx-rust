//! Independent MIT fixtures, observed through actual Z80 execution and devices.
use msx_core::{cpu::Bus, firmware, BootPolicy, Firmware, Machine, Mapper, Model};
fn enter(m: &mut Machine, text: &str) {
    for key in text.bytes().chain([13]) {
        m.text_key(key);
    }
    m.frame();
}
fn boot_cart(m: &mut Machine) {
    for _ in 0..600 {
        m.frame();
        if m.hardware.read(0xc202) == 0xa5 {
            m.frame();
            return;
        }
    }
    panic!("service fixture failed at {:04x}", m.cpu.pc);
}
fn rgb(m: &Machine, x: usize, y: usize) -> &[u8] {
    let i = (y * m.hardware.vdp.width() + x) * 4;
    &m.hardware.vdp.pixels[i..i + 3]
}
#[test]
fn automatic_boot_services_reset_and_eject_on_both_models() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = Machine::default();
        m.set_model(model);
        m.frame();
        assert_eq!(m.boot_policy, BootPolicy::Automatic);
        assert_eq!(m.hardware.firmware, Firmware::WeaveBasic);
        assert!(m.basic.console.transcript.contains("READY"));
        enter(&mut m, "10 PRINT 42");
        let pc = m.cpu.pc;
        let vram = m.hardware.vdp.vram.clone();
        for bytes in [vec![], vec![0; 17], vec![0; 65536]] {
            assert!(m.load_cartridge(&bytes).is_err());
            assert_eq!(m.cpu.pc, pc);
            assert_eq!(m.hardware.vdp.vram, vram);
            assert_eq!(m.hardware.firmware, Firmware::WeaveBasic);
            assert!(m.basic.program.contains_key(&10));
        }
        let cart = if model == Model::Msx1 {
            firmware::cbios_msx1_test_cartridge()
        } else {
            firmware::cbios_test_cartridge()
        };
        m.load_cartridge_with_mapper(&cart, Mapper::Plain).unwrap();
        assert!(m.basic.program.is_empty());
        assert_eq!(m.hardware.firmware, Firmware::Cbios);
        assert_eq!(m.hardware.slots, 0xf0); // C-BIOS gets reset mapping, not AB init.
        assert_eq!(m.hardware.input(0xf0), 255);
        m.hardware.output(0xf1, 0);
        m.step();
        assert!(m.basic.console.transcript.is_empty());
        boot_cart(&mut m);
        assert_eq!(m.hardware.read(0xc200), 0x5a); // WRSLT/RDSLT
        assert_eq!(rgb(&m, 0, 16), [255, 255, 255]);
        assert_eq!(rgb(&m, 1, 16), [0, 0, 0]);
        m.hardware.key(8, 0, true);
        for _ in 0..3 {
            m.frame();
        }
        assert_eq!(m.hardware.read(0xc201), 1); // SNSMAT
        assert_eq!(rgb(&m, 8, 16), [255, 255, 255]);
        assert!(m.hardware.psg.samples.iter().any(|v| v.abs() > 0.05));
        m.reset();
        boot_cart(&mut m);
        assert_eq!(m.hardware.read(0xc201), 0);
        m.unload_cartridge();
        m.frame();
        assert_eq!(m.hardware.model, model);
        assert_eq!(m.hardware.firmware, Firmware::WeaveBasic);
        enter(&mut m, "PRINT 73");
        assert!(m.basic.console.transcript.contains("73\nREADY"));
    }
}
#[test]
fn explicit_firmware_precedes_import_and_eject_and_does_not_leak_models() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = Machine::default();
        m.set_model(model);
        let cart = firmware::test_cartridge();
        m.use_cbios().unwrap();
        let bios = m.hardware.bios.clone();
        let pc = m.cpu.pc;
        if model == Model::Msx1 {
            assert!(m.load_bios(firmware::CBIOS_MSX2_MAIN).is_err());
        } else {
            assert!(m
                .load_msx2_firmware(firmware::CBIOS_MSX1_MAIN, firmware::CBIOS_MSX2_SUB)
                .is_err());
        }
        assert_eq!(m.hardware.bios, bios);
        assert_eq!(m.cpu.pc, pc);
        m.load_cartridge(&cart).unwrap();
        m.unload_cartridge();
        assert_eq!(m.hardware.firmware, Firmware::Cbios);
        assert_eq!(m.boot_policy, BootPolicy::Explicit);
        if model == Model::Msx1 {
            m.load_bios(firmware::CBIOS_MSX1_MAIN).unwrap();
        } else {
            m.load_msx2_firmware(firmware::CBIOS_MSX2_MAIN, firmware::CBIOS_MSX2_SUB)
                .unwrap();
        }
        m.load_cartridge(&cart).unwrap();
        assert_eq!(m.hardware.firmware, Firmware::User);
        m.reset();
        m.unload_cartridge();
        assert_eq!(m.hardware.firmware, Firmware::User);
        m.hardware.output(0xf1, 0);
        m.frame();
        assert!(m.basic.console.transcript.is_empty());
        if model == Model::Msx2 {
            m.use_diagnostic().unwrap();
            m.load_cartridge(&cart).unwrap();
            m.unload_cartridge();
            assert_eq!(m.hardware.firmware, Firmware::Diagnostic);
            assert_eq!(m.hardware.input(0xf0), 255);
        }
        m.use_builtin();
        m.frame();
        assert_eq!(m.boot_policy, BootPolicy::Automatic);
        assert!(m.basic.console.transcript.contains("READY"));
        m.set_model(if model == Model::Msx1 {
            Model::Msx2
        } else {
            Model::Msx1
        });
        m.frame();
        assert_eq!(m.hardware.firmware, Firmware::WeaveBasic);
        assert!(m.hardware.cartridge.is_empty());
        assert!(m.basic.console.transcript.contains("READY"));
    }
}
#[test]
fn msx2_basic_uses_input_arrays_data_subroutines_and_cooperative_execution() {
    let mut m = Machine::default();
    m.set_model(Model::Msx2);
    m.frame();
    for line in [
        "10 DIM A(2)",
        "20 DATA 12,30",
        "30 READ A(1),A(2)",
        "40 INPUT N$",
        "50 GOSUB 100",
        "60 END",
        "100 PRINT N$;A(1)+A(2):RETURN",
        "RUN",
    ] {
        enter(&mut m, line);
    }
    enter(&mut m, "ORBIT");
    assert!(m.basic.console.transcript.contains("ORBIT42\nREADY"));
    enter(&mut m, "BEEP");
    m.frame();
    assert!(m.hardware.psg.samples.iter().any(|s| s.abs() > 0.05));
    enter(&mut m, "SOUND 8,0");
    for _ in 0..16 {
        m.frame();
    }
    assert!(m.hardware.psg.samples.iter().all(|s| s.abs() < 0.0001));
    m.hardware.joystick = 0x2e;
    m.hardware.psg.select(15);
    m.hardware.psg.write(0);
    m.hardware.psg.select(14);
    assert_eq!(m.hardware.input(0xa2) & 0x3f, 0x2e);

    enter(&mut m, "10 GOTO 10");
    enter(&mut m, "RUN");
    for _ in 0..3 {
        m.frame();
    }
    assert!(m.basic.running);
    m.text_key(27);
    m.frame();
    assert!(!m.basic.running);
    assert!(m.basic.console.transcript.contains("BREAK"));
}
#[test]
fn bitmap_primitives_pack_pixels_clip_and_restore_text_on_msx2() {
    for (mode, width, bits, color) in [
        (5, 256, 4, 10),
        (6, 512, 2, 2),
        (7, 512, 4, 10),
        (8, 256, 8, 0x1c),
    ] {
        let mut m = Machine::default();
        m.set_model(Model::Msx2);
        m.frame();
        enter(&mut m, &format!("SCREEN {mode}:COLOR {color},0:LINE (4,4)-(20,20),,B:PAINT (5,5),{color}:CIRCLE (40,40),5,{color}:PSET ({},211),{color}",width-1));
        for _ in 0..4 {
            m.frame();
        }
        assert_eq!(m.hardware.vdp.mode(), mode);
        assert_eq!(m.hardware.vdp.width(), width);
        assert_eq!(m.hardware.vdp.height(), 212);
        let expected = if mode == 8 {
            [255, 0, 0]
        } else {
            m.hardware.vdp.palette[color]
        };
        for (x, y) in [(4, 4), (5, 5), (19, 19), (45, 40), (width - 1, 211)] {
            assert_eq!(rgb(&m, x, y), expected, "mode {mode} at {x},{y}");
        }
        assert_eq!(rgb(&m, 21, 20), [0, 0, 0]);
        // Packed VRAM bytes are independent of framebuffer labels and rendering.
        let index = ((211 * width + width - 1) * bits) / 8;
        assert_eq!(
            m.hardware.vdp.vram[index] & ((1u16 << bits) - 1) as u8,
            color as u8
        );
        m.text_key(13);
        m.frame();
        enter(&mut m, "PRINT 91");
        assert_eq!(m.hardware.vdp.mode(), 0);
        assert_eq!(m.hardware.vdp.height(), 192);
        assert!(m.basic.console.transcript.contains("91\nREADY"));
    }
}

#[test]
fn declared_model_cartridges_commit_atomically_and_preserve_explicit_policy() {
    let mut m = Machine::default();
    m.frame();
    enter(&mut m, "10 PRINT 42");
    let pc = m.cpu.pc;
    let pixels = m.hardware.vdp.pixels.clone();
    assert!(m
        .load_cartridge_for_model_owned(vec![0; 17], Mapper::Plain, Model::Msx2)
        .is_err());
    assert_eq!(m.hardware.model, Model::Msx1);
    assert_eq!(m.hardware.firmware, Firmware::WeaveBasic);
    assert_eq!(m.cpu.pc, pc);
    assert_eq!(m.hardware.vdp.pixels, pixels);
    assert!(m.basic.program.contains_key(&10));
    m.load_cartridge_for_model_owned(firmware::msx2_cartridge(), Mapper::Plain, Model::Msx2)
        .unwrap();
    assert_eq!(m.hardware.model, Model::Msx2);
    assert_eq!(m.hardware.firmware, Firmware::Cbios);
    assert_eq!(m.boot_policy, BootPolicy::Automatic);
    assert!(m.basic.program.is_empty());
    m.unload_cartridge();
    m.frame();
    assert_eq!(m.hardware.model, Model::Msx2);
    assert!(m.basic.console.transcript.contains("READY"));
    m.use_cbios().unwrap();
    m.load_cartridge_for_model_owned(
        firmware::cbios_msx1_test_cartridge(),
        Mapper::Plain,
        Model::Msx1,
    )
    .unwrap();
    assert_eq!(m.hardware.bios, firmware::CBIOS_MSX1_MAIN);
    assert_eq!(m.boot_policy, BootPolicy::Explicit);
    boot_cart(&mut m);
    m.unload_cartridge();
    assert_eq!(m.hardware.firmware, Firmware::Cbios);
    m.load_bios(firmware::CBIOS_MSX1_MAIN).unwrap();
    m.load_cartridge(firmware::CBIOS_MSX1_MAIN).unwrap_err(); // A BIOS is not a cartridge.
    let pc = m.cpu.pc;
    assert!(m
        .load_cartridge_for_model_owned(firmware::msx2_cartridge(), Mapper::Plain, Model::Msx2)
        .unwrap_err()
        .contains("override"));
    assert_eq!(m.hardware.model, Model::Msx1);
    assert_eq!(m.hardware.firmware, Firmware::User);
    assert_eq!(m.cpu.pc, pc);
    m.set_model(Model::Msx2);
    m.use_diagnostic().unwrap();
    m.load_cartridge_for_model_owned(firmware::msx2_cartridge(), Mapper::Plain, Model::Msx2)
        .unwrap();
    let cartridge = m.hardware.cartridge.clone();
    assert!(m
        .load_cartridge_for_model_owned(firmware::test_cartridge(), Mapper::Plain, Model::Msx1)
        .is_err());
    assert_eq!(m.hardware.cartridge, cartridge);
    assert_eq!(m.hardware.model, Model::Msx2);
    assert_eq!(m.hardware.firmware, Firmware::Diagnostic);
}
