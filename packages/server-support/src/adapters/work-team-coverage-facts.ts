import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {nativeSupportReadAll,SUPPORT_RD_WORK_TEAM_COVERAGE} from "./native-support-read";
export function workTeamCoverageFacts(db:SqlDatabase){return {coveredTeam:(actor:WorkContext)=>nativeSupportReadAll(
    SUPPORT_RD_WORK_TEAM_COVERAGE,
    [actor.companyId,actor.userId,actor.branchId,actor.branchId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId],
    db,
  ).then(result=>result.results[0]??null)};}
