import "server-only";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/workspace";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { staffSettings } from "@/server/modules/operations/public/staff-settings";
import { listProjects } from "@/server/modules/projects/public/staff-projects";
import { activeStaff } from "@/server/modules/operations/public/staff-policy";
import { ApiError } from "@/server/modules/runtime/public/http";
import {createStaffPageService} from "../application/staff-page";
export async function staffPage(slug:string){let write:Awaited<ReturnType<typeof holdingResources>>["write"];
 const result=await createStaffPageService({actor:requireWorkContext,prepare:async actor=>{write=(await holdingResources(actor.holdingId)).write;},settings:actor=>staffSettings(write,actor),projects:actor=>listProjects(write,actor),staff:actor=>activeStaff(write,actor.companyId,actor.userId),expectedAbsence:error=>error instanceof ApiError,base:actor=>workspaceBase(publicScope(actor))})(slug);
 return {...result,write:write!};
}
