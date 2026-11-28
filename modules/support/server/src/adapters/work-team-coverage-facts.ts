import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {nativeSupportReadAll,SUPPORT_RD_WORK_TEAM_COVERAGE} from "./native-support-read";
export function workTeamCoverageFacts(db:SqlDatabase){return {coveredTeam:(actor:WorkContext)=>nativeSupportReadAll(
    SUPPORT_RD_WORK_TEAM_COVERAGE,
    [actor.companyId,actor.userId,actor.branchId,actor.branchId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId],
    db,
  ).then(result=>result.results[0]??null)};}
