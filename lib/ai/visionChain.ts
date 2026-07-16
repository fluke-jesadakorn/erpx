import 'server-only';
import { query } from '../db';

let cache: { models: string[]; ts: number } | null = null;
const TTL_MS = 60_000;

export async function getVisionChain(sectionKey: string, fallback: string[]): Promise<string[]> {
  const now = Date.now();
  if (cache && now - cache.ts < TTL_MS && cache.models.length > 0) {
    return cache.models;
  }
  try {
    const r = await query<{ models: string[] }>(
      `SELECT models FROM folio.vision_chain WHERE section_key = $1`,
      [sectionKey],
    );
    if (r.rows.length > 0 && Array.isArray(r.rows[0].models) && r.rows[0].models.length > 0) {
      cache = { models: r.rows[0].models, ts: now };
      return r.rows[0].models;
    }
  } catch {}
  cache = { models: fallback, ts: now };
  return fallback;
}

export function invalidateVisionChainCache(): void {
  cache = null;
}
