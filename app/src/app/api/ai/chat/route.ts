import { NextResponse } from 'next/server';
import { invoke } from '@folio-lib/ai/router';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { parseChartBlocks } from '@/components/chat/chartContract';
import { parseExtractBlocks, SALES_EXTRACT_REGEX } from '@/components/chat/extractContract';
import { cockpitProjectionPrompt, salesExtractPrompt, customerCreditPrompt, getTileSystemPrompt, renderLocaleAwarePrompt } from '@folio-lib/ai/systemPrompts';
import { query } from '@folio-lib/db';
import { getSecondaryLocaleFromHeaders } from '@folio-lib/server/locale';

export const dynamic = 'force-dynamic';

function buildFinanceAssistantPrompt(tileId: string, lang: 'en' | 'th' | 'de', actorName?: string): string {
  const langLine = lang === 'de' ? 'Antworten Sie auf Deutsch.' : lang === 'th' ? 'ตอบเป็นภาษาไทย' : 'Reply in English.';
  const actorLine = actorName ? ` The current user is ${actorName}.` : '';
  const base = `You are a finance assistant embedded in the "${tileId}" tile of a Thai ERP.${actorLine} Keep answers short (1-3 sentences). Prose only, no bullets, no markdown. ${langLine}`;
  const chartRule = ' When the user asks for a chart, append a JSON block exactly: [CHART]{"type":"line|bar|pie|area","title":"...","series":[{"name":"...","data":[...]}],"axes":{"x":...,"y":"..."}}[\\CHART]';
  const extractRule = tileId === 'expense'
    ? ' When the user describes an expense (vendor, amount, date, category), append: [EXTRACT]{"vendor":"...","amount":120,"currency":"THB","transactionDate":"YYYY-MM-DD","categoryCode":"510100","categoryLabel":"...","description":"...","paymentMethod":"cash","confidence":0.9}[\\EXTRACT]'
    : tileId === 'sales'
      ? ' When the user describes a sales order (customer, line items, payment terms), append: [SALES_EXTRACT]{"customer_name":"...","customer_code":null,"payment_terms":"Net 30","items":[{"description":"...","qty":1,"unit_price":0}],"confidence":0.9}[\\SALES_EXTRACT]'
      : '';
  return base + chartRule + extractRule;
}

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: 'ai:chat:use::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor;
  if (!actor) return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });

  let body: any = {};
  try { body = await req.json(); } catch {}
  const { sectionKey, tileId, messages, lang: bodyLang, contextData, systemPrompt: overridePrompt } = body;
  if (!sectionKey || !Array.isArray(messages) || messages.length === 0) {
    return NextResponse.json({ ok: false, error: 'sectionKey and messages required' }, { status: 400 });
  }

  const headerLocale = getSecondaryLocaleFromHeaders(req.headers);
  const lang: 'en' | 'th' | 'de' = headerLocale === 'de' ? 'de' : headerLocale === 'th' ? 'th' : 'en';
  const _ignoredLang = bodyLang;

  let systemPrompt = overridePrompt;
  if (!systemPrompt && sectionKey === 'cockpit:projection') {
    systemPrompt = renderLocaleAwarePrompt(cockpitProjectionPrompt, lang);
  }
  if (!systemPrompt && sectionKey === 'sales:extract') {
    systemPrompt = renderLocaleAwarePrompt(salesExtractPrompt, lang);
  }
  if (!systemPrompt && sectionKey === 'customer:credit-check' && contextData?.customer_id) {
    try {
      const r = await query<{
        name: string;
        credit_limit_thb: string;
        outstanding_ar: string;
        total_paid: string;
      }>(
        `SELECT c.name, c.credit_limit_thb::text,
                COALESCE(h.outstanding_ar, 0)::text AS outstanding_ar,
                COALESCE(h.total_paid, 0)::text AS total_paid
           FROM customers c
           LEFT JOIN customer_ar_history h ON h.customer_id = c.id
          WHERE c.id = $1`,
        [contextData.customer_id],
      );
      const row = r.rows[0];
      if (row) {
        systemPrompt = `${renderLocaleAwarePrompt(customerCreditPrompt, lang)}\n\nCustomer context: ${JSON.stringify(row)}`;
      } else {
        systemPrompt = renderLocaleAwarePrompt(customerCreditPrompt, lang);
      }
    } catch {
      systemPrompt = renderLocaleAwarePrompt(customerCreditPrompt, lang);
    }
  }
  if (!systemPrompt && tileId) {
    const tilePrompt = getTileSystemPrompt(tileId, { locale: lang });
    if (tilePrompt) systemPrompt = tilePrompt;
  }
  if (!systemPrompt) {
    systemPrompt = buildFinanceAssistantPrompt(
      tileId || 'cockpit',
      lang,
      actor?.fullname,
    );
  }
  const result = await invoke(
    sectionKey,
    'chat',
    {
      messages,
      systemPrompt,
      temperature: 0.3,
    },
    { actorId: actor.id },
  );

  if (!result.ok) {
    return NextResponse.json({ ok: false, error: result.error || 'AI call failed' }, { status: 200 });
  }

  const fullText = (result.text || '').replace(/<think>[\s\S]*?<\/think>/g, '').trim();
  const { plain, charts } = parseChartBlocks(fullText);
  const { extracts } = parseExtractBlocks(plain);

  let salesExtract = null;
  if (tileId === 'sales' || sectionKey === 'sales:extract') {
    const m = fullText.match(SALES_EXTRACT_REGEX);
    if (m) {
      try { salesExtract = JSON.parse(m[1]); } catch {}
    }
  }

  return NextResponse.json({
    ok: true,
    text: fullText,
    plain,
    charts,
    extracts,
    salesExtract,
    modelName: result.modelName,
    latencyMs: result.latencyMs,
  });
}