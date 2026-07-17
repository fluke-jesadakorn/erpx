// Centralized server-side guard for the consolidated lib.
//
// All access checks go through the perm-string system via matchPerm().
// No legacy rbac matrix; no separate dept column on users.

import 'server-only';
import { query } from '../db';
import { verifySession, type SessionPayload } from './sessionToken';
import {
  matchPerm, parseDeptFromPerms, parseRoleId,
} from '../perm/grammar';
import { getActorScope, type ActorScope } from '../perm/scope';
import { loadDeptPermissionBundles, expandUserPermissions } from '../perm/deptGrant';

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
    isItOrAdmin: role === 'it' || role === 'admin',
    isCeoOrAdmin: role === 'ceo' || role === 'admin',
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
    permissions: string[];
  }>(
    `SELECT u.id, u.employee_code, u.fullname,
       COALESCE((
         SELECT ur.role_id FROM perm.user_roles ur
          WHERE ur.user_id = u.id
          ORDER BY (CASE WHEN ur.role_id LIKE '%::1' THEN 0
                         WHEN ur.role_id LIKE '%::2' THEN 1
                         WHEN ur.role_id LIKE '%::3' THEN 2
                         WHEN ur.role_id LIKE '%::4' THEN 3
                         WHEN ur.role_id LIKE '%::5' THEN 4
                         ELSE 5 END), ur.granted_at ASC
          LIMIT 1
       ), 'officer::5') AS role_id,
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
           ) t
       ), ARRAY[]::text[]) AS permissions
      FROM users u
     WHERE u.id = $1`,
    [payload.sub],
  );
  const row = res.rows[0];
  if (!row) return null;
  const parsed = parseRoleId(row.role_id ?? 'officer::5');
  const deptId = parseDeptFromPerms(row.permissions ?? []);
  const bundles = await loadDeptPermissionBundles();
  const base = row.permissions ?? [];
  const expanded = expandUserPermissions(base, bundles);
  return {
    id: row.id,
    employee_code: row.employee_code,
    fullname: row.fullname,
    department: deptId,
    dept_group_name: deptId,
    role_id: row.role_id ?? 'officer::5',
    role_name: parsed?.name ?? 'officer',
    level: parsed?.level ?? 5,
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
  return verifySession(token);
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
  if (actor.role_name === 'it' || actor.role_name === 'admin') return;
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
  if (opts.perm) {
    if (!matchPerm(actor.permissions, opts.perm)) {
      throw new GuardError(`missing permission "${opts.perm}"`, 403);
    }
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
