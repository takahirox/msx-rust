import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const expected = [
  'index.html', 'style.css', 'app.js', 'display.js', 'saves.js', 'msx_wasm.wasm',
  'weave-boot.rom', 'weave-msx2-boot.rom', 'msx2-diagnostic-sub.rom', 'weave-disk.rom',
  'cbios_main_msx1.rom', 'cbios_main_msx2.rom', 'cbios_sub.rom',
  'LICENSE', 'ROM-LICENSE.txt', 'CBIOS-LICENSE.txt', 'CBIOS-PROVENANCE.txt', 'emu2413-LICENSE.txt',
];
assert.deepEqual((await readdir('dist')).sort(), expected.sort(), 'Unexpected deployment files; review their provenance.');
for (const file of expected) assert((await readFile(`dist/${file}`)).length > 0, `Empty ${file}`);
for (const [source, file] of [['LICENSE', 'LICENSE'], ['firmware/ROM-LICENSE.txt', 'ROM-LICENSE.txt'], ['third-party/emu2413-LICENSE.txt', 'emu2413-LICENSE.txt']]) {
  assert.deepEqual(await readFile(`dist/${file}`), await readFile(source), `Modified project notice: ${file}`);
}
const module = await WebAssembly.compile(await readFile('dist/msx_wasm.wasm'));
assert.deepEqual(WebAssembly.Module.imports(module), []);
for (const name of ['msx_install_fm_pac','msx_fm_pac_profile','msx_remove_fm_pac','msx_fm_pac_sram','msx_fm_pac_sram_len','msx_restore_fm_pac_sram','msx_install_music_rom','msx_music_profile','msx_remove_music_rom','msx_tape_upload', 'msx_mount_tape', 'msx_rewind_tape', 'msx_eject_tape', 'msx_tape_flags', 'msx_tape_rate', 'msx_tape_position', 'msx_joystick', 'msx_joystick_indexed', 'msx_release_keys', 'memory', 'msx_frame', 'msx_upload', 'msx_cancel_upload', 'msx_load', 'msx_error', 'msx_error_len', 'msx_detected_mapper', 'msx_detection_confidence', 'msx_detection', 'msx_detection_len', 'msx_model', 'msx_width', 'msx_height', 'msx_frame_cycles', 'msx_cbios', 'msx_diagnostic', 'msx_firmware', 'msx_boot_policy', 'msx_sram', 'msx_sram_len', 'msx_restore_sram', 'msx_install_sound_cartridge', 'msx_sound_cartridge_profile', 'msx_remove_sound_cartridge', 'msx_disk_profile', 'msx_install_disk_rom', 'msx_remove_disk_rom', 'msx_mount_disk', 'msx_boot_disk', 'msx_eject_disk', 'msx_disk', 'msx_disk_len', 'msx_disk_flags', 'msx_disk_protect', 'msx_disk_exported', 'msx_disk_boot_state']) {
  assert(WebAssembly.Module.exports(module).some(e => e.name === name), `Missing Wasm export ${name}`);
}
for (const file of ['index.html', 'style.css', 'app.js', 'saves.js', 'display.js']) {
  assert.deepEqual(await readFile(`dist/${file}`), await readFile(`web/${file}`));
}
assert.equal((await readFile('dist/weave-boot.rom')).length, 32768);
const main = await readFile('dist/weave-msx2-boot.rom');
assert.equal(main.length, 32768); assert.equal(main[0x2d], 1);
const sub = await readFile('dist/msx2-diagnostic-sub.rom');
assert.equal(sub.length, 16384); assert.equal(sub.subarray(0,2).toString(), 'CD');
for (const [file, size, hash] of [
  ['cbios_main_msx1.rom', 32768, '921d35edf143f1fde8e53570f92f85e05854610d6a5ea76cce881f2f9040cd9c'],
  ['cbios_main_msx2.rom', 32768, '8767fca1a1ca5928496bb6185a0e951c66d68f65361305134a25f7912085766b'],
  ['cbios_sub.rom', 16384, '95db258195d1dea673b3826a8ef3d4b747f87f93587ae66e137acd2e39c3c0f1'],
]) {
  const data = await readFile(`dist/${file}`);
  assert.equal(data.length, size);
  assert.equal(createHash('sha256').update(data).digest('hex'), hash, `Modified C-BIOS binary ${file}`);
  assert.deepEqual(data, await readFile(`firmware/cbios/${file}`));
}
for (const file of ['CBIOS-LICENSE.txt', 'CBIOS-PROVENANCE.txt']) {
  assert.deepEqual(await readFile(`dist/${file}`), await readFile(`firmware/cbios/${file}`), `Missing/modified C-BIOS notices: ${file}`);
}
const diskRom = await readFile('dist/weave-disk.rom');
assert.equal(diskRom.length, 16384);
assert.equal(diskRom.subarray(0, 2).toString(), 'AB');

const html = await readFile('dist/index.html', 'utf8');
const localLinks = [...html.matchAll(/(?:href|src)="([^"]+)"/g)]
  .map(([, path]) => path).filter(path => !path.startsWith('https://'));
assert.deepEqual([...new Set(localLinks)].sort(), [
  'CBIOS-LICENSE.txt', 'CBIOS-PROVENANCE.txt', 'LICENSE', 'ROM-LICENSE.txt',
  'app.js', 'emu2413-LICENSE.txt', 'style.css',
]);
console.log('Verified deployment inventory, firmware, license and provenance notices, subpath links and import-free Wasm.');
