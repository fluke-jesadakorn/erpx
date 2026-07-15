// sharp-based blankness detector.
// Replaces the inline-Python (PIL) implementation in infra/scripts/ocr-service.js.
// Synchronous-from-the-OCR-pipeline perspective: returns a Promise.

import sharp from 'sharp';

const BLANK_THRESHOLD = 0.99; // >99% white pixels = blank

export async function analyzeBlankness(filePath) {
  try {
    const { data, info } = await sharp(filePath)
      .grayscale()
      .raw()
      .toBuffer({ resolveWithObject: true });
    const total = data.length;
    if (total === 0) {
      return { isBlank: null, error: 'empty_image', width: info.width, height: info.height };
    }
    let dark = 0;
    let sum = 0;
    for (let i = 0; i < total; i++) {
      const p = data[i];
      if (p < 200) dark++;
      sum += p;
    }
    const darkRatio = dark / total;
    const meanLuminance = sum / total;
    return {
      isBlank: darkRatio < (1 - BLANK_THRESHOLD),
      darkRatio,
      meanLuminance,
      width: info.width,
      height: info.height,
    };
  } catch (e) {
    return { isBlank: null, error: String(e?.message || e), width: 0, height: 0 };
  }
}

export async function allPagesBlank(pngPaths) {
  const checks = await Promise.all(pngPaths.map(p => analyzeBlankness(p)));
  const blankCount = checks.filter(c => c.isBlank === true).length;
  return {
    blankCount,
    total: pngPaths.length,
    allBlank: blankCount === pngPaths.length,
    checks,
  };
}