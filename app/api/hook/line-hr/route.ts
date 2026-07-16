import { NextResponse } from 'next/server';
import {
  lineEventType,
  lineExternalId,
  LINE_SIGNATURE_HEADER,
  verifyLineSignature,
  type LineBody,
} from '@/hook/line';
import { loadProvider, persistHookEvent } from '@/hook/persist';
import { safeJson, sanitizeHeaders } from '@/hook/normalize';
import { runHrAgent } from '@/hr/agent';
import { query } from '@/db';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const PROVIDER_ID = 'line_hr';

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

  const firstEvent = body?.events?.[0];
  const lineUserId = firstEvent?.source?.userId ?? null;
  const messageText = (firstEvent?.message as { text?: string } | undefined)?.text ?? null;

  let replyText: string | null = null;
  if (lineUserId && messageText && eventType.startsWith('message:text')) {
    try {
      replyText = await runHrAgent(lineUserId, messageText);
    } catch (e) {
      console.error('HR agent error:', e);
      replyText = null;
    }
  }

  let pushed = false;
  let pushReason: string | undefined;
  if (sigOk && lineUserId && replyText) {
    const pushResult = await pushLineReply(lineUserId, replyText, provider.secretEnv);
    pushed = pushResult.ok;
    pushReason = pushResult.reason;

    await query(
      `UPDATE hr.user_sessions
          SET temp_data = jsonb_set(
                jsonb_set(
                  jsonb_set(COALESCE(temp_data, '{}'::jsonb), '{last_reply}', to_jsonb($2::text)),
                  '{last_reply_pushed}', to_jsonb($3::bool)
                ),
                '{last_push_reason}', to_jsonb($4::text)
              ),
              updated_at = now()
        WHERE line_user_id = $1`,
      [lineUserId, replyText, pushed, pushReason ?? null],
    ).catch(() => { /* swallow */ });
  }

  const responseBody: Record<string, unknown> = { ok: true, id: result.id, duplicate: result.duplicate };
  if (replyText) responseBody.reply = replyText;
  if (replyText) {
    responseBody.pushed = pushed;
    if (pushReason) responseBody.pushReason = pushReason;
  }

  return NextResponse.json(responseBody);
}

const LINE_PUSH = 'https://api.line.me/v2/bot/message/push';

async function pushLineReply(
  lineUserId: string,
  text: string,
  providerSecretEnv: string,
): Promise<{ ok: boolean; reason?: string }> {
  const token = process.env[providerSecretEnv] ?? process.env.HR_LINE_CHANNEL_ACCESS_TOKEN;
  if (!token) {
    console.warn('LINE push skipped: no channel access token configured');
    return { ok: false, reason: 'no_token' };
  }
  try {
    const r = await fetch(LINE_PUSH, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({
        to: lineUserId,
        messages: [{ type: 'text', text: text.slice(0, 5000) }],
      }),
    });
    if (!r.ok) {
      const body = await r.text();
      console.error('LINE Push error:', r.status, body.slice(0, 500));
      return { ok: false, reason: `http_${r.status}` };
    }
    console.log(`LINE Push sent to ${lineUserId} (${text.length} chars)`);
    return { ok: true };
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    console.error('LINE Push fetch error:', msg);
    return { ok: false, reason: msg };
  }
}
