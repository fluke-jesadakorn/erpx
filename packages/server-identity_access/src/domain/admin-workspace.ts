import {parseHoldingWorkspace,workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {safeReturnPath} from '@erpx/server-identity_access/domain/access-problem';

/** A Company is a selection hint, never a grant. Provider management remains
 * a separate exact ERPX Holding destination with its original dual authority. */
export function adminWorkspaceTarget(companyId:string|null,value:string|null){
 const target=safeReturnPath(value);if(!companyId||companyId.length>128||!target)return null;
 try{const parts=new URL(target,'https://admin.invalid').pathname.split('/').filter(Boolean).map(decodeURIComponent),parsed=parts[0]==='holdings'?parseHoldingWorkspace(parts[1],parts.slice(2)):null;if(!parsed)return null;
  const destination=parsed.destination.join('/'),management=destination==='erpx-management'||destination.startsWith('erpx-management/');
  if(management&&(parsed.scope.holdingSlug!=='erpx'||parsed.scope.kind!=='holding'))return null;
  if(!management&&destination!=='administration'&&!destination.startsWith('administration/'))return null;
  return {companyId,target,scope:parsed.scope,destination,kind:management?'management' as const:'company' as const};
 }catch{return null;}
}
export type AdminWorkspaceInput=NonNullable<ReturnType<typeof adminWorkspaceTarget>>;
export function adminTargetAllowed(input:AdminWorkspaceInput,paths:readonly string[]){
 const path=new URL(input.target,'https://admin.invalid').pathname.slice(workspaceBase(input.scope).length+1);
 return paths.some(allowed=>path===allowed||path.startsWith(allowed+'/')||allowed.startsWith(path+'/'));
}
