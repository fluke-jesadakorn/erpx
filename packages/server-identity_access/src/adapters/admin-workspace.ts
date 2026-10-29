import {holdingRoutingEnabled,holdingUiUrl} from '@erpx/contracts/holding-hosts';
import {requestProductOrigin} from '@erpx/feature-runtime/lib/product-hosts';
import 'server-only';
import {NextResponse} from 'next/server';
import {z} from 'zod';
import {productHostForHost,configuredProductOrigin} from '@erpx/feature-runtime/lib/product-hosts';
import {destinationAvailable} from '@erpx/feature-work_projection/lib/destination-access';
import {destinations} from '@erpx/feature-work_projection/lib/destination-registry';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import {ApiError,errorResponse} from '@erpx/server-runtime/public/http';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {administrationAccess} from '@erpx/server-runtime/public/administration';
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import {productCompany} from '@erpx/server-organization/public/product-company';
import {initializeWorkSelection,selectWorkRole} from '@erpx/server-work_projection/public/work-role-store';
import {requireAccountAccess,requirePlatformContext} from '@erpx/server-identity_access/adapters/authorization';
import {accountWorkModuleAccess} from '@erpx/server-identity_access/adapters/work-module-access';
import {betaAccess} from '@erpx/server-identity_access/adapters/beta-access';
import {loadIamAuthority} from '@erpx/server-identity_access/adapters/iam-store';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/adapters/record-source-authority';
import {effectiveWorkModules} from '@erpx/server-identity_access/domain/work-modules';
import {assignmentApplies} from '@erpx/server-identity_access/domain/workspace-access';
import type {AccountAccessContext,WorkAssignment,WorkContext} from '@erpx/server-identity_access/domain/authorization';
import {adminWorkspaceTarget,type AdminWorkspaceInput} from '@erpx/server-identity_access/domain/admin-workspace';
import {createAdminWorkspace} from '@erpx/server-identity_access/application/admin-workspace';

const responseHeaders={'Cache-Control':'private, no-store','Referrer-Policy':'no-referrer'};
const formSchema=z.object({companyId:z.string().min(1).max(128),next:z.string().min(1).max(2000),assignmentId:z.string().min(1).max(128),assignmentVersion:z.coerce.number().int().positive(),revision:z.coerce.number().int().nonnegative()}).strict();
type RoleSource={iam:Awaited<ReturnType<typeof loadIamAuthority>>;beta:boolean};
type RoleReader=(access:AccountAccessContext,assignment:WorkAssignment)=>Promise<RoleSource|null>;
const freshRoleSource:RoleReader=async(access,assignment)=>{
 const modules=await accountWorkModuleAccess(access);if(!modules.modulesByAssignment.has(assignment.id))return null;
 const {write}=await holdingResources(access.scope.holdingId);
 return {iam:await loadIamAuthority(write,access.scope.companyId,access.userId,assignment.role,access.sessionId),beta:modules.beta};
};
async function roleActor(access:AccountAccessContext,assignment:WorkAssignment,readRole:RoleReader=freshRoleSource):Promise<WorkContext|null>{
 if(!assignmentApplies(assignment,access.scope)||!Number.isSafeInteger(assignment.version)||assignment.version!<1)return null;
 const {write}=await holdingResources(access.scope.holdingId),source=await readRole(access,assignment);if(!source)return null;
 const {iam,beta}=source,scope=access.scope;
 const actor:WorkContext={userId:access.userId,name:access.name,email:access.email,assignmentId:assignment.id,role:assignment.role,companyId:scope.companyId,branchId:'branchId'in scope?scope.branchId:null,departmentId:scope.kind==='department'?scope.departmentId:null,holdingId:scope.holdingId,holdingSlug:scope.holdingSlug,holdingName:scope.holdingName,scope,assignmentScope:assignment.scopeKind,inherited:assignment.scopeKind!==scope.kind,modules:effectiveWorkModules(assignment.role,assignment.modules,iam.permissions,beta),assignments:access.assignments,workspaces:[],iam,beta};
 const current=currentAssignmentReadScope(actor,'workforce_actor_scope');return await write.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where} AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND company_id=? AND version=?)`).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args,assignment.id,actor.companyId,assignment.version!).first()?actor:null;
}
function ownerModule(source:string,section:string|undefined,iam:boolean){return source==='technology'?iam?'identity_access':'organization':source==='agents'?'agent':section==='brand'?'brand':section==='approvals'?'operations':section==='usage'||section==='ai'?'identity_access':source==='settings/notifications'?'collaboration':'identity_access';}
/** Read-only prospective discovery repeats the same current assignment and
 * owner navigation policy; it never creates an acting Work context/session. */
async function roleDestinations(access:AccountAccessContext,assignment:WorkAssignment,readRole:RoleReader){
 const actor=await roleActor(access,assignment,readRole);if(!actor)return [];const admin=await administrationAccess(actor),context={workspaceBase:'',scopeKind:actor.scope.kind,role:actor.role,permissions:actor.iam?.permissions,moduleIds:actor.modules,beta:actor.beta,adminSections:admin.sections,canManageApprovals:admin.manageOrganization},ready=new Map<string,Promise<boolean>>(),result:{path:string;label:string}[]=[];
 for(const item of destinations.filter(item=>item.path.startsWith('administration/')&&destinationAvailable(context,item))){const ownerId=ownerModule(item.source,item.selection.section,hasPermission(actor,'iam.read'));let pending=ready.get(ownerId);if(!pending){pending=(async()=>{try{await assertProductionModuleEnabled((await holdingResources(actor.holdingId)).write,actor.companyId,ownerId);return true;}catch(error){if(error instanceof ApiError&&error.code==='FEATURE_COMING_SOON')return false;throw error;}})();ready.set(ownerId,pending);}if(await pending)result.push({path:item.path,label:item.label});}
 return result;
}
export function adminWorkspaceDestinations(access:AccountAccessContext,assignment:WorkAssignment){return roleDestinations(access,assignment,freshRoleSource);}
export async function adminManagementAvailable(access:AccountAccessContext,assignment:WorkAssignment){
 if(access.scope.holdingId!=='h003'||access.scope.holdingSlug!=='erpx'||access.scope.kind!=='holding')return false;const actor=await roleActor(access,assignment);if(!actor||!hasPermission(actor,'manage_erpx.access'))return false;
 try{return (await requirePlatformContext()).userId===access.userId;}catch(error){if(error instanceof ApiError&&[401,403].includes(error.status)||error instanceof Error&&['UNAUTHENTICATED','FORBIDDEN'].includes(error.message))return false;throw error;}
}
function workspaceService(destinations:typeof adminWorkspaceDestinations){return createAdminWorkspace({access:input=>requireAccountAccess(input.scope,{companyId:input.companyId}),destinations,management:adminManagementAvailable,selection:async(access,applicable)=>initializeWorkSelection((await holdingResources(access.scope.holdingId)).write,{sessionId:access.sessionId,userId:access.userId,companyId:access.scope.companyId,expiresAt:access.expiresAt},access.assignments,applicable),select:async(access,option,revision)=>selectWorkRole((await holdingResources(access.scope.holdingId)).write,{sessionId:access.sessionId,userId:access.userId,companyId:access.scope.companyId,expiresAt:access.expiresAt,assignmentId:option.assignmentId,assignmentVersion:option.assignmentVersion,revision}),now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});}
export function adminWorkspaceService(){return workspaceService(adminWorkspaceDestinations);}
export function adminLaunchPath(input:AdminWorkspaceInput){return '/admin/launch?'+new URLSearchParams({companyId:input.companyId,next:input.target});}
export function adminWorkRolePath(input:AdminWorkspaceInput,notice?:string){return '/admin/choose-work-role?'+new URLSearchParams({companyId:input.companyId,next:input.target,...notice?{notice}:{}});}
export async function adminWorkRoleChoices(input:AdminWorkspaceInput){const state=await adminWorkspaceService().read(input),company=await productCompany((await holdingResources(state.access.scope.holdingId)).write,state.access.scope.companyId);if(!company)throw new ApiError(404,'COMPANY_NOT_FOUND','Select an active Company.');return {company,options:state.options.map(({assignmentId,assignmentVersion,label})=>({assignmentId,assignmentVersion,label})),revision:state.selection?.revision??0,selectedId:state.selection?.assignment_id??null,target:state.target};}
export async function selectAdminWorkRole(input:AdminWorkspaceInput,assignmentId:string,revision:number,version:number){return adminWorkspaceService().choose(input,assignmentId,revision,version);}
export async function adminPageAccess(companyId:string,path:string){const input=adminWorkspaceTarget(companyId,path);if(!input)throw new ApiError(404,'ADMIN_PAGE_NOT_FOUND','Select an Admin page.');const state=await adminWorkspaceService().read(input);return {target:state.target,chooseRole:adminWorkRolePath(input)};}
export async function adminWorkspaceRequest(request:Request){
 const origin=requestProductOrigin('admin',request.headers),query=new URL(request.url).searchParams;let input=adminWorkspaceTarget(query.get('companyId'),query.get('next'));
 try{if(productHostForHost(request.headers.get('host'))!=='admin'||!origin)throw new ApiError(400,'INVALID_ADMIN_HOST','Use the Admin host.');
  if(request.method==='POST'){
   if(request.headers.get('origin')!==origin)throw new ApiError(403,'ORIGIN_FORBIDDEN','Choose an Admin role from this origin.');if(!request.headers.get('content-type')?.startsWith('application/x-www-form-urlencoded'))throw new ApiError(415,'FORM_REQUIRED','Submit the work role form.');const body=await request.text();if(body.length>8192)throw new ApiError(413,'REQUEST_TOO_LARGE','The role request is too large.');const values=new URLSearchParams(body);if([...new Set(values.keys())].some(key=>values.getAll(key).length!==1))throw new ApiError(422,'INVALID_FORM','Submit one role choice.');const form=formSchema.parse(Object.fromEntries(values));input=adminWorkspaceTarget(form.companyId,form.next);if(!input)throw new ApiError(400,'INVALID_ADMIN_WORKSPACE','Select a valid Admin workspace.');
   try{await adminWorkspaceService().choose(input,form.assignmentId,form.revision,form.assignmentVersion);}catch(error){if(error instanceof ApiError&&[403,409].includes(error.status))return NextResponse.redirect(new URL(adminWorkRolePath(input,'access-changed'),origin),{status:303,headers:responseHeaders});throw error;}return NextResponse.redirect(new URL(adminLaunchPath(input),origin),{status:303,headers:responseHeaders});
  }
  if(request.method!=='GET'||!input)throw new ApiError(400,'INVALID_ADMIN_WORKSPACE','Select a valid Admin workspace.');const state=await adminWorkspaceService().read(input),response=NextResponse.redirect(new URL(holdingRoutingEnabled(process.env)&&state.target?holdingUiUrl(state.target,state.access.scope.companyId)??state.target:state.target??adminWorkRolePath(input),origin),{status:303,headers:responseHeaders});response.cookies.set(`erpx_company_${state.access.scope.holdingId}`,state.access.scope.companyId,{secure:true,httpOnly:true,sameSite:'lax',path:'/',maxAge:31536000});return response;
 }catch(error){if(input&&origin&&request.method==='GET'&&request.headers.get('accept')?.includes('text/html')){if(error instanceof Error&&error.message==='UNAUTHENTICATED'||error instanceof ApiError&&error.status===401)return NextResponse.redirect(new URL('/sso/start?'+new URLSearchParams({next:adminLaunchPath(input)}),origin),{status:303,headers:responseHeaders});if(error instanceof ApiError&&[403,404].includes(error.status))return NextResponse.redirect(new URL(adminWorkRolePath(input,'access-changed'),origin),{status:303,headers:responseHeaders});}const response=errorResponse(error);for(const [key,value]of Object.entries(responseHeaders))response.headers.set(key,value);return response;}
}

/** One server render owns this discovery reader; preview never initializes or changes a selected role. */
export function adminWorkspaceAvailabilityReader(){
 const betas=new WeakMap<AccountAccessContext,ReturnType<typeof betaAccess>>();
 const readRole:RoleReader=async(access,assignment)=>{
  const {write}=await holdingResources(access.scope.holdingId);let pending=betas.get(access);if(!pending){pending=betaAccess(write,access.scope.companyId,new Headers());betas.set(access,pending);}const beta=await pending;
  try{return {iam:await loadIamAuthority(write,access.scope.companyId,access.userId,assignment.role,access.sessionId),beta};}catch(error){if(error instanceof ApiError&&[401,403].includes(error.status))return null;throw error;}
 };
 const service=workspaceService((access,assignment)=>roleDestinations(access,assignment,readRole));
 return (input:AdminWorkspaceInput)=>service.available(input);
}
