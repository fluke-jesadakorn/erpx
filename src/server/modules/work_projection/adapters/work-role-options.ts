import "server-only";
import {requireAccountAccess} from "../../identity_access/public/authorization";
import {holdingResources} from "../../runtime/public/runtime";
import {workRolePathFacts} from "../../organization/public/work-role-path-facts";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {restoreWorkSelection} from "./work-role-store";
import {createWorkRoleOptions} from "../application/work-role-options";
import type {HoldingWorkspaceScope} from "@/lib/workspace";
export async function workRoleOptions(requested:HoldingWorkspaceScope|string){let write!:SqlDatabase;const result=await createWorkRoleOptions({clock:{now:()=>Date.now()},access:requireAccountAccess,resource:async access=>{write=(await holdingResources(access.scope.holdingId)).write;return {...workRolePathFacts(write),restore:access=>restoreWorkSelection(write,{sessionId:access.sessionId,userId:access.userId,companyId:access.scope.companyId,expiresAt:access.expiresAt},access.assignments)};}})(requested);return {...result,write};}
