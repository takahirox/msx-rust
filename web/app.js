// Browser transport only: Canvas, files, keyboard, pacing, and Web Audio.
const $ = id => document.getElementById(id);
const screen = $('screen');
const context = screen.getContext('2d', { alpha: false });
const decoder = new TextDecoder();
let wasm;
let paused = false;
let audioContext;
let nextAudio = 0;
let tone = false;
let system = 'Weave BASIC';
let cartridge = '';
let previousTime;
let accumulator = 0;
const FRAME_MS = 1000 * 228 * 262 / 3579545;
const held = new Set();
let pendingText = [];
const activeSources = new Set();
function status(message) { $('status').textContent = message; }
function readString(pointer, length) { return decoder.decode(new Uint8Array(wasm.memory.buffer, pointer, length)); }
function flushAudio() { for (const source of activeSources) source.stop(); activeSources.clear(); nextAudio = 0; }
function render() {
  // Recreate views after Wasm calls: growing linear memory invalidates old views.
  const pixels = new Uint8ClampedArray(wasm.memory.buffer, wasm.msx_pixels(), 256 * 192 * 4);
  context.putImageData(new ImageData(pixels, 256, 192), 0, 0);
  const text = readString(wasm.msx_transcript(), wasm.msx_transcript_len());
  if ($('transcript').textContent !== text) $('transcript').textContent = text;
}
function enqueueAudio() {
  if (!audioContext || audioContext.state !== 'running') return;
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
    while (accumulator >= FRAME_MS && count++ < 5) {
      for (const key of pendingText.splice(0, 64)) wasm.msx_text(key);
      wasm.msx_frame();
      enqueueAudio();
      accumulator -= FRAME_MS;
    }
    render();
  } else accumulator = 0;
  requestAnimationFrame(frame);
}
async function enableAudio() {
  if (!audioContext) audioContext = new AudioContext({ sampleRate: 44100 });
  await audioContext.resume();
  $('audio').textContent = audioContext.state === 'running' ? 'Audio enabled' : 'Resume audio';
  return audioContext;
}
function command(text) {
  if (!wasm) return;
  if (pendingText.length + text.length > 16384) { status('Paste is too large (maximum 16 KiB).'); return; }
  for (const c of text) pendingText.push(c === '\n' ? 13 : c.charCodeAt(0));
  screen.focus();
}
function updateStatus() {
  const native = system === 'Weave BASIC' && !cartridge;
  for (const id of ['example', 'graphics', 'tone']) $(id).disabled = !native;
  status(`${paused ? 'Paused' : 'Running'} · ${system}${cartridge ? ` · ${cartridge}` : ''}`); }
function restart(action) { flushAudio(); pendingText = []; tone = false; action(); wasm.msx_frame(); render(); updateStatus(); screen.focus(); }
async function load(file, isSystem) {
  if (!file) return;
  if (file.size > 49152) { status('ROM too large. Plain cartridges only, up to 48 KiB.'); return; }
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    new Uint8Array(wasm.memory.buffer, wasm.msx_upload(), bytes.length).set(bytes);
    if (!wasm.msx_load(bytes.length, isSystem ? 1 : 0)) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    if (isSystem) system = file.name; else cartridge = file.name;
    restart(() => {});
  } catch (error) { status(error.message); }
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
  if (!wasm || event.metaKey) return;
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
$('pause').addEventListener('click', () => { paused = !paused; releaseKeys(); flushAudio(); $('pause').textContent = paused ? 'Start' : 'Pause'; updateStatus(); screen.focus(); });
$('reset').addEventListener('click', () => restart(() => wasm.msx_reset()));
$('builtin').addEventListener('click', () => { system = 'Weave BASIC'; cartridge = ''; tone = false; $('bios').value = ''; $('cartridge').value = ''; restart(() => wasm.msx_builtin()); });
$('eject').addEventListener('click', () => { cartridge = ''; $('cartridge').value = ''; restart(() => wasm.msx_eject()); });
$('audio').addEventListener('click', () => enableAudio().catch(error => status(`Audio: ${error.message}`)));
$('example').addEventListener('click', () => command('10 FOR I=1 TO 5\n20 PRINT I\n30 NEXT I\nRUN\n'));
$('graphics').addEventListener('click', () => command('SCREEN 2 : FOR X=0 TO 255 : PSET X,INT(96+60*SIN(X/20)),10 : NEXT X\n'));
$('tone').addEventListener('click', async () => { try { await enableAudio(); tone = !tone; command(tone ? '\nBEEP\n' : '\nSOUND 8,0\n'); } catch (error) { status(`Audio: ${error.message}`); } });
for (const [id, isSystem] of [['cartridge', false], ['bios', true]]) $(id).addEventListener('change', event => load(event.target.files[0], isSystem));
try {
  const response = await fetch('msx_wasm.wasm');
  if (!response.ok) throw new Error(`Wasm request failed (${response.status})`);
  const bytes = await response.arrayBuffer();
  ({ instance: { exports: wasm } } = await WebAssembly.instantiate(bytes, {}));
  wasm.msx_frame();
  render();
  for (const element of document.querySelectorAll('button, input')) element.disabled = false;
  updateStatus();
  requestAnimationFrame(frame);
} catch (error) { status(`Unable to start: ${error.message}. Build and serve the site over HTTP.`); }
