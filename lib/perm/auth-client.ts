// lib/perm/auth-client.ts — client-safe permission check helpers.
// Pure functions, no DB / no 'server-only'. Safe to import from client components.

import { ADMIN_PERM, matchPerm, parseDeptFromPerms, parseLevelFromRoles, parseRoleId } from './grammar';
import type { PermSession } from './session';

export type PermissionHolder = PermSession | { permissions: string[] | null | undefined } | null;

export function hasPermission(session: PermissionHolder, permission: string): boolean {
  if (!session) return false;
  const perms = (session as { permissions: string[] | null | undefined }).permissions;
  if (!perms) return false;
  return matchPerm(perms, permission);
}

export interface OwnedResource {
  ownerId: number;
  deptId?: string | null;
}

export function canManageResource(
  session: PermissionHolder,
  permission: string,
  resource: OwnedResource,
): boolean {
  if (!session) return false;
  const perms = (session as { permissions: string[] | null | undefined }).permissions;
  if (!perms) return false;
  if (perms.includes(ADMIN_PERM)) return true;
  if (!matchPerm(perms, permission)) return false;
  const sessionUserId = (session as { user?: { id?: number } }).user?.id;
  if (typeof sessionUserId === 'number' && resource.ownerId === sessionUserId) return true;
  const actorDept = parseDeptFromPerms(perms);
  if (resource.deptId && actorDept && resource.deptId === actorDept) return true;
  return false;
}

export function sessionDept(session: PermissionHolder): string | null {
  if (!session) return null;
  const explicit = (session as { user?: { department?: string | null } }).user?.department;
  if (explicit) return explicit;
  return parseDeptFromPerms((session as { permissions: string[] }).permissions ?? []);
}

export function sessionLevel(session: PermissionHolder): number {
  if (!session) return 10;
  const rank = (session as { user?: { rank?: number | null } }).user?.rank;
  if (typeof rank === 'number') return rank;
  const perms = (session as { permissions: string[] | null | undefined }).permissions ?? [];
  const role = (session as { user?: { role?: string } }).user?.role;
  if (role) {
    const lv = parseRoleId(role)?.level;
    if (lv) return lv;
  }
  return parseLevelFromRoles(perms);
}

export function sessionRoleName(session: PermissionHolder): string | null {
  if (!session) return null;
  const role = (session as { user?: { role?: string } }).user?.role;
  return role ? parseRoleId(role)?.name ?? null : null;
}

export { ADMIN_PERM };
export { parseLevelFromRoles as levelFromRoles };
