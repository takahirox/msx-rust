// Browser transport only: Canvas, files, keyboard, pacing, and Web Audio.
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
let model = 0;
let firmware = 'cbios';
const cbiosName = 'C-BIOS 0.29a · cartridge BIOS (no BASIC)';
const diagnosticName = 'Original cartridge loader + diagnostic sub-ROM';
let mainFile;
let subFile;
let cartridge = '';
let importGeneration = 0;
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
  const native = model === 0 && system === 'Weave BASIC' && !cartridge;
  for (const id of ['pause', 'reset', 'audio', 'builtin', 'open-rom', 'cartridge', 'bios', 'mapper', 'focus-screen', 'model', 'subrom', 'apply-firmware', 'firmware']) $(id).disabled = !ready || loading;
  for (const id of ['example', 'graphics', 'tone']) $(id).disabled = !ready || loading || !native;
  $('eject').disabled = !ready || !cartridge;
  $('emulator').setAttribute('aria-busy', String(!ready || loading));
  $('pause').textContent = paused ? 'Resume' : 'Pause';
  $('tone').textContent = tone ? 'Stop tone' : 'Sound demo';
  $('tone').setAttribute('aria-pressed', String(tone));
  $('starter-help').textContent = native ? 'One click, then make it your own.' : 'Select MSX1 and Built-in BASIC in ROMs & advanced system options to try these.';
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
  if (!ready) return;
  if (pendingText.length + text.length > 16384) { status('Paste is too large (maximum 16 KiB).', 'error'); return; }
  resume();
  updateStatus();
  for (const c of text) pendingText.push(c === '\n' ? 13 : c.charCodeAt(0));
  screen.focus();
}
function updateStatus() {
  updateControls();
  status(`${paused ? 'Paused' : 'Running'} · ${model ? 'MSX2 · ' : ''}${system}${cartridge ? ` · ${cartridge}` : ''}`);
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
    const pointer = wasm.msx_upload(bytes.length);
    if (!pointer) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    // Obtain the buffer only AFTER reservation: memory growth detaches old views.
    new Uint8Array(wasm.memory.buffer, pointer >>> 0, bytes.length).set(bytes);
    if (!wasm.msx_load(bytes.length, isSystem ? 1 : 0, mapper)) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    if (isSystem) system = file.name;
    else {
      const selected = wasm.msx_detected_mapper();
      const name = ['Auto', 'Plain', 'ASCII8', 'ASCII16', 'Konami', 'Konami SCC'][selected];
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
document.addEventListener('visibilitychange', () => { previousTime = undefined; accumulator = 0; releaseKeys(); flushAudio(); });
screen.addEventListener('paste', event => { event.preventDefault(); command((event.clipboardData.getData('text/plain')).replace(/\r\n?/g, '\n').replace(/[^\x20-\x7e\n]/g, '')); });
$('pause').addEventListener('click', () => { paused = !paused; releaseKeys(); flushAudio(); updateStatus(); screen.focus(); });
$('reset').addEventListener('click', () => restart(() => wasm.msx_reset()));
function clearRomSelections() {
  cartridge = ''; mainFile = subFile = undefined;
  $('bios').value = ''; $('subrom').value = ''; $('cartridge').value = '';
  updateFirmwareSelection();
}
function restoreFirmware(selected = 'cbios') {
  if (!ready || loading) return;
  // Both built-in pairs are already in Wasm; no async fetch can partially switch them.
  if (model && selected === 'cbios') {
    if (!wasm.msx_cbios()) { status(readString(wasm.msx_error(), wasm.msx_error_len()), 'error'); return; }
  } else wasm.msx_builtin();
  firmware = selected;
  $('firmware').value = selected;
  system = model ? (selected === 'cbios' ? cbiosName : diagnosticName) : 'Weave BASIC';
  clearRomSelections();
  restart(() => {});
}
$('builtin').addEventListener('click', () => restoreFirmware());
$('firmware').addEventListener('change', () => restoreFirmware($('firmware').value));
$('eject').addEventListener('click', () => { cartridge = ''; $('cartridge').value = ''; restart(() => wasm.msx_eject()); });
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
$('model').addEventListener('change', () => {
  model = Number($('model').value);
  firmware = 'cbios'; $('firmware').value = firmware;
  system = model ? cbiosName : 'Weave BASIC';
  clearRomSelections();
  $('msx2-builtins').hidden = !model;
  $('msx2-firmware').hidden = !model;
  $('builtin').textContent = model ? 'Restore built-in C-BIOS' : 'Built-in BASIC';
  updateFirmwareSelection();
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
    system = `${mainFile.name} + ${subFile.name}`;
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
  wasm.msx_frame();
  render();
  ready = true;
  updateStatus();
  requestAnimationFrame(frame);
} catch (error) {
  ready = false;
  updateControls();
  $('emulator').setAttribute('aria-busy', 'false');
  status(`Unable to start: ${error.message}. Reload to retry; serve the built site over HTTP.`, 'error');
}
