import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import type {AdminWorkspaceInput} from '@erpx/contracts/admin-workspace';
export {adminWorkspaceTarget} from '@erpx/contracts/admin-workspace';
export type {AdminWorkspaceInput} from '@erpx/contracts/admin-workspace';
export function adminTargetAllowed(input:AdminWorkspaceInput,paths:readonly string[]){
 const path=new URL(input.target,'https://admin.invalid').pathname.slice(workspaceBase(input.scope).length+1);
 return paths.some(allowed=>path===allowed||path.startsWith(allowed+'/')||allowed.startsWith(path+'/'));
}
