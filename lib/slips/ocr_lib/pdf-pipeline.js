// Per-page waterfall OCR pipeline.
//
// For each PDF page:
//   Pass 1: Vision LLM (raw text)      — primary path
//   Pass 2: Tesseract OCR (tha+eng)    — fallback
//   Pass 3: Swift N-API Vision OCR     — fallback (skipped if addon not built)
//   Pass 4: Vision LLM (structured map) — final fallback
//
// Document-level (after all pages attempted):
//   Pass 5: pdftotext on the original PDF — demoted last resort
//
// Returns the same top-level shape as today's infra/scripts/ocr-service.js
// (so Law-PoC n8n flows keep working) plus a new `pages` array describing
// per-page outcomes.

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { execSync, spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';

import { analyzeBlankness, allPagesBlank } from './blankness.js';
import { RAW_TEXT_PROMPT, STRUCTURED_MAP_SYSTEM_PROMPT, STRUCTURED_MAP_USER_PROMPT } from './prompts.js';

const require = createRequire(import.meta.url);
const visionBridge = require('../../native/vision-ocr/index.js');

const LOW_TEXT_THRESHOLD = parseInt(process.env.OCR_LOW_TEXT_THRESHOLD || '20', 10);
const VISION_TIMEOUT_MS  = parseInt(process.env.OCR_VISION_TIMEOUT_MS  || '60000', 10);
const OLLAMA_URL         = (process.env.OLLAMA_URL || 'http://127.0.0.1:11434') + '/api/generate';
const VISION_MODEL       = process.env.OCR_VISION_MODEL || 'qwen3-vl:4b';

const PASS_LLM_RAW           = process.env.OCR_PASS_LLM_RAW_ENABLED           !== '0';
const PASS_TESSERACT         = process.env.OCR_PASS_TESSERACT_ENABLED         !== '0';
const PASS_NATIVE_VISION     = process.env.OCR_PASS_NATIVE_VISION_ENABLED     !== '0';
const PASS_LLM_MAP           = process.env.OCR_PASS_LLM_MAP_ENABLED           !== '0';
const PASS_PDFTEXT_DEMOTED   = process.env.OCR_PASS_PDFTEXT_DEMOTED_ENABLED   !== '0';

function sh(cmd, opts = {}) {
  const r = spawnSync('bash', ['-c', cmd], {
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    ...opts,
  });
  return { stdout: r.stdout || '', stderr: r.stderr || '', status: r.status };
}

function callOllama({ prompt, system, images, numPredict = 2048 }) {
  const body = JSON.stringify({
    model: VISION_MODEL,
    prompt,
    system: system || undefined,
    images: images || undefined,
    stream: false,
    options: { temperature: 0.1, num_predict: numPredict },
  });
  const r = sh(
    `curl -sS --max-time ${Math.floor(VISION_TIMEOUT_MS / 1000)} ` +
    `-X POST "${OLLAMA_URL}" -H 'Content-Type: application/json' ` +
    `--data-binary @-`,
    { input: body, timeout: VISION_TIMEOUT_MS + 5000 }
  );
  if (r.status !== 0 || !r.stdout.trim()) {
    return { ok: false, error: `status=${r.status} stderr=${r.stderr.slice(0, 200)}` };
  }
  try {
    const j = JSON.parse(r.stdout);
    return {
      ok: true,
      text: String(j.response || '').trim(),
      evalCount: j.eval_count || 0,
      durationMs: j.total_duration ? Math.round(j.total_duration / 1e6) : 0,
    };
  } catch (e) {
    return { ok: false, error: 'json_parse' };
  }
}

async function pass1VisionLlmRaw(pngPath) {
  if (!PASS_LLM_RAW) return { ok: false, skip: 'pass_disabled' };
  const pngB64 = fs.readFileSync(pngPath).toString('base64');
  const r = callOllama({ prompt: RAW_TEXT_PROMPT, images: [pngB64] });
  if (!r.ok) return { ok: false, error: r.error };
  return { ok: true, text: r.text, evalCount: r.evalCount, durationMs: r.durationMs };
}

async function pass2Tesseract(pngPath) {
  if (!PASS_TESSERACT) return { ok: false, skip: 'pass_disabled' };
  const r = sh(`tesseract "${pngPath}" - -l tha+eng 2>/dev/null`);
  if (r.status !== 0) return { ok: false, error: `status=${r.status}` };
  return { ok: true, text: r.stdout };
}

async function pass3NativeVision(pngPath) {
  if (!PASS_NATIVE_VISION) return { ok: false, skip: 'pass_disabled' };
  if (!visionBridge.ocrAvailable()) return { ok: false, skip: 'addon_unavailable' };
  try {
    const sharp = (await import('sharp')).default;
    const { data, info } = await sharp(pngPath)
      .ensureAlpha()
      .raw()
      .toBuffer({ resolveWithObject: true });
    const out = await visionBridge.ocrPixels({
      pixels: data,
      width: info.width,
      height: info.height,
    });
    if (!out.ok) return { ok: false, error: out.error_code || 'vision_failed' };
    return { ok: true, text: out.lines.join('\n') };
  } catch (e) {
    return { ok: false, error: String(e?.message || e) };
  }
}

async function pass4VisionLlmMap(pngPath, mime) {
  if (!PASS_LLM_MAP) return { ok: false, skip: 'pass_disabled' };
  const pngB64 = fs.readFileSync(pngPath).toString('base64');
  const r = callOllama({
    system: STRUCTURED_MAP_SYSTEM_PROMPT,
    prompt: STRUCTURED_MAP_USER_PROMPT(mime || 'image/png'),
    images: [pngB64],
  });
  if (!r.ok) return { ok: false, error: r.error };
  // Try to parse the JSON map; fall back to raw text on parse failure.
  let structured = null;
  try {
    structured = JSON.parse(r.text);
  } catch {
    const m = r.text.match(/\{[\s\S]*\}/);
    if (m) {
      try { structured = JSON.parse(m[0]); } catch { structured = null; }
    }
  }
  if (structured) {
    // Convert structured → flat text so downstream RAG still has something to vectorize.
    const flat = [
      structured.title ? `TITLE: ${structured.title}` : null,
      structured.date ? `DATE: ${structured.date}` : null,
      Array.isArray(structured.parties) && structured.parties.length
        ? `PARTIES: ${structured.parties.join('; ')}` : null,
      Array.isArray(structured.sections) && structured.sections.length
        ? structured.sections.map(s => `${s.heading || ''}\n${s.content || ''}`).join('\n\n')
        : null,
    ].filter(Boolean).join('\n\n');
    return { ok: true, structured, text: flat || r.text };
  }
  return { ok: true, structured: null, text: r.text };
}

async function runPage({ index, pngPath, mime, blank, attempts }) {
  const start = Date.now();
  const rec = (pass, result) => attempts.push({ page: index, pass, ...result });

  if (blank?.isBlank === true) {
    rec(0, { method: 'blank_skip', chars: 0, ms: 0 });
    return {
      page: index, ok: false, method: 'unrecoverable', chars: 0, text: '',
      error_code: 'blank_page', error_message: 'page is >99% white',
    };
  }

  // Pass 1 — Vision LLM raw
  const p1 = await pass1VisionLlmRaw(pngPath);
  rec(1, { method: 'vision_llm_raw', ok: p1.ok, chars: p1.text?.length || 0, ms: Date.now() - start });
  if (p1.ok && (p1.text || '').length >= LOW_TEXT_THRESHOLD) {
    return { page: index, ok: true, method: 'vision_llm_raw', chars: p1.text.length, text: p1.text };
  }

  // Pass 2 — Tesseract
  const p2 = await pass2Tesseract(pngPath);
  rec(2, { method: 'tesseract', ok: p2.ok, chars: p2.text?.length || 0, ms: Date.now() - start });
  if (p2.ok && (p2.text || '').length >= LOW_TEXT_THRESHOLD) {
    return { page: index, ok: true, method: 'tesseract', chars: p2.text.length, text: p2.text };
  }

  // Pass 3 — Swift N-API Vision
  const p3 = await pass3NativeVision(pngPath);
  rec(3, { method: 'vision_native', ok: p3.ok, chars: p3.text?.length || 0, ms: Date.now() - start });
  if (p3.ok && (p3.text || '').length >= LOW_TEXT_THRESHOLD) {
    return { page: index, ok: true, method: 'vision_native', chars: p3.text.length, text: p3.text };
  }

  // Pass 4 — Vision LLM structured map
  const p4 = await pass4VisionLlmMap(pngPath, mime);
  rec(4, { method: 'vision_llm_map', ok: p4.ok, chars: p4.text?.length || 0, ms: Date.now() - start });
  if (p4.ok) {
    return {
      page: index, ok: true, method: 'vision_llm_map',
      chars: (p4.text || '').length, text: p4.text,
      structured: p4.structured,
    };
  }

  return {
    page: index, ok: false, method: 'unrecoverable', chars: 0, text: '',
    error_code: 'all_passes_exhausted',
    error_message: 'Pass 1 (vision LLM), 2 (tesseract), 3 (native vision), 4 (LLM map) all failed or returned too little text',
  };
}

async function pass5PdftextDemoted(pdfPath) {
  if (!PASS_PDFTEXT_DEMOTED) return null;
  const r = sh(`pdftotext "${pdfPath}" -`);
  if (r.status !== 0) return null;
  const trimmed = (r.stdout || '').replace(/\s+/g, '').trim();
  if (trimmed.length === 0) return null;
  const np = sh(`pdfinfo "${pdfPath}" | awk '/^Pages:/{print $2}'`);
  return { text: r.stdout, numpages: parseInt(np.stdout.trim(), 10) || 0 };
}

export async function extractText(pdfBuffer) {
  const start = Date.now();
  const work = fs.mkdtempSync(path.join(os.tmpdir(), `ocr-${crypto.randomBytes(4).toString('hex')}-`));
  const pdfPath = path.join(work, 'in.pdf');
  fs.writeFileSync(pdfPath, pdfBuffer);

  const attempts = [];
  const pages = [];

  try {
    // Render all pages to PNG via pdftoppm @ 200 dpi.
    const pdftoppmR = sh(`pdftoppm -r 200 -png "${pdfPath}" "${path.join(work, 'p')}"`);
    if (pdftoppmR.status !== 0) {
      return {
        ok: false, text: '', numpages: 0, method: 'pdf_unrecoverable',
        error_code: 'pdf_render_failed',
        error_message: `pdftoppm failed: ${pdftoppmR.stderr.slice(0, 200)}`,
        attempts, elapsed_ms: Date.now() - start,
      };
    }

    const pngFiles = fs.readdirSync(work)
      .filter(f => f.startsWith('p-') && f.endsWith('.png'))
      .sort();

    if (pngFiles.length === 0) {
      return {
        ok: false, text: '', numpages: 0, method: 'pdf_unrecoverable',
        error_code: 'pdf_render_empty',
        error_message: 'pdftoppm produced no PNGs',
        attempts, elapsed_ms: Date.now() - start,
      };
    }

    // Sharp blankness check per page.
    const blankChecks = await Promise.all(
      pngFiles.map(f => analyzeBlankness(path.join(work, f)))
    );
    const allBlank = blankChecks.every(c => c.isBlank === true);

    if (allBlank) {
      return {
        ok: false, text: '', numpages: pngFiles.length,
        method: 'blank_pdf',
        error_code: 'blank_pdf',
        error_message: `All ${pngFiles.length} rendered page(s) are blank (>99% white). Likely corrupt zlib streams or no decodable content. Re-export via Mac Preview (File → Export as PDF) to recover.`,
        attempts, elapsed_ms: Date.now() - start,
      };
    }

    // Per-page waterfall.
    for (let i = 0; i < pngFiles.length; i++) {
      const result = await runPage({
        index: i + 1,
        pngPath: path.join(work, pngFiles[i]),
        mime: 'image/png',
        blank: blankChecks[i],
        attempts,
      });
      pages.push(result);
    }

    // Pass 5 (demoted): if any page is unrecoverable, try pdftotext and patch it in.
    const unrecovered = pages.filter(p => !p.ok);
    let pdftotextSupplement = null;
    if (unrecovered.length > 0) {
      pdftotextSupplement = await pass5PdftextDemoted(pdfPath);
      if (pdftotextSupplement && pdftotextSupplement.text) {
        attempts.push({
          pass: 5, method: 'pdftotext_demoted',
          status: 'tried',
          supplemented_pages: unrecovered.length,
          chars: pdftotextSupplement.text.length,
        });
      }
    }

    // Aggregate.
    const fullText = pages.map((p, i) => {
      const head = `[Page ${p.page}]\n`;
      let body = p.text || '';
      // For pages that came from structured map, include the JSON too for downstream consumers.
      if (p.structured) {
        body += '\n' + JSON.stringify(p.structured);
      }
      // If page failed and we have pdftotext text, append it as a tail note.
      if (!p.ok && pdftotextSupplement?.text) {
        body += `\n[pdftotext_demoted_note: see document-level text]`;
      }
      return head + body;
    }).join('\n\n');

    const recoveredCount = pages.filter(p => p.ok).length;
    const dominantMethod = (() => {
      const counts = {};
      for (const p of pages) {
        if (p.ok) counts[p.method] = (counts[p.method] || 0) + 1;
      }
      const sorted = Object.entries(counts).sort((a, b) => b[1] - a[1]);
      return sorted[0]?.[0] || 'pdf_unrecoverable';
    })();

    const result = {
      ok: recoveredCount > 0,
      text: fullText,
      numpages: pages.length,
      method: dominantMethod,
      error_code: recoveredCount === pages.length ? 'pdf_ok' :
                  recoveredCount === 0 ? 'pdf_unrecoverable' : 'pdf_partial',
      error_message: recoveredCount === pages.length ? '' :
        `${pages.length - recoveredCount}/${pages.length} page(s) unrecoverable`,
      pages,
      attempts,
      elapsed_ms: Date.now() - start,
    };

    if (pdftotextSupplement) {
      result.pdftotext_demoted_text = pdftotextSupplement.text;
    }
    return result;
  } finally {
    try { fs.rmSync(work, { recursive: true, force: true }); } catch { /* best effort */ }
  }
}

// ─────────────────────────────────────────────────────────────────────────
// Single-file vision LLM path (used by /vision endpoint and /slips when the
// upload is an image). Mirrors today's behavior in
// infra/scripts/ocr-service.js:extractViaOllamaVision.
// ─────────────────────────────────────────────────────────────────────────

export async function extractViaOllamaVision(fileBuffer, mime, fname) {
  const start = Date.now();
  const ext = (fname || '').toLowerCase().split('.').pop() || '';
  const isPdf = ext === 'pdf' || mime === 'application/pdf';
  const isImage = ['jpg', 'jpeg', 'png', 'webp', 'gif', 'bmp'].includes(ext) || (mime && mime.startsWith('image/'));
  const isText = ['txt', 'md', 'csv', 'log'].includes(ext) || (mime && mime.startsWith('text/'));

  if (isText) {
    try {
      const text = fileBuffer.toString('utf-8');
      return {
        ok: true, text,
        chars: text.length, pages: 0, page_images: [],
        source: 'text-direct',
        vision_model: VISION_MODEL,
        vision_eval_count: 0, vision_duration_ms: 0,
        error_code: 'text_ok', error_message: '',
        elapsed_ms: Date.now() - start,
      };
    } catch (e) {
      return {
        ok: false, text: '', chars: 0, pages: 0, page_images: [],
        source: 'text-fail',
        vision_model: VISION_MODEL,
        vision_eval_count: 0, vision_duration_ms: 0,
        error_code: 'text_decode_failed', error_message: e.message,
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

  const work = fs.mkdtempSync(path.join(os.tmpdir(), `vision-${crypto.randomBytes(4).toString('hex')}-`));
  let imageBuffers = [];
  let imageMimes = [];
  let source = 'unknown';
  let pageCount = 0;
  let totalEval = 0;
  let totalDurationMs = 0;

  try {
    if (isImage) {
      imageBuffers = [fileBuffer];
      imageMimes = [mime || 'image/jpeg'];
      source = 'vision-image';
      pageCount = 1;
    } else {
      const pdfPath = path.join(work, 'in.pdf');
      fs.writeFileSync(pdfPath, fileBuffer);

      // Fast-path: text-based PDFs.
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
          error_code: 'pdftotext_ok', error_message: '',
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
      let blankCount = 0;
      for (const f of jpgFiles) {
        const blank = await analyzeBlankness(path.join(work, f));
        if (blank.isBlank === true) blankCount++;
      }
      if (blankCount === jpgFiles.length) {
        return {
          ok: false, text: '', chars: 0, pages: jpgFiles.length, page_images: [],
          source: 'blank-pdf',
          vision_model: VISION_MODEL,
          vision_eval_count: 0, vision_duration_ms: 0,
          error_code: 'blank_pdf',
          error_message: `All ${jpgFiles.length} rendered page(s) are blank.`,
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

    const pageImages = imageBuffers.map((buf, i) => ({
      page_index: i,
      image_b64:  buf.toString('base64'),
      bytes:      buf.length,
      mime:       imageMimes[i] || 'image/jpeg',
    }));

    const pageTexts = [];
    for (let i = 0; i < imageBuffers.length; i++) {
      const b64 = imageBuffers[i].toString('base64');
      const r = callOllama({ prompt: RAW_TEXT_PROMPT, images: [b64] });
      if (!r.ok) {
        pageTexts.push(`[page ${i+1} vision failed: ${r.error}]`);
        continue;
      }
      if (r.text) pageTexts.push(r.text);
      totalEval += r.evalCount;
      totalDurationMs += r.durationMs;
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
      error_code: 'vision_ok', error_message: '',
      elapsed_ms: Date.now() - start,
    };
  } finally {
    try { fs.rmSync(work, { recursive: true, force: true }); } catch { /* best effort */ }
  }
}