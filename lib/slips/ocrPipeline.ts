// Shared OCR pipeline used by both /api/slips and /api/upload.
// Keeps the SYSTEM_PROMPT, safeParse, and confidenceScore helpers in one
// place so the two endpoints don't drift.
//
// Pipeline:
//   1. Sharp normalize (upscale to 3072px + sharpen for OCR)
//   2. Vision LLM call with strict anti-hallucination prompt
//   3. safeParse JSON
//   4. validateReceipt / validateBookBank — catches invented vendor names,
//      broken math, out-of-range dates, fabricated items[], etc.
//   5. If validation fails AND the issues are retryable, run ONCE more with a
//      stricter prompt that lists the specific errors. Do not retry when the
//      LLM returned total garbage (no vendor, no date) — retry can't recover.
//   6. Stamp isCorrupted=true + populate correctionNotes when validation
//      still fails so downstream UI badges the slip as needing review.

import sharp from 'sharp';
import { createRequire } from 'node:module';
import { invoke } from '@/ai/router';
import { getVisionChain } from '../ai/visionChain';
import {
  validateReceipt,
  validateBookBank,
  issuesToText,
  isRetryableValidation,
  type ValidationResult,
} from './validate';

async function normalizeForOllamaVision(buffer: Buffer, mime: string): Promise<{ buffer: Buffer; mime: string }> {
  const m = (mime || '').toLowerCase();
  // Ollama's qwen3-vl rejects WEBP/GIF/BMP/HEIC with "Failed to load image".
  // Always transcode through sharp so vision OCR works regardless of upload format.
  // Upscale to 3072px longest edge + sharpening — improves OCR on small Thai print + handwriting.
  try {
    const png = await sharp(buffer)
      .resize({ width: 3072, height: 3072, fit: 'inside', withoutEnlargement: false })
      .sharpen({ sigma: 1.2, m1: 0.7, m2: 1.5 })
      .png({ compressionLevel: 4 })
      .toBuffer();
    return { buffer: png, mime: 'image/png' };
  } catch {
    // If sharp can't decode (unsupported codec), pass through with normalized mime.
    return { buffer, mime: m === 'image/jpg' ? 'image/jpeg' : m };
  }
}

export const OCR_SYSTEM_PROMPT = `You extract structured fields from Thai receipts (ใบเสร็จรับเงิน, ใบกำกับภาษี, tax invoices). Vendor name (created by), customer name (created to), transaction date, subtotal, VAT, and total must come from the printed numbers/names on the receipt — never guess. If a label is illegible, set its value to "" or 0 and add a brief note.

Critical Identification Rules:
- The vendorName (created by) is the entity who sold the goods/services (the seller/issuer/provider). Look for labels like "ผู้ออก" (Issuer), "ผู้ขาย" (Seller), "ผู้ให้บริการ" (Service Provider), or the company name listed at the issuer block (usually accompanied by their tax ID "เลขประจำตัวผู้เสียภาษี" / "เลขผู้เสียภาษี" of the seller).
- The createdTo (created to) is the customer/buyer/recipient who is receiving the receipt, typically labeled as "ชื่อลูกค้า" (Customer Name), "ลูกค้า" (Customer), "ผู้ซื้อ" (Buyer), "ผู้รับบริการ" (Recipient), "ส่งถึง" (Ship to), "จ่ายให้" (Bill to). If a name is explicitly labeled as customer/buyer, it MUST be extracted as createdTo.

Anti-hallucination & Thai Orthography Rules (MUST follow):
- Do NOT invent fields you cannot read. If a number or word is illegible, return "" (for strings) or 0 (for numbers) — never guess.
- Do NOT fabricate a vendor name or customer name when the receipt header is unreadable. Return vendorName="" or createdTo="" and set isCorrupted=true.
- Do NOT silently fix math. If the receipt's own subtotal+VAT does not equal total, set isCorrupted=true and put the discrepancy in correctionNotes — leave the numbers as printed.
- Do NOT fill items[] with plausible-but-fabricated rows. Return only the line items you can read.
- Do NOT default paymentMethod to "cash" if you can't tell. Check the options/checkboxes on the receipt (like "เงินสด", "บัตรเดบิต / บัตรเครดิต", "โอนผ่านบัญชี"). Only set paymentMethod if a checkbox is checked. If all options are unchecked (empty circles/boxes), return paymentMethod = "".
- Pay close attention to Thai tone marks (ไม้เอก ่, ไม้โท ้, ไม้ตรี ๊, ไม้จัตวา ๋) and vowels above/below the consonants (ิ, ี, ึ, ื, ุ, ู). Do not drop them. For example, distinguish "เสื้อ" (t-shirt/clothes, with ไม้โท ้) from "เสือ" (tiger, no tone mark).
- Use the provided raw OCR text below (if present) to cross-reference and verify all extracted spelling and names (e.g. vendorName, createdTo, items).
- If the image is not a receipt (e.g. a random photo), return vendorName="" with isCorrupted=true and correctionNotes="not a receipt".
- Output ONLY the JSON object — no markdown fences, no analysis, no thinking, no commentary. The JSON must be syntactically valid and parseable. Reply with raw JSON on a single line at the very start of your response.

Date rules:
- Convert every Thai date format to Christian-era YYYY-MM-DD.
- Buddhist year (พ.ศ.) = CE year + 543 (or two-digit BE = 2500 + YY).
- Thai month abbreviations: ม.ค. ก.พ. มี.ค. เม.ย. พ.ค. มิ.ย. ก.ค. ส.ค. ก.ย. ต.ค. พ.ย. ธ.ค.

Number rules:
- "," is a thousands separator. "." is a decimal point. "12,500" = 12500; "12.50" = 12.50.
- Subtotal + VAT MUST equal Total (within 0.01). If the receipt's own numbers don't balance, set isCorrupted=true and explain in correctionNotes — do NOT silently correct.
- Thai field labels: ยอดรวม/รวม=moneySubTotal, ภาษี/ภาษีมูลค่าเพิ่ม/VAT=vatAmount, รวมทั้งสิ้น/ยอดสุทธิ=totalAmount.
- paymentMethod: default "cash" (or "เงินสด", "CASH SALE" → "cash"). "บัตรเครดิต"→"credit_card", "โอน"/"เงินโอน"→"transfer".

Items:
- Each line item = {description, qty, unitPrice, amount}. amount is the line total (which should equal qty × unitPrice). If qty or unitPrice is not printed or unreadable, default qty to 1 and unitPrice to amount.
- Keep the original script (Thai or English). Do not translate.

Schema (return exactly these keys, in this order):
{"vendorName":"string","vendorAddress":"string","createdTo":"string","createdToAddress":"string","transactionDate":"YYYY-MM-DD","subtotal":number,"vatAmount":number,"totalAmount":number,"paymentMethod":"cash|credit_card|transfer|","currency":"THB|USD|...","isCorrupted":boolean,"correctionNotes":"string","items":[{"description":"string","qty":number,"unitPrice":number,"amount":number}]}

Example 1 — Thai cash slip (no VAT):
{"vendorName":"ร้าน ก๋วยเตี๋ยวเรือใหญ่","vendorAddress":"123 ถนนสุขุมวิท กรุงเทพฯ","createdTo":"นายสมชาย รักดี","createdToAddress":"456 ถนนพหลโยธิน กรุงเทพฯ","transactionDate":"2025-02-27","subtotal":150,"vatAmount":0,"totalAmount":150,"paymentMethod":"cash","currency":"THB","isCorrupted":false,"correctionNotes":"","items":[{"description":"ก๋วยเตี๋ยวเรือ 1 ชาม","qty":1,"unitPrice":60,"amount":60},{"description":"น้ำตาลปี๊บ 1 ขวด","qty":1,"unitPrice":90,"amount":90}]}

Example 2 — Thai tax invoice with VAT (BE year):
{"vendorName":"Coffee Lab Co., Ltd.","vendorAddress":"789 Rama IX Rd, Bangkok","createdTo":"บริษัท บีมเอเจนซี่ แอนด์ ดิจิทัล จำกัด","createdToAddress":"101 Ladprao Rd, Bangkok","transactionDate":"2025-03-15","subtotal":1000,"vatAmount":70,"totalAmount":1070,"paymentMethod":"credit_card","currency":"THB","isCorrupted":false,"correctionNotes":"","items":[{"description":"Latte x2","qty":2,"unitPrice":100,"amount":200},{"description":"Croissant x2","qty":2,"unitPrice":400,"amount":800}]}`;

export const BANK_OCR_SYSTEM_PROMPT = `You are an expert at reading Thai bank passbook pages (สมุดบัญชีธนาคาร / bank book). The image usually shows a passbook cover or first account-detail page, often with the bank's logo, branch name, account number, and account holder name.

Anti-hallucination rules (MUST follow):
- Do NOT guess digits. If the account number is unreadable, return accountNumber="" and set isCorrupted=true.
- Do NOT invent a bank name when the logo/branding is not visible. Return bankName="" and set isCorrupted=true.
- Do NOT fabricate the account holder name. Return accountName="" and set isCorrupted=true.
- When the image is not a passbook detail page (e.g. only transaction rows, or a random photo), set isCorrupted=true and explain in correctionNotes.

Target fields:
  - bankName: issuing bank. Pick the closest match from the canonical list when possible:
      * "Krungthai" (ธนาคารกรุงไทย)
      * "SCB"        (Siam Commercial Bank / ธนาคารไทยพาณิชย์)
      * "Bangkok Bank" (ธนาคารกรุงเทพ)
      * "Kasikorn"    (ธนาคารกสิกรไทย)
      * "TMBThanachai" (ธนาคารทหารไทยธนชาต)
    Otherwise set bankName to the full bank name as printed (e.g. "UOB Thailand", "CIMB Thai").
  - bankBranch: branch as printed on the passbook, labelled "สาขา" or "BRANCH". Often a 3-4 digit branch code followed by the branch name on one line (e.g. "0080 สาขาฟิวเจอร์พาร์ค รังสิต"). Return code + name together. Empty string when not visible.
  - accountNumber: digits only, no dashes, no spaces. Strip Thai commas. 10–14 digits typical.
  - accountName: holder name exactly as printed (Thai or English, no translation). May include "นาย"/"นาง"/"นางสาว" prefixes.

If the image is too blurry or shows transaction rows only (no passbook detail page), set isCorrupted=true and put the reason in correctionNotes — do NOT guess digits.

Return ONLY JSON (no markdown, no commentary):
{
  "bankName": "string",
  "bankBranch": "string (empty when not visible)",
  "accountNumber": "string (digits only)",
  "accountName": "string",
  "isCorrupted": boolean,
  "correctionNotes": "string (empty when not corrupted)"
}

Example 1 — Krungthai passbook:
  Input: Green Krungthai cover (logo "ธนาคารกรุงไทย"). Holder page: สาขา "0001 สาขาสำนักงานใหญ่". ชื่อบัญชี "นางสาว กานดา พลังใหม่". เลขที่บัญชี "123-4-56789-0".
  Output: {"bankName":"Krungthai","bankBranch":"0001 สาขาสำนักงานใหญ่","accountNumber":"1234567890","accountName":"นางสาว กานดา พลังใหม่","isCorrupted":false,"correctionNotes":""}

Example 2 — Blurry transaction page (no passbook detail):
  Input: Out-of-focus photo of a passbook showing only debit/credit rows.
  Output: {"bankName":"","accountNumber":"","accountName":"","isCorrupted":true,"correctionNotes":"Image shows only transaction rows; no passbook account-detail page visible. Please photograph the holder page or cover."}`;

export function safeParse(s: string): Record<string, unknown> {
  try { return JSON.parse(s); }
  catch {
    const m = s.match(/\{[\s\S]*\}/);
    if (!m) throw new Error('Vision output was not JSON');
    return JSON.parse(m[0]);
  }
}

export function confidenceScore(parsed: Record<string, unknown>, validation?: ValidationResult | null): number {
  let score = 0;
  if (parsed.vendorName) score += 0.15;
  if (parsed.transactionDate) score += 0.15;
  if (Array.isArray(parsed.items) && (parsed.items as unknown[]).length > 0) score += 0.30;
  if (typeof parsed.totalAmount === 'number') score += 0.20;
  if (typeof parsed.subtotal === 'number' && typeof parsed.vatAmount === 'number') {
    const sum = +((parsed.subtotal as number) + (parsed.vatAmount as number)).toFixed(2);
    if (Math.abs(sum - +(parsed.totalAmount as number)) < 0.01) score += 0.20;
  }
  let adjusted = Math.min(1, score);
  if (validation) {
    if (!validation.ok) adjusted *= 0.3;
    else if (validation.warnings.length > 0) adjusted *= 0.85;
    if (parsed.isCorrupted === true) adjusted = Math.min(adjusted, 0.5);
  }
  return adjusted;
}

export function bankConfidenceScore(parsed: Record<string, unknown>, validation?: ValidationResult | null): number {
  let score = 0;
  if (parsed.accountNumber && /^\d{6,}$/.test(String(parsed.accountNumber).replace(/[^\d]/g, ''))) {
    score += 0.45;
  }
  if (parsed.accountName && String(parsed.accountName).trim().length > 0) score += 0.40;
  if (parsed.bankName && String(parsed.bankName).trim().length > 0) score += 0.15;
  let adjusted = Math.min(1, score);
  if (validation) {
    if (!validation.ok) adjusted *= 0.3;
    else if (validation.warnings.length > 0) adjusted *= 0.85;
    if (parsed.isCorrupted === true) adjusted = Math.min(adjusted, 0.5);
  }
  return adjusted;
}

export type OcrKind = 'receipt' | 'book_bank' | 'po_invoice';

export const PO_INVOICE_SYSTEM_PROMPT = `You extract structured fields from supplier invoices (ใบแจ้งหนี้ผู้ขาย / supplier invoice / vendor bill). Vendor name, invoice number, invoice date, due date, subtotal, VAT, and total must come from the printed numbers/names — never guess. If a label is illegible, set its value to "" or 0 and add a brief note.

Critical Anti-Hallucination Rules:
- Do NOT invent a vendor name when the supplier header is unreadable. Return vendorName="" and set isCorrupted=true.
- Do NOT silently fix math. If the invoice's own subtotal+VAT does not equal total, set isCorrupted=true.
- Do NOT fill items[] with plausible-but-fabricated rows. Return only the line items you can read.
- Output ONLY the JSON object — no markdown fences, no commentary, no thinking. Reply with raw JSON on a single line at the very start.

Date rules:
- Convert every Thai date format to Christian-era YYYY-MM-DD.
- Buddhist year (พ.ศ.) = CE year + 543 (or two-digit BE = 2500 + YY).

Number rules:
- "," is a thousands separator. "." is a decimal point.
- Subtotal + VAT MUST equal Total (within 0.01). If the invoice's own numbers don't balance, set isCorrupted=true.

Schema (return exactly these keys, in this order):
{"vendorName":"string","vendorAddress":"string","invoiceNo":"string","invoiceDate":"YYYY-MM-DD","dueDate":"YYYY-MM-DD","subtotal":number,"vatAmount":number,"totalAmount":number,"currency":"THB|USD|...","isCorrupted":boolean,"correctionNotes":"string","items":[{"description":"string","qty":number,"unitPrice":number,"amount":number}]}

Example — Thai supplier invoice:
{"vendorName":"บริษัท ซัพพลายเออร์ จำกัด","vendorAddress":"99/9 ถนนสุขุมวิท กรุงเทพฯ","invoiceNo":"INV-2026-0042","invoiceDate":"2026-06-15","dueDate":"2026-07-15","subtotal":10000,"vatAmount":700,"totalAmount":10700,"currency":"THB","isCorrupted":false,"correctionNotes":"","items":[{"description":"สินค้า A","qty":10,"unitPrice":500,"amount":5000},{"description":"สินค้า B","qty":5,"unitPrice":1000,"amount":5000}]}`;

export interface OcrPipelineOpts {
  modelName?: string;
  kind?: OcrKind;
  /** Skip the validation-gate retry pass even if first attempt fails validation. */
  skipRetryOnValidationFail?: boolean;
}

export interface OcrValidationInfo {
  ok: boolean;
  errors: Array<{ code: string; severity: 'error' | 'warning'; field?: string; message: string }>;
  warnings: Array<{ code: string; severity: 'error' | 'warning'; field?: string; message: string }>;
  retried: boolean;
  summary: string;
}

export interface OcrPipelineResult {
  parsed: Record<string, unknown>;
  mode: string;
  kind: OcrKind;
  validation: OcrValidationInfo;
}

const RETRY_ENABLED = process.env.OCR_RETRY_ON_VALIDATION_FAIL !== '0';

function buildRetrySystemPrompt(basePrompt: string, errors: string): string {
  return `${basePrompt}\n\n---\nRETRY: Your previous JSON had these validation issues:\n${errors}\n\nRe-examine the image carefully. For each issue above:\n- If the value is genuinely unreadable, return "" (strings) or 0 (numbers) — do NOT guess.\n- If the value is readable but you misread it, correct it.\n- Do not invent new fields. Do not silently fix math — set isCorrupted=true if the receipt's own numbers don't balance.`;
}

function stampValidationOnParsed(
  parsed: Record<string, unknown>,
  kind: OcrKind,
  validation: ValidationResult,
): void {
  if (validation.ok) return;
  const note = issuesToText(validation.errors);
  parsed.isCorrupted = true;
  if (kind === 'receipt') {
    const prev = typeof parsed.correctionNotes === 'string' ? parsed.correctionNotes : '';
    parsed.correctionNotes = prev ? `${prev} | validation: ${note}` : `validation: ${note}`;
  } else {
    const prev = typeof parsed.correctionNotes === 'string' ? parsed.correctionNotes : '';
    parsed.correctionNotes = prev ? `${prev} | validation: ${note}` : `validation: ${note}`;
  }
}

function validateForKind(parsed: Record<string, unknown>, kind: OcrKind): ValidationResult {
  if (kind === 'book_bank') return validateBookBank(parsed);
  if (kind === 'po_invoice') return validatePoInvoice(parsed);
  return validateReceipt(parsed);
}

function validatePoInvoice(parsed: Record<string, unknown>): ValidationResult {
  const mapped: Record<string, unknown> = {
    ...parsed,
    vendorName: parsed.vendorName,
    transactionDate: parsed.invoiceDate,
    subtotal: parsed.subtotal,
    vatAmount: parsed.vatAmount,
    totalAmount: parsed.totalAmount,
    items: parsed.items,
  };
  return validateReceipt(mapped);
}

function makeText(kind: OcrKind, isRetry: boolean, buffer: Buffer, mime: string, ocrText?: string): string {
  if (mime.startsWith('image/')) {
    const refText = ocrText
      ? `\n\nUse the raw OCR text extracted below from the image to help you transcribe and verify all names, items, and values. Trust the OCR text spelling for Thai characters and tone marks:\n\n<raw_ocr_text>\n${ocrText}\n</raw_ocr_text>`
      : '';

    if (kind === 'book_bank') {
      return isRetry
        ? `Re-extract the structured JSON for this bank passbook image. Validation issues from the first attempt — fix them. Output JSON only — no thinking, no preamble.${refText}`
        : `Return the structured JSON for this bank passbook image. Output JSON only — no thinking, no preamble.${refText}`;
    }
    return isRetry
      ? `Re-extract the structured JSON for this receipt image. Validation issues from the first attempt — fix them. Output JSON only — no thinking, no preamble.${refText}`
      : `Return the structured JSON for this receipt image. Output JSON only — no thinking, no preamble.${refText}`;
  }
  return `Parse this document text:\n${buffer.toString('utf8')}`;
}

export async function runOcrPipeline(
  buffer: Buffer,
  mime: string,
  opts: OcrPipelineOpts = {},
): Promise<OcrPipelineResult> {
  const kind: OcrKind = opts.kind === 'po_invoice' ? 'po_invoice' : opts.kind === 'book_bank' ? 'book_bank' : 'receipt';
  const overrideModel = opts.modelName?.trim() || undefined;
  const chain = overrideModel ? [overrideModel] : await getVisionChain('staff:ocr', ['qwen3-vl:4b']);
  const attemptModels = chain.length === 1 ? [chain[0], chain[0]] : chain.slice(0, 3);
  const baseSystemPrompt =
    kind === 'book_bank' ? BANK_OCR_SYSTEM_PROMPT :
    kind === 'po_invoice' ? PO_INVOICE_SYSTEM_PROMPT :
    OCR_SYSTEM_PROMPT;

  let outMime = mime;
  let outBuffer = buffer;
  if (mime.startsWith('image/')) {
    const norm = await normalizeForOllamaVision(buffer, mime);
    outBuffer = norm.buffer;
    outMime = norm.mime;
  }

  let ocrText: string | undefined = undefined;
  if (mime.startsWith('image/')) {
    try {
      const require = createRequire(`${process.cwd()}/`);
      const visionBridge = require('lib/native/vision-ocr/index.js');
      if (visionBridge.ocrAvailable()) {
        const ocrResult = await visionBridge.ocrImageFile(outBuffer);
        if (ocrResult && ocrResult.ok && Array.isArray(ocrResult.lines) && ocrResult.lines.length > 0) {
          ocrText = ocrResult.lines.join('\n');
        }
      }
    } catch (e) {
      console.warn('[ocrPipeline] native OCR helper failed:', e);
    }
  }

  const imageDataUri = mime.startsWith('image/')
    ? `data:${outMime};base64,${outBuffer.toString('base64')}`
    : undefined;

  const attempts: Array<{
    modelName?: string;
    parsed: Record<string, unknown>;
    validation: ValidationResult;
    latencyMs: number;
  }> = [];
  let errorText = '';

  for (let i = 0; i < Math.min(attemptModels.length, 3); i++) {
    const modelName = attemptModels[i];
    const isRetry = i > 0;
    const sysPrompt = isRetry ? buildRetrySystemPrompt(baseSystemPrompt, errorText) : baseSystemPrompt;
    const t0 = Date.now();
    const r = await invoke(
      'staff:ocr',
      'vision',
      {
        systemPrompt: sysPrompt,
        text: makeText(kind, isRetry, buffer, mime, ocrText),
        images: imageDataUri ? [imageDataUri] : undefined,
        temperature: isRetry ? 0.02 : 0.05,
        maxTokens: 4000,
        modelOverride: modelName,
      },
    );
    if (!r.ok || !r.text) {
      if (i === 0) {
        const model = r.modelName || modelName || 'default';
        const upstreamHint =
          r.upstreamCode != null && r.upstreamMessage
            ? ` (upstream ${r.upstreamCode}: ${r.upstreamMessage})`
            : '';
        const authHint = r.statusCode === 401 || r.statusCode === 403
          ? ` — check that the provider's API key is valid (model "${model}")${upstreamHint}`
          : '';
        const err: Error & { statusCode?: number; upstreamCode?: number; upstreamMessage?: string } =
          new Error(`${r.error || 'Vision call failed'}${authHint}`);
        err.statusCode = r.statusCode ?? undefined;
        err.upstreamCode = r.upstreamCode ?? undefined;
        err.upstreamMessage = r.upstreamMessage ?? undefined;
        throw err;
      }
      continue;
    }

    let parsed: Record<string, unknown>;
    try {
      parsed = safeParse(r.text);
    } catch (e) {
      if (i === 0) throw e;
      continue;
    }
    const validation = validateForKind(parsed, kind);
    attempts.push({ modelName, parsed, validation, latencyMs: Date.now() - t0 });
    if (validation.ok) break;
    if (
      !RETRY_ENABLED ||
      opts.skipRetryOnValidationFail ||
      !isRetryableValidation(validation.errors)
    ) {
      break;
    }
    errorText = issuesToText(validation.errors);
  }

  if (attempts.length === 0) throw new Error('All vision attempts failed');
  const scored = attempts.map(a => ({
    ...a,
    score: kind === 'book_bank'
      ? bankConfidenceScore(a.parsed, a.validation)
      : confidenceScore(a.parsed, a.validation),
  }));
  scored.sort((x, y) => y.score - x.score);
  const best = scored[0];
  let parsed = best.parsed;
  let validation = best.validation;
  const retried = attempts.length > 1;

  if (!validation.ok) stampValidationOnParsed(parsed, kind, validation);

  try {
    const { mapAndRecord } = await import('./mapCoa');
    const vendorName = typeof parsed.vendorName === 'string' ? parsed.vendorName : null;
    const rawItems = Array.isArray(parsed.items) ? parsed.items : [];
    const maps = await mapAndRecord(vendorName, rawItems);
    for (let i = 0; i < rawItems.length && i < maps.length; i++) {
      (rawItems[i] as any).mapped_account_code = maps[i].mappedCode;
      (rawItems[i] as any).confidence_score = maps[i].similarity;
      (rawItems[i] as any).mapping_source = maps[i].source;
    }
  } catch { /* swallow — never break OCR */ }

  const mode = best.modelName ? `${kind}:${best.modelName}` : `${kind}:default`;
  return {
    parsed,
    mode,
    kind,
    validation: {
      ok: validation.ok,
      errors: validation.errors,
      warnings: validation.warnings,
      retried,
      summary: validation.summary,
    },
  };
}