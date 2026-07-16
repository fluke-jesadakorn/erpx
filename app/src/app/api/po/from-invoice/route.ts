import { NextRequest, NextResponse } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { ingestPoInvoice } from '@folio-lib/po/fromInvoice';

export const dynamic = 'force-dynamic';
export const runtime = 'nodejs';
export const maxDuration = 300;

export async function POST(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'tile:po:view::allow' });
  if (guard.response) return guard.response;
  if (!guard.actor) return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });

  const form = await req.formData();
  const file = form.get('file');
  if (!(file instanceof File)) {
    return NextResponse.json({ ok: false, error: 'missing file' }, { status: 400 });
  }
  const buf = Buffer.from(await file.arrayBuffer());
  if (buf.length === 0) {
    return NextResponse.json({ ok: false, error: 'empty file' }, { status: 400 });
  }
  if (buf.length > 25 * 1024 * 1024) {
    return NextResponse.json({ ok: false, error: 'file too large (max 25MB)' }, { status: 413 });
  }
  const actor = guard.actor;
  try {
    const draft = await ingestPoInvoice({
      buffer: buf,
      fileName: file.name || 'invoice.bin',
      mime: file.type || 'application/octet-stream',
      uploadedBy: actor.id,
    });
    return NextResponse.json({ ok: true, draft });
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e);
    return NextResponse.json({ ok: false, error: msg }, { status: 500 });
  }
}