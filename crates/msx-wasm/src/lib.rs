//! Minimal raw Wasm ABI. All emulation and BASIC live in the portable core.
use msx_core::{
    cartridge::{detect_mapper, Confidence},
    cpu::Bus,
    Machine, Mapper, Model,
};
use std::cell::RefCell;
struct App {
    machine: Machine,
    upload: Vec<u8>,
    error: String,
    detection: String,
    detected_mapper: u32,
    confidence: u32,
}
thread_local! {static APP:RefCell<App>=RefCell::new(App{machine:Machine::default(),upload:Vec::new(),error:String::new(),detection:String::new(),detected_mapper:0,confidence:0});}
#[no_mangle]
pub extern "C" fn msx_model(model: u32) {
    APP.with(|a| {
        a.borrow_mut()
            .machine
            .set_model(if model == 1 { Model::Msx2 } else { Model::Msx1 })
    });
}
#[no_mangle]
pub extern "C" fn msx_frame_cycles() -> u32 {
    APP.with(|a| a.borrow().machine.hardware.vdp.frame_cycles() as u32)
}
#[no_mangle]
pub extern "C" fn msx_width() -> u32 {
    APP.with(|a| a.borrow().machine.hardware.vdp.width() as u32)
}
#[no_mangle]
pub extern "C" fn msx_height() -> u32 {
    APP.with(|a| a.borrow().machine.hardware.vdp.height() as u32)
}
#[no_mangle]
pub extern "C" fn msx_frame() {
    APP.with(|a| a.borrow_mut().machine.frame());
}
#[no_mangle]
pub extern "C" fn msx_reset() {
    APP.with(|a| a.borrow_mut().machine.reset());
}
#[no_mangle]
pub extern "C" fn msx_builtin() {
    APP.with(|a| a.borrow_mut().machine.use_builtin());
}
#[no_mangle]
pub extern "C" fn msx_cbios() -> u32 {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        match a.machine.use_cbios() {
            Ok(()) => {
                a.error.clear();
                1
            }
            Err(error) => {
                a.error = error;
                0
            }
        }
    })
}
/// Active firmware comes from the core, including automatic transitions.
#[no_mangle]
pub extern "C" fn msx_firmware() -> u32 {
    APP.with(|a| match a.borrow().machine.hardware.firmware {
        msx_core::Firmware::WeaveBasic => 0,
        msx_core::Firmware::Cbios => 1,
        msx_core::Firmware::Diagnostic => 2,
        msx_core::Firmware::User => 3,
        msx_core::Firmware::Disk => 4,
    })
}
#[no_mangle]
pub extern "C" fn msx_boot_policy() -> u32 {
    APP.with(|a| (a.borrow().machine.boot_policy == msx_core::BootPolicy::Explicit) as u32)
}
#[no_mangle]
pub extern "C" fn msx_diagnostic() -> u32 {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        match a.machine.use_diagnostic() {
            Ok(()) => {
                a.error.clear();
                1
            }
            Err(error) => {
                a.error = error;
                0
            }
        }
    })
}
#[no_mangle]
pub extern "C" fn msx_eject() {
    APP.with(|a| a.borrow_mut().machine.unload_cartridge());
}
#[no_mangle]
pub extern "C" fn msx_text(key: u32) {
    APP.with(|a| a.borrow_mut().machine.text_key(key as u8));
}
#[no_mangle]
pub extern "C" fn msx_key(row: u32, bit: u32, down: u32) {
    APP.with(|a| {
        a.borrow_mut()
            .machine
            .hardware
            .key(row as usize, bit as u8, down != 0)
    });
}
#[no_mangle]
pub extern "C" fn msx_release_keys() {
    APP.with(|a| a.borrow_mut().machine.hardware.release_keys());
}
#[no_mangle]
pub extern "C" fn msx_joystick(value: u32) {
    msx_joystick_indexed(0, value);
}
/// Zero-based connector (0 or 1), six active-low inputs. Returns 1 on success;
/// invalid indices return 0 without mutating either connector.
#[no_mangle]
pub extern "C" fn msx_joystick_indexed(connector: u32, value: u32) -> u32 {
    APP.with(|a| {
        u32::from(
            a.borrow_mut()
                .machine
                .hardware
                .set_joystick(connector as usize, value as u8),
        )
    })
}
#[no_mangle]
pub extern "C" fn msx_pixels() -> *const u8 {
    APP.with(|a| a.borrow().machine.hardware.vdp.pixels.as_ptr())
}
#[no_mangle]
pub extern "C" fn msx_audio() -> *const f32 {
    APP.with(|a| a.borrow().machine.hardware.psg.samples.as_ptr())
}
#[no_mangle]
pub extern "C" fn msx_audio_len() -> u32 {
    APP.with(|a| a.borrow().machine.hardware.psg.samples.len() as u32)
}
#[no_mangle]
pub extern "C" fn msx_upload(length: u32) -> *mut u8 {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        // Reserve before JS obtains memory.buffer: allocation may grow linear memory.
        a.upload = Vec::new();
        let n = length as usize;
        if n == 0 || n > isize::MAX as usize || a.upload.try_reserve_exact(n).is_err() {
            a.error = "Cannot reserve ROM transfer memory. The image is empty or exceeds available browser/Wasm memory.".into();
            return std::ptr::null_mut();
        }
        a.upload.resize(n, 0);
        a.error.clear();
        a.upload.as_mut_ptr()
    })
}
#[no_mangle]
pub extern "C" fn msx_cancel_upload() {
    APP.with(|a| a.borrow_mut().upload = Vec::new());
}
#[no_mangle]
pub extern "C" fn msx_load(length: u32, system: u32, mapper: u32) -> u32 {
    load(length, system, mapper, None)
}
/// The same staged cartridge path, with an atomic declared-model transition.
#[no_mangle]
pub extern "C" fn msx_load_demo(length: u32, mapper: u32, model: u32) -> u32 {
    let model = match model {
        0 => Model::Msx1,
        1 => Model::Msx2,
        _ => {
            APP.with(|a| {
                let mut a = a.borrow_mut();
                a.upload.clear();
                a.error = "Unsupported demo machine.".into();
            });
            return 0;
        }
    };
    load(length, 0, mapper, Some(model))
}
fn load(length: u32, system: u32, mapper: u32, model: Option<Model>) -> u32 {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        let rom = std::mem::take(&mut a.upload);
        a.detection.clear();
        a.detected_mapper = 0;
        a.confidence = Confidence::Invalid as u32;
        let result = if length == 0 || length as usize != rom.len() {
            Err("Invalid ROM transfer length. Select the file again.".into())
        } else if system == 2 {
            if rom.len() != 49152 {
                Err("MSX2 needs a 32 KiB main BIOS and 16 KiB sub-ROM.".into())
            } else {
                a.machine.load_msx2_firmware(&rom[..32768], &rom[32768..])
            }
        } else if system != 0 {
            a.machine.load_bios(&rom)
        } else {
            Mapper::try_from(mapper).and_then(|requested| {
                let selected = if requested == Mapper::Auto {
                    let detection = detect_mapper(&rom);
                    a.confidence = detection.confidence as u32;
                    a.detection = detection.explanation;
                    detection.mapper.ok_or_else(|| a.detection.clone())?
                } else {
                    requested
                };
                let model = model.unwrap_or(a.machine.hardware.model);
                a.machine
                    .load_cartridge_for_model_owned(rom, selected, model)?;
                a.detected_mapper = selected as u32;
                Ok(())
            })
        };
        match result {
            Ok(()) => {
                a.error.clear();
                1
            }
            Err(e) => {
                a.error = e;
                0
            }
        }
    })
}
#[no_mangle]
pub extern "C" fn msx_error() -> *const u8 {
    APP.with(|a| a.borrow().error.as_ptr())
}
#[no_mangle]
pub extern "C" fn msx_error_len() -> u32 {
    APP.with(|a| a.borrow().error.len() as u32)
}
#[no_mangle]
pub extern "C" fn msx_transcript() -> *const u8 {
    APP.with(|a| a.borrow().machine.basic.console.transcript.as_ptr())
}
#[no_mangle]
pub extern "C" fn msx_transcript_len() -> u32 {
    APP.with(|a| a.borrow().machine.basic.console.transcript.len() as u32)
}

/// Metadata for the last cartridge import attempt (0 mapper means no selection).
#[no_mangle]
pub extern "C" fn msx_detected_mapper() -> u32 {
    APP.with(|a| a.borrow().detected_mapper)
}
#[no_mangle]
pub extern "C" fn msx_detection_confidence() -> u32 {
    APP.with(|a| a.borrow().confidence)
}
#[no_mangle]
pub extern "C" fn msx_detection() -> *const u8 {
    APP.with(|a| a.borrow().detection.as_ptr())
}
#[no_mangle]
pub extern "C" fn msx_detection_len() -> u32 {
    APP.with(|a| a.borrow().detection.len() as u32)
}

/// Views are valid until the next mutating Wasm call; reacquire after memory growth.
#[no_mangle]
pub extern "C" fn msx_sram() -> *const u8 {
    APP.with(|a| a.borrow().machine.cartridge_sram().as_ptr())
}
#[no_mangle]
pub extern "C" fn msx_sram_len() -> u32 {
    APP.with(|a| a.borrow().machine.cartridge_sram().len() as u32)
}
/// Consume a staged save; invalid transfers never change the running cartridge.
#[no_mangle]
pub extern "C" fn msx_restore_sram(length: u32) -> u32 {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        let bytes = std::mem::take(&mut a.upload);
        let result = if length as usize != bytes.len() {
            Err("Invalid SRAM transfer length.".into())
        } else {
            a.machine.restore_cartridge_sram(&bytes)
        };
        match result {
            Ok(()) => {
                a.error.clear();
                1
            }
            Err(error) => {
                a.error = error;
                0
            }
        }
    })
}

/// ROMless slot-1 accessory; validate IDs before changing the running session.
#[no_mangle]
pub extern "C" fn msx_install_sound_cartridge(profile: u32) -> u32 {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        let result = msx_core::SoundCartridgeProfile::try_from(profile)
            .and_then(|profile| a.machine.install_sound_cartridge(profile));
        disk_result(&mut a, result)
    })
}
#[no_mangle]
pub extern "C" fn msx_sound_cartridge_profile() -> u32 {
    APP.with(|a| {
        a.borrow()
            .machine
            .sound_cartridge_profile()
            .map_or(0, |p| p as u32)
    })
}
#[no_mangle]
pub extern "C" fn msx_remove_sound_cartridge() {
    APP.with(|a| a.borrow_mut().machine.remove_sound_cartridge());
}

/// Profile 1 is the explicit Philips NMS1200-style local disk ROM. Unknown
/// profiles and incomplete/canceled uploads are rejected before touching hardware.
#[no_mangle]
pub extern "C" fn msx_install_disk_rom(length: u32, profile: u32) -> u32 {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        let rom = std::mem::take(&mut a.upload);
        let result = if profile != 1 {
            Err("Select the Philips NMS1200-style disk interface profile.".into())
        } else if length as usize != rom.len() {
            Err("Invalid disk ROM transfer length.".into())
        } else {
            a.machine.install_philips_disk_rom(rom)
        };
        disk_result(&mut a, result)
    })
}
#[no_mangle]
pub extern "C" fn msx_remove_disk_rom() {
    APP.with(|a| a.borrow_mut().machine.remove_philips_disk_rom());
}
/// 1: independently installed Philips interface; 0: default Weave boot option.
#[no_mangle]
pub extern "C" fn msx_disk_profile() -> u32 {
    APP.with(|a| u32::from(a.borrow().machine.has_philips_disk_rom()))
}

/// Consume only an exactly sized staged disk; failures preserve the running session.
#[no_mangle]
pub extern "C" fn msx_mount_disk(length: u32, protected: u32) -> u32 {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        let bytes = std::mem::take(&mut a.upload);
        let result = if length as usize != bytes.len() {
            Err("Invalid DSK transfer length.".into())
        } else {
            a.machine.mount_disk(bytes, protected != 0)
        };
        disk_result(&mut a, result)
    })
}
#[no_mangle]
pub extern "C" fn msx_boot_disk() -> u32 {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        let result = a.machine.boot_disk();
        disk_result(&mut a, result)
    })
}
#[no_mangle]
pub extern "C" fn msx_eject_disk() -> u32 {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        let result = a.machine.eject_disk();
        disk_result(&mut a, result)
    })
}
fn disk_result(a: &mut App, result: Result<(), String>) -> u32 {
    match result {
        Ok(()) => {
            a.error.clear();
            1
        }
        Err(e) => {
            a.error = e;
            0
        }
    }
}
/// Borrowed views must be reacquired after a mutating call, as with SRAM.
#[no_mangle]
pub extern "C" fn msx_disk() -> *const u8 {
    APP.with(|a| {
        a.borrow()
            .machine
            .hardware
            .fdc
            .disk
            .as_ref()
            .map_or(std::ptr::null(), |d| d.bytes().as_ptr())
    })
}
#[no_mangle]
pub extern "C" fn msx_disk_len() -> u32 {
    APP.with(|a| {
        a.borrow()
            .machine
            .hardware
            .fdc
            .disk
            .as_ref()
            .map_or(0, |d| d.bytes().len() as u32)
    })
}
#[no_mangle]
pub extern "C" fn msx_disk_flags() -> u32 {
    APP.with(|a| {
        a.borrow()
            .machine
            .hardware
            .fdc
            .disk
            .as_ref()
            .map_or(0, |d| {
                1 | (u32::from(d.write_protected) << 1) | (u32::from(d.dirty) << 2)
            })
    })
}
#[no_mangle]
pub extern "C" fn msx_disk_protect(protected: u32) {
    APP.with(|a| {
        if let Some(d) = &mut a.borrow_mut().machine.hardware.fdc.disk {
            d.write_protected = protected != 0;
        }
    });
}
#[no_mangle]
pub extern "C" fn msx_disk_exported() {
    APP.with(|a| {
        if let Some(d) = &mut a.borrow_mut().machine.hardware.fdc.disk {
            d.dirty = false;
        }
    });
}

/// Firmware-written boot progress: 0 loading, 1 entered sector zero, 2 boot error.
#[no_mangle]
pub extern "C" fn msx_disk_boot_state() -> u32 {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        if a.machine.hardware.firmware == msx_core::Firmware::Disk {
            u32::from(a.machine.hardware.read(0xf104))
        } else {
            0
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joystick_abi_preserves_legacy_masks_indices_and_release_reset() {
        for model in [0, 1] {
            msx_model(model);
            msx_joystick(0xffff_ffee);
            assert_eq!(msx_joystick_indexed(1, 0xffff_ffd7), 1);
            for connector in [2, 3, u32::MAX] {
                assert_eq!(msx_joystick_indexed(connector, 0), 0);
            }
            let check = |one, two| {
                APP.with(|a| {
                    let mut a = a.borrow_mut();
                    let h = &mut a.machine.hardware;
                    for (control, expected) in [(0, one), (0x40, two)] {
                        h.output(0xa0, 15);
                        h.output(0xa1, control);
                        h.output(0xa0, 14);
                        assert_eq!(h.input(0xa2), expected);
                    }
                })
            };
            check(0x6e, 0x57);
            msx_joystick(0x3f);
            check(0x7f, 0x57); // legacy entry point cannot change connector two
            msx_key(3, 6, 1);
            msx_release_keys();
            check(0x7f, 0x7f);
            APP.with(|a| assert_eq!(a.borrow().machine.hardware.keyboard, [255; 11]));
            msx_joystick(0);
            msx_joystick_indexed(1, 0);
            msx_key(4, 0, 1);
            msx_reset();
            check(0x7f, 0x7f);
            APP.with(|a| assert_eq!(a.borrow().machine.hardware.keyboard, [255; 11]));
        }
    }

    #[test]
    fn scci_explicit_ids_replacement_reset_and_invalid_requests_are_atomic() {
        for model in [0, 1] {
            msx_model(model);
            assert_eq!(msx_cbios(), 1);
            for profile in [1, 2] {
                assert_eq!(msx_install_sound_cartridge(profile), 1);
                assert_eq!(msx_sound_cartridge_profile(), profile);
                APP.with(|a| {
                    let mut a = a.borrow_mut();
                    let h = &mut a.machine.hardware;
                    h.secondary[3] = 255;
                    h.slots = 0xd4;
                    h.write(0x5000, if profile == 1 { 0 } else { 8 });
                    h.write(0xbffe, 1);
                    h.write(0x4123, 0x7b);
                    a.machine.cpu.pc = 0x1234;
                });
                for id in [0, 3, u32::MAX] {
                    assert_eq!(msx_install_sound_cartridge(id), 0);
                }
                msx_upload(16);
                msx_cancel_upload();
                assert_eq!(msx_load(16, 0, 1), 0);
                APP.with(|a| {
                    let mut a = a.borrow_mut();
                    assert_eq!(a.machine.cpu.pc, 0x1234);
                    assert_eq!(a.machine.hardware.read(0x4123), 0x7b);
                });
                msx_reset();
                assert_eq!(msx_sound_cartridge_profile(), profile);
                assert_eq!(msx_firmware(), 1);
                assert_eq!(msx_boot_policy(), 1);
                msx_remove_sound_cartridge();
                assert_eq!(msx_sound_cartridge_profile(), 0);
                assert_eq!(msx_firmware(), 1);
            }
        }
    }

    #[test]
    fn disk_rom_staging_profile_validation_and_cancellation_are_atomic() {
        msx_model(1);
        assert_eq!(msx_cbios(), 1);
        APP.with(|a| {
            let mut a = a.borrow_mut();
            a.machine.mount_disk(vec![0x39; 368640], false).unwrap();
            a.machine.cpu.pc = 0x1234;
        });
        for (length, profile) in [(16383, 1), (16384, 0), (16384, 2)] {
            msx_upload(16384);
            APP.with(|a| a.borrow_mut().upload[..2].copy_from_slice(b"AB"));
            assert_eq!(msx_install_disk_rom(length, profile), 0);
            assert_eq!(msx_disk_profile(), 0);
        }
        msx_upload(16384);
        msx_cancel_upload();
        assert_eq!(msx_install_disk_rom(16384, 1), 0);
        msx_upload(16384); // correct size but missing AB header
        assert_eq!(msx_install_disk_rom(16384, 1), 0);
        APP.with(|a| {
            let a = a.borrow();
            assert_eq!(a.machine.cpu.pc, 0x1234);
            assert_eq!(a.machine.hardware.firmware, msx_core::Firmware::Cbios);
        });
        msx_upload(16384);
        APP.with(|a| a.borrow_mut().upload[..2].copy_from_slice(b"AB"));
        assert_eq!(msx_install_disk_rom(16384, 1), 1);
        assert_eq!(msx_disk_profile(), 1);
        assert_eq!(msx_boot_disk(), 1);
        assert_eq!(msx_firmware(), 1);
        msx_reset();
        assert_eq!(msx_disk_profile(), 1);
        msx_remove_disk_rom();
        assert_eq!(msx_disk_profile(), 0);
        assert_eq!(msx_firmware(), 1);
        assert_eq!(msx_disk_len(), 368640);
        APP.with(|a| {
            assert!(a
                .borrow()
                .machine
                .hardware
                .fdc
                .disk
                .as_ref()
                .unwrap()
                .bytes()
                .iter()
                .all(|b| *b == 0x39))
        });
    }

    #[test]
    fn disk_boot_status_follows_logical_ram_mapping() {
        msx_model(1);
        APP.with(|a| {
            let mut a = a.borrow_mut();
            a.machine
                .mount_disk(msx_core::firmware::demo_disk(), true)
                .unwrap();
            a.machine.boot_disk().unwrap();
            let h = &mut a.machine.hardware;
            h.output(0xa8, 0xf8);
            h.write(0xffff, 0xff);
            h.output(0xff, 9);
            h.write(0xf104, 2);
            h.output(0xff, 0);
            h.write(0xf104, 1);
        });
        assert_eq!(msx_disk_boot_state(), 1);
        APP.with(|a| a.borrow_mut().machine.hardware.output(0xff, 9));
        assert_eq!(msx_disk_boot_state(), 2);
    }
}

/// Transactional, bounded CAS staging; allocation failures leave the tape intact.
#[no_mangle]
pub extern "C" fn msx_tape_upload(length: u32) -> *mut u8 {
    if length as usize > msx_core::cassette::MAX_BYTES {
        APP.with(|a| {
            let mut a = a.borrow_mut();
            a.upload = Vec::new();
            a.error = "CAS exceeds the 16 MiB image limit.".into();
        });
        return std::ptr::null_mut();
    }
    msx_upload(length)
}
#[no_mangle]
pub extern "C" fn msx_mount_tape(length: u32, rate: u32) -> u32 {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        let bytes = std::mem::take(&mut a.upload);
        let result = if length as usize != bytes.len() {
            Err("Invalid CAS transfer length. Select the file again.".into())
        } else {
            a.machine.hardware.cassette.mount(bytes, rate)
        };
        disk_result(&mut a, result)
    })
}
#[no_mangle]
pub extern "C" fn msx_rewind_tape() {
    APP.with(|a| a.borrow_mut().machine.hardware.cassette.rewind());
}
#[no_mangle]
pub extern "C" fn msx_eject_tape() {
    APP.with(|a| a.borrow_mut().machine.hardware.cassette.eject());
}
/// Bit 0: mounted; bit 1: PPI motor latch on; bit 2: end of tape.
#[no_mangle]
pub extern "C" fn msx_tape_flags() -> u32 {
    APP.with(|a| {
        let a = a.borrow();
        let h = &a.machine.hardware;
        u32::from(h.cassette.mounted())
            | (u32::from(h.ppi_c & 0x10 == 0) << 1)
            | (u32::from(h.cassette.ended()) << 2)
    })
}
#[no_mangle]
pub extern "C" fn msx_tape_rate() -> u32 {
    APP.with(|a| a.borrow().machine.hardware.cassette.rate())
}
#[no_mangle]
pub extern "C" fn msx_tape_position() -> f64 {
    APP.with(|a| a.borrow().machine.hardware.cassette.position_cycles() as f64 / 3_579_545.0)
}

#[cfg(test)]
mod cassette_tests {
    use super::*;
    #[test]
    fn bounded_staging_cancel_bad_rate_and_failed_imports_preserve_session() {
        for model in [0, 1] {
            msx_model(model);
            let bytes = msx_core::firmware::cassette_test_image();
            let n = bytes.len() as u32;
            let stage = || {
                let p = msx_tape_upload(n);
                assert!(!p.is_null());
                unsafe {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
                }
            };
            stage();
            assert_eq!(msx_mount_tape(n, 2400), 1);
            APP.with(|a| {
                let mut a = a.borrow_mut();
                a.machine.cpu.pc = 0x1234;
                a.machine.hardware.cassette.tick(8_123_456, true);
            });
            let position = msx_tape_position();
            for rate in [0, 1, 1199, 2401, u32::MAX] {
                stage();
                assert_eq!(msx_mount_tape(n, rate), 0);
                assert_eq!(msx_tape_position(), position);
                assert_eq!(msx_tape_rate(), 2400);
            }
            stage();
            assert_eq!(msx_mount_tape(n - 1, 1200), 0);
            stage();
            msx_cancel_upload();
            assert_eq!(msx_mount_tape(n, 1200), 0);
            assert!(msx_tape_upload(0).is_null());
            assert!(msx_tape_upload(16 * 1024 * 1024 + 1).is_null());
            assert!(msx_tape_upload(u32::MAX).is_null());
            assert_eq!(msx_mount_tape(0, 1200), 0);
            assert_eq!(msx_tape_position(), position);
            APP.with(|a| assert_eq!(a.borrow().machine.cpu.pc, 0x1234));
            msx_reset();
            assert_eq!(msx_tape_position(), position);
            assert_eq!(msx_tape_flags(), 3);
            msx_rewind_tape();
            assert_eq!(msx_tape_position(), 0.0);
            msx_eject_tape();
            assert_eq!(msx_tape_flags(), 2);
        }
    }
}
