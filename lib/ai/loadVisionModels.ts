import 'server-only';
import { unstable_cache } from 'next/cache';
import { query } from '@/db';
import { decryptKey } from '@/ai/crypto';
import { descriptionFor, ratingsFor } from '@/ai/modelDescriptions';

export interface VisionModel {
  id: number;
  name: string;
  capabilities: string[];
  description: string | null;
  context_window: number | null;
  defaults_json: any;
  provider_name: string;
  is_free: boolean;
  preferred?: boolean;
  auto_registered: boolean;
  speed_rating: number | null;
  accuracy_rating: number | null;
}

interface DbRow {
  id: number;
  name: string;
  capabilities: string[];
  description: string | null;
  context_window: number | null;
  defaults_json: any;
  provider_name: string;
  provider_type: 'ollama' | 'openai_compat' | 'minimax';
  provider_base_url: string;
  provider_id: number | null;
  api_key_enc: Buffer | null;
  is_free: boolean;
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

function hasEnvFallback(providerName: string, type: 'ollama' | 'openai_compat' | 'minimax', baseUrl?: string): boolean {
  if (type === 'ollama') return !!(process.env.OLLAMA_URL || baseUrl);
  if (type === 'openai_compat') {
    return providerName.toLowerCase() === 'openrouter'
      ? !!(process.env.OPENROUTER_API_KEY || process.env.OPENAI_API_KEY)
      : !!process.env.OPENAI_API_KEY;
  }
  if (type === 'minimax') return !!process.env.MINIMAX_API_KEY;
  return false;
}

async function isProviderUsable(providerName: string, type: 'ollama' | 'openai_compat' | 'minimax', apiKeyEnc: Buffer | null, baseUrl: string): Promise<boolean> {
  // Ollama needs a reachable server, not a key.
  if (type === 'ollama') return !!(process.env.OLLAMA_URL || baseUrl);
  // Other providers need a decryptable DB key OR a working env-var fallback.
  if (hasEnvFallback(providerName, type, baseUrl)) return true;
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
    `SELECT m.id, m.name, m.capabilities, m.description, m.context_window, m.defaults_json,
            m.is_free,
            p.name AS provider_name, p.type AS provider_type, p.base_url AS provider_base_url,
            p.id AS provider_id, p.api_key_enc
     FROM ai_models m
     JOIN ai_assignments a ON a.model_id = m.id
       AND a.section_key = 'staff:ocr'
       AND a.task_type = 'vision'
       AND a.enabled = true
       AND a.user_selectable = true
     JOIN ai_providers p ON p.id = m.provider_id AND p.enabled = true
     WHERE m.enabled = true AND 'vision' = ANY(m.capabilities)
     ORDER BY m.id`,
  );

  const merged: VisionModel[] = [];
  const seen = new Set<string>();

  for (const r of dbRes.rows) {
    if (!r.provider_id || !r.provider_type) continue;
    if (seen.has(r.name)) continue;
    const usable = await isProviderUsable(r.provider_name, r.provider_type, r.api_key_enc, r.provider_base_url);
    if (!usable) continue;
    seen.add(r.name);
    const ratings = ratingsFor(r.name);
    merged.push({
      id: r.id,
      name: r.name,
      capabilities: r.capabilities ?? ['vision'],
      description: r.description,
      context_window: r.context_window,
      defaults_json: r.defaults_json,
      provider_name: r.provider_name,
      is_free: r.is_free,
      auto_registered: false,
      speed_rating: ratings?.speed ?? null,
      accuracy_rating: ratings?.accuracy ?? null,
    });
  }

  const ollamaBase = process.env.OLLAMA_URL
    || dbRes.rows.find((row) => row.provider_type === 'ollama')?.provider_base_url
    || 'http://localhost:11434';
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
        capabilities: ['vision'],
        description,
        context_window: context,
        defaults_json: {},
        provider_name: 'local-ollama',
        is_free: false,
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

const loadVisionModelsCached = unstable_cache(
  loadVisionModelsUncached,
  ['vision-models-list'],
  { tags: ['vision-models'], revalidate: 300 },
);

export const VISION_MODELS_CACHE_TAG = 'vision-models';

export async function loadVisionModels(actorId?: number): Promise<VisionModel[]> {
  const models = await loadVisionModelsCached();
  let preferredName: string | null = null;
  if (actorId) {
    const preference = await query<{ model_name: string }>(
      `SELECT m.name AS model_name
         FROM folio.ai_user_section_preferences pref
         JOIN folio.ai_models m ON m.id = pref.model_id AND m.enabled = true
        WHERE pref.user_id = $1 AND pref.section_key = 'staff:ocr'
        LIMIT 1`,
      [actorId],
    );
    preferredName = preference.rows[0]?.model_name ?? null;
  }

  return [...models]
    .map((model) => ({ ...model, preferred: Boolean(preferredName && model.name === preferredName) }))
    .sort((a, b) => {
      if (a.preferred !== b.preferred) return a.preferred ? -1 : 1;
      if (a.is_free !== b.is_free) return a.is_free ? -1 : 1;
      const contextDelta = (b.context_window ?? -1) - (a.context_window ?? -1);
      if (contextDelta !== 0) return contextDelta;
      return a.name.localeCompare(b.name);
    });
}
