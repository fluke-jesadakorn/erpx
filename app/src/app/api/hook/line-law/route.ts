import { NextResponse } from 'next/server';
import {
  lineEventType,
  lineExternalId,
  LINE_SIGNATURE_HEADER,
  verifyLineSignature,
  type LineBody,
} from '@folio-lib/hook/line';
import { loadProvider, persistHookEvent } from '@folio-lib/hook/persist';
import { safeJson, sanitizeHeaders } from '@folio-lib/hook/normalize';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const PROVIDER_ID = 'line_law';

export async function POST(req: Request): Promise<Response> {
  const raw = await req.text();

  const provider = await loadProvider(PROVIDER_ID);
  if (!provider) return NextResponse.json({ ok: false, error: 'unknown_provider' }, { status: 404 });
  if (!provider.enabled) return NextResponse.json({ ok: false, error: 'disabled' }, { status: 403 });

  const secret = process.env[provider.secretEnv] ?? '';
  const sigOk = secret.length > 0 && verifyLineSignature(raw, req.headers.get(LINE_SIGNATURE_HEADER), secret);

  const body = safeJson(raw) as LineBody;
  const eventType = sigOk ? lineEventType(body) : 'unverified';
  const externalId = lineExternalId(body);

  const result = await persistHookEvent({
    providerId: PROVIDER_ID,
    externalId,
    eventType,
    payload: body,
    headers: sanitizeHeaders(req.headers, [LINE_SIGNATURE_HEADER]),
    signatureOk: sigOk,
  });

  if (!sigOk) return NextResponse.json({ ok: false, error: 'bad_signature' }, { status: 401 });
  return NextResponse.json({ ok: true, id: result.id, duplicate: result.duplicate });
}
