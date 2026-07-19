import { NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { PERM } from '@/perm/taxonomy';
import { config } from '@/config';
import { query } from '@/db';
import { put } from '@/slips/storage';
import { extractViaOllamaVision } from '@/slips/ocr_lib/pdf-pipeline.js';
import {
  failContract,
  indexContractText,
  ingestContract,
} from '@/law/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
export const maxDuration = 300;

interface PageImage {
  page_index?: number;
  image_b64?: string;
  bytes?: number;
  mime?: string;
}

async function indexFile(
  contractId: string,
  file: Buffer,
  mime: string,
  fileName: string,
): Promise<void> {
  try {
    const extracted = await extractViaOllamaVision(file, mime, fileName);
    if (extracted.ok !== true) {
      throw new Error(String(extracted.error_message || extracted.error_code || 'Text extraction failed'));
    }
    const pages = Array.isArray(extracted.page_images)
      ? extracted.page_images as PageImage[]
      : [];
    for (const page of pages) {
      if (!page.image_b64 || page.page_index == null) continue;
      const image = Buffer.from(page.image_b64, 'base64');
      const imageMime = page.mime || 'image/jpeg';
      const ext = imageMime === 'image/png' ? 'png' : 'jpg';
      const key = `law/contracts/${contractId}/pages/${page.page_index}.${ext}`;
      await put(key, image, imageMime);
      await query(
        `INSERT INTO law.contract_pages
          (contract_id, page_index, image_data, image_mime, bytes)
         VALUES ($1::uuid, $2, $3, $4, $5)
         ON CONFLICT (contract_id, page_index) DO UPDATE
           SET image_data = EXCLUDED.image_data,
               image_mime = EXCLUDED.image_mime,
               bytes = EXCLUDED.bytes`,
        [contractId, page.page_index, image, imageMime, page.bytes ?? image.length],
      );
    }
    await indexContractText(contractId, String(extracted.text || ''));
  } catch (err) {
    await failContract(contractId, (err as Error).message).catch(() => {});
  }
}

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: PERM.law.contract.upload });
  if (guard.response) return guard.response;
  const form = await req.formData();
  const file = form.get('file');
  if (!file || typeof file === 'string') {
    return NextResponse.json({ ok: false, error: 'file is required' }, { status: 400 });
  }

  const buffer = Buffer.from(await file.arrayBuffer());
  if (buffer.length === 0) {
    return NextResponse.json({ ok: false, error: 'file is empty' }, { status: 400 });
  }
  if (buffer.length > config.maxPdfBytes) {
    return NextResponse.json({ ok: false, error: 'file exceeds 50 MB' }, { status: 413 });
  }

  const fileName = file.name || 'contract.bin';
  const mime = file.type || 'application/octet-stream';
  const lineUserId = String(form.get('line_user_id') || '').trim() || null;
  try {
    const contractId = await ingestContract(buffer, fileName, mime, lineUserId);
    void indexFile(contractId, buffer, mime, fileName);
    return NextResponse.json({ ok: true, contractId, status: 'pending' }, { status: 202 });
  } catch (err) {
    return NextResponse.json({ ok: false, error: (err as Error).message }, { status: 500 });
  }
}
