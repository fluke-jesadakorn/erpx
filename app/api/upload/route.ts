// POST /api/upload  — multipart file → OCR pipeline → persist slip row in
// 'pending' state (no auto expense creation).
//
// Two-step flow:
//   1. POST /api/upload
//      - file lands in MinIO
//      - OCR runs (Ollama vision; user can override model via model_name form field)
//      - slip row inserted in 'pending' state (no parent)
//      - response: { slipId, parsed, fileKey, confidence, mode, modelName, kind }
//   2. POST /api/slips/:id/confirm
//      - uploader reviews/edits OCR fields
//      - draft expense is created (or pr/po link established)
//      - slip row flips to 'confirmed' and links to its parent
//
// For pr/po uploads the caller MUST supply target_type=pr|po + target_id and
// the slip is created directly in 'confirmed' state — the confirm step is
// implicit (the target row already exists).
//
// `kind=book_bank` switches the OCR prompt + the slip row columns to
// {bank_name, bank_branch, account_number, account_name} for transfer payees.
// bank_branch is optional. The confirm flow can attach one book_bank slip to
// the same expense as a receipt slip.

import { NextResponse } from 'next/server';
import { query } from '@/db';
import { apiGuard } from '@/server/apiGuard';
import { put, publicUrlFor, makeKey } from '@/slips/storage';
import {
  runOcrPipeline,
  confidenceScore as ocrConfidence,
  bankConfidenceScore,
  type OcrKind,
} from '@/slips/ocrPipeline';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
export const maxDuration = 300;

const MAX_UPLOAD_BYTES = 15 * 1024 * 1024;
const ALLOWED_MIME = new Set([
  'image/jpeg',
  'image/png',
  'image/webp',
  'image/gif',
  'image/heic',
  'image/heif',
  'image/bmp',
  'image/tiff',
  'image/svg+xml',
  'application/pdf',
]);

function isAllowedMime(mime: string): boolean {
  const m = mime.toLowerCase();
  if (ALLOWED_MIME.has(m)) return true;
  return false;
}

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: 'finance:expense:create::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor;
  if (!actor) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });

  const contentLength = Number(req.headers.get('content-length') || '0');
  if (contentLength > MAX_UPLOAD_BYTES) {
    return NextResponse.json(
      { error: 'payload_too_large', detail: `max ${MAX_UPLOAD_BYTES} bytes`, maxBytes: MAX_UPLOAD_BYTES },
      { status: 413 },
    );
  }

  const form = await req.formData();
  const file = form.get('file');
  if (!file || typeof file === 'string') {
    return NextResponse.json({ error: 'file is required' }, { status: 400 });
  }

  const declaredMime = (file.type || 'application/octet-stream').toLowerCase();
  if (!isAllowedMime(declaredMime)) {
    return NextResponse.json(
      { error: 'unsupported_mime_type', detail: `got ${declaredMime}`, allowed: [...ALLOWED_MIME] },
      { status: 400 },
    );
  }

  const rawKind = String(form.get('kind') || 'receipt').toLowerCase();
  const kind: OcrKind = rawKind === 'book_bank' ? 'book_bank' : 'receipt';

  const rawTargetType = String(form.get('target_type') || '').toLowerCase();
  const rawTargetId = Number(form.get('target_id') || 0);
  if (!['expense', 'pr', 'po', ''].includes(rawTargetType)) {
    return NextResponse.json(
      { error: 'target_type must be one of expense|pr|po' },
      { status: 400 },
    );
  }
  if ((rawTargetType === 'pr' || rawTargetType === 'po') && rawTargetId <= 0) {
    return NextResponse.json(
      { error: 'target_type and target_id are required for slip upload' },
      { status: 400 },
    );
  }
  if (kind === 'book_bank' && rawTargetType && rawTargetType !== 'expense') {
    return NextResponse.json(
      { error: 'book_bank slips can only target an expense' },
      { status: 400 },
    );
  }
  const col = rawTargetType === 'pr' ? 'pr_id'
            : rawTargetType === 'po' ? 'po_id'
            : 'expense_id';

  const arrayBuffer = await file.arrayBuffer();
  const buffer = Buffer.from(arrayBuffer);
  if (buffer.length > MAX_UPLOAD_BYTES) {
    return NextResponse.json(
      { error: 'payload_too_large', detail: `max ${MAX_UPLOAD_BYTES} bytes`, maxBytes: MAX_UPLOAD_BYTES },
      { status: 413 },
    );
  }
  const mime = declaredMime;
  const originalName = file.name || 'slip';
  const key = makeKey(originalName);

  const stored = await put(key, buffer, mime);

  const modelName = String(form.get('model_name') || '').trim() || undefined;

  let parsed: Record<string, unknown>;
  let mode: string;
  let validation: { ok: boolean; errors: Array<{ code: string; severity: 'error' | 'warning'; field?: string; message: string }>; warnings: Array<{ code: string; severity: 'error' | 'warning'; field?: string; message: string }>; retried: boolean; summary: string };
  try {
    const r = await runOcrPipeline(buffer, mime, { modelName, kind });
    parsed = r.parsed;
    mode = r.mode;
    validation = r.validation;
  } catch (err: any) {
    const upstreamStatus = err?.statusCode ?? err?.response?.status ?? null;
    const status =
      typeof upstreamStatus === 'number' && upstreamStatus >= 400 && upstreamStatus < 600
        ? upstreamStatus
        : 502;
    return NextResponse.json(
      {
        error: 'ocr_failed',
        detail: err.message,
        upstreamStatus: upstreamStatus ?? null,
        upstreamCode: err?.upstreamCode ?? null,
        upstreamMessage: err?.upstreamMessage ?? null,
      },
      { status },
    );
  }

  const confidence =
    kind === 'book_bank' ? bankConfidenceScore(parsed, validation) : ocrConfidence(parsed, validation);

  const isPrOrPo = rawTargetType === 'pr' || rawTargetType === 'po';
  const initialStatus = isPrOrPo ? 'confirmed' : 'pending';

  const bankName = kind === 'book_bank' ? (String(parsed.bankName ?? '').trim() || null) : null;
  const bankBranch = kind === 'book_bank' ? (String(parsed.bankBranch ?? '').trim() || null) : null;
  const accountNumber = kind === 'book_bank'
    ? (String(parsed.accountNumber ?? '').replace(/[^\d]/g, '') || null)
    : null;
  const accountName = kind === 'book_bank' ? (String(parsed.accountName ?? '').trim() || null) : null;

  const insert = await query(
    `INSERT INTO slips (
       file_path, mime_type, file_size, ocr_raw_json, ocr_confidence,
       uploaded_by, status, confirmed_at, kind,
       bank_name, bank_branch, account_number, account_name,
       ${isPrOrPo ? col : 'expense_id'}
     )
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
     RETURNING id`,
    [
      stored.key,
      mime,
      stored.size,
      JSON.stringify(parsed || {}),
      confidence ?? null,
      actor.id,
      initialStatus,
      isPrOrPo ? new Date().toISOString() : null,
      kind,
      bankName,
      bankBranch,
      accountNumber,
      accountName,
      isPrOrPo ? rawTargetId : null,
    ],
  );
  const slipId = insert.rows[0].id;

return NextResponse.json({
    slipId,
    status: initialStatus,
    fileUrl: publicUrlFor(stored.key),
    fileKey: stored.key,
    mime,
    size: stored.size,
    parsed,
    confidence,
    mode,
    modelName: modelName ?? null,
    kind,
    validation,
  });
}
