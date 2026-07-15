#!/usr/bin/env node
// Post-install helper: warns clearly when the vision-ocr native addon
// failed to build so that the OCR pipeline can degrade gracefully.
'use strict';
const fs = require('node:fs');
const path = require('node:path');

const candidates = [
  path.join(__dirname, '..', 'build', 'Release', 'vision_ocr.node'),
  path.join(__dirname, '..', 'build', 'Debug', 'vision_ocr.node'),
];
const found = candidates.find(p => fs.existsSync(p));
if (found) {
  console.log('[vision-ocr] native addon built:', found);
} else {
  console.warn('[vision-ocr] native addon NOT built — OCR pipeline will skip Pass 3 (Swift Vision).');
  console.warn('         To enable: install Xcode CLT (xcode-select --install) and reinstall.');
}