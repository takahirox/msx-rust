import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
const expected = ['LICENSE','ROM-LICENSE.txt','app.js','index.html','msx_wasm.wasm','style.css','test-cartridge.rom','weave-boot.rom'];
assert.deepEqual((await readdir('dist')).sort(), expected.sort(), 'Unexpected deployment files; review their provenance.');
for (const file of expected) assert((await readFile(`dist/${file}`)).length > 0, `Empty ${file}`);
const module = await WebAssembly.compile(await readFile('dist/msx_wasm.wasm'));
assert.deepEqual(WebAssembly.Module.imports(module), []);
for (const name of ['memory', 'msx_frame', 'msx_upload', 'msx_cancel_upload', 'msx_load', 'msx_error', 'msx_error_len']) {
  assert(WebAssembly.Module.exports(module).some(e => e.name === name), `Missing Wasm export ${name}`);
}
const cartridge = await readFile('dist/test-cartridge.rom');
assert.equal(cartridge.length, 16384);
assert.equal(cartridge.subarray(0,2).toString(), 'AB');
assert.equal((await readFile('dist/weave-boot.rom')).length, 32768);
const html = await readFile('dist/index.html','utf8');
for (const file of ['app.js','style.css','test-cartridge.rom']) assert(html.includes(file));
assert(!html.includes('src="/') && !html.includes('href="/'), 'Assets must work under a Pages project subpath.');
console.log('Deployment inventory, ROM headers/sizes, subpath links and import-free Wasm verified.');
