use msx_core::{cpu::Bus, firmware, BootPolicy, Firmware, Machine, Mapper, Model};
fn machine() -> Machine {
    let mut m = Machine::default();
    m.set_model(Model::Msx2);
    m
}
fn install(m: &mut Machine) {
    m.install_msx_music(firmware::music_diagnostic_firmware())
        .unwrap();
}
fn tone(m: &mut Machine) {
    for (r, v) in [0x21, 0x21, 0x3f, 0, 0, 0xf0, 0, 0x0f]
        .into_iter()
        .enumerate()
        .map(|(r, v)| (r as u8, v))
        .chain([(0x30, 0), (0x10, 0x22), (0x20, 0x19)])
    {
        m.hardware.output(0xab7c, r);
        for _ in 0..3 {
            m.step();
        }
        m.hardware.output(0xcd7d, v);
        for _ in 0..24 {
            m.step();
        }
    }
}
fn silent(m: &Machine) -> bool {
    m.hardware.psg.samples.iter().all(|&s| s == 0.0)
}
#[test]
fn absent_internal_io_hidden_rom_mapping_exact_bytes_boundaries_and_protection() {
    let mut m = machine();
    m.hardware.slots = 0xff;
    m.hardware.secondary[3] = 0x55;
    assert_eq!(m.hardware.read(0x4000), 255);
    tone(&mut m);
    m.frame();
    assert!(silent(&m));
    let rom: Vec<u8> = (0..16384).map(|i| (i * 73 + i / 256) as u8).collect();
    m.install_msx_music(rom.clone()).unwrap();
    m.hardware.slots = 0xff;
    m.hardware.secondary[3] = 0x55;
    for a in 0x4000..0x8000 {
        let expected = rom[usize::from(a - 0x4000)];
        assert_eq!(m.hardware.read(a), expected);
        m.hardware.write(a, !expected);
        assert_eq!(m.hardware.read(a), expected);
    }
    for a in [0, 0x3fff, 0x8000, 0xbfff, 0xc000, 0xfffe] {
        assert_eq!(m.hardware.read(a), 255);
    }
    assert_eq!(m.hardware.read(0xffff), 0xaa);
    m.hardware.write(0xffff, 0x15);
    assert_eq!(m.hardware.read(0xffff), !0x15);
    for sub in [0, 2, 3] {
        m.hardware.secondary[3] = sub << 2;
        assert_ne!(m.hardware.read(0x4018), rom[0x18]);
    }
    for port in [0x7c, 0x7d, 0xab7c, 0xcd7d] {
        assert_eq!(m.hardware.input(port), 255);
    }
    m.reset();
    tone(&mut m);
    m.frame();
    let control = m.hardware.psg.samples.clone();
    // Hide every ROM page; HALT idle fetches execute from mapped RAM instead.
    m.hardware.slots = 255;
    m.hardware.secondary[3] = 255;
    m.frame();
    assert!(m.hardware.psg.samples.iter().any(|x| x.abs() > 0.01));
    assert!(control.iter().any(|x| x.abs() > 0.01));
}
#[test]
fn original_z80_discovers_calls_fixture_and_programs_ports_while_rom_is_hidden() {
    let mut m = machine();
    install(&mut m);
    m.use_diagnostic().unwrap();
    m.load_cartridge_with_mapper(&firmware::music_test_cartridge(), Mapper::Plain)
        .unwrap();
    for _ in 0..10000 {
        m.step();
        if m.cpu.halted {
            break;
        }
    }
    assert!(m.cpu.halted, "pc={:04x}", m.cpu.pc);
    assert_eq!(
        (0xc100..=0xc105)
            .map(|a| m.hardware.read(a))
            .collect::<Vec<_>>(),
        [1, 8, b'W', 0x65, 255, 0xa5]
    );
    assert_eq!(m.hardware.vdp.registers[7], 2);
    assert_eq!(m.hardware.slots, 0xd4);
    assert_eq!(m.hardware.secondary[3], 255);
    for _ in 0..2 {
        m.frame();
    }
    assert!(m.hardware.psg.samples.iter().any(|x| x.abs() > 0.01));
    for pixel in m.hardware.vdp.pixels[..m.hardware.vdp.width() * m.hardware.vdp.height() * 4]
        .as_chunks::<4>()
        .0
    {
        assert_eq!(*pixel, [36, 218, 36, 255]);
    }
    m.remove_msx_music();
    assert!(!m.has_msx_music());
    m.frame();
    assert!(silent(&m));
}
#[test]
fn transactional_lifecycle_preserves_selected_firmware_sram_disk_and_tape() {
    let mut m = machine();
    m.load_cartridge_with_mapper(
        &firmware::mapper_test_cartridge(Mapper::Ascii8Sram2),
        Mapper::Ascii8Sram2,
    )
    .unwrap();
    m.restore_cartridge_sram(&[0x69; 2048]).unwrap();
    let cartridge = m.hardware.cartridge.clone();
    let mut disk_rom = vec![0x53; 16384];
    disk_rom[..2].copy_from_slice(b"AB");
    m.install_philips_disk_rom(disk_rom).unwrap();
    m.mount_disk(vec![0x42; 368640], false).unwrap();
    m.hardware.fdc.disk.as_mut().unwrap().dirty = true;
    m.hardware
        .cassette
        .mount(firmware::cassette_test_image(), 1200)
        .unwrap();
    m.hardware.cassette.tick(12345, true);
    let tape = m.hardware.cassette.position_cycles();
    m.cpu.pc = 0x1234;
    for n in [0, 1, 16383, 16385, 32768] {
        assert!(m.install_msx_music(vec![0; n]).is_err());
        assert_eq!(m.cpu.pc, 0x1234);
        assert!(!m.has_msx_music());
    }
    install(&mut m);
    assert_eq!(m.hardware.firmware, Firmware::Cbios);
    assert_eq!(m.boot_policy, BootPolicy::Automatic);
    for remove in [false, true] {
        if remove {
            m.remove_msx_music();
        } else {
            m.reset();
        }
        assert_eq!(m.hardware.cartridge, cartridge);
        assert_eq!(m.cartridge_sram(), [0x69; 2048]);
        assert!(m.has_philips_disk_rom());
        let disk = m.hardware.fdc.disk.as_ref().unwrap();
        assert!(disk.dirty);
        assert!(!disk.write_protected);
        assert!(disk.bytes().iter().all(|&x| x == 0x42));
        assert_eq!(m.hardware.cassette.position_cycles(), tape);
    }
    install(&mut m);
    m.use_cbios().unwrap();
    assert!(m.has_msx_music());
    m.use_diagnostic().unwrap();
    assert!(m.has_msx_music());
    m.load_cartridge_with_mapper(&firmware::music_test_cartridge(), Mapper::Plain)
        .unwrap();
    assert!(m.has_msx_music());
    m.unload_cartridge();
    assert!(m.has_msx_music());
    let mut main = vec![0; 32768];
    main[0x2d] = 1;
    let mut sub = vec![0; 16384];
    sub[..2].copy_from_slice(b"CD");
    m.load_msx2_firmware(&main, &sub).unwrap();
    assert!(m.has_msx_music());
    m.remove_msx_music();
    assert_eq!(m.hardware.firmware, Firmware::User);
    assert_eq!(m.hardware.bios, main);
    assert_eq!(m.hardware.subrom, sub);
}
#[test]
fn reset_clears_chip_msx1_is_inactive_and_model_return_reactivates() {
    let mut m = machine();
    install(&mut m);
    tone(&mut m);
    m.frame();
    assert!(!silent(&m));
    m.reset();
    assert!(m.has_msx_music());
    m.frame();
    assert!(silent(&m));
    m.set_model(Model::Msx1);
    assert!(m.has_msx_music());
    tone(&mut m);
    m.frame();
    assert!(silent(&m));
    m.hardware.slots = 0xff;
    m.hardware.write(0x4018, 0x7b);
    assert_eq!(m.hardware.read(0x4018), 0x7b);
    m.set_model(Model::Msx2);
    m.frame();
    assert!(silent(&m));
    m.hardware.slots = 0xfc;
    m.hardware.secondary[3] = 0xf7;
    assert_eq!(m.hardware.read(0x4018), b'W');
    m.reset();
    tone(&mut m);
    m.frame();
    assert!(!silent(&m));
    m.use_builtin();
    assert!(m.has_msx_music());
    m.frame();
    assert!(silent(&m));
}

fn fixture(text: &str, size: usize, fill: u8) -> Vec<u8> {
    let mut bytes = vec![fill; size];
    for line in text.lines().filter(|s| !s.starts_with('#')) {
        let (offset, data) = line.split_once(':').unwrap();
        let offset = usize::from_str_radix(offset, 16).unwrap();
        for (i, v) in data.split_whitespace().enumerate() {
            bytes[offset + i] = u8::from_str_radix(v, 16).unwrap();
        }
    }
    bytes
}
#[test]
fn cpu_philips_transfers_with_scci_psg_and_fm_keep_exact_disk_and_audio() {
    use msx_core::SoundCartridgeProfile;
    for profile in [
        SoundCartridgeProfile::Snatcher,
        SoundCartridgeProfile::SdSnatcher,
    ] {
        let mut fm_pcm = Vec::new();
        let mut disabled_pcm = Vec::new();
        for installed in [false, true] {
            let mut m = machine();
            let mut main = fixture(
                include_str!("../../../tests/fixtures/philips-system.hex"),
                32768,
                0,
            );
            main[0x2d] = 1;
            let mut sub = vec![0; 16384];
            sub[..2].copy_from_slice(b"CD");
            sub[0x100] = 0x7b;
            m.load_msx2_firmware(&main, &sub).unwrap();
            m.install_sound_cartridge(profile).unwrap();
            m.install_philips_disk_rom(fixture(
                include_str!("../../../tests/fixtures/music-disk.hex"),
                16384,
                255,
            ))
            .unwrap();
            if installed {
                install(&mut m);
            }
            let mut image = fixture(
                include_str!("../../../tests/fixtures/philips-scci-sector.hex"),
                368640,
                0x35,
            );
            image[510] = if profile == SoundCartridgeProfile::Snatcher {
                0
            } else {
                8
            };
            let mut expected = image.clone();
            expected[8 * 512..9 * 512].fill(0x6c);
            m.mount_disk(image, false).unwrap();
            m.boot_disk().unwrap();
            while m.cpu.tstates < 4_000_000 && !m.cpu.halted {
                m.step();
            }
            assert!(m.cpu.halted, "pc={:04x}", m.cpu.pc);
            assert_eq!(m.hardware.read(0xc033), 0xa5);
            assert_eq!(m.hardware.read(0xc030), 0x44);
            assert_eq!(m.hardware.read(0xc031), 96);
            assert_eq!(m.hardware.read(0xc032), 0x6d);
            assert_eq!(m.hardware.fdc.disk.as_ref().unwrap().bytes(), expected);
            assert!(m.hardware.fdc.disk.as_ref().unwrap().dirty);
            m.frame();
            assert!(m
                .hardware
                .psg
                .samples
                .iter()
                .all(|x| x.is_finite() && x.abs() <= 1.0));
            assert!(m.hardware.psg.samples.iter().any(|x| x.abs() > 0.05));
            if installed {
                fm_pcm = m.hardware.psg.samples.clone();
            } else {
                disabled_pcm = m.hardware.psg.samples.clone();
            }
        }
        assert_eq!(fm_pcm.len(), disabled_pcm.len());
        assert!(fm_pcm
            .iter()
            .zip(disabled_pcm)
            .any(|(a, b)| (a - b).abs() > 0.01));
    }
}
