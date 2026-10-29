import type {WorkspaceChoice} from '@erpx/feature-work_projection/lib/workspace-choice';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {productWorkspaceCandidates,type ProductWorkspaceCandidate} from '@erpx/server-work_projection/application/product-launcher';
import {adminWorkspaceTarget,type AdminWorkspaceInput} from '@erpx/server-identity_access/public/rules/admin-workspace';

export function createAdminLauncher(ports:{choices():Promise<WorkspaceChoice[]>;available(input:AdminWorkspaceInput):Promise<boolean>;origin(holdingSlug?:string):string|null;denied(error:unknown):boolean}){return async()=>{
 const result:(ProductWorkspaceCandidate & {kind:'company'|'management';href:string})[]=[];
 for(const candidate of productWorkspaceCandidates(await ports.choices()))for(const kind of ['company','management'] as const){
  const origin=ports.origin(candidate.scope.holdingSlug);if(!origin)continue;
  if(kind==='management'&&(candidate.scope.holdingSlug!=='erpx'||candidate.scope.kind!=='holding'))continue;
  const input=adminWorkspaceTarget(candidate.companyId,`${workspaceBase(candidate.scope)}/${kind==='company'?'administration':'erpx-management'}`)!;let allowed:boolean;try{allowed=await ports.available(input);}catch(error){if(ports.denied(error))continue;throw error;}if(!allowed)continue;
  const target='/admin/launch?'+new URLSearchParams({companyId:input.companyId,next:input.target}),href=new URL('/sso/start',origin);href.searchParams.set('next',target);result.push({...candidate,kind,href:href.href});
 }return result;
};}
