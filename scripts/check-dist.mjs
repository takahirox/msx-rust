import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const expected = ['cbios_main_msx1.rom','cbios-msx1-test-cartridge.rom','cbios_main_msx2.rom','cbios_sub.rom','CBIOS-LICENSE.txt','CBIOS-PROVENANCE.txt','cbios-test-cartridge.rom','msx2-test-cartridge.rom','msx2-diagnostic-sub.rom','weave-msx2-boot.rom','LICENSE','ROM-LICENSE.txt','app.js','index.html','msx_wasm.wasm','style.css','test-cartridge.rom','weave-boot.rom'];
assert.deepEqual((await readdir('dist')).sort(), expected.sort(), 'Unexpected deployment files; review their provenance.');
for (const file of expected) assert((await readFile(`dist/${file}`)).length > 0, `Empty ${file}`);
const module = await WebAssembly.compile(await readFile('dist/msx_wasm.wasm'));
assert.deepEqual(WebAssembly.Module.imports(module), []);
for (const name of ['memory', 'msx_frame', 'msx_upload', 'msx_cancel_upload', 'msx_load', 'msx_error', 'msx_error_len', 'msx_detected_mapper', 'msx_detection_confidence', 'msx_detection', 'msx_detection_len', 'msx_model', 'msx_width', 'msx_height', 'msx_frame_cycles', 'msx_cbios', 'msx_diagnostic', 'msx_firmware', 'msx_boot_policy']) {
  assert(WebAssembly.Module.exports(module).some(e => e.name === name), `Missing Wasm export ${name}`);
}
const cartridge = await readFile('dist/test-cartridge.rom');
assert.equal(cartridge.length, 16384);
assert.equal(cartridge.subarray(0,2).toString(), 'AB');
assert.equal((await readFile('dist/weave-boot.rom')).length, 32768);
const msx2 = await readFile('dist/msx2-test-cartridge.rom');
assert.equal(msx2.length, 16384); assert.equal(msx2.subarray(0,2).toString(), 'AB');
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
const biosTest = await readFile('dist/cbios-test-cartridge.rom');
assert.equal(biosTest.length, 16384); assert.equal(biosTest.subarray(0,2).toString(), 'AB');
const html = await readFile('dist/index.html','utf8');
for (const file of ['app.js','style.css','test-cartridge.rom','msx2-test-cartridge.rom','cbios-test-cartridge.rom','cbios-msx1-test-cartridge.rom','CBIOS-LICENSE.txt','CBIOS-PROVENANCE.txt']) assert(html.includes(file));
assert(!html.includes('src="/') && !html.includes('href="/'), 'Assets must work under a Pages project subpath.');
console.log('Deployment inventory, ROM headers/sizes, subpath links and import-free Wasm verified.');
