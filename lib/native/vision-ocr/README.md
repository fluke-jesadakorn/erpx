# @folio/vision-ocr

macOS Vision OCR exposed as a Node.js N-API addon (Swift + Objective-C++).

## Build

Requires macOS 11+ and Xcode Command Line Tools (`xcode-select --install`).

```
npm install   # node-gyp rebuild runs automatically
```

If the build fails, the addon loader returns `ocrAvailable() === false` and the
OCR pipeline skips Pass 3 (Swift Vision) without breaking the rest of the
waterfall. The OCR service logs a one-time warning at boot.

## API

```js
const vision = require('@folio/vision-ocr');

vision.ocrAvailable();            // boolean
await vision.ocrPixels({
  pixels:    Buffer,  // raw RGBA bytes, premultiplied last
  width:     number,
  height:    number,
  languages: ['th-TH', 'en-US'],   // optional, defaults to th+en
});
// → { ok, lines: string[], error_code?, error_message? }

await vision.ocrImageFile(buffer); // JPEG/PNG/etc bytes (NSImage-decoded path)
// → { ok, lines: string[], error_code?, error_message? }
```

## Limitations

- macOS only (`process.platform === 'darwin'`).
- Pass 3 fallback in the OCR pipeline. Pass 1 (Vision LLM) and Pass 2
  (Tesseract) do not depend on this addon.