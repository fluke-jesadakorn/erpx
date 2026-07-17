// perm/server.ts — server-only barrel. Use this for code that runs on the server.

import 'server-only';
export * from './index';
export {
  hasPermission,
  canManageResource,
  loadPermSessionFromHeaders,
  loadPermSessionFromCookieValue,
  loadActivePermSession,
  sessionDept,
  sessionLevel,
  sessionRoleName,
  levelFromRoles,
  type ActivePermSession,
  type DecodedPermToken,
  SESSION_COOKIE,
} from './auth';
export { type OwnedResource } from './auth-client';
export {
  buildAbilityFor,
  loadUserRoleIds,
  loadRoleGrants,
  createAppAbility,
  type AppAbility,
  type Actions,
  type Subjects,
} from './ability';
export {
  resolveApprovalChain,
  canActOnStage,
  getApprovedStages,
  resolveNextStage,
  isFinalApprovalStage,
  levelOf,
  type ApproverCheck,
  type ResolverCtx,
  type ActorCtx,
} from './chain';
export {
  getEffectiveLevel,
  getEffectiveLevels,
  getRoleEffectiveLevel,
  type Level,
} from './level';
export {
  type UserPerm,
  type GrantSource,
  type CreatePermInput,
  type UpsertUserPermsInput,
  createGrant,
  revokeGrant,
  listUserGrants,
  listActiveGrantsForPerm,
  expireOverdueGrants,
  grantActingBundle,
  listActiveUserPerms,
  revokeUserPerm,
  setUserPermanentPerms,
} from './grants';
export {
  type DeptMutation,
  deptPermId,
  setUserDept,
  clearUserDept,
} from './depts.server';
export {
  getActorScope,
  scopeFilter,
  assertInScope,
  parseDeptFromPerms,
  parseDeptsFromPerms,
  type ActorScope,
  type ScopeFilter,
  type ScopeKind,
} from './scope';