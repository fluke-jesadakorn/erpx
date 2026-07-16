import 'server-only';
import { query } from '../db';

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
  ts: string;
  meta?: Record<string, unknown>;
}

export interface ChatSession {
  userId: number;
  scope: string;
  messages: ChatMessage[];
  meta: Record<string, unknown>;
  updatedAt: string;
}

export async function loadSession(userId: number, scope: string): Promise<ChatSession | null> {
  const r = await query<{ messages: ChatMessage[] | string; meta: Record<string, unknown> | string; updated_at: string | Date }>(
    `SELECT messages, meta, updated_at FROM folio.ai_chat_sessions
      WHERE user_id = $1 AND scope = $2`,
    [userId, scope]
  );
  if (r.rows.length === 0) return null;
  const row = r.rows[0];
  return {
    userId,
    scope,
    messages: typeof row.messages === 'string' ? JSON.parse(row.messages) : row.messages ?? [],
    meta: typeof row.meta === 'string' ? JSON.parse(row.meta) : row.meta ?? {},
    updatedAt: typeof row.updated_at === 'string' ? row.updated_at : new Date(row.updated_at).toISOString(),
  };
}

export async function appendMessage(
  userId: number,
  scope: string,
  msg: ChatMessage,
  metaPatch?: Record<string, unknown>,
): Promise<void> {
  await query(
    `INSERT INTO folio.ai_chat_sessions (user_id, scope, messages, meta)
     VALUES ($1, $2, jsonb_build_array($3::jsonb), COALESCE($4::jsonb, '{}'::jsonb))
     ON CONFLICT (user_id, scope) DO UPDATE
       SET messages = folio.ai_chat_sessions.messages || jsonb_build_array($3::jsonb),
           meta = folio.ai_chat_sessions.meta || COALESCE($4::jsonb, '{}'::jsonb),
           updated_at = now()`,
    [
      userId,
      scope,
      JSON.stringify({ role: msg.role, content: msg.content, ts: msg.ts, meta: msg.meta ?? null }),
      metaPatch ? JSON.stringify(metaPatch) : null,
    ]
  );
}

export async function pruneSession(userId: number, scope: string, keepLast = 20): Promise<void> {
  await query(
    `UPDATE folio.ai_chat_sessions
        SET messages = (
          SELECT jsonb_agg(elem)
            FROM (
              SELECT elem
                FROM jsonb_array_elements(messages) AS elem
            ORDER BY (elem->>'ts') DESC
              LIMIT $3
            ) sub
        ),
        updated_at = now()
      WHERE user_id = $1 AND scope = $2`,
    [userId, scope, keepLast]
  );
}

export async function clearSession(userId: number, scope: string): Promise<void> {
  await query(
    `DELETE FROM folio.ai_chat_sessions WHERE user_id = $1 AND scope = $2`,
    [userId, scope]
  );
}

export function nowIso(): string {
  return new Date().toISOString();
}