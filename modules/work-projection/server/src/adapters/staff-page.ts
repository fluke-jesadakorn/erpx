import "server-only";
import {headers} from 'next/headers';
import {productHostForHost} from '@erpx/platform-runtime-ui/lib/product-hosts';
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { staffSettings } from "@erpx/operations-server/public/staff-settings";
import { listProjects } from "@erpx/projects-server/public/staff-projects";
import {optionalProjectOptions} from '@erpx/projects-server/public/optional-project-options';
import { activeStaff } from "@erpx/operations-server/public/staff-policy";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import {createStaffPageService} from "@erpx/work-projection-server/application/staff-page";
export type StaffPageOptions={projectOptions?:'optional'};
export async function staffPage(slug:string,options?:StaffPageOptions){let write:Awaited<ReturnType<typeof holdingResources>>["write"];
 const projectPickerStrict=options?.projectOptions==='optional'&&productHostForHost((await headers()).get('host'))==='apps';
 const projectOptions={strictProduct:projectPickerStrict};
 const result=await createStaffPageService({actor:requireWorkContext,prepare:async actor=>{write=(await holdingResources(actor.holdingId)).write;},settings:actor=>staffSettings(write,actor),projects:actor=>options?.projectOptions==='optional'?optionalProjectOptions(write,actor,projectOptions):listProjects(write,actor),staff:actor=>activeStaff(write,actor.companyId,actor.userId),expectedAbsence:error=>error instanceof ApiError,base:actor=>workspaceBase(publicScope(actor))})(slug);
 return {...result,write:write!,projectPickerStrict};
}
