// Centralized server-side guard for the consolidated lib.
//
// All access checks go through the perm-string system via matchPerm().
// No legacy rbac matrix; no separate dept column on users.

import 'server-only';
import { query } from '../db';
import { verifySession, type SessionPayload } from './sessionToken';
import { validateActiveSession } from './sessionStore';
import { matchPerm, parseRoleId } from '../perm/grammar';
import { getActorScope, type ActorScope } from '../perm/scope';
import { loadDeptPermissionBundles, expandUserPermissions } from '../perm/deptGrant';
import { authorize } from '../perm/authorize';

export interface SessionActor {
  id: number;
  employee_code: string;
  fullname: string;
  department: string | null;
  dept_group_name: string | null;
  role_id: string;
  role_name: string;
  level: number;
  dept_id: string | null;
  permissions: string[];
}

export class GuardError extends Error {
  status: number;
  constructor(message: string, status = 403) {
    super(message);
    this.name = 'GuardError';
    this.status = status;
  }
}

export interface ActorWithScope extends SessionActor {
  isHod: boolean;
  isHr: boolean;
  isHrManager: boolean;
  isItOrAdmin: boolean;
  isCeoOrAdmin: boolean;
}

export function plainHeaders(headers: Headers | Record<string, string | string[] | undefined>): Record<string, string | string[] | undefined> {
  if (headers instanceof Headers) {
    const out: Record<string, string | string[] | undefined> = {};
    headers.forEach((v, k) => { out[k.toLowerCase()] = v; });
    return out;
  }
  const out: Record<string, string | string[] | undefined> = {};
  for (const [k, v] of Object.entries(headers)) out[k.toLowerCase()] = v;
  return out;
}

function enrichActor(a: SessionActor): ActorWithScope {
  const perms = a.permissions ?? [];
  const role = a.role_name;
  return {
    ...a,
    isHod: matchPerm(perms, 'user:subtree:edit::allow'),
    isHr: matchPerm(perms, 'tile:directory:view::allow'),
    isHrManager: matchPerm(perms, 'user:role:assign::allow'),
    isItOrAdmin: a.department === 'it' || matchPerm(perms, 'admin:system:bypass::allow'),
    isCeoOrAdmin: role === 'ceo' || matchPerm(perms, 'admin:system:bypass::allow'),
  };
}

export async function loadActor(): Promise<ActorWithScope | null> {
  const a = await loadActorRaw();
  if (!a) return null;
  return enrichActor(a);
}

export async function loadActorRaw(): Promise<SessionActor | null> {
  const payload = await currentSession();
  if (!payload) return null;

  const res = await query<{
    id: number;
    employee_code: string;
    fullname: string;
    role_id: string | null;
    rank: number | null;
    department_id: string | null;
    permissions: string[];
  }>(
    `SELECT u.id, u.employee_code, u.fullname,
       COALESCE((
         SELECT ur.role_id FROM perm.user_roles ur
          WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
          ORDER BY ur.granted_at ASC
          LIMIT 1
       ), NULL) AS role_id,
       (SELECT r.rank FROM perm.user_roles ur
          JOIN perm.roles r ON r.id = ur.role_id
         WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
         LIMIT 1) AS rank,
       (SELECT ud.department_id FROM perm.user_departments ud WHERE ud.user_id = u.id) AS department_id,
       COALESCE((
         SELECT array_agg(DISTINCT p_id ORDER BY p_id)
           FROM (
             SELECT rp.permission_id AS p_id
               FROM perm.user_roles ur
               JOIN perm.role_permissions rp ON rp.role_id = ur.role_id
              WHERE ur.user_id = u.id
             UNION
             SELECT permission_id AS p_id
               FROM perm.user_permissions
              WHERE user_id = u.id AND revoked_at IS NULL
                AND (ends_at IS NULL OR ends_at > now())
             UNION
             SELECT dp.permission_id AS p_id
               FROM perm.user_departments ud
               JOIN perm.department_permissions dp ON dp.department_id = ud.department_id
              WHERE ud.user_id = u.id
             UNION
             SELECT 'user:dept:' || ud.department_id || '::allow' AS p_id
               FROM perm.user_departments ud
              WHERE ud.user_id = u.id
           ) t
       ), ARRAY[]::text[]) AS permissions
      FROM users u
     WHERE u.id = $1 AND u.is_active IS TRUE`,
    [payload.sub],
  );
  const row = res.rows[0];
  if (!row) return null;
  const parsed = parseRoleId(row.role_id ?? '');
  const deptId = row.department_id;
  const bundles = await loadDeptPermissionBundles();
  const base = row.permissions ?? [];
  const expanded = expandUserPermissions(base, bundles);
  return {
    id: row.id,
    employee_code: row.employee_code,
    fullname: row.fullname,
    department: deptId,
    dept_group_name: deptId,
    role_id: row.role_id ?? 'unconfigured',
    role_name: parsed?.name ?? 'unconfigured',
    level: row.rank ?? parsed?.level ?? 99,
    dept_id: deptId,
    permissions: Array.from(expanded),
  };
}

export async function requireActor(): Promise<ActorWithScope> {
  const a = await loadActor();
  if (!a) throw new GuardError('unauthorized', 401);
  return a;
}

export async function currentSession(): Promise<SessionPayload | null> {
  const token = await currentToken();
  const payload = await verifySession(token);
  return payload ? validateActiveSession(payload) : null;
}

async function currentToken(): Promise<string | null> {
  try {
    const { cookies } = await import('next/headers');
    const c = await cookies();
    const cookieToken = c.get('folio_session')?.value ?? null;
    if (cookieToken) return cookieToken;
  } catch { /* not in a request context */ }
  return null;
}

export function requireTab(actor: ActorWithScope, tab: string): void {
  if (matchPerm(actor.permissions, 'admin:system:bypass::allow')) return;
  const permByTab: Record<string, string> = {
    workbench: 'tile:inbox:view::allow',
    pr: 'tile:pr:view::allow',
    ledger: 'tile:ledger:view::allow',
    cockpit: 'tile:cockpit:view::allow',
    policy: 'tile:policy:view::allow',
    settings: 'tile:settings:view::allow',
    hr: 'tile:hr:view::allow',
  };
  const required = permByTab[tab];
  if (required && !matchPerm(actor.permissions, required)) {
    throw new GuardError(`role "${actor.role_name}" cannot access tab "${tab}"`, 403);
  }
}

export interface RequireActionOpts {
  perm?: string;
  stage?: string;
}

export interface RequireActionResult {
  allowed: boolean;
  override: boolean;
  reason?: string;
}

export async function requireAction(
  actor: ActorWithScope,
  _actionName: string,
  opts: RequireActionOpts = {},
): Promise<RequireActionResult> {
  if (opts.stage || opts.perm) {
    const decision = await authorize(
      {
        id: actor.id,
        permissions: actor.permissions,
        deptId: actor.dept_id,
        level: actor.level,
        roleName: actor.role_name,
      },
      opts.stage
        ? { kind: 'stage', stage: opts.stage }
        : { kind: 'perm', perm: opts.perm as string },
    );
    if (!decision.allow) throw new GuardError(decision.reason, 403);
  }
  return { allowed: true, override: false };
}

export async function slipOwnership(key: string, actor: ActorWithScope): Promise<boolean> {
  if (matchPerm(actor.permissions, 'admin:system:bypass::allow')) return true;
  const r = await query<{ uploaded_by: number | null }>(
    `SELECT uploaded_by FROM slips WHERE file_path = $1 LIMIT 1`,
    [key],
  );
  if (r.rows.length === 0) return false;
  const uploadedBy = r.rows[0].uploaded_by;
  if (!uploadedBy) return false;
  if (uploadedBy === actor.id) return true;
  if (matchPerm(actor.permissions, 'finance:expense:view_all::allow')) return true;
  if (matchPerm(actor.permissions, 'finance:expense:view_own::allow')) return uploadedBy === actor.id;
  const targetDept = await query<{ dept_perm: string | null }>(
    `SELECT (SELECT up.permission_id FROM perm.user_permissions up
              WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS dept_perm
       FROM users u WHERE u.id = $1`,
    [uploadedBy],
  );
  const tDept = targetDept.rows[0]?.dept_perm;
  if (!tDept || !actor.dept_id) return false;
  const targetDeptId = tDept.replace(/^user:dept:/, '').replace(/::allow$/, '');
  return targetDeptId === actor.dept_id;
}

export interface ScopeFilter {
  kind: 'self' | 'department' | 'all';
  department?: string | null;
  userId: number;
}

export async function listScope(actor: ActorWithScope): Promise<ScopeFilter> {
  const scope = await getActorScope(new Set(actor.permissions), actor.id);
  const narrowed: ScopeFilter['kind'] =
    scope.kind === 'all' ? 'all'
    : scope.kind === 'self' ? 'self'
    : 'department';
  return { kind: narrowed, department: actor.dept_id, userId: actor.id };
}

// --- AI guard (deprecated stub kept for backwards callers) -------------------

export interface AiGuardOk { ok: true; actorId: number; role: string; }
export interface AiGuardFail { ok: false; status: 401 | 403; error: string; }
export async function aiGuardForRequest(
  _headers: Record<string, string | string[] | undefined>,
  _opts: { perm?: string },
): Promise<AiGuardOk | AiGuardFail> {
  return { ok: false, status: 401, error: 'aiGuardForRequest is deprecated — use apiGuard from @/server/apiGuard' };
}

export type { ActorScope };
