import { rememberWorkspace } from "@/server/workspace-preferences";
import { runtimeEnv } from "@/server/runtime";
import { NextResponse } from 'next/server';
import { z } from 'zod';
import { workRoleOptions } from '@/server/work-role-options';
import { selectWorkRole } from '@/server/work-role-store';
import { ApiError,errorResponse } from '@/server/http';
const schema=z.object({assignmentId:z.string().min(1).max(128),revision:z.number().int().nonnegative()});
export async function GET(_request:Request,context:RouteContext<'/api/v1/holdings/[holdingSlug]/assignments/active'>){
 try{const state=await workRoleOptions((await context.params).holdingSlug);return NextResponse.json({data:{valid:!!state.selection && state.selection.expires_at>Date.now() && state.options.some(a=>a.id===state.selection!.assignment_id && a.version===state.selection!.assignment_version),options:state.options,revision:state.selection?.revision??0,assignmentId:state.selection?.assignment_id??null,companyId:state.access.scope.companyId}},{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}
}
export async function POST(request:Request,context:RouteContext<'/api/v1/holdings/[holdingSlug]/assignments/active'>){
 try {
 const input=schema.parse(await request.json()),state=await workRoleOptions((await context.params).holdingSlug);
 const a=state.options.find(a=>a.id===input.assignmentId);
 if(!a)throw new ApiError(403,'ASSIGNMENT_UNAVAILABLE','Choose one of your active assignments.');
 const selected=await selectWorkRole(state.write,{sessionId:state.access.sessionId,userId:state.access.userId,companyId:state.access.scope.companyId,assignmentId:a.id,assignmentVersion:a.version!,revision:input.revision,expiresAt:state.access.expiresAt});
 await rememberWorkspace((await runtimeEnv()).CONTROL_DB,state.access.userId,a.href,{holdingId:state.access.scope.holdingId,companyId:state.access.scope.companyId});
 const response=NextResponse.json({data:{assignmentId:a.id,revision:selected.revision,href:a.href}},{headers:{'Cache-Control':'private, no-store'}});
 response.cookies.delete(`erpx_assignment_${state.access.scope.holdingId}`);response.cookies.delete(`erpx_role_preference_${state.access.scope.holdingId}`);
 return response;
 }catch(e){return errorResponse(e);}
}
