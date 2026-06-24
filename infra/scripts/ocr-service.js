#!/usr/bin/env node
/**
 * Local OCR service for n8n (with diagnostic error codes).
 * Listens on 127.0.0.1:8765.
 *   POST /ocr   body { pdf_b64 }           -> multi-pass OCR (pdftotext/qpdf/tesseract/vision)
 *   POST /vision body { file_data_b64, file_mime, file_name }
 *                                          -> single Ollama Vision LLM (qwen3.6)
 *
 * /ocr response shape:
 *   { ok, text, numpages, method, error_code, error_message, elapsed_ms, diagnostics }
 *
 * /vision response shape (matches old Code node output for drop-in replace):
 *   { ok, text, chars, pages, source, vision_model, vision_eval_count,
 *     vision_duration_ms, error_code, error_message, elapsed_ms }
 *
 * Auth: SSRF allowlist (n8n N8N_SSRF_ALLOWED_IP_RANGES=127.0.0.0/8) handles network isolation.
 * Started via nohup or launchd (com.lawpoc.ocr).
 */

const http = require('http');
const { execSync, spawnSync } = require('child_process');
const os = require('os');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const PORT = parseInt(process.env.OCR_SERVICE_PORT || '8765', 10);
const HOST = process.env.OCR_SERVICE_HOST || '127.0.0.1';
const MAX_PDF_BYTES = 50 * 1024 * 1024; // 50MB cap
const BLANK_THRESHOLD = 0.99;   // >99% white pixels = blank
const LOW_TEXT_THRESHOLD = 20;  // <20 chars = low text
const VISION_TIMEOUT_MS = 60000;  // 60s per page (vision returns ~20-30s when working; was 180s = wasted time)
const OLLAMA_URL = (process.env.OLLAMA_URL || 'http://127.0.0.1:11434') + '/api/generate';
const VISION_MODEL = process.env.OLLAMA_VISION_MODEL || 'qwen3.6:35b-a3b-q4_K_M';
// qwen3 vision models REQUIRE the chat template in the prompt — without
// <|im_start|>user\n...<|im_end|>\n<|im_start|>assistant\n they generate
// only whitespace/EOS tokens (eval_count > 0 but response = "").
// qwen3-vl:4b has a bug with long Thai prompts: prompts >= 29 Thai chars
// produce only whitespace/EOS tokens (eval_count > 0 but response = "").
// Keep the user-side instruction very short to avoid the bug.
const VISION_PROMPT_RAW = 'ดึงข้อความจากภาพ';
const VISION_PROMPT = `<|im_start|>user\n${VISION_PROMPT_RAW}<|im_end|>\n<|im_start|>assistant\n`;

function decodeBase64(b64) {
  if (typeof b64 === 'string' && b64.startsWith('data:')) {
    b64 = b64.split(',', 2)[1] || '';
  }
  return Buffer.from(b64, 'base64');
}

function sh(cmd, opts = {}) {
  // Run shell command, capture stdout+stderr, never throw
  const r = spawnSync('bash', ['-c', cmd], {
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    ...opts,
  });
  return { stdout: r.stdout || '', stderr: r.stderr || '', status: r.status };
}

function analyzeBlankness(pngPath) {
  // Use Python + PIL to check if the rendered page is blank.
  // Returns: { isBlank, darkRatio, meanLuminance, width, height }
  const py = `
import sys
from PIL import Image
img = Image.open(sys.argv[1]).convert('L')
w, h = img.size
pixels = list(img.getdata())
dark = sum(1 for p in pixels if p < 200)
total = len(pixels)
mean = sum(pixels) / total if total else 0
print(f"{w} {h} {dark} {total} {mean:.2f}")
`;
  try {
    const r = sh(`python3 -c '${py.replace(/'/g, "'\\''")}' '${pngPath}'`);
    if (r.status !== 0 || !r.stdout.trim()) {
      return { isBlank: null, error: 'blankness_check_failed', stderr: r.stderr.slice(0, 200) };
    }
    const [w, h, dark, total, mean] = r.stdout.trim().split(/\s+/).map(Number);
    const darkRatio = dark / total;
    return {
      isBlank: darkRatio < (1 - BLANK_THRESHOLD),
      darkRatio,
      meanLuminance: mean,
      width: w,
      height: h,
    };
  } catch (e) {
    return { isBlank: null, error: String(e.message || e) };
  }
}

function extractViaOllamaVision(fileBuffer, mime, fname) {
  const start = Date.now();
  const ext = (fname || '').toLowerCase().split('.').pop() || '';
  const isPdf = ext === 'pdf' || mime === 'application/pdf';
  const isImage = ['jpg','jpeg','png','webp','gif','bmp'].includes(ext) || (mime && mime.startsWith('image/'));
  const isText = ['txt','md','csv','log'].includes(ext) || (mime && mime.startsWith('text/'));

  // TEXT: decode directly, no LLM needed
  if (isText) {
    try {
      const text = fileBuffer.toString('utf-8');
      return {
        ok: true, text,
        chars: text.length, pages: 0, page_images: [],
        source: 'text-direct',
        vision_model: VISION_MODEL,
        vision_eval_count: 0,
        vision_duration_ms: 0,
        error_code: 'text_ok',
        error_message: '',
        elapsed_ms: Date.now() - start,
      };
    } catch (e) {
      return {
        ok: false, text: '', chars: 0, pages: 0, page_images: [],
        source: 'text-fail',
        vision_model: VISION_MODEL,
        vision_eval_count: 0, vision_duration_ms: 0,
        error_code: 'text_decode_failed',
        error_message: e.message,
        elapsed_ms: Date.now() - start,
      };
    }
  }

  if (!isPdf && !isImage) {
    return {
      ok: false, text: '', chars: 0, pages: 0, page_images: [],
      source: 'unsupported',
      vision_model: VISION_MODEL,
      vision_eval_count: 0, vision_duration_ms: 0,
      error_code: 'unsupported_file_type',
      error_message: `unsupported file type: ext=${ext}, mime=${mime}`,
      elapsed_ms: Date.now() - start,
    };
  }

  // IMAGE (single) or PDF (multi-page) → render to JPEG → call Ollama vision
  // Use JPEG @ 120 DPI q=80 (≈5× smaller than PNG, vision LLM accuracy unchanged).
  // Also retain page_images[] in response so the n8n flow can persist per-page
  // images to contract_pages for visual retrieval in the admin UI.
  const work = fs.mkdtempSync(path.join(os.tmpdir(), `vision-${crypto.randomBytes(4).toString('hex')}-`));
  let imageBuffers = [];
  let imageMimes  = [];
  let source = 'unknown';
  let pageCount = 0;
  let totalEval = 0;
  let totalDurationMs = 0;

  try {
    if (isImage) {
      imageBuffers = [fileBuffer];
      imageMimes  = [mime || 'image/jpeg'];
      source = 'vision-image';
      pageCount = 1;
    } else {
      // PDF: render all pages as JPEG
      const pdfPath = path.join(work, 'in.pdf');
      fs.writeFileSync(pdfPath, fileBuffer);

      // Fast-path: text-based PDFs (most of them). pdftotext extracts ~50ms
      // and is far more reliable than vision LLMs for Thai/English text layers.
      // Skip vision entirely if we get real text — saves 20-30s and avoids
      // qwen3-vl hallucination on pages with embedded fonts.
      const pdftotextR = sh(`pdftotext "${pdfPath}" -`);
      const pdftotextText = pdftotextR.stdout || '';
      const pdftotextTrimmed = pdftotextText.replace(/\s+/g, '').trim();
      if (pdftotextTrimmed.length >= LOW_TEXT_THRESHOLD) {
        const numpagesR = sh(`pdfinfo "${pdfPath}" | awk '/^Pages:/{print $2}'`);
        const detectedPages = parseInt(numpagesR.stdout.trim(), 10) || 1;
        return {
          ok: true, text: pdftotextText,
          chars: pdftotextText.length, pages: detectedPages, page_images: [],
          source: 'pdftotext-fastpath',
          vision_model: VISION_MODEL,
          vision_eval_count: 0, vision_duration_ms: 0,
          error_code: 'pdftotext_ok',
          error_message: '',
          elapsed_ms: Date.now() - start,
        };
      }

      const jpgPrefix = path.join(work, 'out');
      const pdftoppmR = sh(`pdftoppm -jpeg -r 120 -jpegopt quality=80 "${pdfPath}" "${jpgPrefix}"`, { timeout: 60000 });
      const jpgFiles = fs.readdirSync(work).filter(f => f.startsWith('out-') && f.endsWith('.jpg')).sort();
      if (jpgFiles.length === 0) {
        return {
          ok: false, text: '', chars: 0, pages: 0, page_images: [],
          source: 'render-fail',
          vision_model: VISION_MODEL,
          vision_eval_count: 0, vision_duration_ms: 0,
          error_code: 'pdf_render_empty',
          error_message: `pdftoppm status=${pdftoppmR.status}, stderr=${pdftoppmR.stderr.slice(0, 200)}`,
          elapsed_ms: Date.now() - start,
        };
      }
      // Blank check: if ALL rendered pages are blank (>99% white), the PDF likely has
      // corrupt zlib streams or is image-only with no decodable content. Skip the
      // vision LLM call — qwen3.6 will hallucinate generic text on blank pages,
      // producing a bogus "ready" chunk that pollutes RAG. Return empty so the
      // flow's Has Chunks? IF triggers the rollback path.
      let blankCount = 0;
      for (const f of jpgFiles) {
        const blank = analyzeBlankness(path.join(work, f));
        if (blank.isBlank === true) blankCount++;
      }
      if (blankCount === jpgFiles.length) {
        return {
          ok: false, text: '', chars: 0, pages: jpgFiles.length, page_images: [],
          source: 'blank-pdf',
          vision_model: VISION_MODEL,
          vision_eval_count: 0, vision_duration_ms: 0,
          error_code: 'blank_pdf',
          error_message: `All ${jpgFiles.length} rendered page(s) are blank (likely corrupt zlib streams or no decodable content). Re-export via Mac Preview (File > Export as PDF) to recover.`,
          elapsed_ms: Date.now() - start,
        };
      }
      for (const f of jpgFiles) {
        imageBuffers.push(fs.readFileSync(path.join(work, f)));
        imageMimes.push('image/jpeg');
      }
      source = 'vision-pdf';
      pageCount = jpgFiles.length;
    }

    // Build page_images[] (b64 + meta) for the n8n flow to persist.
    // Carries the raw bytes through the vision call so we don't have to re-render.
    const pageImages = imageBuffers.map((buf, i) => ({
      page_index: i,
      image_b64:  buf.toString('base64'),
      bytes:      buf.length,
      mime:       imageMimes[i] || 'image/jpeg',
    }));

      // Call Ollama vision per page
      const pageTexts = [];
      for (let i = 0; i < imageBuffers.length; i++) {
        const b64 = imageBuffers[i].toString('base64');
        const body = JSON.stringify({
          model: VISION_MODEL,
          prompt: VISION_PROMPT,
          images: [b64],
          stream: false,
          // num_predict caps the response length — without it qwen3-vl:4b
          // can keep generating until VISION_TIMEOUT_MS, wasting time. 2048
          // tokens is enough for one full page of dense Thai text.
          options: { temperature: 0.1, num_predict: 2048 },
        });
      // Use spawnSync to avoid huge arg lists + handle JSON safely
      const r = sh(
        `curl -sS --max-time ${Math.floor(VISION_TIMEOUT_MS / 1000)} ` +
        `-X POST "${OLLAMA_URL}" -H 'Content-Type: application/json' ` +
        `--data-binary @-`,
        { input: body, timeout: VISION_TIMEOUT_MS + 5000 }
      );
      if (r.status !== 0 || !r.stdout.trim()) {
        pageTexts.push(`[page ${i+1} vision failed: status=${r.status}, stderr=${r.stderr.slice(0, 200)}]`);
        continue;
      }
      let resp;
      try { resp = JSON.parse(r.stdout); } catch (_) {
        pageTexts.push(`[page ${i+1} vision failed: JSON parse error]`);
        continue;
      }
      const t = (resp && resp.response ? String(resp.response) : '').trim();
      if (t) pageTexts.push(t);
      totalEval += (resp && resp.eval_count) || 0;
      totalDurationMs += (resp && resp.total_duration) ? Math.round(resp.total_duration / 1e6) : 0;
    }

    const fullText = pageTexts.join('\n\n');
    return {
      ok: true, text: fullText,
      chars: fullText.length, pages: pageCount,
      page_images: pageImages,
      source,
      vision_model: VISION_MODEL,
      vision_eval_count: totalEval,
      vision_duration_ms: totalDurationMs,
      error_code: 'vision_ok',
      error_message: '',
      elapsed_ms: Date.now() - start,
    };
  } finally {
    try { fs.rmSync(work, { recursive: true, force: true }); } catch (_) { /* best effort */ }
  }
}

function extractText(pdfBuffer) {
  const start = Date.now();
  const work = fs.mkdtempSync(path.join(os.tmpdir(), `ocr-${crypto.randomBytes(4).toString('hex')}-`));
  const pdfPath = path.join(work, 'in.pdf');
  fs.writeFileSync(pdfPath, pdfBuffer);

  const diagnostics = {};
  const attempts = [];

  // Helper to record an attempt for diagnostics
  const recordAttempt = (pass, method, result) => {
    attempts.push({ pass, method, ...result });
  };

  try {
    // ===== Pass 1: pdftotext (text-based PDFs) =====
    {
      const r = sh(`pdftotext "${pdfPath}" -`);
      const txt = r.stdout;
      const txtTrim = txt.replace(/\s+/g, '').trim();
      recordAttempt(1, 'pdftotext', { status: r.status, bytes: txt.length, has_text: txtTrim.length > 0 });
      if (txtTrim.length > 0) {
        const numpagesR = sh(`pdfinfo "${pdfPath}" | awk '/^Pages:/{print $2}'`);
        const numpages = parseInt(numpagesR.stdout.trim(), 10) || 1;
        return {
          ok: true, text: txt, numpages, method: 'pdftotext', error_code: 'pdftotext_ok', error_message: '',
          attempts, elapsed_ms: Date.now() - start,
        };
      }
    }

    // ===== Pass 2: qpdf repair + pdftotext (structural corruption) =====
    {
      const repairedPath = path.join(work, 'repaired.pdf');
      const qpdfR = sh(`qpdf --linearize "${pdfPath}" "${repairedPath}" 2>&1`);
      recordAttempt(2, 'qpdf_repair', { status: qpdfR.status, stderr_excerpt: qpdfR.stderr.slice(0, 200) });
      if (qpdfR.status === 0) {
        const r = sh(`pdftotext "${repairedPath}" -`);
        const txtTrim = r.stdout.replace(/\s+/g, '').trim();
        if (txtTrim.length > 0) {
          const numpagesR = sh(`pdfinfo "${repairedPath}" | awk '/^Pages:/{print $2}'`);
          const numpages = parseInt(numpagesR.stdout.trim(), 10) || 1;
          return {
            ok: true, text: r.stdout, numpages, method: 'qpdf_repair', error_code: 'qpdf_repair_ok', error_message: '',
            attempts, elapsed_ms: Date.now() - start,
          };
        }
      }
    }

    // ===== Pass 3: pdftoppm + blank check + Tesseract OCR (image-based PDFs) =====
    let blankCheck = null;
    {
      const pdftoppmR = sh(`pdftoppm -r 200 -png "${pdfPath}" "${path.join(work, 'p')}"`);
      const pngFiles = fs.readdirSync(work).filter(f => f.startsWith('p-') && f.endsWith('.png')).sort();
      recordAttempt(3, 'pdftoppm_tesseract', {
        pdftoppm_status: pdftoppmR.status,
        png_count: pngFiles.length,
        pdftoppm_errors: /Syntax Error/i.test(pdftoppmR.stderr),
      });

      if (pngFiles.length > 0) {
        blankCheck = analyzeBlankness(path.join(work, pngFiles[0]));
        if (blankCheck.isBlank === false) {
          let ocrText = '';
          for (const f of pngFiles) {
            const tR = sh(`tesseract "${path.join(work, f)}" - -l tha+eng 2>/dev/null`);
            ocrText += tR.stdout + '\n';
          }
          const trimmed = ocrText.replace(/\s+/g, '').trim();
          if (trimmed.length >= LOW_TEXT_THRESHOLD) {
            return {
              ok: true, text: ocrText, numpages: pngFiles.length, method: 'tesseract_ocr',
              error_code: 'tesseract_ok', error_message: '',
              attempts, elapsed_ms: Date.now() - start,
            };
          }
          if (trimmed.length > 0) {
            return {
              ok: true, text: ocrText, numpages: pngFiles.length, method: 'tesseract_ocr',
              error_code: 'tesseract_low_text',
              error_message: `OCR found only ${trimmed.length} chars (likely partial extraction)`,
              attempts, elapsed_ms: Date.now() - start,
            };
          }
        }
        // else: blank, will try Vision next
      }
    }

    // ===== Pass 4: macOS PDFKit + Vision (most robust, handles corrupt PDFs) =====
    const visionScript = path.join(os.homedir(), 'Desktop/Work/Contents/infra/scripts/pdf-ocr-vision.swift');
    if (fs.existsSync(visionScript) && process.platform === 'darwin') {
      const resultPath = path.join(work, 'vision-result.json');
      const swiftR = sh(`swift "${visionScript}" "${pdfPath}" "${resultPath}" 2>&1`);
      recordAttempt(4, 'pdfkit_vision', { swift_status: swiftR.status, stderr_excerpt: swiftR.stderr.slice(0, 200) });
      if (fs.existsSync(resultPath)) {
        try {
          const vr = JSON.parse(fs.readFileSync(resultPath, 'utf8'));
          const txt = (vr.text || '').toString();
          const txtTrim = txt.replace(/\s+/g, '').trim();
          if (txtTrim.length >= LOW_TEXT_THRESHOLD) {
            return {
              ok: true, text: txt, numpages: vr.page_count || 1, method: 'pdfkit_vision',
              error_code: 'vision_ok', error_message: '',
              attempts, elapsed_ms: Date.now() - start,
            };
          }
          if (txtTrim.length > 0) {
            return {
              ok: true, text: txt, numpages: vr.page_count || 1, method: 'pdfkit_vision',
              error_code: 'vision_low_text',
              error_message: `Vision OCR found only ${txtTrim.length} chars (likely partial extraction)`,
              attempts, elapsed_ms: Date.now() - start,
            };
          }
        } catch (e) { /* fall through */ }
      }
    }

    // ===== Pass 5: Ollama Vision LLM (last-resort OCR for scanned/image-only PDFs) =====
    // Re-uses PNGs from Pass 3 (200 dpi). Only attempted when pdftotext/qpdf/Tesseract/
    // macOS Vision all failed — this is the same qwen3.6 vision path the /vision endpoint
    // uses, so /ocr and /vision now produce equivalent results. Blank check gates the call
    // to avoid qwen3.6 hallucinating on truly blank pages.
    {
      const pngFiles = fs.readdirSync(work).filter(f => f.startsWith('p-') && f.endsWith('.png')).sort();
      if (pngFiles.length > 0) {
        let blankCount = 0;
        for (const f of pngFiles) {
          const blank = analyzeBlankness(path.join(work, f));
          if (blank.isBlank === true) blankCount++;
        }
        if (blankCount === pngFiles.length) {
          recordAttempt(5, 'ollama_vision', { status: 'skipped', reason: 'all_pages_blank' });
        } else {
          const pageTexts = [];
          let totalEval = 0;
          let totalDurationMs = 0;
          for (const f of pngFiles) {
            const imgBuf = fs.readFileSync(path.join(work, f));
            const body = JSON.stringify({
              model: VISION_MODEL,
              prompt: VISION_PROMPT,
              images: [imgBuf.toString('base64')],
              stream: false,
              options: { temperature: 0.1, num_predict: 2048 },
            });
            const r = sh(
              `curl -sS --max-time ${Math.floor(VISION_TIMEOUT_MS / 1000)} ` +
              `-X POST "${OLLAMA_URL}" -H 'Content-Type: application/json' ` +
              `--data-binary @-`,
              { input: body, timeout: VISION_TIMEOUT_MS + 5000 }
            );
            if (r.status === 0 && r.stdout.trim()) {
              try {
                const resp = JSON.parse(r.stdout);
                const t = (resp.response || '').toString().trim();
                if (t) pageTexts.push(t);
                totalEval += resp.eval_count || 0;
                totalDurationMs += resp.total_duration ? Math.round(resp.total_duration / 1e6) : 0;
              } catch (_) { /* skip parse error */ }
            }
          }
          recordAttempt(5, 'ollama_vision', {
            status: 'tried', eval_count: totalEval, duration_ms: totalDurationMs,
            pages_attempted: pngFiles.length, pages_with_text: pageTexts.length,
          });
          const ocrText = pageTexts.join('\n\n');
          const trimmed = ocrText.replace(/\s+/g, '').trim();
          if (trimmed.length >= LOW_TEXT_THRESHOLD) {
            return {
              ok: true, text: ocrText, numpages: pngFiles.length, method: 'ollama_vision',
              error_code: 'ollama_vision_ok', error_message: '',
              attempts, elapsed_ms: Date.now() - start,
            };
          }
          if (trimmed.length > 0) {
            return {
              ok: true, text: ocrText, numpages: pngFiles.length, method: 'ollama_vision',
              error_code: 'ollama_vision_low_text',
              error_message: `Ollama vision returned only ${trimmed.length} chars (likely partial extraction or hallucination)`,
              attempts, elapsed_ms: Date.now() - start,
            };
          }
        }
      }
    }

    // ===== All passes failed — file is unrecoverable =====
    const numpagesR = sh(`pdfinfo "${pdfPath}" 2>/dev/null | awk '/^Pages:/{print $2}'`);
    const detectedPages = parseInt(numpagesR.stdout.trim(), 10) || 0;
    const blankSummary = blankCheck?.isBlank === true
      ? ` All renderers produced blank pages (${(blankCheck.darkRatio * 100).toFixed(2)}% dark, mean ${blankCheck.meanLuminance?.toFixed(0)}/255).`
      : '';
    return {
      ok: false, text: '', numpages: detectedPages, method: 'pdf_unrecoverable',
      error_code: 'pdf_unrecoverable',
      error_message: `PDF content unrecoverable: pdftotext, qpdf, Tesseract, macOS Vision, and Ollama Vision all failed.${blankSummary} Likely corrupt zlib streams. Re-export via Mac Preview (File → Export as PDF) to recover.`,
      attempts, elapsed_ms: Date.now() - start,
    };
  } finally {
    try { fs.rmSync(work, { recursive: true, force: true }); } catch (_) { /* best effort */ }
  }
}

function okResponse(res, payload) {
  res.writeHead(200, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify(payload));
}
function errResponse(res, code, status, error_code, error_message) {
  res.writeHead(status, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify({
    ok: false,
    text: '',
    numpages: 0,
    method: error_code,
    error_code,
    error_message,
    elapsed_ms: 0,
  }));
}

const server = http.createServer((req, res) => {
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'POST, GET, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', 'Content-Type');

  if (req.method === 'OPTIONS') { res.writeHead(204); return res.end(); }

  if (req.method === 'GET' && req.url === '/health') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    return res.end(JSON.stringify({ ok: true, service: 'ocr', port: PORT, pid: process.pid }));
  }

  if (req.method === 'POST' && (req.url === '/ocr' || req.url.startsWith('/ocr?'))) {
    let body = '';
    let aborted = false;
    req.on('data', chunk => {
      body += chunk.toString('utf8');
      if (body.length > MAX_PDF_BYTES * 2) {
        aborted = true;
        return errResponse(res, null, 413, 'request_too_large', `request body > ${MAX_PDF_BYTES * 2} bytes`);
      }
    });
    req.on('end', () => {
      if (aborted) { req.destroy(); return; }
      let payload;
      try { payload = JSON.parse(body); } catch (_) {
        return errResponse(res, null, 400, 'invalid_json', 'body is not valid JSON');
      }
      const b64 = payload.pdf_b64 || payload.file_data_b64;
      if (!b64 || typeof b64 !== 'string') {
        return errResponse(res, null, 400, 'missing_pdf_b64', 'request body missing pdf_b64 field');
      }
      const pdfBuffer = decodeBase64(b64);
      if (pdfBuffer.length === 0) {
        return errResponse(res, null, 400, 'invalid_pdf_size', 'decoded PDF is empty (0 bytes)');
      }
      if (pdfBuffer.length > MAX_PDF_BYTES) {
        return errResponse(res, null, 400, 'invalid_pdf_size', `decoded PDF is ${pdfBuffer.length} bytes (cap ${MAX_PDF_BYTES})`);
      }
      if (pdfBuffer.slice(0, 4).toString('ascii') !== '%PDF') {
        return errResponse(res, null, 400, 'not_a_pdf', `first 4 bytes are ${pdfBuffer.slice(0, 4).toString('hex')} (expected 25504446 = %PDF)`);
      }
      try {
        return okResponse(res, extractText(pdfBuffer));
      } catch (e) {
        return errResponse(res, null, 500, 'ocr_failed', `internal error: ${e && e.message || e}`);
      }
    });
    return;
  }

  if (req.method === 'POST' && (req.url === '/vision' || req.url.startsWith('/vision?'))) {
    let body = '';
    let aborted = false;
    req.on('data', chunk => {
      body += chunk.toString('utf8');
      // b64 expands ~33%; cap same as PDF + headroom for JSON envelope
      if (body.length > MAX_PDF_BYTES * 2) {
        aborted = true;
        return errResponse(res, null, 413, 'request_too_large', `request body > ${MAX_PDF_BYTES * 2} bytes`);
      }
    });
    req.on('end', () => {
      if (aborted) { req.destroy(); return; }
      let payload;
      try { payload = JSON.parse(body); } catch (_) {
        return errResponse(res, null, 400, 'invalid_json', 'body is not valid JSON');
      }
      const b64 = payload.file_data_b64 || payload.pdf_b64;
      const mime = payload.file_mime || '';
      const fname = payload.file_name || 'upload';
      if (!b64 || typeof b64 !== 'string') {
        return errResponse(res, null, 400, 'missing_file_data_b64', 'request body missing file_data_b64');
      }
      const fileBuffer = decodeBase64(b64);
      if (fileBuffer.length === 0) {
        return errResponse(res, null, 400, 'invalid_file_size', 'decoded file is empty (0 bytes)');
      }
      if (fileBuffer.length > MAX_PDF_BYTES) {
        return errResponse(res, null, 400, 'invalid_file_size', `decoded file is ${fileBuffer.length} bytes (cap ${MAX_PDF_BYTES})`);
      }
      try {
        return okResponse(res, extractViaOllamaVision(fileBuffer, mime, fname));
      } catch (e) {
        return errResponse(res, null, 500, 'vision_failed', `internal error: ${e && e.message || e}`);
      }
    });
    return;
  }

  errResponse(res, null, 404, 'not_found', `path ${req.url} not found (try /health, POST /ocr, POST /vision)`);
});

server.listen(PORT, HOST, () => {
  console.log(`[ocr-service] listening on http://${HOST}:${PORT}  pid=${process.pid}  ${new Date().toISOString()}`);
});

function shutdown(sig) {
  console.log(`[ocr-service] received ${sig}, shutting down`);
  server.close(() => process.exit(0));
  setTimeout(() => process.exit(1), 5000).unref();
}
process.on('SIGTERM', () => shutdown('SIGTERM'));
process.on('SIGINT', () => shutdown('SIGINT'));
