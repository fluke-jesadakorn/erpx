// lib/perm/grammar.ts — single source of truth for permission/role-id parsing.
//
// Permission grammar:  <domain>:<subject>:<verb>[:<qualifier>]::<effect>
//   qualifier ∈ {self, dept, subtree, all, <dept_id>, *} — optional, default 'all'
//   effect    ∈ {allow, deny}
//
// Role-id grammar:     <name>::<level>
//   level integer 1–10 (1 = highest authority)
//
// Department membership is encoded as a permission: `user:dept:<id>::allow`.
// Authority level is encoded in the role-id suffix.

export type Effect = 'allow' | 'deny';

const QUALIFIERS = new Set(['self', 'dept', 'subtree', 'all', '*']);

export const PERM_ID_REGEX =
  /^[a-z][a-z0-9_]*:[a-z][a-z0-9_]*:[a-z][a-z0-9_]*(?::[a-z0-9_*-]+)?::(?:allow|deny)$/;
export const ROLE_ID_REGEX = /^[a-z][a-z0-9_-]*(?:::[1-9]\d?)?$/;

export const ADMIN_PERM = 'admin:system:bypass::allow';
export const SYSTEM_PERMS = new Set<string>([ADMIN_PERM]);

export interface PermParts {
  domain: string;
  subject: string;
  verb: string;
  qualifier: string | null;
  effect: Effect;
}

export interface RoleParts {
  name: string;
  level: number;
}

const ROLE_RANKS: Record<string, number> = {
  ceo: 1,
  cfo: 2,
  director: 3,
  manager: 4,
  supervisor: 5,
  officer: 6,
  staff: 7,
  system_admin: 1,
};

export function parsePerm(id: string): PermParts | null {
  const idx = id.indexOf('::');
  if (idx < 0) return null;
  const head = id.slice(0, idx);
  const tail = id.slice(idx + 2);
  if (tail !== 'allow' && tail !== 'deny') return null;
  const seg = head.split(':');
  if (seg.length < 3 || seg.length > 4) return null;
  const [domain, subject, verb, qualifier] = seg;
  if (!domain || !subject || !verb) return null;
  return { domain, subject, verb, qualifier: qualifier ?? null, effect: tail };
}

export function buildPerm(
  parts: { domain: string; subject: string; verb: string; qualifier?: string | null },
  effect: Effect = 'allow',
): string {
  const base = `${parts.domain}:${parts.subject}:${parts.verb}`;
  const q = parts.qualifier && parts.qualifier !== '*' ? parts.qualifier : null;
  return q ? `${base}:${q}::${effect}` : `${base}::${effect}`;
}

export function parseRoleId(id: string): RoleParts | null {
  const idx = id.indexOf('::');
  if (idx < 0) {
    const level = ROLE_RANKS[id];
    return level ? { name: id, level } : null;
  }
  const name = id.slice(0, idx);
  const levelStr = id.slice(idx + 2);
  if (!/^\d+$/.test(levelStr)) return null;
  const level = parseInt(levelStr, 10);
  if (!Number.isFinite(level) || level < 1 || level > 10) return null;
  return { name, level };
}

export function buildRoleId(name: string, level: number): string {
  return ROLE_RANKS[name] === level ? name : `${name}::${level}`;
}

export function effectOf(id: string): Effect | null {
  const idx = id.lastIndexOf('::');
  if (idx < 0) return null;
  const tail = id.slice(idx + 2);
  if (/^\d+$/.test(tail)) return null;
  return tail === 'allow' || tail === 'deny' ? tail : null;
}

export function isAllow(id: string): boolean {
  return effectOf(id) === 'allow';
}

export function isDeny(id: string): boolean {
  return effectOf(id) === 'deny';
}

export function roleNameOf(roleId: string): string {
  const idx = roleId.indexOf('::');
  return idx > 0 ? roleId.slice(0, idx) : roleId;
}

export function levelOf(roleId: string): number | null {
  return parseRoleId(roleId)?.level ?? null;
}

const DEPT_PERM_RE = /^user:dept:([a-z0-9_-]+)::allow$/;

export function parseDeptFromPerms(perms: Iterable<string>): string | null {
  for (const p of perms) {
    const m = DEPT_PERM_RE.exec(p);
    if (m) return m[1];
  }
  return null;
}

export function parseDeptsFromPerms(perms: Iterable<string>): string[] {
  const out: string[] = [];
  for (const p of perms) {
    const m = DEPT_PERM_RE.exec(p);
    if (m) out.push(m[1]);
  }
  return out;
}

export function parseLevelFromRoles(roleIds: Iterable<string>): number {
  let min = 10;
  let found = false;
  for (const id of roleIds) {
    const lv = parseRoleId(id)?.level;
    if (lv !== undefined && lv < min) {
      min = lv;
      found = true;
    }
  }
  return found ? min : 10;
}

// Session-permission matcher.
//   1. admin:system:bypass::allow grants everything
//   2. exact match wins
//   3. qualified request matches an unqualified grant (the unqualified form is "all")
//   4. unqualified request matches a `:all` qualified grant
export function matchPerm(sessionPerms: string[] | Set<string>, requested: string): boolean {
  const arr = Array.isArray(sessionPerms) ? sessionPerms : Array.from(sessionPerms);
  const req = parsePerm(requested);
  if (!req) return false;
  const base = `${req.domain}:${req.subject}:${req.verb}`;
  const denies = req.qualifier
    ? [`${base}:${req.qualifier}::deny`, `${base}:all::deny`, `${base}::deny`]
    : [`${base}::deny`, `${base}:all::deny`];
  if (denies.some((p) => arr.includes(p))) return false;
  if (arr.includes(ADMIN_PERM)) return true;
  if (arr.includes(requested)) return true;
  if (req.qualifier && arr.includes(`${base}::allow`)) return true;
  if (!req.qualifier && arr.includes(`${base}:all::allow`)) return true;
  return false;
}

export function isSystemPerm(perm: string): boolean {
  return SYSTEM_PERMS.has(perm);
}

export function isKnownQualifier(q: string | null | undefined): boolean {
  return q !== null && q !== undefined && QUALIFIERS.has(q);
}
