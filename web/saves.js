// Versioned local SRAM containers; never contain original ROM bytes.
export const savePrefix = 'msx-rust:sram:v1:';
export function checksum(bytes) {
  // FNV-1a detects accidental corruption; this is not an authenticity signature.
  let hash = 0x811c9dc5;
  for (const byte of bytes) hash = Math.imul(hash ^ byte, 0x01000193) >>> 0;
  return hash.toString(16).padStart(8, '0');
}
export async function romIdentity(bytes) {
  return Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), b => b.toString(16).padStart(2, '0')).join('');
}
export function encodeSave(bytes, rom, mapper) {
  return JSON.stringify({ version: 1, rom, mapper, size: bytes.length, checksum: checksum(bytes), data: btoa(String.fromCharCode(...bytes)) });
}
export function decodeSave(text, rom, mapper, size) {
  if (typeof text !== 'string' || text.length > 16384) throw new Error('Save is too large.');
  const save = JSON.parse(text);
  if (save?.version !== 1 || save.rom !== rom || save.mapper !== mapper || save.size !== size
      || ![2048, 8192].includes(size) || typeof save.data !== 'string' || save.data.length !== 4 * Math.ceil(size / 3)) throw new Error('Save version, ROM identity, mapper or size does not match.');
  const bytes = Uint8Array.from(atob(save.data), c => c.charCodeAt(0));
  if (bytes.length !== size || btoa(String.fromCharCode(...bytes)) !== save.data || checksum(bytes) !== save.checksum) throw new Error('Save data is corrupted (size/checksum).');
  return bytes;
}
