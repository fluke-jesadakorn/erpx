import 'server-only';

import { query } from '../db';

export type ThinkingLevel = 'auto' | 'low' | 'medium' | 'high';

export interface SelectableModel {
  id: number;
  name: string;
  providerId: number;
  providerName: string;
  capabilities: string[];
  contextWindow: number | null;
  description: string | null;
  isFree: boolean;
  reasoningLevels: ThinkingLevel[];
}

export interface UserAiPreference {
  modelId: number;
  modelName: string;
  providerName: string;
  thinkLevel: ThinkingLevel;
}

function validThinkLevel(value: unknown): value is ThinkingLevel {
  return value === 'auto' || value === 'low' || value === 'medium' || value === 'high';
}

export async function listSelectableModels(sectionKey: string, task: 'embed' | 'chat' | 'vision'): Promise<SelectableModel[]> {
  const result = await query<{
    id: number;
    name: string;
    provider_id: number;
    provider_name: string;
    capabilities: string[];
    context_window: number | null;
    description: string | null;
    is_free: boolean;
    reasoning_levels: string[];
  }>(
    `SELECT DISTINCT m.id, m.name, p.id AS provider_id, p.name AS provider_name,
            m.capabilities, m.context_window, m.description, m.is_free,
            m.reasoning_levels
       FROM ai_assignments a
       JOIN ai_models m ON m.id = a.model_id AND m.enabled = true
       JOIN ai_providers p ON p.id = m.provider_id AND p.enabled = true
      WHERE (a.section_key = $1 OR ($1 LIKE 'chat:%' AND a.section_key = 'chat:global'))
        AND a.task_type = $2
        AND a.enabled = true
        AND a.user_selectable = true
        AND $2 = ANY(m.capabilities)
      ORDER BY m.is_free DESC, p.name, m.name`,
    [sectionKey, task],
  );

  return result.rows.map((row) => ({
    id: row.id,
    name: row.name,
    providerId: row.provider_id,
    providerName: row.provider_name,
    capabilities: row.capabilities ?? [],
    contextWindow: row.context_window,
    description: row.description,
    isFree: row.is_free,
    reasoningLevels: (row.reasoning_levels ?? []).filter(validThinkLevel),
  }));
}

export async function loadUserAiPreference(userId: number, sectionKey: string): Promise<UserAiPreference | null> {
  const result = await query<{
    model_id: number;
    model_name: string;
    provider_name: string;
    think_level: ThinkingLevel;
  }>(
    `SELECT pref.model_id, m.name AS model_name, p.name AS provider_name, pref.think_level
       FROM folio.ai_user_section_preferences pref
       JOIN folio.ai_models m ON m.id = pref.model_id AND m.enabled = true
       JOIN folio.ai_providers p ON p.id = m.provider_id AND p.enabled = true
      WHERE pref.user_id = $1 AND pref.section_key = $2
      LIMIT 1`,
    [userId, sectionKey],
  );
  const row = result.rows[0];
  if (!row) return null;
  return {
    modelId: row.model_id,
    modelName: row.model_name,
    providerName: row.provider_name,
    thinkLevel: validThinkLevel(row.think_level) ? row.think_level : 'auto',
  };
}

export async function saveUserAiPreference(
  userId: number,
  sectionKey: string,
  modelId: number,
  thinkLevel: ThinkingLevel,
  task: 'embed' | 'chat' | 'vision' = 'chat',
): Promise<UserAiPreference> {
  if (!validThinkLevel(thinkLevel)) throw new Error('invalid thinking level');
  const allowed = await query<{ id: number }>(
    `SELECT m.id
       FROM ai_assignments a
       JOIN ai_models m ON m.id = a.model_id AND m.enabled = true
      WHERE (a.section_key = $1 OR ($1 LIKE 'chat:%' AND a.section_key = 'chat:global'))
        AND a.task_type = $3
        AND a.enabled = true AND a.user_selectable = true AND m.id = $2
      LIMIT 1`,
    [sectionKey, modelId, task],
  );
  if (!allowed.rows[0]) throw new Error('model is not whitelisted for this AI section');

  await query(
    `INSERT INTO folio.ai_user_section_preferences (user_id, section_key, model_id, think_level)
     VALUES ($1, $2, $3, $4)
     ON CONFLICT (user_id, section_key) DO UPDATE
       SET model_id = EXCLUDED.model_id,
           think_level = EXCLUDED.think_level,
           updated_at = now()`,
    [userId, sectionKey, modelId, thinkLevel],
  );
  const preference = await loadUserAiPreference(userId, sectionKey);
  if (!preference) throw new Error('preference was not saved');
  return preference;
}

export async function clearUserAiPreference(userId: number, sectionKey: string): Promise<void> {
  await query('DELETE FROM folio.ai_user_section_preferences WHERE user_id = $1 AND section_key = $2', [userId, sectionKey]);
}
