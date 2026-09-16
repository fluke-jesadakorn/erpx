import { requireAccountAccess } from './authorization';
import { holdingResources } from './runtime';
import { restoreWorkSelection } from './work-role-store';
import { workspaceHref } from '@/lib/workspace';
import type { HoldingWorkspaceScope } from '@/lib/workspace';
export async function workRoleOptions(requested:HoldingWorkspaceScope|string) {
 const access=await requireAccountAccess(requested),{write}=await holdingResources(access.scope.holdingId);
 const selection=await restoreWorkSelection(write,{sessionId:access.sessionId,userId:access.userId,companyId:access.scope.companyId,expiresAt:access.expiresAt},access.assignments);
 const options=await Promise.all(access.assignments.map(async a=>{
 let href=workspaceHref({kind:'holding',holdingSlug:access.scope.holdingSlug});
 if(a.scopeKind!=='holding'){
  const node=await write.prepare('SELECT n.slug,b.slug branch_slug FROM organization_nodes n LEFT JOIN organization_nodes b ON b.id=n.branch_id WHERE n.id=? AND n.company_id=?').bind(a.scopeNodeId,access.scope.companyId).first<{slug:string;branch_slug:string|null}>();
  if(node)href=workspaceHref(a.scopeKind==='branch'?{kind:'branch',holdingSlug:access.scope.holdingSlug,branchSlug:node.slug}:{kind:'department',holdingSlug:access.scope.holdingSlug,departmentSlug:node.slug,...(node.branch_slug?{branchSlug:node.branch_slug}:{})});
 }
 return {...a,href};
 }));
 const selected=selection && selection.expires_at>Date.now() ? options.find(option=>option.id===selection.assignment_id && option.version===selection.assignment_version) : undefined;
 return {access,write,selection,options,selected};
}
