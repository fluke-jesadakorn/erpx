import type {WorkspaceChoice} from '@erpx/feature-work_projection/lib/workspace-choice';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {productWorkspaceCandidates} from '@erpx/server-work_projection/application/product-launcher';
import {adminWorkspaceTarget,type AdminWorkspaceInput} from '@erpx/server-identity_access/public/rules/admin-workspace';

export function createAdminLauncher(ports:{choices():Promise<WorkspaceChoice[]>;available(input:AdminWorkspaceInput):Promise<boolean>;origin():string|null;denied(error:unknown):boolean}){return async()=>{
 const origin=ports.origin();if(!origin)return [];const result:{holding:string;company:string;companyId:string;workspace:string;kind:'company'|'management';href:string}[]=[];
 for(const candidate of productWorkspaceCandidates(await ports.choices()))for(const kind of ['company','management'] as const){
  if(kind==='management'&&(candidate.scope.holdingSlug!=='erpx'||candidate.scope.kind!=='holding'))continue;
  const input=adminWorkspaceTarget(candidate.companyId,`${workspaceBase(candidate.scope)}/${kind==='company'?'administration':'erpx-management'}`)!;let allowed:boolean;try{allowed=await ports.available(input);}catch(error){if(ports.denied(error))continue;throw error;}if(!allowed)continue;
  const target='/admin/launch?'+new URLSearchParams({companyId:input.companyId,next:input.target}),href=new URL('/sso/start',origin);href.searchParams.set('next',target);result.push({holding:candidate.holding,company:candidate.company,companyId:candidate.companyId,workspace:candidate.workspace,kind,href:href.href});
 }return result;
};}
