import 'server-only';
import {staffPickerDisplayOptions} from '@erpx/feature-work_projection/lib/staff-picker-display';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {ApiError} from '@erpx/server-runtime/public/http';
import {activeStaff} from '@erpx/server-operations/public/staff-policy';
import {staffSettings} from '@erpx/server-operations/public/staff-settings';
import {getProject} from '@erpx/server-projects/adapters/staff-projects';
import {projectPurchaseControls} from '@erpx/server-projects/adapters/purchases';
/** Native Project creation keeps the original Operations task owner/API.
 * Query labels cannot create a Project, pick an acting role or license Work. */
export async function projectTaskCreationWorkspace(actor:WorkContext,id:string){
 const database=(await holdingResources(actor.holdingId)).write,project=await getProject(database,actor,id),purchases=await projectPurchaseControls(database,actor),hasPurchase=!purchases||!!purchases['projects.wbs']?.newWork&&!!purchases['projects.tasks']?.newWork;
 const linked=await activeStaff(database,actor.companyId,actor.userId).then(()=>true).catch(error=>{if(error instanceof ApiError&&error.code==='STAFF_LINK_REQUIRED')return false;throw error;});
 const canCreate=!actor.beta&&project.lifecycle_status==='active'&&hasPermission(actor,'records.write')&&hasPurchase&&linked;
 const options=canCreate?await staffSettings(database,actor):{people:[],levels:[],departments:[]};
 return {project:{id:project.id,title:project.title,status:project.lifecycle_status},canCreate,reason:!linked?'An active staff profile is required to own this Project task.':!canCreate?'New tasks are unavailable for this Project under your current role and purchase.':null,options:staffPickerDisplayOptions(options),...(purchases?{purchases}:{})};
}
