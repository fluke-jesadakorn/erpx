import 'server-only';

import { query } from '@/db';

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

const DOMAIN_TILE_PERM: Record<string, string | null> = {
  expenses: 'tile:expense:view::allow',
  pr: 'tile:pr:view::allow',
  po: 'tile:po:view::allow',
  slips: 'tile:expense:view::allow',
  users: 'tile:roles:view::allow',
  departments: 'tile:org_chart:view::allow',
  audit: 'tile:audit:view::allow',
  ai_settings: 'tile:models:view::allow',
  notifications: null,
  customers: 'tile:customers:view::allow',
  sales: 'tile:sales:view::allow',
};

const ADMIN_PERM = 'admin:system:bypass::allow';

const domainFanoutCache: Map<string, Set<number>> = new Map();

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
  domainFanoutCache.clear();
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
    const broadcast = await expandByDomainScope(domainId);
    for (const uid of broadcast) ids.add(uid);
  }

  return [...ids];
}

async function expandByDomainScope(domainId: string): Promise<number[]> {
  const cached = domainFanoutCache.get(domainId);
  if (cached) return [...cached];

  const tilePerm = DOMAIN_TILE_PERM[domainId];
  if (tilePerm === undefined) return [];
  const empty: number[] = [];
  if (tilePerm === null) {
    domainFanoutCache.set(domainId, new Set(empty));
    return empty;
  }

  const r = await query<{ id: number }>(
    `SELECT DISTINCT u.id
       FROM users u
      WHERE u.is_active IS NOT FALSE
        AND (
          EXISTS (SELECT 1 FROM perm.user_permissions up
                   WHERE up.user_id = u.id AND up.revoked_at IS NULL
                     AND (up.ends_at IS NULL OR up.ends_at > now())
                     AND up.permission_id IN ($1, $2))
          OR EXISTS (SELECT 1 FROM perm.user_roles ur
                     JOIN perm.role_permissions rp ON rp.role_id = ur.role_id
                    WHERE ur.user_id = u.id
                      AND rp.permission_id IN ($1, $2))
        )`,
    [tilePerm, ADMIN_PERM],
  );

  const out = new Set<number>();
  for (const row of r.rows) out.add(row.id);
  domainFanoutCache.set(domainId, out);
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

async function lookupSupervisor(userId: number): Promise<number | null> {
  const r = await query<{ head_user_id: number | null }>(
    `SELECT parent.head_user_id
       FROM perm.user_roles ur
       JOIN perm.roles own ON own.id = ur.role_id
       LEFT JOIN perm.roles parent ON parent.id = own.parent_role_id
      WHERE ur.user_id = $1
        AND own.is_system = false
      ORDER BY split_part(ur.role_id, '::', 2)::int ASC, ur.granted_at ASC
      LIMIT 1`,
    [userId],
  );
  return r.rows[0]?.head_user_id ?? null;
}
