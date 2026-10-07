import 'server-only';
import { canEnterManageErpx, currentSession, requireAccountAccess, requirePlatformContext } from '@/server/modules/identity_access/public/authorization';
import { ApiError } from '@/server/modules/runtime/public/http';
import { holdingResources } from '@/server/modules/runtime/public/runtime';
import { initializeWorkSelection } from '@/server/modules/work_projection/public/work-role-store';
import { workspaceChoices, workspaceChoicesForAssignment } from '@/server/modules/organization/public/workspace-choices';
import { createAccountWorkContext } from '../application/account-work-context';
export const accountWorkContext = createAccountWorkContext({ session: currentSession, choices: workspaceChoices, manage: canEnterManageErpx, platform: requirePlatformContext, access: requireAccountAccess,
    selection: async (access, input, assignments, eligible) => { const { write } = await holdingResources(access.scope.holdingId); return initializeWorkSelection(write, input, assignments, eligible); },
    assignmentChoices: async (holdingId, assignmentId) => workspaceChoicesForAssignment(holdingId as Parameters<typeof workspaceChoicesForAssignment>[0], assignmentId), now: Date.now, error: (...args) => new ApiError(...args), apiError: (error): error is ApiError => error instanceof ApiError });
