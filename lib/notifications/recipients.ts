import 'server-only';

import { query } from '@folio-lib/db';

export interface RecipientContext {
  type: string;
  refType: string | null;
  refId: number | null;
  actorId: number | null;
  payload?: Record<string, any>;
}

const NO_FANOUT_TYPES = new Set(['policy.updated']);

const DOMAIN_BY_REF: Record<string, string> = {
  expense: 'expenses',
  pr: 'pr',
  po: 'po',
  slip: 'slips',
  user: 'users',
  department: 'departments',
  audit: 'audit',
  ai: 'ai_settings',
  notification: 'notifications',
};

function mapEventToDomain(type: string, refType: string | null): string | null {
  if (refType && DOMAIN_BY_REF[refType]) return DOMAIN_BY_REF[refType];
  if (type.startsWith('expense.')) return 'expenses';
  if (type.startsWith('pr.')) return 'pr';
  if (type.startsWith('po.')) return 'po';
  if (type.startsWith('slip.')) return 'slips';
  if (type.startsWith('user.')) return 'users';
  if (type.startsWith('audit.')) return 'audit';
  if (type.startsWith('ai.')) return 'ai_settings';
  if (type.startsWith('notification.') || type.startsWith('notif.')) return 'notifications';
  return null;
}

export async function computeRecipients(ctx: RecipientContext): Promise<number[]> {
  if (NO_FANOUT_TYPES.has(ctx.type)) return [];
  const ids = new Set<number>();
  const ownerId = await lookupRefOwner(ctx.refType, ctx.refId);
  const submitterId = ownerId ?? ctx.actorId ?? null;
  const causeId = ctx.actorId ?? null;

  if (ctx.type.startsWith('ceo.override')) {
    if (ownerId) ids.add(ownerId);
  } else if (ctx.type.startsWith('expense.')) {
    if (submitterId) ids.add(submitterId);
    if (causeId && causeId !== submitterId) ids.add(causeId);
  } else if (ctx.type.startsWith('pr.') || ctx.type.startsWith('po.')) {
    if (submitterId) ids.add(submitterId);
    if (causeId && causeId !== submitterId) ids.add(causeId);
  } else {
    if (causeId) ids.add(causeId);
  }

  if (submitterId) {
    const sup = await lookupSupervisor(submitterId);
    if (sup && sup !== submitterId) ids.add(sup);
  }

  const domainId = mapEventToDomain(ctx.type, ctx.refType);
  if (domainId) {
    const broadcast = await expandByDomainScope(domainId, ctx.refType, ctx.refId);
    for (const uid of broadcast) ids.add(uid);
  }

  return [...ids];
}

async function expandByDomainScope(
  domainId: string,
  refType: string | null,
  refId: number | null,
): Promise<number[]> {
  const roles = await query<{ user_id: number; scope_kind: string }>(
    `SELECT u.id AS user_id, 'all'::text AS scope_kind
       FROM users u
      WHERE u.is_active IS NOT FALSE`,
  );

  if (roles.rows.length === 0) return [];

  let _anchorDept: string | null = null;
  let anchorSubmitter: number | null = null;
  if (refType === 'expense' && refId) {
    const r = await query<{ submitter_id: number | null }>(
      `SELECT submitter_id FROM expenses WHERE id = $1`,
      [refId],
    );
    anchorSubmitter = r.rows[0]?.submitter_id ?? null;
    if (anchorSubmitter) {
      const u = await query<{ dept_id: string | null }>(
        `SELECT split_part(up.permission_id, ':', 3) AS dept_id
           FROM perm.user_permissions up
          WHERE up.user_id = $1
            AND up.permission_id LIKE 'user:dept:%'
            AND up.revoked_at IS NULL
            AND (up.ends_at IS NULL OR up.ends_at > now())
          ORDER BY up.permission_id
          LIMIT 1`,
        [anchorSubmitter],
      );
      _anchorDept = u.rows[0]?.dept_id ?? null;
    }
  }

  const out = new Set<number>();
  for (const row of roles.rows) {
    out.add(row.user_id);
  }
  return [...out];
}

async function lookupRefOwner(refType: string | null, refId: number | null): Promise<number | null> {
  if (!refType || !refId) return null;
  if (refType === 'expense') {
    const r = await query<{ submitter_id: number | null }>(
      `SELECT submitter_id FROM expenses WHERE id = $1`,
      [refId]
    );
    return r.rows[0]?.submitter_id ?? null;
  }
  if (refType === 'pr') {
    const r = await query<{ requester_id: number | null }>(
      `SELECT requester_id FROM purchase_requisitions WHERE id = $1`,
      [refId]
    );
    return r.rows[0]?.requester_id ?? null;
  }
  if (refType === 'po') {
    const r = await query<{ requester_id: number | null }>(
      `SELECT pr.requester_id FROM purchase_orders po JOIN purchase_requisitions pr ON pr.id = po.pr_id WHERE po.id = $1`,
      [refId]
    );
    return r.rows[0]?.requester_id ?? null;
  }
  return null;
}

async function lookupSupervisor(_userId: number): Promise<number | null> {
  return null;
}
