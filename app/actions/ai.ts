'use server';

import { revalidateTag } from 'next/cache';
import { query } from '@/db';
import { aiInvoke } from '@/ai/router';
import { encryptKey } from '@/ai/crypto';
import * as ollama from '@/ai/providers/ollama';
import * as openai from '@/ai/providers/openai';
import { providerApiKey } from '@/ai/providerKey';
import { clearUserAiPreference, saveUserAiPreference } from '@/ai/preferences';
import { getSection } from '@/ai/sections';
import { VISION_MODELS_CACHE_TAG } from '@/ai/loadVisionModels';
import { loadActor, type ActorWithScope } from '@/server/guard';
import { hasPermission, PERM } from '@/perm/server';

async function requireActorOrNull(): Promise<ActorWithScope | null> {
  return loadActor();
}

async function requirePermission(permission: string): Promise<ActorWithScope> {
  const actor = await requireActorOrNull();
  if (!actor) throw new Error('unauthorized');
  if (!hasPermission(actor, permission)) throw new Error('forbidden');
  return actor;
}

function invalidateVisionModels() {
  revalidateTag(VISION_MODELS_CACHE_TAG, 'max');
}

export async function listModels(_capability?: string): Promise<unknown[]> {
  const actor = await requireActorOrNull();
  if (!actor) return [];
  if (!hasPermission(actor, PERM.ai.model.read)) return [];
  try {
    const res = await query(
      `SELECT m.id, m.name, m.provider_id, m.capabilities, m.context_window, m.enabled, m.description, m.is_free
         FROM ai_models m WHERE m.enabled = true
          AND ($1::text IS NULL OR $1 = ANY(m.capabilities))
        ORDER BY m.is_free DESC, (m.name = 'openrouter/free') DESC, m.name`,
      [_capability ?? null],
    );
    return res.rows.map((r: any) => ({
      id: r.id,
      name: r.name,
      provider_id: r.provider_id,
      capabilities: r.capabilities ?? [],
      context_window: r.context_window,
      enabled: r.enabled,
      description: r.description,
      is_free: r.is_free,
    }));
  } catch {
    return [];
  }
}

export async function createProvider(input: any) {
  await requirePermission(PERM.ai.provider.create);
  if (!input?.name || !input?.type || !input?.base_url) throw new Error('name, type and base_url are required');
  const key = input.api_key ? await encryptKey(String(input.api_key)) : null;
  await query(
    `INSERT INTO ai_providers (name, type, base_url, api_key_enc, enabled, preset, notes)
     VALUES ($1,$2,$3,$4,$5,$6,$7)`,
    [input.name, input.type, input.base_url, key, input.enabled !== false, input.preset || null, input.notes || null],
  );
  invalidateVisionModels();
}

export async function updateProvider(id: number, patch: any) {
  await requirePermission(PERM.ai.provider.update);
  const sets: string[] = [];
  const values: unknown[] = [];
  let index = 1;
  for (const field of ['name', 'base_url', 'enabled', 'notes']) {
    if (patch?.[field] !== undefined) { sets.push(`${field} = $${index++}`); values.push(patch[field]); }
  }
  if (patch?.api_key !== undefined) { sets.push(`api_key_enc = $${index++}`); values.push(await encryptKey(String(patch.api_key))); }
  if (!sets.length) return;
  values.push(id);
  await query(`UPDATE ai_providers SET ${sets.join(', ')}, updated_at = now() WHERE id = $${index}`, values);
  invalidateVisionModels();
}

export async function deleteProvider(id: number) {
  await requirePermission(PERM.ai.provider.delete);
  await query('DELETE FROM ai_providers WHERE id = $1', [id]);
  invalidateVisionModels();
}

export async function testProvider(id: number): Promise<{ ok: boolean; modelCount: number; baseUrl: string; latencyMs: number; error?: string }> {
  await requirePermission(PERM.ai.provider.test);
  const result = await query<{ name: string; type: 'ollama' | 'openai_compat' | 'minimax'; base_url: string; api_key_enc: Buffer | null }>(
    'SELECT name, type, base_url, api_key_enc FROM ai_providers WHERE id = $1', [id],
  );
  const provider = result.rows[0];
  if (!provider) throw new Error('provider not found');
  const t0 = Date.now();
  try {
    const apiKey = await providerApiKey(provider.name, provider.api_key_enc);
    const models = provider.type === 'ollama'
      ? await ollama.ollamaListModels({ baseUrl: provider.base_url })
      : await openai.openaiListModels({ baseUrl: provider.base_url, apiKey });
    return { ok: true, modelCount: models.length, baseUrl: provider.base_url, latencyMs: Date.now() - t0 };
  } catch (error) {
    return { ok: false, modelCount: 0, baseUrl: provider.base_url, latencyMs: Date.now() - t0, error: error instanceof Error ? error.message : String(error) };
  }
}

export async function createModel(input: any) {
  await requirePermission(PERM.ai.model.create);
  if (!input?.name || !input?.provider_id) throw new Error('name and provider_id are required');
  await query(
    `INSERT INTO ai_models (name, provider_id, capabilities, context_window, enabled, defaults_json)
     VALUES ($1,$2,$3,$4,$5,$6)`,
    [input.name, input.provider_id, input.capabilities || [], input.context_window || null, input.enabled !== false, JSON.stringify(input.defaults_json || {})],
  );
  invalidateVisionModels();
}

export async function updateModel(id: number, patch: any) {
  await requirePermission(PERM.ai.model.update);
  const allowed = ['name', 'enabled', 'capabilities', 'context_window', 'defaults_json', 'description', 'reasoning_levels', 'is_free'];
  const sets: string[] = [];
  const values: unknown[] = [];
  let index = 1;
  for (const field of allowed) {
    if (patch?.[field] !== undefined) {
      sets.push(`${field} = $${index++}`);
      values.push(field === 'defaults_json' ? JSON.stringify(patch[field]) : patch[field]);
    }
  }
  if (!sets.length) return;
  values.push(id);
  await query(`UPDATE ai_models SET ${sets.join(', ')} WHERE id = $${index}`, values);
  invalidateVisionModels();
}

export async function deleteModel(id: number) {
  await requirePermission(PERM.ai.model.delete);
  await query('DELETE FROM ai_models WHERE id = $1', [id]);
  invalidateVisionModels();
}

export async function createAssignment(input: any) {
  await requirePermission(PERM.ai.assignment.create);
  if (!input?.section_key || !input?.task_type || !input?.model_id) throw new Error('section_key, task_type and model_id are required');
  await query(
    `INSERT INTO ai_assignments (section_key, task_type, provider_id, model_id, params_json, priority, enabled, user_selectable)
     SELECT $1,$2,m.provider_id,$3,$4,$5,$6,$7 FROM ai_models m WHERE m.id = $3`,
    [input.section_key, input.task_type, input.model_id, JSON.stringify(input.params_json || {}), input.priority ?? 100, input.enabled !== false, input.user_selectable !== false],
  );
  invalidateVisionModels();
}

export async function updateAssignment(id: number, patch: any) {
  await requirePermission('ai:assignment:update::allow');
  const allowed = ['provider_id', 'model_id', 'priority', 'enabled', 'user_selectable', 'params_json'];
  const sets: string[] = [];
  const values: unknown[] = [];
  let index = 1;
  for (const field of allowed) {
    if (patch?.[field] !== undefined) { sets.push(`${field} = $${index++}`); values.push(field === 'params_json' ? JSON.stringify(patch[field]) : patch[field]); }
  }
  if (!sets.length) return;
  values.push(id);
  await query(`UPDATE ai_assignments SET ${sets.join(', ')}, updated_at = now() WHERE id = $${index}`, values);
  invalidateVisionModels();
}

export async function deleteAssignment(id: number) {
  await requirePermission(PERM.ai.assignment.delete);
  await query('DELETE FROM ai_assignments WHERE id = $1', [id]);
  invalidateVisionModels();
}

export async function setMyDefault(sectionKey: string, modelId: number) {
  const actor = await requirePermission('ai:preference:update::allow');
  const section = getSection(sectionKey);
  if (!section) throw new Error('unknown AI section');
  await saveUserAiPreference(actor.id, sectionKey, modelId, 'auto', section.task);
}

export async function deleteMyDefault(sectionKey: string) {
  const actor = await requirePermission('ai:preference:update::allow');
  await clearUserAiPreference(actor.id, sectionKey);
}

export async function getSemanticSuggestions(description: string) {
  // Anyone authenticated with read access to expenses may search COA.
  const actor = await requireActorOrNull();
  if (!actor) return { success: false, error: 'unauthorized' } as const;
  if (!hasPermission(actor, PERM.tile.search_coa.view)) {
    return { success: false, error: 'forbidden' } as const;
  }

  if (!description || description.trim() === '') {
    return { success: true, suggestions: [] };
  }
  try {
    const ai = await aiInvoke('acct:coa-search', 'embed', { text: description }, { actorId: actor.id });
    if (!ai.ok || !ai.embedding) {
      return { success: false, error: ai.error || 'Could not generate embedding.' };
    }
    const vectorStr = `[${ai.embedding.join(',')}]`;
    const suggestionsRes = await query(`
      SELECT code, name, name_th, account_type,
             (1 - (embedding <=> $1::vector)) as similarity
      FROM chart_of_accounts
      ORDER BY similarity DESC
      LIMIT 3
    `, [vectorStr]);
    return {
      success: true,
      suggestions: suggestionsRes.rows.map((r: any) => ({
        code: r.code,
        name: r.name,
        name_th: r.name_th,
        account_type: r.account_type,
        similarity: parseFloat((r.similarity * 100).toFixed(1)),
      })),
    };
  } catch (error: any) {
    console.error('Semantic search error:', error);
    return { success: false, error: error.message };
  }
}
