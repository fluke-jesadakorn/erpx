import 'server-only';

import { query } from '@/db';

function safeParse(s: string): any {
  try { return JSON.parse(s); } catch { return {}; }
}

function severityClass(sev: string | null | undefined): string {
  switch (sev) {
    case 'success': return 'bg-emerald-500/15 text-emerald-300 border-emerald-500/30';
    case 'warning': return 'bg-amber-500/15 text-amber-300 border-amber-500/30';
    case 'error':   return 'bg-rose-500/15 text-rose-300 border-rose-500/30';
    default:        return 'bg-slate-500/15 text-slate-300 border-slate-500/30';
  }
}

export async function listRecentNotifications(limit = 30) {
  const r = await query(
    `SELECT id, type, payload, severity, created_at
     FROM domain_events
     ORDER BY id DESC
     LIMIT $1`,
    [limit]
  );
  return r.rows.map((row: any) => {
    const payload = typeof row.payload === 'string' ? safeParse(row.payload) : (row.payload || {});
    return {
      id: String(row.id),
      type: row.type,
      message: payload?.message || row.type,
      actorId: null as number | null,
      actorName: null as string | null,
      refType: null as string | null,
      refId: null as number | null,
      severity: (row.severity || 'info') as 'info' | 'success' | 'warning' | 'error',
      severityClass: severityClass(row.severity),
      createdAt: typeof row.created_at === 'string' ? row.created_at : new Date(row.created_at).toISOString(),
    };
  });
}

export interface ListUserNotificationsOpts {
  includeCleared?: boolean;
  onlyUnread?: boolean;
}

export async function listUserNotifications(
  actorId: number,
  limit = 30,
  opts: ListUserNotificationsOpts = {},
) {
  const where: string[] = ['user_id = $1'];
  const params: any[] = [actorId];
  if (!opts.includeCleared) where.push('cleared_at IS NULL');
  if (opts.onlyUnread) where.push('read_at IS NULL');
  params.push(limit);

  const r = await query(
    `SELECT id, type, payload_json, target_type, target_id, read_at, cleared_at, created_at
     FROM notifications
     WHERE ${where.join(' AND ')}
     ORDER BY (read_at IS NULL) DESC, id DESC
     LIMIT $${params.length}`,
    params,
  );
  if (r.rows.length === 0) return [];

  return r.rows.map((row: any) => {
    const payload = typeof row.payload_json === 'string' ? safeParse(row.payload_json) : (row.payload_json || {});
    const actorIdFromPayload = (payload?.actorId as number | null) ?? null;
    const actorName = (payload?.actorName as string | null) ?? null;
    const severity = (payload?.severity as string | null) || 'info';
    return {
      id: String(row.id),
      type: row.type,
      message: payload?.message || row.type,
      actorId: actorIdFromPayload,
      actorName,
      refType: row.target_type as string | null,
      refId: row.target_id != null ? Number(row.target_id) : null,
      severity: severity as 'info' | 'success' | 'warning' | 'error',
      severityClass: severityClass(severity),
      readAt: row.read_at ? new Date(row.read_at).toISOString() : null,
      clearedAt: row.cleared_at ? new Date(row.cleared_at).toISOString() : null,
      createdAt: typeof row.created_at === 'string' ? row.created_at : new Date(row.created_at).toISOString(),
    };
  });
}

export async function listUnreadCount(actorId: number): Promise<number> {
  const r = await query<{ n: number }>(
    `SELECT COUNT(*)::int AS n FROM notifications WHERE user_id = $1 AND read_at IS NULL AND cleared_at IS NULL`,
    [actorId]
  );
  return r.rows[0]?.n ?? 0;
}

export type ReadStateMode = 'mark' | 'unmark' | 'clear';

export async function setReadState(
  actorId: number,
  ids: number[],
  mode: ReadStateMode,
  all = false,
): Promise<{ updated: number }> {
  if (mode === 'clear') {
    if (all) {
      const r = await query(
        `UPDATE notifications SET cleared_at = NOW()
         WHERE user_id = $1 AND cleared_at IS NULL`,
        [actorId]
      );
      return { updated: r.rowCount ?? 0 };
    }
    if (ids.length === 0) return { updated: 0 };
    const r = await query(
      `UPDATE notifications SET cleared_at = NOW()
       WHERE user_id = $1 AND id = ANY($2::bigint[]) AND cleared_at IS NULL`,
      [actorId, ids]
    );
    return { updated: r.rowCount ?? 0 };
  }

  if (mode === 'mark') {
    if (all) {
      const r = await query(
        `UPDATE notifications SET read_at = NOW()
         WHERE user_id = $1 AND read_at IS NULL AND cleared_at IS NULL`,
        [actorId]
      );
      return { updated: r.rowCount ?? 0 };
    }
    if (ids.length === 0) return { updated: 0 };
    const r = await query(
      `UPDATE notifications SET read_at = NOW()
       WHERE user_id = $1 AND id = ANY($2::bigint[]) AND read_at IS NULL AND cleared_at IS NULL`,
      [actorId, ids]
    );
    return { updated: r.rowCount ?? 0 };
  }

  if (ids.length === 0) return { updated: 0 };
  const r = await query(
    `UPDATE notifications SET read_at = NULL
     WHERE user_id = $1 AND id = ANY($2::bigint[]) AND cleared_at IS NULL`,
    [actorId, ids]
  );
  return { updated: r.rowCount ?? 0 };
}
