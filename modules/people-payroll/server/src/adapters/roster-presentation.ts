import 'server-only';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {holdingResources} from '../../../../../platform/runtime/server/src/public/runtime';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {rosterData} from '@erpx/people-payroll-server/public/people-roster';
export async function rosterPresentation(holdingSlug:string,query:{week?:string;department?:string}) {
 const actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId);
 return {base:workspaceBase(publicScope(actor)),data:await rosterData(write,actor,query)};
}
