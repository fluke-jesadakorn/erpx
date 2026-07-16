import 'server-only';
import { aiInvoke } from '@/ai/router';
import { salesExtractPrompt, renderLocaleAwarePrompt } from '@/ai/systemPrompts';
import { searchCustomers } from '@/customer/queries';
import { query } from '../db';

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

function safeParse(s: string): Partial<ExtractedSo> | null {
  const m = s.match(/\[SALES_EXTRACT\]([\s\S]*?)\[\/SALES_EXTRACT\]/) ?? s.match(/\{[\s\S]*\}/);
  if (!m) return null;
  try {
    return JSON.parse(m[1] ?? m[0]);
  } catch {
    return null;
  }
}

export async function extractSoFromText(text: string, lang: 'en' | 'th' | 'de' = 'en'): Promise<ExtractedSo | null> {
  const r = await aiInvoke('sales:extract', 'chat', {
    systemPrompt: renderLocaleAwarePrompt(salesExtractPrompt, lang),
    text,
    temperature: 0.1,
    maxTokens: 800,
  });
  if (!r.ok || !r.text) return null;

  const parsed = safeParse(r.text);
  if (!parsed || !parsed.customer_name || !Array.isArray(parsed.items)) return null;

  const items: ExtractedSoItem[] = parsed.items
    .filter((it: any) => it && typeof it.description === 'string' && Number.isFinite(it.qty) && Number.isFinite(it.unit_price))
    .map((it: any) => ({
      description: String(it.description),
      qty: Number(it.qty),
      unit_price: Number(it.unit_price),
    }));

  let customerId: number | null = null;
  let customerCode: string | null = parsed.customer_code ?? null;
  const matches = await searchCustomers(parsed.customer_name, 3);
  if (matches.length > 0) {
    customerId = matches[0].id;
    customerCode = matches[0].code;
  }

  const confidence = (() => {
    let s = 0;
    if (parsed.customer_name) s += 0.3;
    if (customerId) s += 0.2;
    if (items.length > 0) s += 0.3;
    if (items.every((it) => it.qty > 0 && it.unit_price > 0)) s += 0.2;
    return Math.min(1, s);
  })();

  return {
    customer_name: parsed.customer_name,
    customer_code: customerCode,
    customer_id: customerId,
    payment_terms: parsed.payment_terms ?? 'Net 30',
    items,
    confidence,
    raw: r.text,
  };
}
