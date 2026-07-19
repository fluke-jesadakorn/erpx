import 'server-only';
import { withTransaction } from '@/db';
import { DEFAULT_CHAT_MODEL } from '@/ai/defaults';

export interface ChatSession {
  id: string;
  userId: number;
  title: string;
  modelName: string;
  createdAt: string;
  updatedAt: string;
}

export interface SqlResolved {
  question: string;
  sql: string;
  columns: string[];
  rows: Record<string, unknown>[];
  rowCount: number;
  explanation: string;
}

export interface ChatBlocks {
  plain: string;
  charts: unknown[];
  htmls: string[];
  sqls: SqlResolved[];
}

export interface ChatMessage {
  id: string;
  sessionId: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  blocks: ChatBlocks;
  modelName?: string | null;
  latencyMs?: number | null;
  createdAt: string;
}

interface SessionRow {
  id: string;
  user_id: number;
  title: string;
  model_name: string;
  created_at: Date;
  updated_at: Date;
}

interface MessageRow {
  id: string;
  session_id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  blocks: ChatBlocks | null;
  model_name: string | null;
  latency_ms: number | null;
  created_at: Date;
}

function mapSession(r: SessionRow): ChatSession {
  return {
    id: r.id,
    userId: r.user_id,
    title: r.title,
    modelName: r.model_name,
    createdAt: r.created_at.toISOString(),
    updatedAt: r.updated_at.toISOString(),
  };
}

function mapMessage(r: MessageRow): ChatMessage {
  return {
    id: r.id,
    sessionId: r.session_id,
    role: r.role,
    content: r.content,
    blocks: r.blocks ?? { plain: '', charts: [], htmls: [], sqls: [] },
    modelName: r.model_name,
    latencyMs: r.latency_ms,
    createdAt: r.created_at.toISOString(),
  };
}

export async function listSessions(userId: number): Promise<ChatSession[]> {
  return withTransaction(async (q) => {
    await q(`SELECT set_config('app.user_id', $1, true)`, [String(userId)]);
    const r = await q<SessionRow>(
      `SELECT id, user_id, title, model_name, created_at, updated_at
         FROM chat.sessions
        WHERE user_id = $1::int
        ORDER BY updated_at DESC
        LIMIT 200`,
      [userId],
    );
    return r.rows.map(mapSession);
  });
}

export async function createSession(
  userId: number,
  title?: string,
  modelName?: string,
): Promise<ChatSession> {
  return withTransaction(async (q) => {
    await q(`SELECT set_config('app.user_id', $1, true)`, [String(userId)]);
    const r = await q<SessionRow>(
      `INSERT INTO chat.sessions (user_id, title, model_name)
       VALUES ($1::int, $2, $3)
       RETURNING id, user_id, title, model_name, created_at, updated_at`,
      [userId, title ?? 'New chat', modelName ?? DEFAULT_CHAT_MODEL],
    );
    return mapSession(r.rows[0]);
  });
}

export async function loadSession(
  userId: number,
  sessionId: string,
): Promise<{ session: ChatSession; messages: ChatMessage[] } | null> {
  return withTransaction(async (q) => {
    await q(`SELECT set_config('app.user_id', $1, true)`, [String(userId)]);
    const sRes = await q<SessionRow>(
      `SELECT id, user_id, title, model_name, created_at, updated_at
         FROM chat.sessions
        WHERE id = $1::uuid AND user_id = $2::int
        LIMIT 1`,
      [sessionId, userId],
    );
    const sRow = sRes.rows[0];
    if (!sRow) return null;
    const mRes = await q<MessageRow>(
      `SELECT id, session_id, role, content, blocks, model_name, latency_ms, created_at
         FROM chat.messages
        WHERE session_id = $1::uuid
        ORDER BY created_at ASC, id ASC`,
      [sessionId],
    );
    return { session: mapSession(sRow), messages: mRes.rows.map(mapMessage) };
  });
}

export async function renameSession(userId: number, sessionId: string, title: string): Promise<void> {
  await withTransaction(async (q) => {
    await q(`SELECT set_config('app.user_id', $1, true)`, [String(userId)]);
    await q(
      `UPDATE chat.sessions
          SET title = $3, updated_at = now()
        WHERE id = $1::uuid AND user_id = $2::int`,
      [sessionId, userId, title],
    );
  });
}

export async function deleteSession(userId: number, sessionId: string): Promise<void> {
  await withTransaction(async (q) => {
    await q(`SELECT set_config('app.user_id', $1, true)`, [String(userId)]);
    await q(
      `DELETE FROM chat.sessions
        WHERE id = $1::uuid AND user_id = $2::int`,
      [sessionId, userId],
    );
  });
}

export async function rewindSession(
  userId: number,
  sessionId: string,
  messageId: string,
): Promise<void> {
  await withTransaction(async (q) => {
    await q(`SELECT set_config('app.user_id', $1, true)`, [String(userId)]);
    const target = await q<{ id: string; role: string }>(
      `SELECT m.id, m.role
         FROM chat.messages m
         JOIN chat.sessions s ON s.id = m.session_id
        WHERE m.id = $1::bigint
          AND m.session_id = $2::uuid
          AND s.user_id = $3::int
        FOR UPDATE`,
      [messageId, sessionId, userId],
    );
    if (target.rows[0]?.role !== 'user') throw new Error('editable checkpoint not found');
    await q(
      `DELETE FROM chat.messages
        WHERE session_id = $1::uuid
          AND id >= $2::bigint`,
      [sessionId, messageId],
    );
    await q(
      `UPDATE chat.sessions SET updated_at = now() WHERE id = $1::uuid`,
      [sessionId],
    );
  });
}

export interface AppendMessageInput {
  role: 'user' | 'assistant' | 'system';
  content: string;
  blocks?: ChatBlocks;
  modelName?: string | null;
  latencyMs?: number | null;
}

export async function appendMessage(
  userId: number,
  sessionId: string,
  msg: AppendMessageInput,
): Promise<ChatMessage> {
  return withTransaction(async (q) => {
    await q(`SELECT set_config('app.user_id', $1, true)`, [String(userId)]);
    const own = await q<{ id: string }>(
      `SELECT id FROM chat.sessions
        WHERE id = $1::uuid AND user_id = $2::int
        FOR UPDATE`,
      [sessionId, userId],
    );
    if (own.rows.length === 0) {
      throw new Error('session not found');
    }
    const blocksJson = JSON.stringify(msg.blocks ?? { plain: msg.content, charts: [], htmls: [], sqls: [] });
    const r = await q<MessageRow>(
      `INSERT INTO chat.messages (session_id, role, content, blocks, model_name, latency_ms)
       VALUES ($1::uuid, $2, $3, $4::jsonb, $5, $6)
       RETURNING id, session_id, role, content, blocks, model_name, latency_ms, created_at`,
      [sessionId, msg.role, msg.content, blocksJson, msg.modelName ?? null, msg.latencyMs ?? null],
    );
    await q(
      `UPDATE chat.sessions SET updated_at = now() WHERE id = $1::uuid`,
      [sessionId],
    );
    return mapMessage(r.rows[0]);
  });
}

export async function maybeAutoRename(
  _userId: number,
  _sessionId: string,
  _triggerContent: string,
): Promise<string | null> {
  return null;
}
