import 'server-only';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {holdingResources} from '@/server/modules/runtime/public/runtime';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {rosterData} from '../public/people-roster';
export async function rosterPresentation(holdingSlug:string,query:{week?:string;department?:string}) {
 const actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId);
 return {base:workspaceBase(publicScope(actor)),data:await rosterData(write,actor,query)};
}
