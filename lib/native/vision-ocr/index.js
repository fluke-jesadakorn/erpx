// Loader for the vision-ocr native addon.
// Falls back to a no-op stub if the addon failed to build (e.g., not on macOS
// or Xcode CLT missing). The OCR pipeline calls ocrPixels/ocrImageFile and
// gracefully degrades to the next pass when ocrAvailable() === false.

'use strict';

const path = require('node:path');
const fs = require('node:fs');
const { createRequire } = require('node:module');
const runtimeRequire = createRequire(__filename);

function tryLoad() {
  if (process.platform !== 'darwin') return null;
  const candidates = [
    path.join(__dirname, 'build', 'Release', 'vision_ocr.node'),
    path.join(__dirname, 'build', 'Debug', 'vision_ocr.node'),
  ];
  for (const p of candidates) {
    if (fs.existsSync(p)) {
      try {
        return runtimeRequire(p);
      } catch (e) {
        console.warn('[vision-ocr] require failed for', p, e.message);
        return null;
      }
    }
  }
  return null;
}

let addon = tryLoad();
if (!addon) {
  console.warn('[vision-ocr] native addon not available — Pass 3 (Swift Vision) will be skipped');
}

module.exports = {
  ocrAvailable() { return addon !== null && addon !== undefined; },

  async ocrPixels({ pixels, width, height, languages }) {
    if (!addon) return { ok: false, error_code: 'addon_unavailable', lines: [] };
    try {
      const out = addon.ocrPixels({ pixels, width, height, languages });
      return { ok: !!out.ok, lines: Array.isArray(out.lines) ? out.lines : [], error_code: out.ok ? null : 'vision_failed' };
    } catch (e) {
      return { ok: false, error_code: 'exception', error_message: String(e.message || e), lines: [] };
    }
  },

  async ocrImageFile(buffer) {
    if (!addon) return { ok: false, error_code: 'addon_unavailable', lines: [] };
    try {
      const out = addon.ocrImageFile({ data: buffer });
      return { ok: !!out.ok, lines: Array.isArray(out.lines) ? out.lines : [], error_code: out.ok ? null : 'vision_failed' };
    } catch (e) {
      return { ok: false, error_code: 'exception', error_message: String(e.message || e), lines: [] };
    }
  },
};