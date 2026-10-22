import 'server-only';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {rosterData} from '@erpx/server-people_payroll/public/people-roster';
export async function rosterPresentation(holdingSlug:string,query:{week?:string;department?:string}) {
 const actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId);
 return {base:workspaceBase(publicScope(actor)),data:await rosterData(write,actor,query)};
}
