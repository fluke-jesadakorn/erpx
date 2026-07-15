import { NextResponse } from 'next/server';
import { extractText, extractViaOllamaVision } from '@folio-lib/slips/ocr_lib/pdf-pipeline.js';
import { config } from '@folio-lib/config';
import { apiGuard } from '@folio-lib/server/apiGuard';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: 'finance:expense:create::allow' });
  if (guard.response) return guard.response;

  const body = (await req.json().catch(() => ({}))) as { pdf_b64?: string; file_data_b64?: string };
  if (!body.pdf_b64 && !body.file_data_b64) {
    return NextResponse.json({ ok: false, error_code: 'missing_pdf_b64', error_message: 'pdf_b64 required' }, { status: 400 });
  }
  const b64 = body.pdf_b64 || body.file_data_b64!;
  const pdfBuffer = Buffer.from(b64.startsWith('data:') ? b64.split(',', 2)[1] || b64 : b64, 'base64');
  if (pdfBuffer.length === 0) {
    return NextResponse.json({ ok: false, error_code: 'invalid_pdf_size', error_message: 'decoded PDF is empty' }, { status: 400 });
  }
  if (pdfBuffer.length > config.maxPdfBytes) {
    return NextResponse.json({ ok: false, error_code: 'invalid_pdf_size', error_message: `decoded PDF too large (${pdfBuffer.length})` }, { status: 400 });
  }
  if (pdfBuffer.slice(0, 4).toString('ascii') !== '%PDF') {
    return NextResponse.json({ ok: false, error_code: 'not_a_pdf', error_message: 'first 4 bytes are not PDF magic' }, { status: 400 });
  }
  try {
    return NextResponse.json(await extractText(pdfBuffer));
  } catch (e: unknown) {
    return NextResponse.json({ ok: false, error_code: 'ocr_failed', error_message: (e as Error).message }, { status: 500 });
  }
}

export async function PUT(req: Request) {
  const guard = await apiGuard(req, { perm: 'finance:expense:create::allow' });
  if (guard.response) return guard.response;

  const body = (await req.json().catch(() => ({}))) as { file_data_b64?: string; pdf_b64?: string; file_mime?: string; file_name?: string };
  const b64 = body.file_data_b64 || body.pdf_b64;
  if (!b64) return NextResponse.json({ ok: false, error_code: 'missing_file_data_b64', error_message: 'file_data_b64 required' }, { status: 400 });
  const fileBuffer = Buffer.from(b64.startsWith('data:') ? b64.split(',', 2)[1] || b64 : b64, 'base64');
  if (fileBuffer.length === 0 || fileBuffer.length > config.maxPdfBytes) {
    return NextResponse.json({ ok: false, error_code: 'invalid_file_size', error_message: 'file size out of range' }, { status: 400 });
  }
  try {
    return NextResponse.json(await extractViaOllamaVision(fileBuffer, body.file_mime || '', body.file_name || 'upload'));
  } catch (e: unknown) {
    return NextResponse.json({ ok: false, error_code: 'vision_failed', error_message: (e as Error).message }, { status: 500 });
  }
}