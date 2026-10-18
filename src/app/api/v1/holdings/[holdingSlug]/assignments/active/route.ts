import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { NextResponse } from 'next/server';
import { z } from 'zod';
import { assignmentApplies } from "@/server/modules/identity_access/public/authorization";
import { workRoleOptions } from "@/server/modules/work_projection/public/work-role-options";
import { selectWorkRole } from "@/server/modules/work_projection/public/work-role-store";
import { ApiError,errorResponse } from "@/server/modules/runtime/public/http";
const schema=z.object({assignmentId:z.string().min(1).max(128),revision:z.number().int().nonnegative()});
export async function GET(_request:Request,context:RouteContext<'/api/v1/holdings/[holdingSlug]/assignments/active'>){
 try{const state=await workRoleOptions((await context.params).holdingSlug);return NextResponse.json(canonicalDestinationLinks({data:{valid:!!state.selection && state.selection.expires_at>Date.now() && state.options.some(a=>a.id===state.selection!.assignment_id && a.version===state.selection!.assignment_version && assignmentApplies(a,state.access.scope)),options:state.options,revision:state.selection?.revision??0,assignmentId:state.selection?.assignment_id??null,companyId:state.access.scope.companyId,userId:state.access.userId}}),{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}
}
export async function POST(request:Request,context:RouteContext<'/api/v1/holdings/[holdingSlug]/assignments/active'>){
 try {
 const input=schema.parse(await request.json()),state=await workRoleOptions((await context.params).holdingSlug);
 const a=state.options.find(a=>a.id===input.assignmentId);
 if(!a)throw new ApiError(403,'ASSIGNMENT_UNAVAILABLE','Choose one of your active assignments.');
 const unchanged=state.selection?.assignment_id===a.id && state.selection.assignment_version===a.version && state.selection.revision===input.revision && state.selection.expires_at>Date.now();
 const selected=unchanged ? state.selection! : await selectWorkRole(state.write,{sessionId:state.access.sessionId,userId:state.access.userId,companyId:state.access.scope.companyId,assignmentId:a.id,assignmentVersion:a.version!,revision:input.revision,expiresAt:state.access.expiresAt});
 // Role selection remembers the role itself. A workspace preference changes
 // only after the user explicitly opens that workspace.
 const response=NextResponse.json(canonicalDestinationLinks({data:{assignmentId:a.id,revision:selected.revision,href:a.href,companyId:state.access.scope.companyId,userId:state.access.userId,appliesToWorkspace:assignmentApplies(a,state.access.scope)}}),{headers:{'Cache-Control':'private, no-store'}});
 response.cookies.delete(`erpx_assignment_${state.access.scope.holdingId}`);response.cookies.delete(`erpx_role_preference_${state.access.scope.holdingId}`);
 return response;
 }catch(e){return errorResponse(e);}
}
