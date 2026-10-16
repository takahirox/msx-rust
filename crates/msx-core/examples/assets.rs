use std::{env, fs};
fn main() {
    let dir = env::args().nth(1).expect("output directory");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        format!("{dir}/test-cartridge.rom"),
        msx_core::firmware::test_cartridge(),
    )
    .unwrap();
    fs::write(format!("{dir}/weave-boot.rom"), msx_core::firmware::bios()).unwrap();
}
