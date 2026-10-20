import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const expected = ['scc-test-cartridge.rom','cbios_main_msx1.rom','cbios-msx1-test-cartridge.rom','cbios_main_msx2.rom','cbios_sub.rom','CBIOS-LICENSE.txt','CBIOS-PROVENANCE.txt','cbios-test-cartridge.rom','msx2-test-cartridge.rom','msx2-diagnostic-sub.rom','weave-msx2-boot.rom','LICENSE','ROM-LICENSE.txt','app.js','index.html','msx_wasm.wasm','style.css','test-cartridge.rom','weave-boot.rom'];
assert.deepEqual((await readdir('dist')).sort(), [...expected, 'demos'].sort(), 'Unexpected deployment files; review their provenance.');
for (const file of expected) assert((await readFile(`dist/${file}`)).length > 0, `Empty ${file}`);
for (const [source, file] of [['LICENSE', 'LICENSE'], ['firmware/ROM-LICENSE.txt', 'ROM-LICENSE.txt']]) {
  assert.deepEqual(await readFile(`dist/${file}`), await readFile(source), `Modified project notice: ${file}`);
}
const module = await WebAssembly.compile(await readFile('dist/msx_wasm.wasm'));
assert.deepEqual(WebAssembly.Module.imports(module), []);
for (const name of ['memory', 'msx_frame', 'msx_upload', 'msx_cancel_upload', 'msx_load', 'msx_load_demo', 'msx_error', 'msx_error_len', 'msx_detected_mapper', 'msx_detection_confidence', 'msx_detection', 'msx_detection_len', 'msx_model', 'msx_width', 'msx_height', 'msx_frame_cycles', 'msx_cbios', 'msx_diagnostic', 'msx_firmware', 'msx_boot_policy']) {
  assert(WebAssembly.Module.exports(module).some(e => e.name === name), `Missing Wasm export ${name}`);
}
const scc = await readFile('dist/scc-test-cartridge.rom');
assert.equal(scc.length, 65536);
assert.equal(scc.subarray(0,2).toString(), 'AB');
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
for (const file of ['scc-test-cartridge.rom','app.js','style.css','test-cartridge.rom','msx2-test-cartridge.rom','cbios-test-cartridge.rom','cbios-msx1-test-cartridge.rom','CBIOS-LICENSE.txt','CBIOS-PROVENANCE.txt']) assert(html.includes(file));
assert(!html.includes('src="/') && !html.includes('href="/'), 'Assets must work under a Pages project subpath.');
const hash = data => createHash('sha256').update(data).digest('hex');
const manifestBytes = await readFile('demos/manifest.json');
assert.deepEqual(await readFile('dist/demos/manifest.json'), manifestBytes);
const manifest = JSON.parse(manifestBytes);
assert.equal(manifest.version, 1);
assert.equal(new Set(manifest.demos.map(d => d.id)).size, manifest.demos.length);
assert(manifest.demos.filter(d => d.model === 'MSX1' && d.kind === 'game').length >= 2);
assert(manifest.demos.some(d => d.model === 'MSX2' && d.kind === 'diagnostic'));
const demoFiles = new Set(['manifest.json']);
for (const demo of manifest.demos) {
  for (const key of ['title', 'creator', 'description', 'controls', 'license', 'licenseFile', 'source', 'download', 'version', 'changes']) assert(demo[key]?.length, `Missing ${key}: ${demo.id}`);
  assert(['MSX1', 'MSX2'].includes(demo.model));
  assert(['Plain', 'ASCII8', 'ASCII16', 'Konami', 'Konami SCC'].includes(demo.mapper));
  assert(/^(demos\/)?[a-zA-Z0-9-]+\.rom$/.test(demo.file));
  assert(/^[a-f0-9]{64}$/.test(demo.sha256));
  const rom = await readFile(`dist/${demo.file}`);
  assert.equal(rom.length, demo.size, `Demo size: ${demo.id}`);
  assert.equal(hash(rom), demo.sha256, `Demo SHA-256: ${demo.id}`);
  assert.equal(rom.subarray(0, 2).toString(), 'AB');
  if (demo.kind === 'game') {
    assert.equal(demo.license, 'MIT'); // Changing the curated licenses requires a compliance review here.
    assert(/^[a-f0-9]{40}$/.test(demo.version));
    assert(demo.source.includes(`/tree/${demo.version}`));
    assert(demo.download.startsWith('https://raw.githubusercontent.com/'));
    assert(demo.download.includes(`/${demo.version}/MSX1/BIN/`));
    assert.deepEqual(rom, await readFile(demo.file));
    demoFiles.add(demo.file.slice('demos/'.length));
    assert(demo.notices.includes('demos/Z88DK-LICENSE.txt'));
    const license = await readFile(`dist/${demo.licenseFile}`, 'utf8');
    assert(license.includes('Copyright') && license.includes('m@3') && license.includes('Permission is hereby granted'));
  }
  assert(demo.notices.includes(demo.licenseFile) && demo.notices.includes('demos/PROVENANCE.txt'));
  for (const file of demo.notices) {
    assert(/^(demos\/)?[a-zA-Z0-9.-]+$/.test(file));
    assert((await readFile(`dist/${file}`)).length > 0);
    if (file.startsWith('demos/')) assert(manifest.noticeHashes[file], `Unpinned notice: ${file}`);
  }
}
for (const [file, digest] of Object.entries(manifest.noticeHashes)) {
  assert(/^demos\/[A-Z0-9-]+\.txt$/.test(file));
  assert(/^[a-f0-9]{64}$/.test(digest));
  assert.equal(hash(await readFile(file)), digest, `Modified license/provenance: ${file}`);
  assert.deepEqual(await readFile(`dist/${file}`), await readFile(file));
  demoFiles.add(file.slice('demos/'.length));
}
for (const directory of ['demos', 'dist/demos']) assert.deepEqual((await readdir(directory)).sort(), [...demoFiles].sort(), `Unaccounted demo assets in ${directory}`);
const provenance = await readFile('dist/demos/PROVENANCE.txt', 'utf8');
for (const demo of manifest.demos.filter(d => d.kind === 'game')) {
  assert(provenance.includes(demo.source) && provenance.includes(demo.creator.split(' (')[0]));
}
assert(html.includes('demos/PROVENANCE.txt'));
console.log('Deployment inventory, pinned demo ROM/notice hashes, provenance, subpath links and import-free Wasm verified.');
