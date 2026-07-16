// perm/index.ts — client-safe barrel. No 'server-only' modules re-exported here.
// Server code should import from '@/perm/server' instead.

export * from './grammar';
export * from './schema';
export * from './session';
export { PERM, DOMAINS, type Domain } from './taxonomy';
export {
  STAGE_ORDER, STAGE_TO_ROLE, STAGE_TO_PERM, stageRoles, stagePrimaryRole, type StageName,
} from './stages';
export {
  hasPermission,
  canManageResource,
  sessionDept,
  sessionLevel,
  sessionRoleName,
  ADMIN_PERM,
  levelFromRoles,
  type OwnedResource,
} from './auth-client';
export type Action = 'create' | 'read' | 'update' | 'delete';