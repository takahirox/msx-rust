use std::{env, fs};
fn main() {
    let dir = env::args().nth(1).expect("output directory");
    fs::create_dir_all(&dir).unwrap();
    for mapper in [
        msx_core::Mapper::Ascii8Sram2,
        msx_core::Mapper::Ascii8Sram8,
        msx_core::Mapper::Ascii16Sram2,
        msx_core::Mapper::Ascii16Sram8,
        msx_core::Mapper::RType,
    ] {
        fs::write(
            format!("{dir}/mapper-{}.rom", mapper as u32),
            msx_core::firmware::mapper_test_cartridge(mapper),
        )
        .unwrap();
    }
    fs::write(
        format!("{dir}/scc-test-cartridge.rom"),
        msx_core::firmware::scc_test_cartridge(),
    )
    .unwrap();
    fs::write(
        format!("{dir}/cbios-msx1-test-cartridge.rom"),
        msx_core::firmware::cbios_msx1_test_cartridge(),
    )
    .unwrap();
    fs::write(
        format!("{dir}/cbios-test-cartridge.rom"),
        msx_core::firmware::cbios_test_cartridge(),
    )
    .unwrap();
    fs::write(
        format!("{dir}/test-cartridge.rom"),
        msx_core::firmware::test_cartridge(),
    )
    .unwrap();
    fs::write(
        format!("{dir}/msx2-test-cartridge.rom"),
        msx_core::firmware::msx2_cartridge(),
    )
    .unwrap();
    fs::write(
        format!("{dir}/msx2-diagnostic-sub.rom"),
        msx_core::firmware::msx2_subrom(),
    )
    .unwrap();
    fs::write(
        format!("{dir}/weave-msx2-boot.rom"),
        msx_core::firmware::msx2_bios(),
    )
    .unwrap();
    fs::write(format!("{dir}/weave-boot.rom"), msx_core::firmware::bios()).unwrap();
}
