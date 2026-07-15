import 'server-only';
import { unstable_cache } from 'next/cache';
import { query } from '@folio-lib/db';
import { decryptKey } from '@folio-lib/ai/crypto';
import { descriptionFor, ratingsFor } from '@folio-lib/ai/modelDescriptions';

export interface VisionModel {
  id: number;
  name: string;
  description: string | null;
  context_window: number | null;
  defaults_json: any;
  provider_name: string;
  auto_registered: boolean;
  speed_rating: number | null;
  accuracy_rating: number | null;
}

interface DbRow {
  id: number;
  name: string;
  description: string | null;
  context_window: number | null;
  defaults_json: any;
  provider_name: string;
  provider_type: 'ollama' | 'openai_compat' | 'minimax';
  provider_id: number | null;
  api_key_enc: Buffer | null;
}

interface OllamaTagModel {
  name: string;
  details?: { context_length?: number };
  capabilities?: string[];
}

async function fetchOllamaVisionModels(baseUrl: string): Promise<OllamaTagModel[]> {
  try {
    const res = await fetch(`${baseUrl}/api/tags`, { signal: AbortSignal.timeout(5000) });
    if (!res.ok) return [];
    const data = await res.json();
    return (data.models ?? []).filter((m: OllamaTagModel) =>
      Array.isArray(m.capabilities) && m.capabilities.includes('vision'),
    );
  } catch {
    return [];
  }
}

async function resolveProviderId(name: string): Promise<number | null> {
  const r = await query(`SELECT id FROM ai_providers WHERE name = $1 LIMIT 1`, [name]);
  return r.rows[0]?.id ?? null;
}

function hasEnvFallback(type: 'ollama' | 'openai_compat' | 'minimax'): boolean {
  if (type === 'ollama') return !!process.env.OLLAMA_URL;
  if (type === 'openai_compat') return !!(process.env.OPENAI_API_KEY || process.env.OPENROUTER_API_KEY);
  if (type === 'minimax') return !!process.env.MINIMAX_API_KEY;
  return false;
}

async function isProviderUsable(type: 'ollama' | 'openai_compat' | 'minimax', apiKeyEnc: Buffer | null): Promise<boolean> {
  // Ollama needs a reachable server, not a key.
  if (type === 'ollama') return !!process.env.OLLAMA_URL;
  // Other providers need a decryptable DB key OR a working env-var fallback.
  if (hasEnvFallback(type)) return true;
  if (!apiKeyEnc) return false;
  try {
    const k = await decryptKey(apiKeyEnc);
    return !!k;
  } catch {
    return false;
  }
}

async function loadVisionModelsUncached(): Promise<VisionModel[]> {
  const dbRes = await query<DbRow>(
    `SELECT m.id, m.name, m.description, m.context_window, m.defaults_json,
            p.name AS provider_name, p.type AS provider_type, p.id AS provider_id, p.api_key_enc
     FROM ai_models m
     LEFT JOIN ai_providers p ON p.id = m.provider_id
     WHERE m.enabled = true AND 'vision' = ANY(m.capabilities)
     ORDER BY m.id`,
  );

  const merged: VisionModel[] = [];
  const seen = new Set<string>();

  for (const r of dbRes.rows) {
    if (!r.provider_id || !r.provider_type) continue;
    const usable = await isProviderUsable(r.provider_type, r.api_key_enc);
    if (!usable) continue;
    seen.add(r.name);
    const ratings = ratingsFor(r.name);
    merged.push({
      id: r.id,
      name: r.name,
      description: r.description,
      context_window: r.context_window,
      defaults_json: r.defaults_json,
      provider_name: r.provider_name,
      auto_registered: false,
      speed_rating: ratings?.speed ?? null,
      accuracy_rating: ratings?.accuracy ?? null,
    });
  }

  const ollamaBase = process.env.OLLAMA_URL || 'http://localhost:11434';
  const ollamaVision = await fetchOllamaVisionModels(ollamaBase);

  if (ollamaVision.length > 0) {
    const providerId = await resolveProviderId('local-ollama');
    for (const m of ollamaVision) {
      if (seen.has(m.name)) continue;
      seen.add(m.name);
      const description = descriptionFor(m.name);
      const ratings = ratingsFor(m.name);
      const context = m.details?.context_length ?? null;
      let id: number;
      try {
        const ins = await query(
          `INSERT INTO ai_models (provider_id, name, capabilities, context_window, description, enabled)
           VALUES ($1, $2, $3, $4, $5, true)
           ON CONFLICT (provider_id, name) DO UPDATE SET description = COALESCE(ai_models.description, EXCLUDED.description)
           RETURNING id`,
          [providerId, m.name, ['vision'], context, description],
        );
        id = ins.rows[0].id;
      } catch {
        continue;
      }
      merged.push({
        id,
        name: m.name,
        description,
        context_window: context,
        defaults_json: {},
        provider_name: 'local-ollama',
        auto_registered: true,
        speed_rating: ratings?.speed ?? null,
        accuracy_rating: ratings?.accuracy ?? null,
      });
    }
  }

  for (const row of merged) {
    if (!row.description) {
      const desc = descriptionFor(row.name);
      if (desc) {
        try {
          await query(`UPDATE ai_models SET description = $1 WHERE id = $2 AND description IS NULL`, [desc, row.id]);
        } catch { /* best effort */ }
        row.description = desc;
      }
    }
  }

  return merged;
}

export const loadVisionModels = unstable_cache(
  loadVisionModelsUncached,
  ['vision-models-list'],
  { tags: ['vision-models'], revalidate: 300 },
);
