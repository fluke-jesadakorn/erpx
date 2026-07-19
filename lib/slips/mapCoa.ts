import 'server-only';
import { getSemanticSuggestions } from '../waybill/queries';
import { getLearnedMapping } from './learned';

export interface ItemCoaMap {
  description: string;
  mappedCode: string | null;
  similarity: number | null;
  source: 'learned' | 'semantic' | 'none';
}

const LEARNED_THRESHOLD = 0.6;
const SEMANTIC_THRESHOLD = 0.82;

export async function mapItemsToCoa(
  items: Array<{ description: string }>,
): Promise<ItemCoaMap[]> {
  const out: ItemCoaMap[] = [];
  for (const it of items) {
    const desc = (it.description ?? '').trim();
    if (!desc) {
      out.push({ description: it.description, mappedCode: null, similarity: null, source: 'none' });
      continue;
    }
    const sem = await getSemanticSuggestions(desc).catch(() => ({ success: false, suggestions: [] as any[] }));
    const suggestions = sem.success ? (sem.suggestions ?? []) : [];
    const top = suggestions.length > 0 ? suggestions[0] : null;
    const topSim = top ? Number(top.similarity) / 100 : 0;
    if (top && topSim >= SEMANTIC_THRESHOLD) {
      out.push({
        description: it.description,
        mappedCode: top.code,
        similarity: topSim,
        source: 'semantic',
      });
    } else {
      out.push({
        description: it.description,
        mappedCode: null,
        similarity: topSim || null,
        source: 'none',
      });
    }
  }
  return out;
}

export async function mapAndRecord(
  vendorName: string | null,
  items: Array<{ description: string }>,
): Promise<ItemCoaMap[]> {
  const maps = await mapItemsToCoa(items);
  if (vendorName) {
    const learned = await getLearnedMapping(vendorName);
    if (learned && learned.hits >= 3) {
      const conf = Math.min(1, learned.hits / 10);
      if (conf >= LEARNED_THRESHOLD) {
        for (const m of maps) {
          if (!m.mappedCode) {
            m.mappedCode = learned.accountCode;
            m.similarity = conf;
            m.source = 'learned';
          }
        }
      }
    }
  }
  return maps;
}
