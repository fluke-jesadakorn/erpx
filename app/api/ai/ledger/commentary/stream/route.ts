import { invoke } from '@folio-lib/ai/router';
import { NextRequest } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { query } from '@folio-lib/db';
import { renderLocaleAwarePrompt } from '@folio-lib/ai/systemPrompts';
import { sseHeaders } from '@folio-lib/ai/streamProtocol';
import { ledgerLineCommentaryPrompt } from '@folio-lib/ai/systemPrompts';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'finance:gl:view::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor!;
  const body = await req.json().catch(() => ({}));
  const { accountCode, periodLabel, lang, model } = body as {
    accountCode?: string;
    periodLabel?: string;
    lang?: 'en' | 'th' | 'de';
    model?: string;
  };
  if (!accountCode) {
    return new Response(JSON.stringify({ ok: false, error: 'accountCode required' }), { status: 400 });
  }
  const locale = lang ?? 'en';
  const acctRes = await query<{ name: string | null; name_th: string | null }>(
    `SELECT name, name_th FROM chart_of_accounts WHERE code = $1`,
    [accountCode],
  );
  const acct = acctRes.rows[0];
  const systemPrompt = renderLocaleAwarePrompt(ledgerLineCommentaryPrompt, locale);
  const userText = `Account ${accountCode} ${acct ? `(${acct.name})` : ''} — ${periodLabel ?? 'latest period'}`;

  const encoder = new TextEncoder();
  const t0 = Date.now();
  const stream = new ReadableStream({
    async start(controller) {
      const send = (event: string, data: unknown) => {
        controller.enqueue(encoder.encode(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`));
      };
      try {
        const r = await invoke('ledger:commentary', 'chat', {
          systemPrompt,
          text: userText,
          modelOverride: model,
          temperature: 0.2,
        }, { actorId: actor.id });
        if (!r.ok || !r.text) {
          send('error', { message: r.error || 'AI call failed', statusCode: r.statusCode });
          controller.close();
          return;
        }
        controller.enqueue(encoder.encode(`data: ${JSON.stringify({ chunk: r.text })}\n\n`));
        send('meta', { latencyMs: Date.now() - t0, modelName: r.modelName ?? null });
        controller.close();
      } catch (e: any) {
        send('error', { message: e?.message ?? String(e) });
        controller.close();
      }
    },
  });
  return new Response(stream, { headers: sseHeaders() });
}
