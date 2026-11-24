// Browser transport only: Canvas, files, keyboard, pacing, and Web Audio.
import { savePrefix, romIdentity, encodeSave, decodeSave, fmPacSavePrefix, encodeFmPacSave, decodeFmPacSave } from './saves.js';
import { createDisplay } from './display.js';
const $ = id => document.getElementById(id);
const screen = $('screen');
const decoder = new TextDecoder();
let wasm;
let ready = false;
let loading = false;
let audioEnabled = false;
let paused = false;
let audioContext;
let outputGain;
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
let tapeName = '';
let diskRomName = '';
let musicRomName = '';
let pacRomName = '';
const gameSaves = { status: 'save-status', prefix: savePrefix, pointer: () => wasm.msx_sram(), size: () => wasm.msx_sram_len(), sizes: [2048, 8192], encode: encodeSave, decode: decodeSave };
const pacSaves = { status: 'pac-save-status', prefix: fmPacSavePrefix, pointer: () => wasm.msx_fm_pac_sram(), size: () => wasm.msx_fm_pac_sram_len(), sizes: [8190], encode: encodeFmPacSave, decode: decodeFmPacSave };
let importGeneration = 0;
let tapeImportGeneration;
let catalogLoading = false;
let previousTime;
let accumulator = 0;
function frameMilliseconds() { return 1000 * wasm.msx_frame_cycles() / 3579545; }
const held = new Set();
let pendingText = [];
const activeSources = new Set();
function status(message, kind = 'info', media) {
  $('status').textContent = message;
  $('status').dataset.kind = kind;
  const settingsMessage = $('settings-message');
  settingsMessage.hidden = kind !== 'error';
  settingsMessage.textContent = settingsMessage.hidden ? '' : message;
  const settings = $('advanced-panel');
  if (!settingsMessage.hidden && settings.matches(':popover-open') &&
      (settings.contains(document.activeElement) || document.activeElement === document.body)) {
    settingsMessage.focus({ preventScroll: true });
    settingsMessage.scrollIntoView({ block: 'start' });
  }
  for (const slot of ['cartridge', 'disk', 'tape']) {
    const notice = $(`${slot}-message`);
    notice.hidden = kind !== 'error' || media !== slot;
    notice.textContent = notice.hidden ? '' : message;
  }
}
function updateControls() {
  const native = ready && wasm.msx_firmware() === 0;
  for (const id of ['pause', 'reset', 'audio', 'builtin', 'insert-cartridge', 'open-rom', 'cartridge', 'bios', 'mapper', 'focus-screen', 'model', 'subrom', 'apply-firmware', 'firmware']) $(id).disabled = !ready || loading;
  $('starters').hidden = !native;
  for (const id of ['example', 'graphics', 'tone']) $(id).disabled = !ready || loading || !native;
  for (const button of $('demo-list').querySelectorAll('button')) button.disabled = !ready || loading;
  $('retry-demos').disabled = !ready || loading || catalogLoading;
  const sound = ready && wasm.msx_sound_cartridge_profile();
  const name = cartridge || (sound ? `${soundProfileNames[sound]} sound cartridge` : '');
  $('cartridge-status').textContent = name || 'Empty';
  $('eject').disabled = $('eject').hidden = !name;
  updateSoundCartridge();
  updateMusic();
  updatePac();
  for (const id of ['export-save', 'import-save']) $(id).disabled = !ready || loading || !gameSaves.active;
  updateDisk();
  updateTape();
  $('emulator').setAttribute('aria-busy', String(!ready || loading));
  $('pause').textContent = paused ? 'Resume' : 'Pause';
  $('tone').textContent = tone ? 'Stop tone' : 'Sound demo';
  $('tone').setAttribute('aria-pressed', String(tone));
}
function updateAudioState() {
  const running = audioEnabled && audioContext?.state === 'running';
  $('audio').textContent = running ? 'Mute audio' : audioEnabled ? 'Resume audio' : 'Enable audio';
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

function panelTrigger(panel) { return document.querySelector(`button[popovertarget="${panel.id}"]`); }
function positionPicker(panel) {
  const anchor = panelTrigger(panel).getBoundingClientRect();
  const { clientWidth, clientHeight } = document.documentElement;
  const { width, height } = panel.getBoundingClientRect();
  const left = Math.max(12, Math.min(anchor.left, clientWidth - width - 12));
  const below = anchor.bottom + 6;
  const above = anchor.top - height - 6;
  const maxTop = clientHeight - height - 12;
  const top = Math.max(12, Math.min(below <= maxTop ? below : above, maxTop));
  panel.style.setProperty('--picker-left', `${left}px`);
  panel.style.setProperty('--picker-top', `${top}px`);
}
function positionOpenPicker() {
  const panel = document.querySelector('.media-picker:popover-open');
  if (panel) positionPicker(panel);
}
const pickerResize = new ResizeObserver(entries => {
  for (const { target } of entries) if (target.matches(':popover-open')) positionPicker(target);
});
for (const panel of document.querySelectorAll('.media-picker')) pickerResize.observe(panel);
window.addEventListener('resize', positionOpenPicker);
window.addEventListener('scroll', positionOpenPicker);
for (const panel of document.querySelectorAll('[popover]')) {
  panel.addEventListener('beforetoggle', event => {
    if (event.newState === 'open') releaseKeys();
  });
}
document.addEventListener('focusin', event => {
  const panel = document.querySelector('[popover]:popover-open');
  // Let the invoker toggle the popover itself when clicked.
  if (panel && !panel.contains(event.target) && event.target !== panelTrigger(panel)) panel.hidePopover();
});
function closePanel(id) {
  const panel = $(id);
  if (!panel.matches(':popover-open')) return;
  panel.hidePopover();
  return panelTrigger(panel);
}
function restorePickerFocus(trigger) {
  if (document.activeElement === document.body) trigger?.focus();
}
function retainSettingsFocus(fallback) {
  const panel = $('advanced-panel');
  if (!panel.matches(':popover-open')) return false;
  const focused = document.activeElement;
  if (focused !== $('settings') && (!panel.contains(focused) || focused.matches(':disabled'))) $(fallback).focus();
  return true;
}

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
  releaseKeys();
  document.querySelector('[popover]:popover-open')?.hidePopover();
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
document.addEventListener('fullscreenchange', () => { releaseKeys(); updateView(); });
document.addEventListener('keydown', event => {
  if (event.key !== 'Escape') return;
  const panel = document.querySelector('[popover]:popover-open');
  if (!panel && !expanded()) return;
  event.preventDefault();
  event.stopPropagation();
  if (panel) {
    panel.hidePopover();
    panelTrigger(panel).focus();
    return;
  }
  releaseKeys();
  if (document.fullscreenElement === $('emulator')) document.exitFullscreen();
  else fallbackView(false);
  $('fullscreen').focus();
}, true);
const toolbar = document.querySelector('.toolbar');
const toolbarResize = new ResizeObserver(([entry]) => {
  $('emulator').style.setProperty('--toolbar-height', `${entry.borderBoxSize[0].blockSize}px`);
});
toolbarResize.observe(toolbar, { box: 'border-box' });
$('scale').value = '3';
$('scale').addEventListener('change', () => {
  $('emulator').style.setProperty('--display-width', `${256 * Number($('scale').value)}px`);
});
function readString(pointer, length) { return decoder.decode(new Uint8Array(wasm.memory.buffer, pointer, length)); }
function flushAudio() { for (const source of activeSources) source.stop(); activeSources.clear(); nextAudio = 0; }
const display = createDisplay(screen, $('crt-screen'), $('display'), $('display-status'), () => {
  // Recreate views after Wasm calls: growing linear memory invalidates old views.
  const width = wasm.msx_width(), height = wasm.msx_height();
  const pixels = new Uint8ClampedArray(wasm.memory.buffer, wasm.msx_pixels(), width * height * 4);
  return { width, height, pixels };
});
function render() {
  display.present();
  updateDisk();
  updateTape();
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
  nextAudio = Math.max(audioContext.currentTime + 0.025, nextAudio);
  // Limit scheduling latency after a background tab or debugger pause.
  if (nextAudio > audioContext.currentTime + 0.15) return;
  source.connect(outputGain);
  source.start(nextAudio);
  nextAudio += n / 44100;
  activeSources.add(source);
  source.onended = () => { activeSources.delete(source); source.disconnect(); };
}
function frame(time) {
  if (previousTime === undefined) previousTime = time;
  const elapsed = Math.min(time - previousTime, 100);
  previousTime = time;
  let count = 0;
  if (!paused && !document.hidden) {
    accumulator += elapsed;
    while (accumulator >= frameMilliseconds() && count < 5) {
      // Charge the budget used on entry. The CPU may fix a new R9 timing
      // during this call; charging its new budget would lose or duplicate time.
      const elapsedFrame = frameMilliseconds();
      for (const key of pendingText.splice(0, 64)) wasm.msx_text(key);
      wasm.msx_frame();
      enqueueAudio();
      accumulator -= elapsedFrame;
      count++;
    }
  } else accumulator = 0;
  if (count) render();
  else display.refreshScale();
  requestAnimationFrame(frame);
}
async function enableAudio() {
  if (!audioContext) {
    audioContext = new AudioContext({ sampleRate: 44100 });
    outputGain = audioContext.createGain();
    outputGain.gain.value = Number($('volume').value) / 100;
    outputGain.connect(audioContext.destination);
    audioContext.addEventListener('statechange', updateAudioState);
  }
  await audioContext.resume();
  audioEnabled = true;
  updateAudioState();
  screen.focus();
  return audioContext;
}
$('volume').value = $('volume').defaultValue;
$('volume').addEventListener('input', () => {
  if (outputGain) outputGain.gain.setTargetAtTime(Number($('volume').value) / 100, audioContext.currentTime, 0.015);
});
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
  status(`${paused ? 'Paused' : 'Running'} · ${model && active !== 1 ? 'MSX2 · ' : ''}${system}${wasm.msx_disk_profile() === 1 ? ' · Philips NMS1200-style' : ''}${cartridge ? ` · ${cartridge}${active === 4 ? ' (parked)' : ''}` : ''}${wasm.msx_sound_cartridge_profile() ? ` · ${soundProfileNames[wasm.msx_sound_cartridge_profile()]} sound cartridge` : ''}`);
}
function restart(action, settingsFallback) {
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
  if (!settingsFallback || !retainSettingsFocus(settingsFallback)) screen.focus();
}
async function load(file, isSystem) {
  if (!file || !ready || loading) return;
  const trigger = isSystem ? undefined : closePanel('cartridge-picker');
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
    if (isSystem) {
      localName = file.name;
      closePanel('advanced-panel');
    } else {
      const selected = wasm.msx_detected_mapper();
      const name = mapperNames[selected];
      cartridge = `${file.name} (${mapper === 0 ? `Auto: ${name}${wasm.msx_detection_confidence() === 4 ? ', estimate' : ', recognized format'}` : name})`;
    }
    restart(() => {});
  } catch (error) {
    if (generation !== importGeneration) return;
    wasm.msx_cancel_upload();
    const message = error instanceof RangeError ? 'Not enough browser/Wasm memory to import this image. Close other tabs or try a smaller image.' : `ROM import failed: ${error.message}`;
    status(`${message} Selected ROMs are unchanged: ${system}${wasm.msx_disk_profile() === 1 ? ' · Philips NMS1200-style' : ''}${cartridge ? ` · ${cartridge}` : ''}.`, 'error', isSystem ? undefined : 'cartridge');
    $(isSystem ? 'bios' : 'cartridge').value = '';
  } finally {
    if (generation === importGeneration) { loading = false; updateControls(); restorePickerFocus(trigger); }
    $(isSystem ? 'bios' : 'cartridge').value = '';
  }
}
const soundProfileNames = ['', 'Snatcher', 'SD Snatcher'];
function updateSoundCartridge() {
  const profile = ready ? wasm.msx_sound_cartridge_profile() : 0;
  $('sound-cartridge-status').textContent = profile ? `${soundProfileNames[profile]} · 64 KiB RAM · SCC-I · slot 1` : 'No sound cartridge installed';
  for (const id of ['sound-cartridge', 'install-sound-cartridge']) $(id).disabled = !ready || loading;
  $('remove-sound-cartridge').disabled = !ready || (!profile && !loading);
  $('remove-sound-cartridge').textContent = loading ? 'Cancel import' : 'Remove sound cartridge';
}
$('install-sound-cartridge').addEventListener('click', () => {
  if (!ready || loading) return;
  persistSave();
  if (!wasm.msx_install_sound_cartridge(Number($('sound-cartridge').value))) {
    status(readString(wasm.msx_error(), wasm.msx_error_len()), 'error'); return;
  }
  detachSave(); cartridge = ''; $('cartridge').value = '';
  restart(() => wasm.msx_cancel_upload(), 'install-sound-cartridge');
});
$('remove-sound-cartridge').addEventListener('click', () => {
  if (loading) {
    importGeneration++; loading = false; wasm.msx_cancel_upload(); updateControls();
    status('Import canceled. Session, sound cartridge and media unchanged.');
    retainSettingsFocus('install-sound-cartridge');
    return;
  }
  restart(() => { wasm.msx_cancel_upload(); wasm.msx_remove_sound_cartridge(); }, 'install-sound-cartridge');
});

// Tape uses the same generation/staging boundary as other local media imports.
function updateTape() {
  const flags = ready ? wasm.msx_tape_flags() : 0;
  const mounting = loading && tapeImportGeneration === importGeneration;
  for (const id of ['insert-cassette', 'open-cas', 'tape', 'tape-rate']) $(id).disabled = !ready || loading;
  $('rewind-tape').hidden = !(flags & 1);
  $('eject-tape').hidden = !(flags & 1) && !mounting;
  $('rewind-tape').disabled = !(flags & 1) || loading;
  $('eject-tape').disabled = !ready || (!(flags & 1) && !loading);
  $('eject-tape').textContent = loading ? 'Cancel import' : 'Eject tape';
  const text = flags & 1 ? `${tapeName} · ${wasm.msx_tape_rate()} baud · ${flags & 4 ? 'end of tape' : paused ? 'paused' : flags & 2 ? 'playing' : 'stopped'} · motor ${flags & 2 ? 'on' : 'off'} · ${Math.floor(wasm.msx_tape_position())} s` : 'Empty';
  if ($('tape-status').textContent !== text) $('tape-status').textContent = text;
}
function cancelMediaImport() {
  importGeneration++;
  loading = false;
  wasm.msx_cancel_upload();
}
$('open-cas').addEventListener('click', () => $('tape').click());
$('tape').addEventListener('cancel', () => {
  cancelMediaImport(); updateControls(); status('CAS import canceled. Session and tape unchanged.');
});
$('tape').addEventListener('change', async event => {
  const file = event.target.files[0];
  if (!file || !ready) return;
  const trigger = closePanel('cassette-picker');
  const generation = ++importGeneration;
  tapeImportGeneration = generation;
  const rate = Number($('tape-rate').value);
  loading = true; updateControls(); status(`Loading CAS ${file.name}…`);
  try {
    if (!/\.cas$/i.test(file.name) || !file.size || file.size > 16 * 1024 * 1024) throw new Error('Select a nonempty MSX .cas image up to 16 MiB.');
    if (![1200, 2400].includes(rate)) throw new Error('CAS rate must be 1200 or 2400 baud.');
    const bytes = new Uint8Array(await file.arrayBuffer());
    if (generation !== importGeneration) return;
    const pointer = wasm.msx_tape_upload(bytes.length);
    if (!pointer) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    new Uint8Array(wasm.memory.buffer, pointer >>> 0, bytes.length).set(bytes);
    if (!wasm.msx_mount_tape(bytes.length, rate)) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    tapeName = file.name;
    status('CAS mounted at the beginning. Software controls the cassette motor.');
  } catch (error) {
    if (generation !== importGeneration) return;
    wasm.msx_cancel_upload();
    status(`CAS import failed: ${error.message} Session and previous tape unchanged.`, 'error', 'tape');
  } finally {
    if (generation === importGeneration) { loading = false; updateControls(); restorePickerFocus(trigger); }
    event.target.value = '';
  }
});
$('rewind-tape').addEventListener('click', () => {
  cancelMediaImport(); wasm.msx_rewind_tape(); updateControls();
  status('Tape rewound. Execution unchanged.');
});
$('eject-tape').addEventListener('click', () => {
  const canceled = loading;
  cancelMediaImport();
  if (!canceled) { wasm.msx_eject_tape(); tapeName = ''; }
  updateControls(); status(canceled ? 'Import canceled. Session and tape unchanged.' : 'Tape ejected. Execution unchanged.');
  if ($('eject-tape').hidden) $('insert-cassette').focus();
});

// Internal FM firmware shares the transactional local-media staging boundary.
function updateMusic() {
  const installed = ready && wasm.msx_music_profile() === 1;
  for (const id of ['music-profile', 'open-music-rom', 'music-rom']) $(id).disabled = !ready || loading;
  $('remove-music-rom').disabled = !ready || (!installed && !loading);
  $('remove-music-rom').textContent = loading ? 'Cancel import' : 'Remove MSX-MUSIC';
  $('music-status').textContent = installed ? `MSX-MUSIC · ${musicRomName} · slot 3-1 · ${model ? 'active' : 'inactive on MSX1'}` : 'MSX-MUSIC not installed';
}
$('open-music-rom').addEventListener('click', () => $('music-rom').click());
$('music-rom').addEventListener('change', async event => {
  const file = event.target.files[0];
  if (!file || !ready || loading) { event.target.value = ''; return; }
  const generation = ++importGeneration;
  const profile = Number($('music-profile').value);
  loading = true; updateControls();
  status(`Loading MSX-MUSIC firmware ${file.name}…`);
  try {
    if (!/\.(rom|bin)$/i.test(file.name) || file.size !== 16384) throw new Error('Select a separate 16 KiB internal-profile .rom or .bin firmware image.');
    const bytes = new Uint8Array(await file.arrayBuffer());
    if (generation !== importGeneration) return;
    const pointer = wasm.msx_upload(bytes.length);
    if (!pointer) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    new Uint8Array(wasm.memory.buffer, pointer >>> 0, bytes.length).set(bytes);
    persistSave();
    if (!wasm.msx_install_music_rom(bytes.length, profile)) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    musicRomName = file.name;
    restart(() => {}, 'open-music-rom');
  } catch (error) {
    if (generation !== importGeneration) return;
    wasm.msx_cancel_upload();
    status(`MSX-MUSIC import failed: ${error.message} Session and installation unchanged.`, 'error');
  } finally {
    if (generation === importGeneration) { loading = false; updateControls(); }
    event.target.value = '';
  }
});
$('remove-music-rom').addEventListener('click', () => {
  if (loading) {
    importGeneration++; loading = false; wasm.msx_cancel_upload(); updateControls();
    status('Import canceled. Session and installation unchanged.');
    retainSettingsFocus('open-music-rom');
    return;
  }
  persistSave();
  restart(() => { wasm.msx_cancel_upload(); wasm.msx_remove_music_rom(); musicRomName = ''; }, 'open-music-rom');
});

// The external accessory has its own firmware identity and shared battery save.
function updatePac() {
  const installed = ready && wasm.msx_fm_pac_profile() === 1;
  for (const id of ['open-pac-rom', 'pac-rom']) $(id).disabled = !ready || loading;
  $('remove-pac-rom').disabled = !ready || (!installed && !loading);
  $('remove-pac-rom').textContent = loading ? 'Cancel import' : 'Remove FM-PAC';
  $('pac-status').textContent = installed ? `FM-PAC · ${pacRomName} · slot 2-1 · active` : 'FM-PAC not installed';
  for (const id of ['export-pac-save', 'import-pac-save']) $(id).disabled = !ready || loading || !pacSaves.active;
}
$('open-pac-rom').addEventListener('click', () => $('pac-rom').click());
$('pac-rom').addEventListener('change', async event => {
  const file = event.target.files[0];
  if (!file || !ready || loading) { event.target.value = ''; return; }
  const generation = ++importGeneration;
  loading = true; updateControls(); status(`Loading FM-PAC firmware ${file.name}…`);
  try {
    if (!/\.(rom|bin)$/i.test(file.name) || file.size !== 65536) throw new Error('Select a separate 64 KiB original FM-PAC .rom or .bin firmware image.');
    const bytes = new Uint8Array(await file.arrayBuffer());
    const identity = await romIdentity(bytes);
    if (generation !== importGeneration) return;
    const pointer = wasm.msx_upload(bytes.length);
    if (!pointer) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    new Uint8Array(wasm.memory.buffer, pointer >>> 0, bytes.length).set(bytes);
    persistSave();
    if (!wasm.msx_install_fm_pac(bytes.length, 1)) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    pacRomName = file.name;
    attachSave(identity, 'original-fm-pac', pacSaves);
    restart(() => {}, 'open-pac-rom');
  } catch (error) {
    if (generation !== importGeneration) return;
    wasm.msx_cancel_upload();
    status(`FM-PAC import failed: ${error.message} Session and installation unchanged.`, 'error');
  } finally {
    if (generation === importGeneration) { loading = false; updateControls(); }
    event.target.value = '';
  }
});
$('remove-pac-rom').addEventListener('click', () => {
  if (loading) {
    importGeneration++; loading = false; wasm.msx_cancel_upload(); updateControls();
    status('Import canceled. Session and installation unchanged.');
    retainSettingsFocus('open-pac-rom');
    return;
  }
  persistSave(); detachSave(pacSaves);
  restart(() => { wasm.msx_cancel_upload(); wasm.msx_remove_fm_pac(); pacRomName = ''; }, 'open-pac-rom');
});

// Disk media and interface imports share the ROM generation token and staging allocation.
function updateDisk() {
  const philips = ready && wasm.msx_disk_profile() === 1;
  const profile = philips ? `Philips NMS1200-style · ${diskRomName} · slot 2 · selected system firmware` : 'Weave disk/demo boot option · no external interface installed';
  if ($('disk-profile-status').textContent !== profile) $('disk-profile-status').textContent = profile;
  for (const id of ['open-disk-rom', 'disk-rom']) $(id).disabled = !ready || loading;
  $('remove-disk-rom').disabled = !ready || (!philips && !loading);
  $('remove-disk-rom').textContent = loading ? 'Cancel import' : 'Remove disk interface';
  const flags = ready ? wasm.msx_disk_flags() : 0;
  const mounted = Boolean(flags & 1);
  for (const id of ['insert-disk', 'open-dsk', 'disk', 'mount-disk-demo']) $(id).disabled = !ready || loading;
  $('disk-actions').hidden = $('eject-disk').hidden = !mounted;
  for (const id of ['boot-disk', 'export-disk', 'disk-protected']) $(id).disabled = !ready || loading || !mounted;
  // Eject is also a cancellation boundary, like cartridge Eject.
  $('eject-disk').disabled = !ready || !mounted;
  if (mounted) $('disk-protected').checked = Boolean(flags & 2);
  const bootError = ready && wasm.msx_disk_boot_state() === 2 ? ' · boot failed: this firmware requires a WV boot sector; check media or use the original demo' : '';
  const text = mounted ? `${diskName} · ${wasm.msx_disk_len() / 1024} KiB · ${flags & 2 ? 'read-only' : 'writable'} · ${flags & 4 ? 'unsaved modifications — export to keep' : 'no unsaved modifications'}${bootError}` : 'Empty';
  if ($('disk-status').textContent !== text) {
    $('disk-status').textContent = text;
    $('disk-status').dataset.warning = String(Boolean((flags & 4) || bootError));
  }
}
$('open-disk-rom').addEventListener('click', () => $('disk-rom').click());
$('disk-rom').addEventListener('change', async event => {
  const file = event.target.files[0];
  if (!file || !ready || loading) { event.target.value = ''; return; }
  const generation = ++importGeneration;
  loading = true; updateControls();
  status(`Loading Philips disk ROM ${file.name}…`);
  try {
    if (!/\.(rom|bin)$/i.test(file.name) || file.size !== 16384) throw new Error('Select a 16 KiB AB-header .rom or .bin disk ROM.');
    const bytes = new Uint8Array(await file.arrayBuffer());
    if (generation !== importGeneration) return;
    const pointer = wasm.msx_upload(bytes.length);
    if (!pointer) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    new Uint8Array(wasm.memory.buffer, pointer >>> 0, bytes.length).set(bytes);
    persistSave();
    if (!wasm.msx_install_disk_rom(bytes.length, 1)) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    diskRomName = file.name;
    restart(() => {}, 'open-disk-rom');
  } catch (error) {
    if (generation !== importGeneration) return;
    wasm.msx_cancel_upload();
    status(`Disk ROM import failed: ${error.message} Session, interface and media unchanged.`, 'error');
  } finally {
    if (generation === importGeneration) { loading = false; updateControls(); }
    event.target.value = '';
  }
});
$('remove-disk-rom').addEventListener('click', () => {
  if (loading) {
    importGeneration++; loading = false; wasm.msx_cancel_upload(); updateControls();
    status('Import canceled. Session, interface and media unchanged.');
    retainSettingsFocus('open-disk-rom');
    return;
  }
  persistSave();
  restart(() => { wasm.msx_cancel_upload(); wasm.msx_remove_disk_rom(); diskRomName = ''; }, 'open-disk-rom');
});
$('open-dsk').addEventListener('click', () => $('disk').click());
$('disk').addEventListener('change', async event => {
  const file = event.target.files[0];
  if (file) await mountDisk(file.name, async () => {
    if (!/\.dsk$/i.test(file.name) || ![368640, 737280].includes(file.size)) throw new Error('Use a raw .dsk of exactly 360 or 720 KiB (80 tracks, 1/2 sides, 9 sectors of 512 bytes). DMK/FDI and truncated images are unsupported.');
    return file.arrayBuffer();
  });
  event.target.value = '';
});
$('mount-disk-demo').addEventListener('click', () => {
  mountDisk('weave-disk-demo.dsk', async () => (await staticAsset('weave-disk-demo.dsk')).arrayBuffer());
});
async function mountDisk(name, readBytes) {
  if (!ready || loading) return;
  const trigger = closePanel('disk-picker');
  const generation = ++importGeneration;
  const protectedDisk = $('disk-protected').checked;
  loading = true; updateControls();
  status(`Loading DSK ${name}…`);
  try {
    const bytes = new Uint8Array(await readBytes());
    if (generation !== importGeneration) return;
    const pointer = wasm.msx_upload(bytes.length);
    if (!pointer) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    new Uint8Array(wasm.memory.buffer, pointer >>> 0, bytes.length).set(bytes);
    if (!wasm.msx_mount_disk(bytes.length, protectedDisk ? 1 : 0)) throw new Error(readString(wasm.msx_error(), wasm.msx_error_len()));
    diskName = name;
    status(wasm.msx_disk_profile() === 1 ? 'Disk mounted in drive A:. Boot disk restarts the selected system firmware with the Philips interface.' : 'Disk mounted in drive A:. Choose Boot disk to restart with original disk firmware.');
  } catch (error) {
    if (generation !== importGeneration) return;
    wasm.msx_cancel_upload();
    status(`DSK import failed: ${error.message} Session and previous disk unchanged.`, 'error', 'disk');
  } finally {
    if (generation === importGeneration) { loading = false; updateControls(); restorePickerFocus(trigger); }
  }
}
$('boot-disk').addEventListener('click', () => {
  persistSave();
  if (!wasm.msx_boot_disk()) { status(readString(wasm.msx_error(), wasm.msx_error_len()), 'error', 'disk'); return; }
  restart(() => {});
});
$('eject-disk').addEventListener('click', () => {
  if (!wasm.msx_eject_disk()) { status(readString(wasm.msx_error(), wasm.msx_error_len()), 'error', 'disk'); return; }
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
  status('DSK exported.');
  setTimeout(() => URL.revokeObjectURL(url), 1000);
});
window.addEventListener('beforeunload', event => {
  if (ready && wasm.msx_disk_flags() & 4) { event.preventDefault(); event.returnValue = ''; }
});

// Save failures have their own persistent status, independent of emulator messages.
function saveStatus(message, target = gameSaves) {
  $(target.status).textContent = message;
  const warning = $(target === gameSaves ? 'cartridge-save-warning' : 'pac-save-warning');
  warning.hidden = !target.active?.blocked;
  warning.textContent = warning.hidden ? '' : `${target === gameSaves ? 'Cartridge' : 'FM-PAC'} SRAM needs attention. Open Settings to export a backup or recover the save.`;
}
function sramBytes(target = gameSaves) { return new Uint8Array(wasm.memory.buffer, target.pointer() >>> 0, target.size()).slice(); }
function restoreSave(bytes, target = gameSaves) {
  if (bytes.length !== target.size() || !target.sizes.includes(bytes.length)) throw new Error('Save size does not match cartridge SRAM.');
  // Validation is complete and JS runs synchronously between CPU frames. Copy
  // into the reserved SRAM view without a second fallible Wasm allocation after
  // a cartridge has committed. The staged ABI is available to other callers.
  new Uint8Array(wasm.memory.buffer, target.pointer() >>> 0, bytes.length).set(bytes);
}
function detachSave(target = gameSaves) {
  target.active = undefined;
  saveStatus(target === pacSaves ? 'No FM-PAC installed.' : 'No SRAM cartridge loaded.', target);
}
function attachSave(rom, mapper, target = gameSaves) {
  if (!target.size()) { detachSave(target); return; }
  // Reopening identical bytes must also retain unsaved RAM when storage is unavailable.
  if (target.active?.rom === rom && target.active.mapper === mapper) {
    restoreSave(target.decode(target.active.current, rom, mapper, target.size()), target);
    return;
  }
  target.active = { rom, mapper, key: `${target.prefix}${rom}:${mapper}`, stored: null, current: null, blocked: false };
  try {
    target.active.stored = localStorage.getItem(target.active.key);
    if (target.active.stored !== null) {
      restoreSave(target.decode(target.active.stored, rom, mapper, target.size()), target);
      saveStatus('SRAM restored from this browser. Reset keeps it.', target);
    } else saveStatus('New SRAM. Saved locally while playing; Reset keeps it.', target);
  } catch (error) {
    target.active.blocked = true;
    saveStatus(`SRAM restore unavailable: ${error.message} Playing is possible. Existing storage is preserved; export a backup or explicitly import a valid save.`, target);
  }
  target.active.current = target.encode(sramBytes(target), rom, mapper);
}
function persistOneSave(target) {
  if (!target.active || !ready) return;
  const save = target.active;
  try {
    save.current = target.encode(sramBytes(target), save.rom, save.mapper);
    if (save.blocked || save.current === save.stored) return;
    if (localStorage.getItem(save.key) !== save.stored) {
      save.blocked = true;
      throw new Error('Another tab changed this save. Export a backup; reimport it explicitly to replace local data.');
    }
    localStorage.setItem(save.key, save.current);
    save.stored = save.current;
    saveStatus('SRAM saved in this browser. Reset keeps it. Export a backup for safekeeping.', target);
  } catch (error) {
    save.blocked = true;
    saveStatus(`SRAM is in memory only: ${error.message} Export a backup before leaving or changing cartridges.`, target);
  }
}
function persistSave() { persistOneSave(gameSaves); persistOneSave(pacSaves); }
setInterval(persistSave, 1000);
window.addEventListener('pagehide', persistSave);
for (const [target, exportId, importId, filename] of [
  [gameSaves, 'export-save', 'import-save', 'msx-sram'],
  [pacSaves, 'export-pac-save', 'import-pac-save', 'msx-fm-pac'],
]) {
  $(exportId).addEventListener('click', () => {
    if (!target.active) return;
    persistOneSave(target);
    const save = target.active;
    const url = URL.createObjectURL(new Blob([save.current], { type: 'application/json' }));
    const link = document.createElement('a');
    link.href = url; link.download = `${filename}-${save.rom}-${save.mapper}.json`; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  });
  $(importId).addEventListener('change', async event => {
    const file = event.target.files[0];
    const save = target.active, generation = importGeneration;
    try {
      if (!file || !save || loading) return;
      if (file.size > 16384) throw new Error('Save is too large.');
      const text = await file.text();
      if (save !== target.active || generation !== importGeneration || loading) return;
      const bytes = target.decode(text, save.rom, save.mapper, target.size());
      restoreSave(bytes, target);
      // Explicit import authorizes replacement of corrupt/conflicting records.
      save.current = target.encode(bytes, save.rom, save.mapper);
      save.blocked = false;
      try { save.stored = localStorage.getItem(save.key); }
      catch { /* persistOneSave reports errors while keeping execution possible. */ }
      persistOneSave(target);
      if (!save.blocked) saveStatus('Imported SRAM and saved locally. Reset to let software read it at startup.', target);
    } catch (error) {
      if (save === target.active && generation === importGeneration) saveStatus(`Save import rejected: ${error.message} Current SRAM is unchanged.`, target);
    } finally { event.target.value = ''; }
  });
}

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
function demoChoice(demo) {
  const row = document.createElement('div');
  row.className = 'demo-choice';
  const play = document.createElement('button');
  play.id = `demo-${demo.id}`;
  const modelLabel = document.createElement('span');
  modelLabel.className = 'demo-model';
  modelLabel.textContent = demo.model;
  play.append(demo.title, modelLabel);
  play.addEventListener('click', () => playDemo(demo));
  const details = document.createElement('details');
  details.className = 'demo-details';
  const summary = document.createElement('summary');
  summary.textContent = 'Controls & credits';
  summary.setAttribute('aria-label', `${demo.title} controls and credits`);
  const description = document.createElement('p');
  description.textContent = `${demo.mapper} mapper · ${demo.description} ${demo.controls}`;
  const credit = document.createElement('p');
  const license = document.createElement('a');
  license.href = demo.licenseFile; license.textContent = demo.license;
  const source = document.createElement('a');
  source.href = demo.source; source.textContent = 'Source';
  credit.append(`${demo.creator} · `, license, ' · ', source);
  details.append(summary, description, credit);
  row.append(play, details);
  return row;
}
async function loadCatalog() {
  if (!ready || loading || catalogLoading) return;
  catalogLoading = true; updateControls();
  $('demo-message').textContent = 'Loading demos…';
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
    $('demo-list').replaceChildren(...manifest.demos.map(demoChoice));
    $('demo-message').textContent = '';
    $('retry-demos').hidden = true;
  } catch (error) {
    $('demo-message').textContent = `Demos unavailable: ${error.message} Your session is unchanged. Retry to reload the catalog.`;
    $('retry-demos').hidden = false;
  } finally { catalogLoading = false; updateControls(); }
}
$('retry-demos').addEventListener('click', loadCatalog);
async function playDemo(demo) {
  if (!ready || loading || catalogLoading) return;
  const trigger = closePanel('cartridge-picker');
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
    status(`Demo load failed: ${error.message} Session unchanged: ${system}${wasm.msx_disk_profile() === 1 ? ' · Philips NMS1200-style' : ''}${cartridge ? ` · ${cartridge}` : ''}.`, 'error', 'cartridge');
  } finally {
    if (generation === importGeneration) { loading = false; updateControls(); restorePickerFocus(trigger); }
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
  const controls = [
    ['ArrowUp','ArrowDown','ArrowLeft','ArrowRight','KeyZ','KeyX'],
    ['KeyI','KeyK','KeyJ','KeyL','KeyN','KeyM'],
  ];
  controls.forEach((keys, connector) => {
    let value = 63;
    keys.forEach((code, bit) => { if (held.has(code)) value &= ~(1 << bit); });
    wasm.msx_joystick_indexed(connector, value);
  });
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
  closePanel('advanced-panel');
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
$('cartridge').addEventListener('change', event => {
  if (loading) { event.target.value = ''; return; }
  load(event.target.files[0], false);
});
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
    closePanel('advanced-panel');
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
