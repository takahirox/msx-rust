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
    APP.with(|a| a.borrow_mut().machine.hardware.joystick = value as u8 & 0x3f);
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
