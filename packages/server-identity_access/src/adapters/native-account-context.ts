import 'server-only';
import type {NativeWorkRoleState} from '@erpx/contracts/field-service-reserve';
import type {HoldingWorkspaceScope} from '@erpx/feature-work_projection/lib/workspace';
import {workspaceHref} from '@erpx/feature-work_projection/lib/workspace';
import {nativeHoldingConfiguration,nativeHoldingTargetById,nativeHoldingTargetForHost} from '@erpx/native-sdk/holding-target';
import {holdingBindingBySlug} from '@erpx/server-runtime/public/runtime';
import {originProfile} from '@erpx/feature-runtime/lib/holding-hosts';
import {ApiError} from '@erpx/server-runtime/public/http';
import {readNativeAccountPreferences} from '@erpx/server-work_projection/public/workspace-preferences';
import {cookieValue, type AccountAccessContext, type IdentitySession, type WorkAssignment, type WorkContext, type WorkspaceResolutionOptions} from '../domain/authorization';
import {createNativeAccountCompaniesReader} from './native-account-companies';
import {createNativeWorkRoleTransport} from './native-work-role-transport';
function fail(status:number,code:string,message:string):never{throw new ApiError(status,code,message);}
const assignment=(a:NativeWorkRoleState['options'][number]):WorkAssignment=>({id:a.id,version:a.version,label:a.label,validUntil:a.validUntil,role:a.role,companyId:a.companyId,branchId:a.branchId,modules:a.modules,scopeKind:a.scopeKind,scopeNodeId:a.scopeNodeId});
/** Only maps native, session-bound read models into rendering props. It cannot
 * authorize writes; native commands resolve their own current authority. */
export function nativeAccountReadModel(state:NativeWorkRoleState,session:IdentitySession,incoming:Headers,requested:HoldingWorkspaceScope):AccountAccessContext{
 if(session.session.id!==state.sessionId||session.user.id!==state.userId)fail(409,'WORK_CONTEXT_CHANGED','The current account changed.');
 const base={holdingId:state.holdingId,holdingSlug:requested.holdingSlug,holdingName:state.holdingName,companyId:state.companyId};
 let scope:AccountAccessContext['scope'];
 if(requested.kind==='holding'){
  if(state.branchId!==null||state.departmentId!==null)fail(502,'NATIVE_WORK_ROLE_RESPONSE_INVALID','The native workspace does not match this page.');scope={...base,kind:'holding'};
 }else if(requested.kind==='branch'){
  if(!state.branchId||!state.branchName||state.branchSlug!==requested.branchSlug||state.departmentId!==null)fail(502,'NATIVE_WORK_ROLE_RESPONSE_INVALID','The native branch does not match this page.');scope={...base,kind:'branch',branchId:state.branchId,branchSlug:state.branchSlug,branchName:state.branchName};
 }else{
  if(!state.departmentId||!state.departmentName||state.departmentSlug!==requested.departmentSlug||(state.branchSlug??undefined)!==requested.branchSlug)fail(502,'NATIVE_WORK_ROLE_RESPONSE_INVALID','The native department does not match this page.');scope={...base,kind:'department',branchId:state.branchId,...(state.branchSlug?{branchSlug:state.branchSlug,branchName:state.branchName??undefined}:{}),departmentId:state.departmentId,departmentSlug:state.departmentSlug,departmentName:state.departmentName};
 }
 return {sessionId:state.sessionId,expiresAt:state.expiresAt,userId:state.userId,name:session.user.name,email:session.user.email,scope,assignments:state.options.map(assignment),requestHeaders:incoming};
}
export function nativeWorkReadModel(state:NativeWorkRoleState,access:AccountAccessContext):WorkContext{
 const selected=state.selection;
 if(!selected?.assignmentId)fail(409,'WORK_ROLE_REQUIRED','Choose a work role to continue.');
 const role=access.assignments.find(a=>a.id===selected.assignmentId&&a.version===selected.assignmentVersion);
 if(!state.valid||!role||!state.context)fail(409,'WORK_ROLE_UNAVAILABLE','Your selected role no longer covers this workspace. Choose an available role.');
 const c=state.context,scope=access.scope;
 const {hrAppointmentVersion,...iam}=c.iam;
 return {...(access.requestHeaders.get('x-erpx-api')==='false'?{pageRequest:true as const}:{}),iam:{...iam,...(hrAppointmentVersion===null?{}:{hrAppointmentVersion})},userId:access.userId,name:access.name,email:access.email,assignmentId:role.id,role:role.role,companyId:scope.companyId,branchId:state.branchId,departmentId:state.departmentId,holdingId:scope.holdingId,holdingSlug:scope.holdingSlug,holdingName:scope.holdingName,scope,assignmentScope:role.scopeKind,inherited:role.scopeKind!==scope.kind,modules:c.modules,beta:c.beta,assignments:[role],availableAssignments:access.assignments,authority:{sessionId:access.sessionId,revision:selected.revision,assignmentVersion:selected.assignmentVersion!},workspaces:state.workspaces.map(w=>{
  const s:HoldingWorkspaceScope=w.kind==='holding'?{kind:'holding',holdingSlug:scope.holdingSlug}:w.kind==='branch'?{kind:'branch',holdingSlug:scope.holdingSlug,branchSlug:w.branchSlug!}:{kind:'department',holdingSlug:scope.holdingSlug,departmentSlug:w.departmentSlug!,...(w.branchSlug?{branchSlug:w.branchSlug}:{})};
  return {key:w.kind==='holding'?`holding:${scope.holdingSlug}`:`${w.kind}:${w.id}`,href:workspaceHref(s),label:w.label,kind:w.kind,inherited:w.inherited};
 })};
}
/** A UI selection hint only. Rust resolves the actual Company and grants. */
export async function nativeCompanyHint(incoming:Headers,holdingId:string,requested:HoldingWorkspaceScope,options:WorkspaceResolutionOptions,memory:()=>Promise<string|undefined>,available:()=>Promise<{id:string}[]>){
 if(requested.kind!=='holding')return options.companyId;
 if(incoming.get('x-erpx-front-company'))return undefined;
 const explicit=options.companyId??incoming.get('x-erpx-context-company')??cookieValue(incoming.get('cookie'),`erpx_company_${holdingId}`);
 if(explicit!==null&&explicit!==undefined)return explicit;
 const preferred=await memory();if(preferred!==undefined)return preferred;
 const companies=await available();return companies.length===1?companies[0].id:undefined;
}
export async function readNativeAccountContext(session:IdentitySession,incoming:Headers,requested:HoldingWorkspaceScope,options:WorkspaceResolutionOptions,context:boolean){
 const raw=process.env.ERPX_NATIVE_FIELD_SERVICE_WORKSPACE_CONFIG;
 if(!raw||!process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG)fail(503,'NATIVE_ACCOUNT_CONTEXT_CONFIGURATION_REQUIRED','The native Account context is not configured.');
 const profile=originProfile(),config=nativeHoldingConfiguration(raw,profile),sourceHost=incoming.get('host'),loopback=profile==='loopback'&&sourceHost!==null&&/^127\.0\.0\.1:\d+$/.test(sourceHost);
 let target;
 if(loopback){target=config.holdings.find(h=>h.holdingSlug===requested.holdingSlug);}
 else{
  let binding:{id:string}|undefined;
  try{binding=await holdingBindingBySlug(requested.holdingSlug);}catch{binding=undefined;}
  target=(binding?nativeHoldingTargetById(config,binding.id):undefined)??nativeHoldingTargetForHost(config,sourceHost);
 }
 if(!target)fail(404,'WORKSPACE_NOT_FOUND','Workspace not found.');
 const transport=new Headers(incoming);
 // Route selectors remain untrusted. Never leave a previous route's scope in
 // the request when resolving a different requested workspace.
 for(const k of ['x-erpx-workspace-branch','x-erpx-workspace-department','x-erpx-branch-slug','x-erpx-department-slug'])transport.delete(k);
 if(requested.kind!=='holding'&&requested.branchSlug)transport.set('x-erpx-workspace-branch',requested.branchSlug);
 if(requested.kind==='department')transport.set('x-erpx-workspace-department',requested.departmentSlug);
 const companyId=await nativeCompanyHint(incoming,target.holdingId,requested,options,async()=>{
  const pref=await readNativeAccountPreferences(incoming,session.user.id);if(pref.sessionId!==session.session.id)fail(409,'WORK_CONTEXT_CHANGED','The current account changed.');return pref.company?.companyId;
 },async()=>(await createNativeAccountCompaniesReader(raw)(incoming,{sessionId:session.session.id,userId:session.user.id},target.holdingId)).companies);
 const state=await createNativeWorkRoleTransport(raw)(transport,{sessionId:session.session.id,userId:session.user.id,...(companyId?{companyId}:{})},context?'context':'access',undefined,target.holdingId);
 const access=nativeAccountReadModel(state,session,incoming,requested);
 return {state,access};
}
