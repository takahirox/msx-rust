//! Minimal raw Wasm ABI. All emulation and BASIC live in the portable core.
use msx_core::Machine;
use std::cell::RefCell;
struct App {
    machine: Machine,
    upload: Vec<u8>,
    error: String,
}
thread_local! {static APP:RefCell<App>=RefCell::new(App{machine:Machine::default(),upload:vec![0;49152],error:String::new()});}
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
pub extern "C" fn msx_upload() -> *mut u8 {
    APP.with(|a| a.borrow_mut().upload.as_mut_ptr())
}
#[no_mangle]
pub extern "C" fn msx_load(length: u32, system: u32) -> u32 {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        let n = length as usize;
        if n > a.upload.len() {
            a.error = "ROM is too large.".into();
            return 0;
        }
        let rom = a.upload[..n].to_vec();
        let result = if system != 0 {
            a.machine.load_bios(&rom)
        } else {
            a.machine.load_cartridge(&rom)
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
