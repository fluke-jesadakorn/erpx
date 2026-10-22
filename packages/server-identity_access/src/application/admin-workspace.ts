import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import type {AccountAccessContext,WorkAssignment} from '@erpx/server-identity_access/domain/authorization';
import {assignmentApplies} from '@erpx/server-identity_access/domain/workspace-access';
import {adminWorkspaceTarget,adminTargetAllowed,type AdminWorkspaceInput} from '@erpx/server-identity_access/domain/admin-workspace';
import type {ActiveWorkSelection} from '@erpx/server-work_projection/public/work-role-store';

export type AdminWorkRoleOption={assignmentId:string;assignmentVersion:number;role:string;label:string;target:string};
export type AdminWorkspacePorts={
 access(input:AdminWorkspaceInput):Promise<AccountAccessContext>;
 destinations(access:AccountAccessContext,assignment:WorkAssignment):Promise<readonly {path:string;label:string}[]>;
 management(access:AccountAccessContext,assignment:WorkAssignment):Promise<boolean>;
 selection(access:AccountAccessContext,applicable:WorkAssignment[]):Promise<ActiveWorkSelection|null>;
 select(access:AccountAccessContext,option:AdminWorkRoleOption,revision:number):Promise<ActiveWorkSelection>;
 now():number;error(status:number,code:string,message:string):Error;
};
export function createAdminWorkspace(ports:AdminWorkspacePorts){
 async function inspect(input:AdminWorkspaceInput,firstAllowed:boolean){
  const valid=adminWorkspaceTarget(input.companyId,input.target);if(!valid||workspaceBase(valid.scope)!==workspaceBase(input.scope)||valid.kind!==input.kind)throw ports.error(400,'INVALID_ADMIN_WORKSPACE','Select a valid Company Admin workspace.');
  const access=await ports.access(valid);if(access.expiresAt<=ports.now())throw ports.error(401,'SESSION_EXPIRED','Sign in again.');if(access.scope.companyId!==valid.companyId)throw ports.error(403,'COMPANY_REQUIRED','Select an authorized Company.');
  const applicable=access.assignments.filter(assignment=>assignmentApplies(assignment,access.scope)),options:AdminWorkRoleOption[]=[];
  for(const assignment of applicable){if(!Number.isSafeInteger(assignment.version)||assignment.version!<1)continue;
   const allowed=valid.kind==='management'?access.scope.holdingId==='h003'&&access.scope.kind==='holding'&&await ports.management(access,assignment):adminTargetAllowed(valid,(await ports.destinations(access,assignment)).map(item=>item.path));
   if(allowed){options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll('_',' '),target:valid.target});if(firstAllowed)break;}
  }
  if(!options.length)throw ports.error(403,'ADMIN_ROLE_REQUIRED','None of your current roles can open this Company Admin page.');
  return {access,options,applicable};
 }
 function preview(input:AdminWorkspaceInput){return inspect(input,false);}
 async function read(input:AdminWorkspaceInput){
  const {access,options,applicable}=await preview(input);
  const selection=await ports.selection(access,applicable),selected=selection&&selection.expires_at>ports.now()?options.find(option=>option.assignmentId===selection.assignment_id&&option.assignmentVersion===selection.assignment_version):undefined;
  return {access,options,selection,target:selected?.target??null};
 }
 return {preview,read,async available(input:AdminWorkspaceInput){await inspect(input,true);return true;},async choose(input:AdminWorkspaceInput,assignmentId:string,revision:number,assignmentVersion:number){
  const state=await read(input),option=state.options.find(option=>option.assignmentId===assignmentId);if(!option)throw ports.error(403,'ASSIGNMENT_UNAVAILABLE','Choose a current authorized Admin role.');
  if(option.assignmentVersion!==assignmentVersion||(state.selection?.revision??0)!==revision)throw ports.error(409,'WORK_CONTEXT_CHANGED','Your work role changed. Review the current choices.');
  if(state.target&&state.selection?.assignment_id===assignmentId)return state;
  await ports.select(state.access,option,revision);return state;
 }};
}
