import 'server-only';
import {headers} from 'next/headers';
import {requireAccountAccess} from '@erpx/server-identity_access/public/authorization';
import {ApiError} from '@erpx/server-runtime/public/http';
import {workspaceHref,type HoldingWorkspaceScope} from '@erpx/feature-work_projection/lib/workspace';
import {createNativeWorkRoleTransport} from '@erpx/server-identity_access/public/native-work-role-transport';
export async function workRoleOptions(requested:HoldingWorkspaceScope|string){
 const access=await requireAccountAccess(requested),raw=process.env.ERPX_NATIVE_FIELD_SERVICE_WORKSPACE_CONFIG;
 if(!raw)throw new ApiError(503,'NATIVE_WORK_ROLE_TARGET_UNAVAILABLE','The native work role selection is not configured.');
 const incoming=new Headers(await headers());
 if(access.scope.kind!=='holding'){if(access.scope.branchSlug)incoming.set('x-erpx-workspace-branch',access.scope.branchSlug);if(access.scope.kind==='department')incoming.set('x-erpx-workspace-department',access.scope.departmentSlug);}
 const state=await createNativeWorkRoleTransport(raw)(incoming,{sessionId:access.sessionId,userId:access.userId,companyId:access.scope.companyId},'restore');
 if(state.holdingId!==access.scope.holdingId||state.branchId!==(access.scope.kind==='holding'?null:access.scope.branchId)||state.departmentId!==(access.scope.kind==='department'?access.scope.departmentId:null))throw new ApiError(409,'WORK_CONTEXT_CHANGED','Review your current Holding.');
 const options=state.options.map(a=>{
  let scope:HoldingWorkspaceScope={kind:'holding',holdingSlug:access.scope.holdingSlug};
  if(a.scopeKind==='branch'){if(!a.branchSlug)throw new ApiError(502,'NATIVE_WORK_ROLE_RESPONSE_INVALID','The native workspace path is unavailable.');scope={kind:'branch',holdingSlug:access.scope.holdingSlug,branchSlug:a.branchSlug};}
  if(a.scopeKind==='department'){if(!a.departmentSlug)throw new ApiError(502,'NATIVE_WORK_ROLE_RESPONSE_INVALID','The native workspace path is unavailable.');scope={kind:'department',holdingSlug:access.scope.holdingSlug,departmentSlug:a.departmentSlug,...(a.branchSlug?{branchSlug:a.branchSlug}:{})};}
  return {...a,href:workspaceHref(scope)};
 });
 const s=state.selection,selection=s?{session_id:state.sessionId,user_id:state.userId,company_id:state.companyId,assignment_id:s.assignmentId,assignment_version:s.assignmentVersion,revision:s.revision,expires_at:s.expiresAt}:null;
 const selected=selection&&selection.expires_at>Date.now()?options.find(a=>a.id===selection.assignment_id&&a.version===selection.assignment_version):undefined;
 return {access:{...access,assignments:options},selection,selected,options};
}
