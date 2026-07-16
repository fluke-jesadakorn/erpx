export interface ExtractFields {
  vendor?: string;
  amount?: number;
  currency?: string;
  transactionDate?: string;
  categoryCode?: string;
  categoryLabel?: string;
  description?: string;
  paymentMethod?: string;
  confidence: number;
}

export interface SalesExtractFields {
  customer_name?: string;
  customer_code?: string | null;
  payment_terms?: string;
  items?: Array<{ description: string; qty: number; unit_price: number }>;
  confidence?: number;
}

const BLOCK = /\[EXTRACT\]\s*([\s\S]*?)\s*\[\/EXTRACT\]/g;
export const SALES_EXTRACT_REGEX = /\[SALES_EXTRACT\]\s*([\s\S]*?)\s*\[\/SALES_EXTRACT\]/g;

export function parseExtractBlocks(text: string): { plain: string; extracts: ExtractFields[] } {
  const extracts: ExtractFields[] = [];
  if (!text) return { plain: '', extracts };
  const plain = text.replace(BLOCK, (_m, body: string) => {
    try {
      const parsed = JSON.parse(body);
      if (parsed && typeof parsed === 'object') {
        extracts.push({
          vendor: typeof parsed.vendor === 'string' ? parsed.vendor : undefined,
          amount: typeof parsed.amount === 'number' ? parsed.amount : undefined,
          currency: typeof parsed.currency === 'string' ? parsed.currency : undefined,
          transactionDate: typeof parsed.transactionDate === 'string' ? parsed.transactionDate : undefined,
          categoryCode: typeof parsed.categoryCode === 'string' ? parsed.categoryCode : undefined,
          categoryLabel: typeof parsed.categoryLabel === 'string' ? parsed.categoryLabel : undefined,
          description: typeof parsed.description === 'string' ? parsed.description : undefined,
          paymentMethod: typeof parsed.paymentMethod === 'string' ? parsed.paymentMethod : undefined,
          confidence: typeof parsed.confidence === 'number' ? Math.max(0, Math.min(1, parsed.confidence)) : 0,
        });
      }
    } catch {}
    return '';
  });
  return { plain: plain.trim(), extracts };
}

export function parseSalesExtract(text: string): SalesExtractFields | null {
  if (!text) return null;
  const m = text.match(SALES_EXTRACT_REGEX);
  if (!m) return null;
  try { return JSON.parse(m[1]); } catch { return null; }
}
