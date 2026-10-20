//! Force real allocation errors without changing production allocation behavior.
use msx_core::{firmware, BootPolicy, Firmware, Machine, Mapper, Model};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! { static DENY: Cell<bool> = const { Cell::new(false) }; }
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if DENY.try_with(Cell::get).unwrap_or(false)
            || FAIL_SIZE
                .try_with(|s| s.get() == layout.size())
                .unwrap_or(false)
        {
            std::ptr::null_mut()
        } else {
            System.alloc(layout)
        }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        System.dealloc(p, layout);
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
#[test]
fn firmware_commits_reuse_memory_and_cartridge_reservation_failure_is_atomic() {
    for model in [Model::Msx1, Model::Msx2] {
        let mut m = Machine::default();
        m.set_model(model);
        m.frame();
        m.basic.submit("10 PRINT 42").unwrap();
        let rom = firmware::test_cartridge();
        // Only fail the ROM-sized reservation; error reporting can still allocate.
        FAIL_SIZE.with(|s| s.set(rom.len()));
        let result = m.load_cartridge_with_mapper(&rom, Mapper::Plain);
        FAIL_SIZE.with(|s| s.set(0));
        assert!(result.is_err());
        assert_eq!(m.hardware.firmware, Firmware::WeaveBasic);
        assert!(m.basic.program.contains_key(&10));
        let main = m.hardware.bios.as_ptr();
        let sub = m.hardware.subrom.as_ptr();
        DENY.with(|s| s.set(true));
        m.load_cartridge_owned(rom, Mapper::Plain).unwrap();
        m.reset();
        m.unload_cartridge();
        m.use_cbios().unwrap();
        if model == Model::Msx2 {
            m.use_diagnostic().unwrap();
            m.load_msx2_firmware(firmware::CBIOS_MSX2_MAIN, firmware::CBIOS_MSX2_SUB)
                .unwrap();
        } else {
            m.load_bios(firmware::CBIOS_MSX1_MAIN).unwrap();
        }
        m.use_builtin();
        DENY.with(|s| s.set(false));
        assert_eq!(m.hardware.bios.as_ptr(), main);
        assert_eq!(m.hardware.subrom.as_ptr(), sub);
        assert_eq!(m.boot_policy, BootPolicy::Automatic);
        m.frame();
        assert!(m.basic.console.transcript.contains("READY"));
    }
}
thread_local! { static FAIL_SIZE: Cell<usize> = const { Cell::new(0) }; }
