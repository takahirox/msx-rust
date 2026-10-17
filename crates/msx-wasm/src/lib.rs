//! Minimal raw Wasm ABI. All emulation and BASIC live in the portable core.
use msx_core::{Machine, Mapper, Model};
use std::cell::RefCell;
struct App {
    machine: Machine,
    upload: Vec<u8>,
    error: String,
}
thread_local! {static APP:RefCell<App>=RefCell::new(App{machine:Machine::default(),upload:Vec::new(),error:String::new()});}
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
    APP.with(|a| {
        let mut a = a.borrow_mut();
        let rom = std::mem::take(&mut a.upload);
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
            Mapper::try_from(mapper).and_then(|mapper| a.machine.load_cartridge_owned(rom, mapper))
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
