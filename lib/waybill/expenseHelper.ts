import 'server-only';
import { aiInvoke } from '@folio-lib/ai/router';
import { query } from '../db';
import { getSemanticSuggestions } from './queries';

export interface SubmitHint {
  description: string;
  descriptionTh: string | null;
  coaSuggestions: Array<{ code: string; name: string | null; name_th: string | null; similarity: number }>;
  vendorSuggestion: { id: number; code: string; name: string } | null;
  amountHypothesis: number | null;
}

interface VendorMatch {
  id: number;
  code: string;
  name: string;
}

export async function helpWithExpenseSubmit(args: {
  rawText: string;
  submitterId: number;
  lang?: 'en' | 'th' | 'de';
}): Promise<SubmitHint | null> {
  const lang = args.lang ?? 'en';
  const text = args.rawText.trim();
  if (!text) return null;

  const r = await aiInvoke('staff:submit', 'chat', {
    systemPrompt: `You help a Thai staff member write an expense description. Output JSON only, no markdown: {"description":"<english, 4-10 words>","descriptionTh":"<thai equivalent>","amountHypothesis":<number or null>}. Be conservative; if you cannot estimate amount, set it null. ${lang === 'th' ? 'ตอบเป็นภาษาไทย' : lang === 'de' ? 'Antworten Sie auf Deutsch' : 'Reply in English.'}`,
    text,
    temperature: 0.2,
    maxTokens: 300,
  });

  let description = text;
  let descriptionTh: string | null = null;
  let amountHypothesis: number | null = null;

  if (r.ok && r.text) {
    const m = r.text.match(/\{[\s\S]*\}/);
    if (m) {
      try {
        const parsed = JSON.parse(m[0]);
        if (typeof parsed.description === 'string') description = parsed.description;
        if (typeof parsed.descriptionTh === 'string') descriptionTh = parsed.descriptionTh;
        if (Number.isFinite(parsed.amountHypothesis)) amountHypothesis = Number(parsed.amountHypothesis);
      } catch { /* keep raw text */ }
    }
  }

  const sem = await getSemanticSuggestions(description).catch(() => ({ success: false, suggestions: [] }));
  const coaSuggestions = sem.success
    ? (sem.suggestions as Array<{ code: string; name: string | null; name_th: string | null; similarity: number }>)
    : [];

  const vendorRes = await query<VendorMatch>(
    `SELECT id, code, name FROM customers
      WHERE is_active = TRUE
        AND (name ILIKE $1 OR name_th ILIKE $1)
      ORDER BY name
      LIMIT 1`,
    [`%${text.split(/\s+/).slice(0, 2).join(' ')}%`]
  );
  const vendorSuggestion = vendorRes.rows[0]
    ? { id: vendorRes.rows[0].id, code: vendorRes.rows[0].code, name: vendorRes.rows[0].name }
    : null;

  return {
    description,
    descriptionTh,
    coaSuggestions,
    vendorSuggestion,
    amountHypothesis,
  };
}