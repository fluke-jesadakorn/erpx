import "server-only";
import {requireAccountAccess} from "@erpx/server-identity_access/public/authorization";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {workRolePathFacts} from "@erpx/server-organization/public/work-role-path-facts";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {restoreWorkSelection} from "@erpx/server-work_projection/adapters/work-role-store";
import {createWorkRoleOptions} from "@erpx/server-work_projection/application/work-role-options";
import type {HoldingWorkspaceScope} from "@erpx/feature-work_projection/lib/workspace";
export async function workRoleOptions(requested:HoldingWorkspaceScope|string){let write!:SqlDatabase;const result=await createWorkRoleOptions({clock:{now:()=>Date.now()},access:requireAccountAccess,resource:async access=>{write=(await holdingResources(access.scope.holdingId)).write;return {...workRolePathFacts(write),restore:access=>restoreWorkSelection(write,{sessionId:access.sessionId,userId:access.userId,companyId:access.scope.companyId,expiresAt:access.expiresAt},access.assignments)};}})(requested);return {...result,write};}
