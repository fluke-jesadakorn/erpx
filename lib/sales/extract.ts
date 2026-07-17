import 'server-only';
import { aiInvoke } from '@/ai/router';
import { salesExtractPrompt, renderLocaleAwarePrompt } from '@/ai/systemPrompts';
import { searchCustomers } from '@/customer/queries';

export interface ExtractedSoItem {
  description: string;
  qty: number;
  unit_price: number;
}

export interface ExtractedSo {
  customer_name: string;
  customer_code: string | null;
  customer_id: number | null;
  payment_terms: string;
  items: ExtractedSoItem[];
  confidence: number;
  raw: string;
}

function stripThinkBlocks(s: string): string {
  return s.replace(/<think>[\s\S]*?<\/think>/gi, '').trim();
}

function stripCodeFences(s: string): string {
  return s
    .replace(/```json\s*/gi, '')
    .replace(/```\s*/g, '')
    .trim();
}

function safeParse(s: string): Partial<ExtractedSo> | null {
  const cleaned = stripCodeFences(stripThinkBlocks(s));
  const tagMatch = cleaned.match(/\[SALES_EXTRACT\]([\s\S]*?)\[\/SALES_EXTRACT\]/);
  if (tagMatch) {
    try {
      return JSON.parse(tagMatch[1]);
    } catch {
      /* fall through */
    }
  }
  const objMatch = cleaned.match(/\{[\s\S]*\}/);
  if (objMatch) {
    try {
      return JSON.parse(objMatch[0]);
    } catch {
      return null;
    }
  }
  return null;
}

function parseTotalAmountTHB(text: string): number | null {
  const m = text.match(/(\d{1,3}(?:[,\s]\d{3})+|\d+)\s*(?:บาท|THB|baht)/i);
  if (!m) return null;
  const n = Number(m[1].replace(/[,\s]/g, ''));
  return Number.isFinite(n) && n > 0 ? n : null;
}

function parseProductHint(text: string): string {
  const cleaned = text
    .replace(/\[(ลูกค้า|Sales|Customer)\]:.*?(?=\[(?:ลูกค้า|Sales|Customer)\]:|$)/gs, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  const phrases = [
    /AI\s+(?:Factory|ระบบ\s*AI|System)[^\s,.]*/i,
    /ระบบ\s*AI[^\s,.]*/,
    /Private\s+Scope[^\s,.]*/i,
    /Local\s+LLM[^\s,.]*/i,
    /PoC[^\s,.]*/,
    /package[^\s,.]*/i,
    /แพ็กเกจ[^\s,.]*/,
  ];
  for (const p of phrases) {
    const m = cleaned.match(p);
    if (m) return m[0].trim();
  }
  return '';
}

export async function extractSoFromText(text: string, lang: 'en' | 'th' | 'de' = 'en'): Promise<ExtractedSo | null> {
  const r = await aiInvoke('sales:extract', 'chat', {
    systemPrompt: renderLocaleAwarePrompt(salesExtractPrompt, lang),
    text,
    temperature: 0.1,
    maxTokens: 2000,
  });
  if (!r.ok || !r.text) return null;

  const parsed = safeParse(r.text);
  if (!parsed) return null;

  const customer_name = typeof parsed.customer_name === 'string' ? parsed.customer_name.trim() : '';

  let items: ExtractedSoItem[] = Array.isArray(parsed.items)
    ? parsed.items
        .filter(
          (it: any) =>
            it &&
            typeof it.description === 'string' &&
            Number.isFinite(it.qty) &&
            Number.isFinite(it.unit_price),
        )
        .map((it: any) => ({
          description: String(it.description).trim(),
          qty: Number(it.qty),
          unit_price: Number(it.unit_price),
        }))
        .filter((it: ExtractedSoItem) => it.qty > 0 && it.unit_price > 0 && it.description)
    : [];

  if (items.length === 0) {
    const total = parseTotalAmountTHB(text);
    if (total) {
      items = [
        {
          description: parseProductHint(text) || 'Service package',
          qty: 1,
          unit_price: total,
        },
      ];
    }
  }

  if (items.length === 0) return null;

  let customerId: number | null = null;
  let customerCode: string | null = parsed.customer_code ?? null;
  if (customer_name) {
    try {
      const matches = await searchCustomers(customer_name, 3);
      if (matches.length > 0) {
        customerId = matches[0].id;
        customerCode = matches[0].code;
      }
    } catch {
      /* no customers table yet — fine */
    }
  }

  const confidence = (() => {
    let s = 0;
    if (customer_name) s += 0.3;
    if (customerId) s += 0.2;
    if (items.length > 0) s += 0.3;
    if (items.every((it) => it.qty > 0 && it.unit_price > 0)) s += 0.2;
    return Math.min(1, s);
  })();

  return {
    customer_name: customer_name || 'Customer',
    customer_code: customerCode,
    customer_id: customerId,
    payment_terms: parsed.payment_terms ?? 'Net 30',
    items,
    confidence,
    raw: r.text,
  };
}
