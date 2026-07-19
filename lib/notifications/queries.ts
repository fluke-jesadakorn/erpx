import 'server-only';

import { cache } from 'react';
import { query } from '@/db';
import { renderNotificationMessage, type NotificationArgs, type NotificationCategory } from './catalog';

export type { NotificationCategory };

export type NotificationAudience = 'owner' | 'approver' | 'watcher';
export type NotificationSeverity = 'info' | 'success' | 'warning' | 'error';
export type NotificationView = 'actions' | 'notifications' | 'all';
export type NotificationReadFilter = 'all' | 'unread' | 'read';

export interface NotificationItem {
  id: string;
  eventId: string | null;
  type: string;
  category: NotificationCategory;
  audience: NotificationAudience;
  domain: 'expense' | 'so' | 'pr' | 'po' | 'other';
  stageKey: string | null;
  messageKey: string;
  messageArgs: NotificationArgs;
  message: string;
  severity: NotificationSeverity;
  href: string | null;
  waybillId: string | null;
  refType: string | null;
  refId: number | null;
  readAt: string | null;
  resolvedAt: string | null;
  resolvedBy: string | null;
  firstOpenedAt: string | null;
  lastOpenedAt: string | null;
  openCount: number;
  createdAt: string;
}

export interface ListUserNotificationsOpts {
  view?: NotificationView;
  read?: NotificationReadFilter;
  domain?: 'expense' | 'so' | 'pr' | 'all';
  watchingOnly?: boolean;
  cursor?: string | null;
  since?: string | null;
}

export const notificationSchemaReady = cache(async (): Promise<boolean> => {
  const r = await query<{ ready: boolean }>(
    `SELECT EXISTS (
       SELECT 1
         FROM pg_attribute
        WHERE attrelid = to_regclass('notifications')
          AND attname = 'waybill_id'
          AND NOT attisdropped
     ) AS ready`,
  );
  return r.rows[0]?.ready ?? false;
});

interface NotificationDbRow {
  id: number;
  event_id: string | null;
  type: string;
  category: NotificationCategory;
  audience: NotificationAudience;
  origin: 'expense' | 'so' | 'pr' | 'po' | null;
  stage_key: string | null;
  message_key: string | null;
  payload_json: Record<string, unknown> | null;
  severity: NotificationSeverity | null;
  href: string | null;
  waybill_id: string | null;
  target_type: string | null;
  target_id: number | null;
  read_at: Date | string | null;
  resolved_at: Date | string | null;
  resolved_by_name: string | null;
  first_opened_at: Date | string | null;
  last_opened_at: Date | string | null;
  open_count: number;
  created_at: Date | string;
}

function iso(value: Date | string | null | undefined): string | null {
  if (value == null) return null;
  return typeof value === 'string' ? value : value.toISOString();
}

function safeArgs(value: unknown): NotificationArgs {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
  return value as NotificationArgs;
}

function mapRow(row: NotificationDbRow): NotificationItem {
  const args = safeArgs(row.payload_json);
  const messageKey = row.message_key ?? row.type;
  return {
    id: String(row.id),
    eventId: row.event_id,
    type: row.type,
    category: row.category,
    audience: row.audience,
    domain: row.origin ?? 'other',
    stageKey: row.stage_key,
    messageKey,
    messageArgs: args,
    message: typeof args.message === 'string' ? args.message : renderNotificationMessage(messageKey, args),
    severity: row.severity ?? (typeof args.severity === 'string' ? args.severity as NotificationSeverity : 'info'),
    href: row.href ?? (row.waybill_id ? `/waybill/${encodeURIComponent(row.waybill_id)}` : null),
    waybillId: row.waybill_id,
    refType: row.target_type,
    refId: row.target_id == null ? null : Number(row.target_id),
    readAt: iso(row.read_at),
    resolvedAt: iso(row.resolved_at),
    resolvedBy: row.resolved_by_name,
    firstOpenedAt: iso(row.first_opened_at),
    lastOpenedAt: iso(row.last_opened_at),
    openCount: Number(row.open_count ?? 0),
    createdAt: iso(row.created_at) ?? new Date(0).toISOString(),
  };
}

function buildWhere(actorId: number, opts: ListUserNotificationsOpts): { sql: string; params: unknown[] } {
  const where = ['n.user_id = $1'];
  const params: unknown[] = [actorId];
  const view = opts.view ?? 'all';

  if (view === 'actions') where.push("n.category = 'action' AND n.resolved_at IS NULL");
  if (view === 'notifications') where.push("(n.category = 'update' OR n.resolved_at IS NOT NULL)");
  if (opts.read === 'unread') where.push('n.read_at IS NULL');
  if (opts.read === 'read') where.push('n.read_at IS NOT NULL');
  if (opts.watchingOnly) where.push("n.audience = 'watcher'");
  if (opts.domain && opts.domain !== 'all') {
    params.push(opts.domain);
    where.push(`wb.origin = $${params.length}`);
  }
  if (opts.cursor && /^\d+$/.test(opts.cursor)) {
    params.push(Number(opts.cursor));
    where.push(`n.id < $${params.length}`);
  }
  if (opts.since && !Number.isNaN(Date.parse(opts.since))) {
    params.push(opts.since);
    where.push(`n.created_at > $${params.length}::timestamptz`);
  }

  return { sql: where.join(' AND '), params };
}

const selectSql = `
  SELECT n.id, n.event_id, n.type, n.category, n.audience,
         wb.origin, n.stage_key, n.message_key, n.payload_json,
         n.severity, n.href, n.waybill_id, n.target_type, n.target_id,
         n.read_at, n.resolved_at, resolved.fullname AS resolved_by_name,
         n.first_opened_at, n.last_opened_at, n.open_count, n.created_at
    FROM notifications n
    LEFT JOIN waybills wb ON wb.id = n.waybill_id
    LEFT JOIN users resolved ON resolved.id = n.resolved_by
`;

export async function listUserNotifications(
  actorId: number,
  limit = 30,
  opts: ListUserNotificationsOpts = {},
): Promise<NotificationItem[]> {
  const bounded = Math.max(1, Math.min(100, limit));
  const built = buildWhere(actorId, opts);
  const params = [...built.params, bounded];
  const r = await query<NotificationDbRow>(
    `${selectSql}
     WHERE ${built.sql}
     ORDER BY n.id DESC
     LIMIT $${params.length}`,
    params,
  );
  return r.rows.map(mapRow);
}

export async function listUnreadCount(actorId: number): Promise<number> {
  const r = await query<{ n: number }>(
    `SELECT COUNT(*)::int AS n
       FROM notifications
      WHERE user_id = $1 AND read_at IS NULL`,
    [actorId],
  );
  return r.rows[0]?.n ?? 0;
}

export async function listActionCount(actorId: number): Promise<number> {
  const r = await query<{ n: number }>(
    `SELECT COUNT(*)::int AS n
       FROM notifications
      WHERE user_id = $1 AND category = 'action' AND resolved_at IS NULL`,
    [actorId],
  );
  return r.rows[0]?.n ?? 0;
}

export async function getNotificationForUser(actorId: number, id: number): Promise<NotificationItem | null> {
  const r = await query<NotificationDbRow>(
    `${selectSql}
     WHERE n.user_id = $1 AND n.id = $2
     LIMIT 1`,
    [actorId, id],
  );
  return r.rows[0] ? mapRow(r.rows[0]) : null;
}

export async function openNotification(actorId: number, id: number): Promise<NotificationItem | null> {
  const r = await query<NotificationDbRow>(
    `WITH opened AS (
       UPDATE notifications
          SET read_at = COALESCE(read_at, now()),
              first_opened_at = COALESCE(first_opened_at, now()),
              last_opened_at = now(),
              open_count = open_count + 1
        WHERE user_id = $1 AND id = $2
        RETURNING id
     )
     ${selectSql}
      JOIN opened ON opened.id = n.id
      WHERE n.user_id = $1`,
    [actorId, id],
  );
  return r.rows[0] ? mapRow(r.rows[0]) : null;
}

export type ReadStateMode = 'mark' | 'unmark';

export async function setReadState(
  actorId: number,
  ids: number[],
  mode: ReadStateMode,
  all = false,
): Promise<{ updated: number }> {
  const readAt = mode === 'mark' ? 'now()' : 'NULL';
  if (all) {
    const r = await query(
      `UPDATE notifications SET read_at = ${readAt} WHERE user_id = $1`,
      [actorId],
    );
    return { updated: r.rowCount ?? 0 };
  }
  if (ids.length === 0) return { updated: 0 };
  const r = await query(
    `UPDATE notifications SET read_at = ${readAt}
      WHERE user_id = $1 AND id = ANY($2::int[])`,
    [actorId, ids],
  );
  return { updated: r.rowCount ?? 0 };
}

export async function deleteNotifications(
  actorId: number,
  ids: number[],
  all = false,
): Promise<{ deleted: number; blocked: number }> {
  const allowed = all
    ? await query<{ id: number }>(
        `DELETE FROM notifications
          WHERE user_id = $1 AND (category = 'update' OR resolved_at IS NOT NULL)
          RETURNING id`,
        [actorId],
      )
    : ids.length > 0
      ? await query<{ id: number }>(
          `DELETE FROM notifications
            WHERE user_id = $1 AND id = ANY($2::int[])
              AND (category = 'update' OR resolved_at IS NOT NULL)
            RETURNING id`,
          [actorId, ids],
        )
      : { rows: [], rowCount: 0 };

  const blocked = all
    ? await query<{ n: number }>(
        `SELECT COUNT(*)::int AS n FROM notifications
          WHERE user_id = $1 AND category = 'action' AND resolved_at IS NULL`,
        [actorId],
      )
    : { rows: [{ n: Math.max(0, ids.length - (allowed.rowCount ?? 0)) }] };
  return { deleted: allowed.rowCount ?? 0, blocked: blocked.rows[0]?.n ?? 0 };
}

export async function listRecentNotificationsForUser(actorId: number, limit = 50): Promise<NotificationItem[]> {
  return listUserNotifications(actorId, limit, { view: 'all' });
}
