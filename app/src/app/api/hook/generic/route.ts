import { NextResponse } from 'next/server';
import { loadProvider, persistHookEvent } from '@folio-lib/hook/persist';
import { safeJson, sanitizeHeaders } from '@folio-lib/hook/normalize';
import { signHmacSha256, verifyHmacSha256 } from '@folio-lib/hook/verify';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const PROVIDER_ID = 'generic';
const SIGNATURE_HEADER = 'x-hook-signature';
const EVENT_ID_HEADER = 'x-hook-event-id';

export async function POST(req: Request): Promise<Response> {
  const raw = await req.text();
  const sig = req.headers.get(SIGNATURE_HEADER);
  const externalId = req.headers.get(EVENT_ID_HEADER);

  const provider = await loadProvider(PROVIDER_ID);
  if (!provider) return NextResponse.json({ ok: false, error: 'unknown_provider' }, { status: 404 });
  if (!provider.enabled) return NextResponse.json({ ok: false, error: 'disabled' }, { status: 403 });

  const secret = process.env[provider.secretEnv] ?? '';
  const sigOk = secret.length > 0 && verifyHmacSha256(raw, sig ?? '', secret);

  const result = await persistHookEvent({
    providerId: PROVIDER_ID,
    externalId,
    eventType: 'generic',
    payload: safeJson(raw),
    headers: sanitizeHeaders(req.headers, [SIGNATURE_HEADER]),
    signatureOk: sigOk,
  });

  if (!sigOk) return NextResponse.json({ ok: false, error: 'bad_signature' }, { status: 401 });
  return NextResponse.json({
    ok: true,
    id: result.id,
    duplicate: result.duplicate,
    verifyHint: process.env.NODE_ENV === 'production' ? undefined : signHmacSha256(raw, secret),
  });
}