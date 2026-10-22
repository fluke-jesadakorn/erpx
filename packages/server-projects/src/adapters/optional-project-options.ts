import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {StaffActor} from '@erpx/server-operations/public/staff-policy';
import {projectsPackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import {projectPurchaseControls} from '@erpx/server-projects/adapters/purchases';
import {listProjects} from '@erpx/server-projects/adapters/staff-projects';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {betaAccess} from '@erpx/server-identity_access/public/beta-access';

async function strictReferenceReady(database:SqlDatabase,actor:StaffActor){
 if(await betaAccess(database,actor.companyId,new Headers()))return false;
 const readiness=await database.prepare(`SELECT (${scheduledFeaturePredicate('project_reference.company_id','projects')} AND ${scheduledFeaturePredicate('project_reference.company_id','operations')}) allowed FROM (SELECT ?::text company_id) project_reference`).bind(actor.companyId).first<{allowed:boolean}>();
 return readiness?.allowed===true;
}

/** Optional references never grant Project management. A configured Work host
 * uses the current purchased history even while legacy enforcement is disabled;
 * Console callers retain the existing mode and original Project record policy. */
export async function optionalProjectOptions(database:SqlDatabase,actor:StaffActor,options?:{strictProduct?:boolean}){
 const strict=options?.strictProduct===true;
 if(strict||await projectsPackageEnforcementMode()==='enforced'){
  const access=await projectPurchaseControls(database,actor,strict?{strictProduct:true}:undefined);
  if(!access?.['projects.wbs'].history)return [];
  if(strict&&!await strictReferenceReady(database,actor))return [];
 }
 const projects=await listProjects(database,actor);
 if(strict&&(!((await projectPurchaseControls(database,actor,{strictProduct:true}))?.['projects.wbs'].history)||!await strictReferenceReady(database,actor)))return [];
 return projects;
}
