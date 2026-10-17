import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
const expected = ['msx2-test-cartridge.rom','msx2-diagnostic-sub.rom','weave-msx2-boot.rom','LICENSE','ROM-LICENSE.txt','app.js','index.html','msx_wasm.wasm','style.css','test-cartridge.rom','weave-boot.rom'];
assert.deepEqual((await readdir('dist')).sort(), expected.sort(), 'Unexpected deployment files; review their provenance.');
for (const file of expected) assert((await readFile(`dist/${file}`)).length > 0, `Empty ${file}`);
const module = await WebAssembly.compile(await readFile('dist/msx_wasm.wasm'));
assert.deepEqual(WebAssembly.Module.imports(module), []);
for (const name of ['memory', 'msx_frame', 'msx_upload', 'msx_cancel_upload', 'msx_load', 'msx_error', 'msx_error_len', 'msx_detected_mapper', 'msx_detection_confidence', 'msx_detection', 'msx_detection_len', 'msx_model', 'msx_width', 'msx_height', 'msx_frame_cycles']) {
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
const html = await readFile('dist/index.html','utf8');
for (const file of ['app.js','style.css','test-cartridge.rom','msx2-test-cartridge.rom']) assert(html.includes(file));
assert(!html.includes('src="/') && !html.includes('href="/'), 'Assets must work under a Pages project subpath.');
console.log('Deployment inventory, ROM headers/sizes, subpath links and import-free Wasm verified.');
