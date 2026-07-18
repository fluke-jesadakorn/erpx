'use server';

import { query } from '@/db';
import { aiInvoke } from '@/ai/router';
import { loadActor, type ActorWithScope } from '@/server/guard';
import { hasPermission, PERM } from '@/perm/server';

async function requireActorOrNull(): Promise<ActorWithScope | null> {
  return loadActor();
}

function notImplemented(name: string): never {
  throw new Error(`${name} is not implemented`);
}

export async function listModels(_capability?: string): Promise<unknown[]> {
  const actor = await requireActorOrNull();
  if (!actor) return [];
  if (!hasPermission(actor, PERM.ai.model.read)) return [];
  try {
    const res = await query(
      `SELECT m.id, m.name, m.provider_id, m.capabilities, m.context_window, m.enabled, m.description
         FROM ai_models m WHERE m.enabled = true
        ORDER BY m.name`,
    );
    return res.rows.map((r: any) => ({
      id: r.id,
      name: r.name,
      provider_id: r.provider_id,
      capabilities: r.capabilities ?? [],
      context_window: r.context_window,
      enabled: r.enabled,
      description: r.description,
    }));
  } catch {
    return [];
  }
}

export async function createProvider(_input: object) {
  notImplemented('createProvider');
}

export async function updateProvider(_id: number, _patch: object) {
  notImplemented('updateProvider');
}

export async function deleteProvider(_id: number) {
  notImplemented('deleteProvider');
}

export async function testProvider(_id: number): Promise<{ ok: boolean; modelCount: number; baseUrl: string; latencyMs: number; error?: string }> {
  notImplemented('testProvider');
}

export async function createModel(_input: object) {
  notImplemented('createModel');
}

export async function updateModel(_id: number, _patch: object) {
  notImplemented('updateModel');
}

export async function deleteModel(_id: number) {
  notImplemented('deleteModel');
}

export async function createAssignment(_input: object) {
  notImplemented('createAssignment');
}

export async function updateAssignment(_id: number, _patch: object) {
  notImplemented('updateAssignment');
}

export async function deleteAssignment(_id: number) {
  notImplemented('deleteAssignment');
}

export async function setMyDefault(_sectionKey: string, _modelId: number) {
  notImplemented('setMyDefault');
}

export async function deleteMyDefault(_sectionKey: string) {
  notImplemented('deleteMyDefault');
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
    const ai = await aiInvoke('acct:coa-search', 'embed', { text: description });
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