import 'server-only';
import snapshot from '@/data/business-readiness.generated.json';
import type {WorkContext} from './authorization';
import {holdingResources} from './runtime';
import {companyAuthority} from './staff-policy';
import {ApiError} from './http';

export async function businessReadiness(actor:WorkContext) {
 const {write}=await holdingResources(actor.holdingId);
 if(actor.branchId||actor.departmentId||!await companyAuthority(write,actor))throw new ApiError(403,'COMPANY_READINESS_ACCESS_REQUIRED','Use an active Company organization-management assignment to review readiness.');
 const superseded=new Set(snapshot.observations.flatMap(record=>record.supersedes?[record.supersedes]:[]));
 return {
  scope:'workspace_simulation' as const,
  companySignOff:false as const,
  evidenceLedgerSha256:snapshot.evidenceLedgerSha256,
  capabilities:snapshot.capabilities.map(capability=>({...capability,package:String.fromCharCode(65+capability.phase),observations:snapshot.observations.filter(record=>record.capabilityIds.includes(capability.id)&&!superseded.has(record.id))})),
  scenarios:Array.from({length:21},(_,index)=>{
   const id=`S${String(index+1).padStart(2,'0')}`;
   return {id,capabilityIds:snapshot.capabilities.filter(row=>row.scenarios.includes(id)).map(row=>row.id),observationIds:snapshot.observations.filter(record=>record.scenarioIds.includes(id)&&!superseded.has(record.id)).map(record=>record.id)};
  }),
 };
}
