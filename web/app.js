// Browser transport only: Canvas, files, keyboard, pacing, and Web Audio.
import { savePrefix, romIdentity, encodeSave, decodeSave } from './saves.js';
const $ = id => document.getElementById(id);
const screen = $('screen');
const context = screen.getContext('2d', { alpha: false });
const decoder = new TextDecoder();
let wasm;
let ready = false;
let loading = false;
let audioEnabled = false;
let paused = false;
let audioContext;
let nextAudio = 0;
let tone = false;
let system = 'Weave BASIC';
// Use the HTML default even if the browser restores an older form selection.
let model = Number($('model').querySelector('option[selected]').value);
let firmware = 'auto';
let localName = '';
function cbiosName() { return `${model ? 'MSX2' : 'MSX1'} C-BIOS 0.29a · cartridge BIOS (no BASIC)`; }
const diagnosticName = 'Original cartridge loader + diagnostic sub-ROM';
let mainFile;
let subFile;
let cartridge = '';
let diskName = '';
let activeSave;
let importGeneration = 0;
let demos;
let catalogLoading = false;
let previousTime;
let accumulator = 0;
function frameMilliseconds() { return 1000 * wasm.msx_frame_cycles() / 3579545; }
const held = new Set();
let pendingText = [];
const activeSources = new Set();
function status(message, kind = 'info') {
  $('status').textContent = message;
  $('status').dataset.kind = kind;
}
function updateControls() {
  const native = ready && wasm.msx_firmware() === 0;
  for (const id of ['pause', 'reset', 'audio', 'builtin', 'open-rom', 'cartridge', 'bios', 'mapper', 'focus-screen', 'model', 'subrom', 'apply-firmware', 'firmware']) $(id).disabled = !ready || loading;
  for (const id of ['example', 'graphics', 'tone']) $(id).disabled = !ready || loading || !native;
  $('demo').disabled = !ready || loading || !demos;
  $('play-demo').disabled = !ready || loading || catalogLoading;
  $('eject').disabled = !ready || !cartridge;
  for (const id of ['export-save', 'import-save']) $(id).disabled = !ready || loading || !activeSave;
  updateDisk();
  $('emulator').setAttribute('aria-busy', String(!ready || loading));
  $('pause').textContent = paused ? 'Resume' : 'Pause';
  $('tone').textContent = tone ? 'Stop tone' : 'Sound demo';
  $('tone').setAttribute('aria-pressed', String(tone));
  $('starter-help').textContent = native ? 'One click, then make it your own.' : 'Choose Use Weave BASIC in ROMs & advanced system options to try these on either machine.';
}
function updateAudioState() {
  const running = audioEnabled && audioContext?.state === 'running';
  $('audio').textContent = running ? 'Mute audio' : audioEnabled ? 'Resume audio' : 'Enable audio';
  $('audio').setAttribute('aria-pressed', String(running));
  $('audio-state').textContent = running ? 'Audio on' : audioEnabled ? 'Audio suspended · resume to hear sound' : 'Audio off · enable to hear sound';
}
function resume() {
  paused = false;
  previousTime = undefined;
  accumulator = 0;
}
function updateFocus() {
  const focused = document.activeElement === screen;
  $('focus-screen').textContent = focused ? 'Keyboard active' : 'Click / tap to type';
}
$('focus-screen').addEventListener('click', () => screen.focus());
screen.addEventListener('focus', updateFocus);
screen.addEventListener('blur', updateFocus);
screen.addEventListener('click', () => screen.focus());

// Keep the toolbar available in both native fullscreen and the CSS fallback.
function expanded() { return document.fullscreenElement === $('emulator') || $('emulator').classList.contains('focus-view'); }
function updateView() {
  const active = expanded();
  $('fullscreen').textContent = active ? 'Exit fullscreen' : 'Fullscreen';
  $('fullscreen').setAttribute('aria-pressed', String(active));
}
function fallbackView(active) {
  $('emulator').classList.toggle('focus-view', active);
  document.body.classList.toggle('focus-view-active', active);
  for (const element of document.querySelector('main').children) if (element !== $('emulator')) element.inert = active;
  updateView();
}
$('fullscreen').addEventListener('click', async () => {
  if (document.fullscreenElement === $('emulator')) await document.exitFullscreen();
  else if (expanded()) fallbackView(false);
  else {
    try {
      if (!$('emulator').requestFullscreen) throw new Error('Fullscreen unavailable');
      await $('emulator').requestFullscreen();
    } catch { fallbackView(true); }
  }
  updateView();
  screen.focus();
});
document.addEventListener('fullscreenchange', updateView);
document.addEventListener('keydown', event => {
  if (event.key === 'Escape' && $('emulator').classList.contains('focus-view')) {
    event.preventDefault();
    event.stopPropagation();
    fallbackView(false);
    screen.focus();
  }
}, true);
function readString(pointer, length) { return decoder.decode(new Uint8Array(wasm.memory.buffer, pointer, length)); }
function flushAudio() { for (const source of activeSources) source.stop(); activeSources.clear(); nextAudio = 0; }
function render() {
  // Recreate views after Wasm calls: growing linear memory invalidates old views.
  const width = wasm.msx_width(), height = wasm.msx_height();
  if (screen.width !== width || screen.height !== height) { screen.width = width; screen.height = height; }
  const pixels = new Uint8ClampedArray(wasm.memory.buffer, wasm.msx_pixels(), width * height * 4);
  context.putImageData(new ImageData(pixels, width, height), 0, 0);
  updateDisk();
  const text = readString(wasm.msx_transcript(), wasm.msx_transcript_len());
  if ($('transcript').textContent !== text) $('transcript').textContent = text;
}
function enqueueAudio() {
  if (!audioEnabled || !audioContext || audioContext.state !== 'running') return;
  const n = wasm.msx_audio_len();
  if (!n) return;
  const buffer = audioContext.createBuffer(1, n, 44100);
  buffer.copyToChannel(new Float32Array(wasm.memory.buffer, wasm.msx_audio(), n), 0);
  const source = audioContext.createBufferSource();
  source.buffer = buffer;
  source.connect(audioContext.destination);
  nextAudio = Math.max(audioContext.currentTime + 0.025, nextAudio);
  // Limit scheduling latency after a background tab or debugger pause.
  if (nextAudio > audioContext.currentTime + 0.15) return;
  source.start(nextAudio);
  nextAudio += n / 44100;
  activeSources.add(source);
  source.onended = () => { activeSources.delete(source); source.disconnect(); };
}
function frame(time) {
  if (previousTime === undefined) previousTime = time;
  const elapsed = Math.min(time - previousTime, 100);
  previousTime = time;
  if (!paused && !document.hidden) {
    accumulator += elapsed;
    let count = 0;
    while (accumulator >= frameMilliseconds() && count++ < 5) {
      const elapsedFrame = frameMilliseconds();
      for (const key of pendingText.splice(0, 64)) wasm.msx_text(key);
      wasm.msx_frame();
      enqueueAudio();
      accumulator -= elapsedFrame;
    }
    render();
  } else accumulator = 0;
  requestAnimationFrame(frame);
}
async function enableAudio() {
  if (!audioContext) {
    audioContext = new AudioContext({ sampleRate: 44100 });
    audioContext.addEventListener('statechange', updateAudioState);
  }
  await audioContext.resume();
  audioEnabled = true;
  updateAudioState();
  screen.focus();
  return audioContext;
}
function command(text) {
  if (!ready || wasm.msx_firmware() !== 0) return;
  if (pendingText.length + text.length > 16384) { status('Paste is too large (maximum 16 KiB).', 'error'); return; }
  resume();
  updateStatus();
  for (const c of text) pendingText.push(c === '\n' ? 13 : c.charCodeAt(0));
  screen.focus();
}
function updateStatus() {
  const active = wasm.msx_firmware();
  firmware = wasm.msx_boot_policy() === 0 ? 'auto' : ['auto', 'cbios', 'diagnostic', 'local'][active];
  $('firmware').value = firmware;
  system = ['Weave BASIC', cbiosName(), diagnosticName, `user BIOS · ${localName}`, 'Weave disk firmware · drive A: boot'][active];
  updateControls();
  status(`${paused ? 'Paused' : 'Running'} · ${model && active !== 1 ? 'MSX2 · ' : ''}${system}${cartridge ? ` · ${cartridge}${active === 4 ? ' (parked)' : ''}` : ''}`);
}
function restart(action) {
  importGeneration++;
  loading = false;
  releaseKeys();
  flushAudio();
  pendingText = [];
  tone = false;
  resume();
  action();
  wasm.msx_frame();
  render();
  updateStatus();
  screen.focus();
}
async function load(file, isSystem) {
  if (!file || !ready || loading) return;
  const generation = ++importGeneration;
  const mapper = Number($('mapper').value);
  loading = true;
  updateControls();
  status(`Loading ${file.name}…`);
  try {
    if (!/\.(rom|bin)$/i.test(file.name)) throw new Error('Select a .rom or .bin ROM image.');
    if (isSystem && file.size !== 32768) throw new Error('Select a 32 KiB MSX1 BIOS/BASIC system ROM.');
    // u32 is the raw Wasm ABI length type, not a cartridge size policy.
    if (!Number.isSafeInteger(file.size) || file.size <= 0 || file.size > 0xffffffff) throw new Error('The image is empty or exceeds this browser/Wasm address space.');
    const bytes = new Uint8Array(await file.arrayBuffer());
    if (generation !== importGeneration) return;
    const identity = !isSystem && mapper >= 6 && mapper <= 9 ? await romIdentity(bytes) : undefined;
    if (generation !== importGeneration) return;
    uploadRom(bytes, isSystem ? 1 : 0, mapper);
    if (!isSystem) attachSave(identity, wasm.msx_detected_mapper());
    if (isSystem) localName = file.name;
    else {
      const selected = wasm.msx_detected_mapper();
      const name = mapperNames[selected];
      cartridge = `${file.name} (${mapper === 0 ? `Auto: ${name}${wasm.msx_detection_confidence() === 4 ? ', estimate' : ', recognized format'}` : name})`;
    }
    restart(() => {});
  } catch (error) {
    if (generation !== importGeneration) return;
    wasm.msx_cancel_upload();
    const message = error instanceof RangeError ? 'Not enough browser/Wasm memory to import this image. Close other tabs or try a smaller image.' : `ROM import failed: ${error.message}`;
    status(`${message} Selected ROMs are unchanged: ${system}${cartridge ? ` · ${cartridge}` : ''}.`, 'error');
    $(isSystem ? 'bios' : 'cartridge').value = '';
  } finally {
    if (generation === importGeneration) { loading = false; updateControls(); }
    $(isSystem ? 'bios' : 'cartridge').value = '';
  }
}
// Disk imports share the ROM generation token and staging allocation. Firmware is
// already built into Wasm, so no asynchronous firmware fetch can partially boot.
function updateDisk() {
  const flags = ready ? wasm.msx_disk_flags() : 0;
  const mounted = Boolean(flags & 1);
  for (const id of ['open-dsk', 'disk']) $(id).disabled = !ready || loading;
  for (const id of ['boot-disk', 'export-disk', 'disk-protected']) $(id).disabled = !ready || loading || !mounted;
  // Eject is also a cancellation boundary, like cartridge Eject.
  $('eject-disk').disabled = !ready || !mounted;
  if (mounted) $('disk-protected').checked = Boolean(flags & 2);
  const bootError = ready && wasm.msx_disk_boot_state() === 2 ? ' · boot failed: this firmware requires a WV boot sector; check media or use the original demo' : '';
  const text = mounted ? `Drive A: ${diskName} · ${wasm.msx_disk_len() / 1024} KiB · ${flags & 2 ? 'read-only' : 'writable'} · ${flags & 4 ? 'unsaved modifications — export to keep' : 'no unsaved modifications'}${bootError}` : 'Drive A: empty';
  if ($('disk-status').textContent !== text) $('disk-status').textContent = text;
}
$('open-dsk').addEventListener('click', () => $('disk').click());
$('disk').addEventListener('change', async event => {
  const file = event.target.files[0];
  if (!file || !ready || loading) { event.target.value = ''; return; }
  const generation = ++importGeneration;
  loading = true; updateControls();
  try {
    if (!/\.dsk$/i.test(file.name) || ![368640, 737280].includes(file.size)) throw new Error('Use a raw .dsk of exactly 360 or 720 KiB (80 tracks, 1/2 sides, 9 sectors of 512 bytes). DMK/FDI and truncated images are unsupported.');
    const bytes = new Uint8Array(await file.arrayBuffer());
    if (generation !== importGeneration) return;
    const pointer = wasm.msx_upload(bytes.length);
    if (!pointer) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    new Uint8Array(wasm.memory.buffer, pointer >>> 0, bytes.length).set(bytes);
    if (!wasm.msx_mount_disk(bytes.length, $('disk-protected').checked ? 1 : 0)) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    diskName = file.name;
    status('Disk mounted in drive A:. Choose Boot disk to restart with original disk firmware.');
  } catch (error) {
    if (generation !== importGeneration) return;
    wasm.msx_cancel_upload();
    status(`DSK import failed: ${error.message} Session and previous disk unchanged.`, 'error');
  } finally {
    if (generation === importGeneration) { loading = false; updateControls(); }
    event.target.value = '';
  }
});
$('boot-disk').addEventListener('click', () => {
  persistSave();
  if (!wasm.msx_boot_disk()) { status(readString(wasm.msx_error(), wasm.msx_error_len()), 'error'); return; }
  restart(() => {});
});
$('eject-disk').addEventListener('click', () => {
  if (!wasm.msx_eject_disk()) { status(readString(wasm.msx_error(), wasm.msx_error_len()), 'error'); return; }
  diskName = '';
  restart(() => {});
});
$('disk-protected').addEventListener('change', () => { wasm.msx_disk_protect($('disk-protected').checked ? 1 : 0); updateDisk(); });
$('export-disk').addEventListener('click', () => {
  const bytes = new Uint8Array(wasm.memory.buffer, wasm.msx_disk() >>> 0, wasm.msx_disk_len()).slice();
  const url = URL.createObjectURL(new Blob([bytes], { type: 'application/octet-stream' }));
  const link = document.createElement('a');
  link.href = url; link.download = diskName.replace(/\.dsk$/i, '') + '-modified.dsk'; link.click();
  wasm.msx_disk_exported(); updateDisk();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
});
window.addEventListener('beforeunload', event => {
  if (ready && wasm.msx_disk_flags() & 4) { event.preventDefault(); event.returnValue = ''; }
});

// Save failures have their own persistent status, independent of emulator messages.
function saveStatus(message) { $('save-status').textContent = message; }
function sramBytes() { return new Uint8Array(wasm.memory.buffer, wasm.msx_sram() >>> 0, wasm.msx_sram_len()).slice(); }
function restoreSave(bytes) {
  if (bytes.length !== wasm.msx_sram_len() || ![2048, 8192].includes(bytes.length)) throw new Error('Save size does not match cartridge SRAM.');
  // Validation is complete and JS runs synchronously between CPU frames. Copy
  // into the reserved SRAM view without a second fallible Wasm allocation after
  // a cartridge has committed. The staged ABI is available to other callers.
  new Uint8Array(wasm.memory.buffer, wasm.msx_sram() >>> 0, bytes.length).set(bytes);
}
function detachSave() {
  activeSave = undefined;
  saveStatus('No SRAM cartridge loaded.');
}
function attachSave(rom, mapper) {
  if (!wasm.msx_sram_len()) { detachSave(); return; }
  // Reopening identical bytes must also retain unsaved RAM when storage is unavailable.
  if (activeSave?.rom === rom && activeSave.mapper === mapper) {
    restoreSave(decodeSave(activeSave.current, rom, mapper, wasm.msx_sram_len()));
    return;
  }
  activeSave = { rom, mapper, key: `${savePrefix}${rom}:${mapper}`, stored: null, current: null, blocked: false };
  try {
    activeSave.stored = localStorage.getItem(activeSave.key);
    if (activeSave.stored !== null) {
      restoreSave(decodeSave(activeSave.stored, rom, mapper, wasm.msx_sram_len()));
      saveStatus('SRAM restored from this browser. Reset keeps it.');
    } else saveStatus('New SRAM. Saved locally while playing; Reset keeps it.');
  } catch (error) {
    activeSave.blocked = true;
    saveStatus(`SRAM restore unavailable: ${error.message} Playing is possible. Existing storage is preserved; export a backup or explicitly import a valid save.`);
  }
  activeSave.current = encodeSave(sramBytes(), rom, mapper);
}
function persistSave() {
  if (!activeSave || !ready) return;
  const save = activeSave;
  try {
    save.current = encodeSave(sramBytes(), save.rom, save.mapper);
    if (save.blocked || save.current === save.stored) return;
    if (localStorage.getItem(save.key) !== save.stored) {
      save.blocked = true;
      throw new Error('Another tab changed this save. Export a backup; reimport it explicitly to replace local data.');
    }
    localStorage.setItem(save.key, save.current);
    save.stored = save.current;
    saveStatus('SRAM saved in this browser. Reset keeps it. Export a backup for safekeeping.');
  } catch (error) {
    save.blocked = true;
    saveStatus(`SRAM is in memory only: ${error.message} Export a backup before leaving or changing cartridges.`);
  }
}
setInterval(persistSave, 1000);
window.addEventListener('pagehide', persistSave);
$('export-save').addEventListener('click', () => {
  if (!activeSave) return;
  persistSave();
  const url = URL.createObjectURL(new Blob([activeSave.current], { type: 'application/json' }));
  const link = document.createElement('a');
  link.href = url; link.download = `msx-sram-${activeSave.rom}-${activeSave.mapper}.json`; link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
});
$('import-save').addEventListener('change', async event => {
  const file = event.target.files[0];
  const save = activeSave, generation = importGeneration;
  try {
    if (!file || !save || loading) return;
    if (file.size > 16384) throw new Error('Save is too large.');
    const text = await file.text();
    if (save !== activeSave || generation !== importGeneration || loading) return;
    const bytes = decodeSave(text, save.rom, save.mapper, wasm.msx_sram_len());
    restoreSave(bytes);
    // Explicit import authorizes replacement, including a corrupt/conflicting stored record.
    save.current = encodeSave(bytes, save.rom, save.mapper);
    save.blocked = false;
    try { save.stored = localStorage.getItem(save.key); }
    catch { /* persistSave reports storage errors and leaves execution possible. */ }
    persistSave();
    if (!save.blocked) saveStatus('Imported SRAM and saved locally. Reset to let the game read it at startup.');
  } catch (error) {
    if (save === activeSave && generation === importGeneration) saveStatus(`Save import rejected: ${error.message} Current SRAM is unchanged.`);
  } finally { event.target.value = ''; }
});

// Both local files and curated static assets use the same staged Rust cartridge API.
function uploadRom(bytes, kind, mapper, targetModel) {
  const pointer = wasm.msx_upload(bytes.length);
  if (!pointer) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
  new Uint8Array(wasm.memory.buffer, pointer >>> 0, bytes.length).set(bytes);
  persistSave();
  const ok = targetModel === undefined ? wasm.msx_load(bytes.length, kind, mapper) : wasm.msx_load_demo(bytes.length, mapper, targetModel);
  if (!ok) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
}
const mapperNames = ['Auto', 'Plain', 'ASCII8', 'ASCII16', 'Konami', 'Konami SCC', 'ASCII8SRAM2', 'ASCII8SRAM8', 'ASCII16SRAM2', 'ASCII16SRAM8', 'R-Type'];
const mapperIds = { Plain: 1, ASCII8: 2, ASCII16: 3, Konami: 4, 'Konami SCC': 5 };
const localAsset = path => typeof path === 'string' && /^(?:demos\/)?[a-zA-Z0-9_.-]+$/.test(path) && !path.includes('..');
async function staticAsset(path) {
  if (!localAsset(path)) throw new Error('Invalid demo asset path.');
  const response = await fetch(path, { mode: 'same-origin', credentials: 'omit', redirect: 'error', signal: AbortSignal.timeout(15000) });
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status}. Reload or try another demo.`);
  return response;
}
function describeDemo() {
  const demo = demos?.find(item => item.id === $('demo').value);
  if (!demo) return;
  $('demo-description').textContent = `${demo.model} · ${demo.mapper} · ${demo.description}`;
  $('demo-controls-help').textContent = `Controls: ${demo.controls}`;
  const license = document.createElement('a');
  license.href = demo.licenseFile; license.textContent = demo.license;
  const source = document.createElement('a');
  source.href = demo.source; source.textContent = 'Source';
  $('demo-credit').replaceChildren(document.createTextNode(`${demo.creator} · `), license, document.createTextNode(' · '), source);
}
async function loadCatalog() {
  catalogLoading = true; updateControls();
  try {
    const manifest = await (await staticAsset('demos/manifest.json')).json();
    if (manifest.version !== 1 || !Array.isArray(manifest.demos) || !manifest.demos.length) throw new Error('Invalid demo manifest.');
    for (const demo of manifest.demos) {
      if (!['MSX1', 'MSX2'].includes(demo.model) || !Object.hasOwn(mapperIds, demo.mapper)
          || !localAsset(demo.file) || !localAsset(demo.licenseFile)
          || !/^[a-f0-9]{64}$/.test(demo.sha256) || !Number.isSafeInteger(demo.size) || demo.size <= 0
          || ![demo.id, demo.title, demo.creator, demo.description, demo.controls, demo.license].every(value => typeof value === 'string' && value.length)
          || !demo.source?.startsWith('https://github.com/')) throw new Error('Invalid demo metadata.');
    }
    demos = manifest.demos;
    $('demo').replaceChildren(...demos.map(demo => new Option(demo.title, demo.id)));
    describeDemo();
    $('demo-message').textContent = '';
    $('play-demo').textContent = 'Play demo';
  } catch (error) {
    $('demo-message').textContent = `Demos unavailable: ${error.message} Your session is unchanged. Retry to reload the catalog.`;
    $('play-demo').textContent = 'Retry demos';
  } finally { catalogLoading = false; updateControls(); }
}
$('demo').addEventListener('change', describeDemo);
$('play-demo').addEventListener('click', async () => {
  if (!ready || loading || catalogLoading) return;
  if (!demos) { await loadCatalog(); return; }
  const demo = demos.find(item => item.id === $('demo').value);
  if (!demo) return;
  const generation = ++importGeneration;
  loading = true; updateControls();
  status(`Loading ${demo.title}…`);
  try {
    const bytes = new Uint8Array(await (await staticAsset(demo.file)).arrayBuffer());
    if (generation !== importGeneration) return;
    const digest = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), byte => byte.toString(16).padStart(2, '0')).join('');
    if (generation !== importGeneration) return;
    if (bytes.length !== demo.size || digest !== demo.sha256) throw new Error('Demo integrity check failed (size/SHA-256). Reload or try another demo.');
    const targetModel = demo.model === 'MSX2' ? 1 : 0;
    uploadRom(bytes, 0, mapperIds[demo.mapper], targetModel);
    detachSave();
    if (model !== targetModel) {
      model = targetModel;
      // Only clear staged local file choices after a successful profile transition.
      mainFile = subFile = undefined;
      $('bios').value = ''; $('subrom').value = '';
      updateModelControls();
    }
    cartridge = `${demo.title} (${demo.mapper})`;
    $('cartridge').value = '';
    $('demo-message').textContent = '';
    restart(() => {});
  } catch (error) {
    if (generation !== importGeneration) return;
    wasm.msx_cancel_upload();
    status(`Demo load failed: ${error.message} Session unchanged: ${system}${cartridge ? ` · ${cartridge}` : ''}.`, 'error');
  } finally { if (generation === importGeneration) { loading = false; updateControls(); } }
});
// International MSX keyboard matrix (US key placement). Matrix keys continue
// to work with a user BIOS; text/editor events are consumed only by our firmware.
const matrix = new Map();
function row(n, keys) { keys.forEach((key, bit) => matrix.set(key, [n, bit])); }
row(0, ['Digit0','Digit1','Digit2','Digit3','Digit4','Digit5','Digit6','Digit7']);
row(1, ['Digit8','Digit9','Minus','Equal','Backslash','BracketLeft','BracketRight','Semicolon']);
row(2, ['Quote','Backquote','Comma','Period','Slash','IntlBackslash','KeyA','KeyB']);
row(3, ['KeyC','KeyD','KeyE','KeyF','KeyG','KeyH','KeyI','KeyJ']);
row(4, ['KeyK','KeyL','KeyM','KeyN','KeyO','KeyP','KeyQ','KeyR']);
row(5, ['KeyS','KeyT','KeyU','KeyV','KeyW','KeyX','KeyY','KeyZ']);
row(6, ['ShiftLeft','ControlLeft','AltLeft','CapsLock','AltRight','F1','F2','F3']);
row(7, ['F4','F5','Escape','Tab','Pause','Backspace','End','Enter']);
row(8, ['Space','Home','Insert','Delete','ArrowLeft','ArrowUp','ArrowDown','ArrowRight']);
matrix.set('ShiftRight', [6, 0]); matrix.set('ControlRight', [6, 1]); matrix.set('NumpadEnter', [7, 7]);
const editor = { Enter:13, Backspace:8, Delete:127, ArrowLeft:128, ArrowRight:129, Home:130, End:131, Escape:27 };
function updateJoystick() {
  let value = 63;
  for (const [code, bit] of [['ArrowUp',0],['ArrowDown',1],['ArrowLeft',2],['ArrowRight',3],['KeyZ',4],['KeyX',5]]) if (held.has(code)) value &= ~(1 << bit);
  wasm.msx_joystick(value);
}
function releaseKeys() { held.clear(); if (wasm) wasm.msx_release_keys(); }
screen.addEventListener('keydown', event => {
  // Let the browser handle navigation and paste shortcuts while Canvas is focused.
  if (!ready || event.metaKey || event.code === 'Tab' || (event.ctrlKey && event.code === 'KeyV')) return;
  if (event.code in editor || matrix.has(event.code) || event.key.length === 1) event.preventDefault();
  held.add(event.code);
  for (const [code, [r, bit]] of matrix) if (held.has(code)) wasm.msx_key(r, bit, 1);
  updateJoystick();
  if (event.code === 'Escape') { pendingText = []; wasm.msx_text(27); }
  else if (event.code in editor) pendingText.push(editor[event.code]);
  else if (!event.ctrlKey && !event.altKey && event.key.length === 1 && event.key.charCodeAt(0) >= 32 && event.key.charCodeAt(0) < 127) pendingText.push(event.key.charCodeAt(0));
});
window.addEventListener('keyup', event => {
  if (!wasm) return;
  held.delete(event.code);
  // Rebuild, since either Shift key can hold the same matrix bit.
  wasm.msx_release_keys();
  for (const code of held) { const key = matrix.get(code); if (key) wasm.msx_key(...key, 1); }
  updateJoystick();
});
screen.addEventListener('blur', releaseKeys);
window.addEventListener('blur', releaseKeys);
document.addEventListener('visibilitychange', () => { persistSave(); previousTime = undefined; accumulator = 0; releaseKeys(); flushAudio(); });
screen.addEventListener('paste', event => { event.preventDefault(); command((event.clipboardData.getData('text/plain')).replace(/\r\n?/g, '\n').replace(/[^\x20-\x7e\n]/g, '')); });
$('pause').addEventListener('click', () => { paused = !paused; releaseKeys(); flushAudio(); updateStatus(); screen.focus(); });
$('reset').addEventListener('click', () => restart(() => wasm.msx_reset()));
function clearRomSelections() {
  cartridge = ''; mainFile = subFile = undefined;
  $('bios').value = ''; $('subrom').value = ''; $('cartridge').value = '';
  updateFirmwareSelection();
}
function restoreFirmware(selected = 'auto') {
  if (!ready || loading) return;
  persistSave();
  if (selected === 'cbios' || selected === 'diagnostic') {
    const ok = selected === 'cbios' ? wasm.msx_cbios() : wasm.msx_diagnostic();
    if (!ok) { status(readString(wasm.msx_error(), wasm.msx_error_len()), 'error'); return; }
  } else wasm.msx_builtin();
  detachSave();
  localName = '';
  clearRomSelections();
  restart(() => {});
}
$('builtin').addEventListener('click', () => restoreFirmware());
$('firmware').addEventListener('change', () => restoreFirmware($('firmware').value));
$('eject').addEventListener('click', () => { persistSave(); detachSave(); cartridge = ''; $('cartridge').value = ''; restart(() => wasm.msx_eject()); });
$('open-rom').addEventListener('click', () => $('cartridge').click());
$('audio').addEventListener('click', async () => {
  try {
    if (audioEnabled && audioContext.state === 'running') {
      audioEnabled = false;
      flushAudio();
      await audioContext.suspend();
      updateAudioState();
      screen.focus();
    } else await enableAudio();
  } catch (error) { status(`Audio: ${error.message}. Try Enable audio again.`, 'error'); }
});
$('example').addEventListener('click', () => command('\n10 FOR I=1 TO 5\n20 PRINT I\n30 NEXT I\nRUN\n'));
$('graphics').addEventListener('click', () => command('\nSCREEN 2 : FOR X=0 TO 255 : PSET X,INT(96+60*SIN(X/20)),10 : NEXT X\n'));
$('tone').addEventListener('click', async () => { try { await enableAudio(); tone = !tone; command(tone ? '\nBEEP\n' : '\nSOUND 8,0\n'); } catch (error) { status(`Audio: ${error.message}. Try Enable audio again.`, 'error'); } });
function updateFirmwareSelection() {
  $('firmware-selection').textContent = `Main: ${mainFile?.name || 'missing'} · Sub-ROM: ${subFile?.name || 'missing'}. Apply to restart with this pair.`;
}
function updateModelControls() {
  $('model').value = String(model);
  $('firmware').querySelector('[value=diagnostic]').disabled = !model;
  $('msx2-firmware').hidden = !model;
  $('builtin').textContent = 'Use Weave BASIC';
  updateFirmwareSelection();
}
$('model').addEventListener('change', () => {
  persistSave(); detachSave();
  model = Number($('model').value);
  firmware = 'auto'; $('firmware').value = firmware;
  localName = '';
  clearRomSelections();
  updateModelControls();
  restart(() => wasm.msx_model(model));
});
$('cartridge').addEventListener('change', event => load(event.target.files[0], false));
$('bios').addEventListener('change', event => {
  if (!model) load(event.target.files[0], true);
  else { mainFile = event.target.files[0]; updateFirmwareSelection(); }
});
$('subrom').addEventListener('change', event => { subFile = event.target.files[0]; updateFirmwareSelection(); });
$('apply-firmware').addEventListener('click', async () => {
  if (!ready || loading) return;
  const generation = ++importGeneration;
  try {
    if (!mainFile || !subFile) throw new Error('Select both the 32 KiB main BIOS/BASIC and the 16 KiB sub-ROM.');
    if (mainFile.size !== 32768 || subFile.size !== 16384) throw new Error('MSX2 requires a 32 KiB main BIOS/BASIC and a separate 16 KiB sub-ROM.');
    if (![mainFile, subFile].every(file => /\.(rom|bin)$/i.test(file.name))) throw new Error('Select .rom or .bin firmware images.');
    loading = true; updateControls();
    const [main, sub] = await Promise.all([mainFile.arrayBuffer(), subFile.arrayBuffer()]);
    if (generation !== importGeneration) return;
    const pointer = wasm.msx_upload(49152);
    if (!pointer) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    const transfer = new Uint8Array(wasm.memory.buffer, pointer >>> 0, 49152);
    transfer.set(new Uint8Array(main)); transfer.set(new Uint8Array(sub), 32768);
    if (!wasm.msx_load(49152, 2, 0)) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    localName = `${mainFile.name} + ${subFile.name}`;
    firmware = 'local'; $('firmware').value = firmware;
    restart(() => {});
  } catch (error) {
    if (generation === importGeneration) { wasm.msx_cancel_upload(); status(`ROM import failed: ${error.message} Active firmware is unchanged: ${system}.`, 'error'); }
  } finally { if (generation === importGeneration) { loading = false; updateControls(); } }
});
try {
  const response = await fetch('msx_wasm.wasm');
  if (!response.ok) throw new Error(`Wasm request failed (${response.status})`);
  const bytes = await response.arrayBuffer();
  ({ instance: { exports: wasm } } = await WebAssembly.instantiate(bytes, {}));
  wasm.msx_model(model);
  updateModelControls();
  wasm.msx_frame();
  render();
  ready = true;
  updateStatus();
  requestAnimationFrame(frame);
  loadCatalog();
} catch (error) {
  ready = false;
  updateControls();
  $('emulator').setAttribute('aria-busy', 'false');
  status(`Unable to start: ${error.message}. Reload to retry; serve the built site over HTTP.`, 'error');
}
